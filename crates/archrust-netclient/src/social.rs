//! Actions sociales (« Emotes ») : AGPMCHARACTER (0x02, drapeaux 4) op 16 `SOCIAL_ANIMATION`.
//!
//! ```text
//! C → S (Archonia FUN_1404b8570) : 0 op 16, 1 son CID, 16 type (U8, 0-16 = ligne de SocialAction.txt − 1)
//! S → C (FUN_140173f10 cas 0x10 → FUN_1401780d0) : 1 CID de n'importe quel personnage connu, 16 type
//! ```
//!
//! Aucun code de résultat. Le client n'anime rien à l'envoi : l'animation et le son de `INI\SOCIAL_PC.txt`
//! (ligne = type, colonne = TID du personnage) suivent l'op 16 reçue, le serveur la renvoyant aussi au lanceur.

use archrust_protocol::packet::{sparse, Value};
use archrust_protocol::schema::{main, packet_type, Schema};

/// Opération d'AGPMCHARACTER.
pub const OP_SOCIAL_ANIMATION: i64 = 16;
/// Champ du type d'action.
pub const FIELD_TYPE: usize = 16;
/// Types envoyés par un joueur : 0 salut … 16 assis (les types suivants, SELECT* et GM_*, ne viennent pas de lui).
pub const PLAYER_TYPES: u8 = 17;

/// Action sociale d'un personnage (op 16 reçue).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocialEvent {
    pub cid: i32,
    pub kind: u8,
}

/// Demande du joueur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocialCommand {
    /// Action sociale de ce type (0-16).
    Play(u8),
}

fn int(values: &[Option<Value>], i: usize) -> Option<i64> {
    values.get(i).and_then(Option::as_ref).and_then(Value::as_i64)
}

/// Paquet AGPMCHARACTER op 16 traduit, sinon `None`.
pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<SocialEvent> {
    if ptype != packet_type::CHARACTER || int(values, 0)? != OP_SOCIAL_ANIMATION {
        return None;
    }
    Some(SocialEvent { cid: int(values, 1)? as i32, kind: int(values, FIELD_TYPE)? as u8 })
}

/// Paquet d'une demande pour le personnage `me`.
pub fn command_packet(cmd: &SocialCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::CHARACTER;
    match cmd {
        SocialCommand::Play(kind) => (s, sparse(s, &[(0, Value::I8(OP_SOCIAL_ANIMATION as i8)), (1, Value::I32(me)), (FIELD_TYPE, Value::U8(*kind))])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet};

    #[test]
    fn envoi_et_reception() {
        let (schema, values) = command_packet(&SocialCommand::Play(5), 42);
        assert_eq!(schema.id, packet_type::CHARACTER);
        let raw = build_packet(schema, &values).unwrap();
        let decoded = decode_packet(schema, &raw).unwrap();
        assert_eq!(translate(schema.id, &decoded.values), Some(SocialEvent { cid: 42, kind: 5 }));
        // Autre opération d'AGPMCHARACTER : rien.
        let other = sparse(schema, &[(0, Value::I8(1)), (1, Value::I32(42)), (FIELD_TYPE, Value::U8(5))]);
        let decoded = decode_packet(schema, &build_packet(schema, &other).unwrap()).unwrap();
        assert_eq!(translate(schema.id, &decoded.values), None);
    }
}
