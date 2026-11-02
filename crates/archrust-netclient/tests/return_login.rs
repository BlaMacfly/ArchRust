//! Retour à la sélection des personnages contre de faux serveurs qui rejouent les paquets d'ArchCore
//! (`world/return_login.rs`, `login.rs`, scénario `archcore sim returnlogin`) : au monde, op 1 → op 2 (refus) puis
//! op 1 → op 5 (clé, fin de la boucle de jeu) ; au serveur de connexion, op 6 → clé de hachage, connexion avec les
//! identifiants gardés → AGPMLOGIN op 1 puis 0x2F op 7, monde (op 3/4/5) et liste des personnages (op 6/7).

use archcore_net::schema::{login_op, mini, CHARACTER, LOGIN, RETURN_TO_LOGIN};
use archcore_net::{build_mini, build_packet, decode_packet, parse_header, Fields, Packet};
use archrust_netclient::return_login::{ReturnLoginEvent, ReturnTicket};
use archrust_netclient::world::SessionEnd;
use archrust_netclient::{Command, Connection, LoginClient, WorldClient, WorldEvent, WorldTicket};
use archrust_protocol::framing::{CryptoSession, Direction, FrameSplitter};
use archrust_protocol::handshake::ServerHandshake;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

const CID: u32 = 42;
const KEY: u32 = 123_456;

/// Faux serveur : poignée de main, paquets du client en clair (HELLO des extensions écarté), envois chiffrés.
struct FakeServer {
    stream: TcpStream,
    crypto: CryptoSession,
    splitter: FrameSplitter,
    pending: Vec<Vec<u8>>,
}

impl FakeServer {
    async fn accept(listener: &TcpListener) -> Self {
        let (stream, _) = listener.accept().await.unwrap();
        let mut s = FakeServer { stream, crypto: CryptoSession::new(), splitter: FrameSplitter::new(), pending: Vec::new() };
        let mut hs = ServerHandshake::default();
        while !hs.done {
            let p = s.recv_any().await;
            let out = hs.on_packet(&mut s.crypto, &p).unwrap();
            for plain in out.plain {
                s.stream.write_all(&plain).await.unwrap();
            }
            for enc in out.encrypted {
                s.stream.write_all(&enc).await.unwrap();
            }
        }
        s
    }

    async fn recv_any(&mut self) -> Vec<u8> {
        loop {
            if !self.pending.is_empty() {
                return self.pending.remove(0);
            }
            let mut buf = vec![0u8; 4096];
            let n = self.stream.read(&mut buf).await.unwrap();
            assert!(n > 0, "client parti");
            for f in self.splitter.push(&buf[..n]).unwrap() {
                let plain = if f.encrypted { self.crypto.decode_frame(&f.data).unwrap() } else { f.data };
                self.pending.push(plain);
            }
        }
    }

    /// Prochain paquet du schéma donné (HELLO 0xAC et keepalive ignorés).
    async fn expect(&mut self, schema: &archcore_net::Schema) -> Packet {
        loop {
            let raw = tokio::time::timeout(Duration::from_secs(5), self.recv_any()).await.expect("paquet du client");
            if parse_header(&raw).is_some_and(|h| h.ptype == schema.id) {
                return decode_packet(schema, &raw).unwrap();
            }
        }
    }

    async fn send(&mut self, plain: &[u8]) {
        let f = self.crypto.encode_frame(plain, Direction::ServerToClient).unwrap();
        self.stream.write_all(&f).await.unwrap();
    }
}

fn ret(map: impl FnOnce(Fields) -> Fields) -> Vec<u8> {
    build_packet(&RETURN_TO_LOGIN, &map(Fields::new(&RETURN_TO_LOGIN)))
}

fn login(map: impl FnOnce(Fields) -> Fields) -> Vec<u8> {
    build_packet(&LOGIN, &map(Fields::new(&LOGIN)))
}

async fn listener() -> (TcpListener, u16) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    (l, port)
}

async fn next_event(rx: &mut mpsc::Receiver<WorldEvent>) -> WorldEvent {
    loop {
        let ev = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await.expect("événement").expect("canal ouvert");
        if matches!(ev, WorldEvent::ReturnToLogin(_)) {
            return ev;
        }
    }
}

