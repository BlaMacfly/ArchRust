//! Rémission des points de meurtrier (AgpmRemission, 0x2A ; pardonneurs, fonction de PNJ 31).
//!
//! Op 0 : demande de la fenêtre au pardonneur (mini EVENT_BASE) ; op 1 : accord (fenêtre `UI_Remission`) ; op 2 :
//! rémission, mini {type, résultat} (type 4 lettres de pardon : résultat 1 réussite, 3 sans point, 7 lettres
//! insuffisantes ; type 1 : délai de 2 heures pas écoulé).

use archrust_protocol::packet::{build_mini, decode_mini, sparse, Value};
use archrust_protocol::schema::{main, mini, packet_type, Schema};

/// Fonction de PNJ du pardonneur.
pub const FUNCTION: i32 = 31;
/// Rémission par lettres de pardon (la seule de la fenêtre), et lettres exigées.
pub const TYPE_LETTERS: i8 = 4;
pub const LETTERS: i64 = 10;
/// « Letter of Pardon » et « Letter of Great Pardon ».
pub const LETTER_TIDS: [i32; 2] = [3311, 1836];

pub mod op {
    pub const REQUEST: i8 = 0;
    pub const GRANT: i8 = 1;
    pub const REMIT: i8 = 2;
}

pub mod result {
    pub const SUCCESS: i32 = 1;
    pub const NO_POINT: i32 = 3;
    pub const NOT_ENOUGH_LETTERS: i32 = 7;
}

#[derive(Debug, Clone, PartialEq)]
pub enum RemissionCommand {
    /// Ouvrir la fenêtre auprès du pardonneur `npc`.
    Request(i32),
    /// Rémission par lettres de pardon.
    Remit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RemissionEvent {
    Granted,
    Result { kind: i8, result: i32 },
}

pub fn command_packet(cmd: &RemissionCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::REMISSION;
    match cmd {
        RemissionCommand::Request(npc) => (s, sparse(s, &[(0, Value::I8(op::REQUEST)), (1, Value::I32(me)), (2, crate::npc::event_base(*npc, FUNCTION))])),
        RemissionCommand::Remit => {
            let m = mini::REMISSION;
            let detail = Value::Packet(build_mini(m, &sparse(m, &[(0, Value::I8(TYPE_LETTERS))])).unwrap_or_default());
            (s, sparse(s, &[(0, Value::I8(op::REMIT)), (1, Value::I32(me)), (2, detail)]))
        }
    }
}

pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<RemissionEvent> {
    if ptype != packet_type::REMISSION {
        return None;
    }
    let int = |i: usize| values.get(i).and_then(Option::as_ref).and_then(Value::as_i64);
    match int(0)? as i8 {
        op::GRANT => Some(RemissionEvent::Granted),
        op::REMIT => {
            let bytes = values.get(2).and_then(Option::as_ref).and_then(Value::as_bytes)?;
            let (_, v) = decode_mini(mini::REMISSION, bytes).ok()?;
            let get = |i: usize| v.get(i).and_then(Option::as_ref).and_then(Value::as_i64).unwrap_or(0);
            Some(RemissionEvent::Result { kind: get(0) as i8, result: get(1) as i32 })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demande_de_remission() {
        let (s, v) = command_packet(&RemissionCommand::Remit, 7);
        assert_eq!(s.id, packet_type::REMISSION);
        let bytes = v[2].as_ref().and_then(Value::as_bytes).unwrap();
        let (_, m) = decode_mini(mini::REMISSION, bytes).unwrap();
        assert_eq!(m[0].as_ref().and_then(Value::as_i64), Some(4));
    }
}
