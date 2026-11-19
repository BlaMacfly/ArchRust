//! Lien d'objet de la discussion contre ArchCore, sans affichage : GmTest crée une épée et chuchote son lien
//! `<ITEM::IID::nom>` à Second ; Second reçoit le texte intact, demande l'objet (AGPMITEM op 31, genre 8) et
//! reçoit l'op 32 (IID, TID, genre renvoyé) ; un objet inconnu, puis l'épée détruite, restent sans réponse.
//!
//! ```text
//! cargo run -p archrust-netclient --example item_link -- [hôte[:port]]
//! ```
//! Par défaut 127.0.0.1:11102, comptes des bases de test gmtest/GmTest (MJ) et test2/Second.

use archrust_netclient::items::{info_kind, ItemCommand};
use archrust_netclient::{Command, LoginClient, WorldClient, WorldEvent};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{timeout, Instant};

/// Épée courte (guerrier).
const SWORD: i32 = 11;

struct Session {
    name: String,
    commands: mpsc::Sender<Command>,
    events: mpsc::Receiver<WorldEvent>,
}

impl Session {
    async fn send(&self, c: Command) {
        self.commands.send(c).await.expect("liaison fermée");
    }

    async fn expect(&mut self, what: &str, secs: u64, mut want: impl FnMut(&WorldEvent) -> bool) -> anyhow::Result<WorldEvent> {
        let deadline = Instant::now() + Duration::from_secs(secs);
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match timeout(left, self.events.recv()).await {
                Ok(Some(WorldEvent::Disconnected(why))) => anyhow::bail!("{} déconnecté : {why}", self.name),
                Ok(Some(ev)) if want(&ev) => return Ok(ev),
                Ok(Some(_)) => {}
                Ok(None) => anyhow::bail!("{} : liaison fermée", self.name),
                Err(_) => break,
            }
        }
        anyhow::bail!("{what} : rien reçu en {secs} s")
    }

    /// Aucun événement accepté par `want` pendant `ms` millisecondes.
    async fn none(&mut self, ms: u64, mut want: impl FnMut(&WorldEvent) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_millis(ms);
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match timeout(left, self.events.recv()).await {
                Ok(Some(ev)) if want(&ev) => return false,
                Ok(Some(_)) => {}
                _ => break,
            }
        }
        true
    }

    async fn drain(&mut self, ms: u64) {
        while let Ok(Some(_)) = timeout(Duration::from_millis(ms), self.events.recv()).await {}
    }
}

async fn enter(host: &str, port: u16, user: &str, pass: &str, name: &str) -> anyhow::Result<Session> {
    let mut login = LoginClient::connect(host, port).await?;
    login.authenticate(user, pass).await?;
    let world = login.worlds.first().map(|w| w.name.clone()).unwrap_or_else(|| "ArchCore".into());
    let names = login.select_world(&world).await?;
    anyhow::ensure!(names.iter().any(|n| n.eq_ignore_ascii_case(name)), "{name} absent du compte {user}");
    let ticket = login.enter_world(name, &world).await?;
    login.close().await;
    let mut wc = WorldClient::connect(ticket).await?;
    wc.enter().await?;
    println!("{name} en jeu (cid {})", wc.cid);
    let (etx, erx) = mpsc::channel(512);
    let (ctx, crx) = mpsc::channel(64);
    tokio::spawn(wc.run(etx, crx));
    Ok(Session { name: name.into(), commands: ctx, events: erx })
}

/// IID du premier lien `<ITEM::IID::…>` d'un texte.
fn link_iid(text: &str) -> Option<i32> {
    let rest = &text[text.find("<ITEM::")? + 7..];
    rest[..rest.find("::")?].parse().ok()
}

fn is_info(iid: i32) -> impl Fn(&WorldEvent) -> bool {
    move |e| matches!(e, WorldEvent::ItemInfo(i) if i.iid == iid)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let hostport = std::env::args().nth(1).unwrap_or_else(|| "127.0.0.1:11102".into());
    let (host, port) = hostport.split_once(':').map_or((hostport.as_str(), 11102), |(h, p)| (h, p.parse().unwrap_or(11102)));
    let mut a = enter(host, port, "gmtest", "gmtest", "GmTest").await?;
    let mut b = enter(host, port, "test2", "test2", "Second").await?;
    a.drain(1500).await;
    b.drain(1500).await;

    println!("\n▶ GmTest crée une épée");
    a.send(Command::Say(format!("/create \"GmTest\" {SWORD} 1"))).await;
    let ev = a.expect("épée créée", 5, |e| matches!(e, WorldEvent::ItemChanged(i) if i.template_id == Some(SWORD) && i.operation == 0)).await?;
    let WorldEvent::ItemChanged(sword) = ev else { unreachable!() };
    println!("  ✓ IID {}", sword.id);

    println!("\n▶ lien chuchoté à Second");
    let text = format!("regarde <ITEM::{}::Épée courte> !", sword.id);
    a.send(Command::Whisper { to: "Second".into(), message: text.clone() }).await;
    let ev = b.expect("chuchotement", 5, |e| matches!(e, WorldEvent::Chat { message, .. } if message.contains("<ITEM::"))).await?;
    let WorldEvent::Chat { message, .. } = ev else { unreachable!() };
    anyhow::ensure!(message.ends_with(&text), "texte reçu : {message:?}");
    let iid = link_iid(&message).ok_or_else(|| anyhow::anyhow!("lien illisible : {message:?}"))?;
    anyhow::ensure!(iid == sword.id, "IID du lien {iid}");
    println!("  ✓ « {message} »");

    println!("\n▶ Second demande l'objet (op 31, genre 8)");
    b.send(Command::Item(ItemCommand::RequestInfo { iid, tid: 0, kind: info_kind::CHAT_LINK })).await;
    let ev = b.expect("op 32", 5, is_info(iid)).await?;
    let WorldEvent::ItemInfo(info) = ev else { unreachable!() };
    anyhow::ensure!(info.tid == SWORD && info.kind == info_kind::CHAT_LINK, "op 32 : {info:?}");
    println!("  ✓ op 32 : {info:?}");

    println!("\n▶ objet inconnu : pas de réponse");
    b.send(Command::Item(ItemCommand::RequestInfo { iid: 999_999_999, tid: 0, kind: info_kind::CHAT_LINK })).await;
    anyhow::ensure!(b.none(1500, is_info(999_999_999)).await, "réponse pour un objet inconnu");
    println!("  ✓ rien");

    println!("\n▶ épée détruite, lien périmé : pas de réponse");
    a.send(Command::Item(ItemCommand::Destroy(sword.id))).await;
    a.expect("épée détruite", 5, |e| matches!(e, WorldEvent::ItemChanged(i) if i.id == sword.id && i.operation == 1)).await?;
    b.send(Command::Item(ItemCommand::RequestInfo { iid, tid: 0, kind: info_kind::CHAT_LINK })).await;
    anyhow::ensure!(b.none(1500, is_info(iid)).await, "réponse pour l'épée détruite");
    println!("  ✓ rien");

    a.send(Command::Quit).await;
    b.send(Command::Quit).await;
    println!("\nLien d'objet : tout est bon.");
    Ok(())
}
