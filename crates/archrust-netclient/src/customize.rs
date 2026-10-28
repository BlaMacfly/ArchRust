//! Changement d'apparence chez les « Stylist » (AgpmEventCharCustomize, 0x43 ; fonction de PNJ 29).
//!
//! Relevé dans Archonia (sha256 e9c7609b…, `FUN_14022ceb0`, `FUN_14022d200`) : op 0 demande (EVENT_BASE du PNJ,
//! CID), op 1 accord (même désignation, CID du joueur), op 2 achat (visage au champ 3, coiffure au champ 4,
//! -1 pour ce qui ne change pas). Le protocole n'a **aucun** code de résultat : le client ferme ses fenêtres à
//! l'envoi et le changement revient par AGPMCHARACTER op 1 (champs 20 visage, 21 coiffure, appliqués s'ils
//! sont ≥ 0 par `FUN_140176730`), à tout personnage qui voit le joueur.

use archrust_protocol::packet::{decode_mini, sparse, Value};
use archrust_protocol::schema::{character_op, main, mini, packet_type, Schema};

/// Fonction de PNJ du coiffeur.
pub const FUNCTION: i32 = 29;

pub mod op {
    pub const REQUEST: i8 = 0;
    pub const GRANT: i8 = 1;
    pub const BUY: i8 = 2;
}

#[derive(Debug, Clone, PartialEq)]
pub enum CustomizeCommand {
    /// Ouvrir la fenêtre auprès du coiffeur `npc`.
    Request(i32),
    /// Achat : numéros de visage et de coiffure (-1 : inchangé).
    Buy { npc: i32, face: i32, hair: i32 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CustomizeEvent {
    /// Accord du coiffeur `npc` pour le personnage `cid` (op 1).
    Granted { npc: i32, cid: i32 },
    /// Visage ou coiffure d'un personnage changés (AGPMCHARACTER op 1, champs 20/21 ≥ 0).
    Appearance { cid: i32, face: Option<i32>, hair: Option<i32> },
}

pub fn command_packet(cmd: &CustomizeCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::EVENT_CHAR_CUSTOMIZE;
    let values = match cmd {
        CustomizeCommand::Request(npc) => sparse(s, &[(0, Value::I8(op::REQUEST)), (1, crate::npc::event_base(*npc, FUNCTION)), (2, Value::I32(me))]),
        CustomizeCommand::Buy { npc, face, hair } => sparse(
            s,
            &[(0, Value::I8(op::BUY)), (1, crate::npc::event_base(*npc, FUNCTION)), (2, Value::I32(me)), (3, Value::I8(*face as i8)), (4, Value::I8(*hair as i8))],
        ),
    };
    (s, values)
}

pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<CustomizeEvent> {
    if ptype != packet_type::EVENT_CHAR_CUSTOMIZE {
        return None;
    }
    let int = |i: usize| values.get(i).and_then(Option::as_ref).and_then(Value::as_i64);
    if int(0)? as i8 != op::GRANT {
        return None;
    }
    let npc = values
        .get(1)
        .and_then(Option::as_ref)
        .and_then(Value::as_bytes)
        .and_then(|b| decode_mini(mini::EVENT_BASE, b).ok())
        .and_then(|(_, v)| v.get(1).and_then(Option::as_ref).and_then(Value::as_i64))
        .unwrap_or(0) as i32;
    Some(CustomizeEvent::Granted { npc, cid: int(2).unwrap_or(0) as i32 })
}

/// Apparence portée par un AGPMCHARACTER op 1 (champs 20 et 21, gardés s'ils sont ≥ 0).
pub fn appearance_of(ptype: u8, values: &[Option<Value>]) -> Option<CustomizeEvent> {
    if ptype != packet_type::CHARACTER {
        return None;
    }
    let int = |i: usize| values.get(i).and_then(Option::as_ref).and_then(Value::as_i64);
    if int(0)? != character_op::UPDATE as i64 {
        return None;
    }
    let face = int(20).filter(|v| *v >= 0).map(|v| v as i32);
    let hair = int(21).filter(|v| *v >= 0).map(|v| v as i32);
    if face.is_none() && hair.is_none() {
        return None;
    }
    Some(CustomizeEvent::Appearance { cid: int(1)? as i32, face, hair })
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet};

    #[test]
    fn achat_et_accord() {
        let (s, v) = command_packet(&CustomizeCommand::Buy { npc: 900, face: 3, hair: -1 }, 7);
        let p = decode_packet(s, &build_packet(s, &v).unwrap()).unwrap();
        let int = |i: usize| p.values[i].as_ref().and_then(Value::as_i64);
        assert_eq!((int(0), int(2), int(3), int(4)), (Some(2), Some(7), Some(3), Some(-1)));
        let (_, base) = decode_mini(mini::EVENT_BASE, p.values[1].as_ref().and_then(Value::as_bytes).unwrap()).unwrap();
        assert_eq!(base[1].as_ref().and_then(Value::as_i64), Some(900));
        assert_eq!(base[3].as_ref().and_then(Value::as_i64), Some(29));

        let grant = sparse(s, &[(0, Value::I8(op::GRANT)), (1, crate::npc::event_base(900, FUNCTION)), (2, Value::I32(7))]);
        let p = decode_packet(s, &build_packet(s, &grant).unwrap()).unwrap();
        assert_eq!(translate(packet_type::EVENT_CHAR_CUSTOMIZE, &p.values), Some(CustomizeEvent::Granted { npc: 900, cid: 7 }));
    }

    #[test]
    fn apparence_du_paquet_personnage() {
        let s = main::CHARACTER;
        let v = sparse(s, &[(0, Value::I8(1)), (1, Value::I32(42)), (20, Value::I8(-1)), (21, Value::I8(12))]);
        let p = decode_packet(s, &build_packet(s, &v).unwrap()).unwrap();
        assert_eq!(appearance_of(packet_type::CHARACTER, &p.values), Some(CustomizeEvent::Appearance { cid: 42, face: None, hair: Some(12) }));
        let rien = sparse(s, &[(0, Value::I8(1)), (1, Value::I32(42)), (20, Value::I8(-1))]);
        assert_eq!(appearance_of(packet_type::CHARACTER, &rien), None);
    }
}