#[tokio::test]
async fn monde_refus_puis_cle() {
    let (l, port) = listener().await;
    let (conn, mut server) = tokio::join!(
        async move {
            let mut c = Connection::connect("world", "127.0.0.1", port).await.unwrap();
            c.ext_enabled = false;
            c.handshake().await.unwrap();
            c
        },
        FakeServer::accept(&l)
    );
    let ticket = WorldTicket {
        character: "GmTest".into(),
        world: "ArchCore".into(),
        auth_key: 1,
        host: "127.0.0.1".into(),
        port,
        language: None,
    };
    let wc = WorldClient {
        conn,
        ticket,
        cid: CID as i32,
        position: glam::Vec3::ZERO,
        seen: Vec::new(),
        account: "gmtest".into(),
        password: "gmtest".into(),
        reconnect_key: 0,
    };
    let (etx, mut erx) = mpsc::channel(64);
    let (ctx, crx) = mpsc::channel(8);
    let run = tokio::spawn(wc.run_until(etx, crx));

    // Op 1 → op 2 : refus (en combat, mort).
    ctx.send(Command::ReturnToLogin).await.unwrap();
    let p = server.expect(&RETURN_TO_LOGIN).await;
    assert_eq!((p.int(0), p.int(1)), (Some(1), Some(CID as i64)));
    server.send(&ret(|f| f.with(0, 2).with(1, CID))).await;
    assert!(matches!(next_event(&mut erx).await, WorldEvent::ReturnToLogin(ReturnLoginEvent::Refused)));

    // Op 1 → op 5 sans adresse : départ du monde, la boucle rend la clé.
    ctx.send(Command::ReturnToLogin).await.unwrap();
    server.expect(&RETURN_TO_LOGIN).await;
    server.send(&ret(|f| f.with(0, 5).with(1, CID).with(5, KEY))).await;
    assert!(matches!(next_event(&mut erx).await, WorldEvent::ReturnToLogin(ReturnLoginEvent::Leaving)));
    let (end, _commands) = tokio::time::timeout(Duration::from_secs(5), run).await.unwrap().unwrap();
    assert_eq!(end, SessionEnd::ReturnToLogin(ReturnTicket { key: KEY as i32, address: None }));
}

/// Serveur de connexion d'ArchCore pour un retour : `accept` dit si la clé est bonne.
async fn login_server(l: TcpListener, accept: bool) {
    let mut s = FakeServer::accept(&l).await;
    let p = s.expect(&RETURN_TO_LOGIN).await;
    assert_eq!((p.int(0), p.text(2).as_deref(), p.int(5)), (Some(6), Some("gmtest"), Some(KEY as i64)));
    if !accept {
        let info = build_mini(&mini::LOGIN_CHAR_INFO, &Fields::new(&mini::LOGIN_CHAR_INFO));
        s.send(&login(|f| f.with(0, login_op::LOGIN_RESULT).with(8, info).with(10, 0))).await;
        return;
    }
    let hash = "abcdefgh";
    s.send(&login(|f| f.with(0, login_op::HASHKEY).with(1, hash))).await;
    let p = s.expect(&LOGIN).await;
    assert_eq!(p.int(0), Some(login_op::USER_LOGIN));
    let user = archrust_crypto::capi_crypt(hash, &p.bytes(2).unwrap()[..p.int(3).unwrap() as usize]);
    let pass = archrust_crypto::capi_crypt(hash, &p.bytes(4).unwrap()[..p.int(5).unwrap() as usize]);
    assert_eq!((user.as_slice(), pass.as_slice()), (&b"gmtest"[..], &b"secret"[..]), "identifiants gardés");
    let acct_raw = p.bytes(2).unwrap().to_vec();
    s.send(&login(|f| f.with(0, login_op::USER_LOGIN).with(2, acct_raw).with(14, 0).with(15, 0))).await;
    s.send(&ret(|f| f.with(0, 7))).await;
    // Monde choisi, puis liste des personnages.
    let p = s.expect(&LOGIN).await;
    assert_eq!((p.int(0), p.text(7).as_deref()), (Some(login_op::UNION_INFO), Some("ArchCore")));
    s.send(&login(|f| f.with(0, login_op::UNION_INFO))).await;
    let info = build_mini(&mini::LOGIN_CHAR_INFO, &Fields::new(&mini::LOGIN_CHAR_INFO).with(1, "GmTest"));
    s.send(&login(|f| f.with(0, login_op::CHAR_NAME).with(8, info))).await;
    s.send(&login(|f| f.with(0, login_op::CHAR_NAME_FINISH))).await;
    let p = s.expect(&LOGIN).await;
    assert_eq!(p.int(0), Some(login_op::CHAR_LIST));
    s.send(&build_packet(&CHARACTER, &Fields::new(&CHARACTER).with(0, 0).with(1, CID).with(2, 96u32).with(3, "GmTest"))).await;
    s.send(&login(|f| f.with(0, login_op::CHAR_LIST_FINISH))).await;
    // Le client garde la connexion pour les écrans de sélection.
    let _ = tokio::time::timeout(Duration::from_millis(300), s.recv_any()).await;
}

#[tokio::test]
async fn connexion_op6_op7_et_liste() {
    let (l, port) = listener().await;
    let server = tokio::spawn(login_server(l, true));
    let (client, chars) =
        LoginClient::return_to_selection("127.0.0.1", port, None, "gmtest", "secret", KEY as i32, "ArchCore").await.expect("retour");
    assert_eq!(client.names, vec!["GmTest".to_string()]);
    assert_eq!(chars.len(), 1);
    assert_eq!((chars[0].cid, chars[0].tid, chars[0].name.as_str()), (CID as i32, 96, "GmTest"));
    server.await.unwrap();
}

#[tokio::test]
async fn cle_refusee() {
    let (l, port) = listener().await;
    let server = tokio::spawn(login_server(l, false));
    let r = LoginClient::return_to_selection("127.0.0.1", port, None, "gmtest", "secret", KEY as i32, "ArchCore").await;
    assert!(r.is_err(), "clé inconnue ou échue : AGPMLOGIN op 15");
    server.await.unwrap();
}
