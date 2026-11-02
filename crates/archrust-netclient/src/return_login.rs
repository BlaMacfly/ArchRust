//! Retour à la sélection des personnages (AgpmReturnToLogin, 0x2F, drapeaux 1) : bouton « Select Character » de
//! `UI_EXIT` (`EXIT_ReturnLoginCharacterSelect`). Champs : 0 opération, 1 CID, 2 compte (49), 3 groupe de serveurs
//! (32), 4 adresse du serveur de connexion (25), 5 clé.
//!
//! ```text
//!   client → monde op 1 (1 CID)               après les contrôles du client (FUN_1405a65d0 d'AgcmUILogin)
//!   monde → client op 2                       refus : rappel 0 d'AgcmReturnToLogin, qu'aucun module n'écoute
//!                                             (rien d'affiché)
//!   monde → client op 5 (1 CID, 4 adresse,    FUN_140686900 : fenêtres fermées, personnages, compétences et objets
//!                        5 clé)               oubliés, connexion au monde fermée, clé gardée, connexion au serveur
//!                                             de connexion (adresse du champ 4, vide : celle d'AgcmLogin)
//!   client → connexion op 6 (2 compte, 5 clé) une fois la connexion chiffrée (FUN_1405eccd0 → FUN_140686fe0), au
//!                                             lieu de la demande de clé de hachage
//!   connexion → client AGPMLOGIN op 0         clé de hachage ; le client se reconnecte avec le compte et le mot de
//!                                             passe gardés (AGPMLOGIN op 1)
//!   connexion → client op 7                   FUN_1406868b0 : rappel 1 (AgcmUILogin FUN_1405a6910 : effets, caméra,
//!                                             nature et fenêtres remis à zéro, scène de sélection), clé oubliée,
//!                                             puis AGPMLOGIN op 3 du monde choisi et liste des personnages
//! ```
//! Contrôles avant l'envoi (FUN_1405a65d0) : moins de 5 s depuis la dernière demande ou le dernier combat →
//! `CannotUseByStatus` ; dans une arène ou en combat → `BattleNotUse` ; sinon fenêtres fermées puis op 1
//! (FUN_140686f60 → FUN_1402cfec0). Côté serveur : `world/return_login.rs` et `login.rs` d'ArchCore, scénario
//! `archcore sim returnlogin`. Enchaînement de la liaison : `WorldClient::run_until` et
//! `LoginClient::return_to_selection`.

use crate::connection::Incoming;
use archrust_protocol::packet::{sparse, Value};
use archrust_protocol::schema::{main, packet_type, Schema};

pub mod op {
    /// Demande du client (FUN_1402cfec0 : champ 0 = 1).
    pub const REQUEST: i8 = 1;
    /// Refus du serveur de jeu.
    pub const REQUEST_FAILED: i8 = 2;
    /// Clé et adresse du serveur de connexion.
    pub const SEND_KEY_ADDR: i8 = 5;
    /// Reconnexion au serveur de connexion (FUN_1402d00c0 : compte et clé).
    pub const RECONNECT_LOGIN_SERVER: i8 = 6;
    /// Fin du retour, envoyée par le serveur de connexion après la connexion réussie.
    pub const END_PROCESS: i8 = 7;
}

/// Ce que le moteur apprend du retour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnLoginEvent {
    /// Op 2 : retour refusé par le serveur de jeu (en combat, mort… : choix d'ArchCore).
    Refused,
    /// Op 5 : le personnage a quitté le monde ; la liaison rejoint le serveur de connexion.
    Leaving,
}

/// Clé remise par le serveur de jeu (op 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnTicket {
    pub key: i32,
    /// Adresse « hôte:port » du serveur de connexion ; `None` : celle où le client s'est connecté.
    pub address: Option<(String, u16)>,
}

/// Adresse « hôte:port » du champ 4 ; vide ou illisible : `None`.
pub fn parse_address(s: &str) -> Option<(String, u16)> {
    let (host, port) = s.trim().rsplit_once(':')?;
    let port = port.trim().parse::<u16>().ok()?;
    (!host.trim().is_empty()).then(|| (host.trim().to_string(), port))
}

/// Op 5 reçu : clé et adresse.
pub fn ticket_of(p: &Incoming) -> Option<ReturnTicket> {
    if !p.is(packet_type::RETURN_TO_LOGIN, Some(op::SEND_KEY_ADDR as i64)) {
        return None;
    }
    let key = p.get(5).and_then(Value::as_i64).unwrap_or(0) as i32;
    let address = p.get(4).and_then(Value::as_text).and_then(|a| parse_address(&a));
    Some(ReturnTicket { key, address })
}

/// Op 2 reçu : retour refusé.
pub fn is_refused(p: &Incoming) -> bool {
    p.is(packet_type::RETURN_TO_LOGIN, Some(op::REQUEST_FAILED as i64))
}

