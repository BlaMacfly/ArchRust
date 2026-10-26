//! Reconnexion automatique au serveur de jeu (AGPMRECONNECT, 0x63), comme AgcmReconnect d'Archonia.
//!
//! ```text
//! entrée en jeu    ← op 2 (1 CID, 5 clé, 6 accord 0) : la clé est gardée (FUN_14063bd10)
//! coupure          tentative toutes les 5 s pendant 60 s (FUN_14063be40) : nouvelle connexion à l'adresse du
//!                  monde, puis op 1 (1 CID, 2 nom, 3 compte, 4 mot de passe, 5 clé) au lieu de l'entrée normale
//!                  (FUN_1404b8100)
//!                  ← op 2 avec accord 1 : la partie reprend sur la nouvelle connexion (nouvelle clé)
//! 60 s écoulées    abandon : déconnexion
//! ```
//! Validé contre ArchCore par `archcore sim reconnect`.

use crate::connection::Incoming;
use archrust_protocol::packet::{sparse, Value};
use archrust_protocol::schema::{main, packet_type, Schema};
use std::time::Duration;

/// Délai de reconnexion (60 000 ms d'AgcmReconnect).
pub const WINDOW: Duration = Duration::from_secs(60);
/// Intervalle des tentatives (5000 ms, constructeur FUN_14063bbe0).
pub const RETRY: Duration = Duration::from_secs(5);

pub mod op {
    pub const REQUEST: i8 = 1;
    pub const RESULT: i8 = 2;
}

/// Réponse du serveur (op 2) : clé (0 : inchangée) et accord de reprise.
pub fn result_of(p: &Incoming) -> Option<(i32, bool)> {
    if !p.is(packet_type::RECONNECT, Some(op::RESULT as i64)) {
        return None;
    }
    let key = p.get(5).and_then(Value::as_i64).unwrap_or(0) as i32;
    let accepted = p.get(6).and_then(Value::as_i64) == Some(1);
    Some((key, accepted))
}

/// Demande de reprise (op 1).
pub fn request_packet(cid: i32, name: &str, account: &str, password: &str, key: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::RECONNECT;
    (
        s,
        sparse(
            s,
            &[
                (0, Value::I8(op::REQUEST)),
                (1, Value::I32(cid)),
                (2, Value::str(name)),
                (3, Value::str(account)),
                (4, Value::str(password)),
                (5, Value::I32(key)),
                (6, Value::I8(0)),
            ],
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demande() {
        let (s, v) = request_packet(7, "Testeur", "test", "secret", 1234);
        assert_eq!(s.id, 0x63);
        assert_eq!(v[0].as_ref().and_then(Value::as_i64), Some(1));
        assert_eq!(v[5].as_ref().and_then(Value::as_i64), Some(1234));
        assert_eq!(v[3].as_ref().and_then(Value::as_bytes).map(<[u8]>::to_vec), Some(b"test".to_vec()));
    }
}