/// Op 7 reçu : fin du retour.
pub fn is_end(p: &Incoming) -> bool {
    p.is(packet_type::RETURN_TO_LOGIN, Some(op::END_PROCESS as i64))
}

/// Paquet de la demande de retour à la sélection du personnage.
pub fn request_packet(me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    (main::RETURN_TO_LOGIN, sparse(main::RETURN_TO_LOGIN, &[(0, Value::I8(op::REQUEST)), (1, Value::I32(me))]))
}

/// Reconnexion au serveur de connexion (op 6) : compte (champ 2) et clé (champ 5), comme FUN_1402d00c0.
pub fn reconnect_packet(account: &str, key: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::RETURN_TO_LOGIN;
    (s, sparse(s, &[(0, Value::I8(op::RECONNECT_LOGIN_SERVER)), (2, Value::str(account)), (5, Value::I32(key))]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use archcore_net::schema as srv;
    use archcore_net::{build_packet as srv_build, decode_packet as srv_decode, Fields};
    use archrust_protocol::packet::{build_packet, decode_packet};

    /// Paquet reçu tel que `Connection` le remet, depuis les octets du serveur.
    fn incoming(raw: Vec<u8>) -> Incoming {
        let packet = decode_packet(main::RETURN_TO_LOGIN, &raw).ok();
        Incoming { packet_type: packet_type::RETURN_TO_LOGIN, packet, raw }
    }

    #[test]
    fn demande_de_retour_a_la_selection() {
        let (schema, v) = request_packet(42);
        assert_eq!(schema.name, "AGPMRETURNTOLOGIN");
        assert_eq!((v[0].clone(), v[1].clone()), (Some(Value::I8(1)), Some(Value::I32(42))));
        assert!(v[2..].iter().all(Option::is_none), "seuls l'opération et le CID");
        // Paquet complet : type 0x2F, drapeaux 0b11 (champs 0 et 1), relu à l'identique.
        let raw = build_packet(schema, &v).expect("encodage");
        let back = decode_packet(schema, &raw).expect("décodage");
        assert_eq!(back.header.packet_type, 0x2f);
        assert_eq!(back.flags, 0b11);
        assert_eq!(back.values, v);
        // Lu par ArchCore : op 1 et CID.
        let p = srv_decode(&srv::RETURN_TO_LOGIN, &raw).unwrap();
        assert_eq!((p.int(0), p.int(1)), (Some(1), Some(42)));
    }

    #[test]
    fn reconnexion_au_serveur_de_connexion() {
        let (schema, v) = reconnect_packet("gmtest", 123_456);
        let raw = build_packet(schema, &v).expect("encodage");
        let back = decode_packet(schema, &raw).expect("décodage");
        assert_eq!(back.flags, 0b10_0101, "champs 0, 2 et 5");
        // Lu comme le fait `login.rs` d'ArchCore (`on_return`).
        let p = srv_decode(&srv::RETURN_TO_LOGIN, &raw).unwrap();
        assert_eq!((p.int(0), p.text(2).as_deref(), p.int(5)), (Some(6), Some("gmtest"), Some(123_456)));
    }

    #[test]
    fn reponses_du_serveur() {
        let s = &srv::RETURN_TO_LOGIN;
        // Op 5 sans adresse (`ReturnToLogin.LoginAddress` vide) : celle déjà connue.
        let p = incoming(srv_build(s, &Fields::new(s).with(0, 5).with(1, 42u32).with(5, 123_456u32)));
        assert_eq!(ticket_of(&p), Some(ReturnTicket { key: 123_456, address: None }));
        assert!(!is_refused(&p) && !is_end(&p));
        // Op 5 avec adresse.
        let p = incoming(srv_build(s, &Fields::new(s).with(0, 5).with(1, 42u32).with(4, "127.0.0.1:11002").with(5, 7u32)));
        assert_eq!(ticket_of(&p), Some(ReturnTicket { key: 7, address: Some(("127.0.0.1".into(), 11002)) }));
        // Op 2 (refus) et op 7 (fin).
        let p = incoming(srv_build(s, &Fields::new(s).with(0, 2).with(1, 42u32)));
        assert!(is_refused(&p) && ticket_of(&p).is_none());
        let p = incoming(srv_build(s, &Fields::new(s).with(0, 7)));
        assert!(is_end(&p) && !is_refused(&p));
    }

    #[test]
    fn adresses() {
        assert_eq!(parse_address("10.0.0.2:11002"), Some(("10.0.0.2".into(), 11002)));
        assert_eq!(parse_address(""), None);
        assert_eq!(parse_address("hôte"), None);
        assert_eq!(parse_address(":11002"), None);
    }
}
