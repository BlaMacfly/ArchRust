//! Montures (AgpmRide, 0x4D). Monter et descendre passent par l'équipement de l'objet de monture
//! (partie 14, AGPMITEM op 2 statut 3 / 2, voir `items`) : Archonia n'émet jamais ce paquet
//! (`FUN_14027b2f0` sans appelant), il ne fait que le recevoir (`FUN_14027b080`, rappels d'AgcmRide
//! `FUN_14061d870`).
//!
//! ```text
//! 0x4D op 2 (1 CID, 2 IID de l'objet monture)     monté : monture = SubType de l'objet (FUN_14061d400)
//! 0x4D op 4 (1 CID)                               descendu (FUN_14061d530)
//! 0x4D op 5                                       fin de durée : sans effet (FUN_1400f62d0)
//! 0x4D op 6 (1 CID, 2 TID du modèle de monture)   monté sur un modèle donné (FUN_14061d780)
//! AGPMCHARACTER champ 18 / vue optimisée champ 15 : bit 0 métamorphosé, bit 1 monté, bit 2 évolution
//! ```

use archrust_protocol::packet::Value;
use archrust_protocol::schema::packet_type;

/// Opérations reçues.
pub mod op {
    pub const RIDE: i8 = 2;
    pub const DISMOUNT: i8 = 4;
    pub const TIMEOUT: i8 = 5;
    pub const RIDE_TID: i8 = 6;
}

/// Bits de l'octet « transformation » (AGPMCHARACTER champ 18, `FUN_140175bb0`).
pub mod state {
    pub const TRANSFORMED: i64 = 1;
    pub const RIDING: i64 = 2;
    pub const EVOLVED: i64 = 4;
}

/// Premier CID des montures créées par le client (`FUN_140750530`), jamais envoyé au serveur.
pub const LOCAL_CID_BASE: i32 = 1_000_000;

/// Annonces du serveur.
#[derive(Debug, Clone, PartialEq)]
pub enum RideEvent {
    /// Monté sur l'objet `iid` (son `SubType` donne le modèle).
    Mounted { cid: i32, iid: i32 },
    /// Monté sur le modèle de monture `tid` (op 6).
    MountedTid { cid: i32, tid: i32 },
    Dismounted { cid: i32 },
    /// Octet d'état d'un paquet personnage ou d'une vue (bit 1 : monté).
    State { cid: i32, riding: bool },
}

fn int(values: &[Option<Value>], i: usize) -> Option<i64> {
    values.get(i).and_then(Option::as_ref).and_then(Value::as_i64)
}

/// Paquet AgpmRide traduit (l'op 5 ne fait rien dans Archonia), sinon `None`.
pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<RideEvent> {
    if ptype != packet_type::RIDE {
        return None;
    }
    let cid = int(values, 1)? as i32;
    match int(values, 0)? as i8 {
        op::RIDE => Some(RideEvent::Mounted { cid, iid: int(values, 2).unwrap_or(0) as i32 }),
        op::RIDE_TID => Some(RideEvent::MountedTid { cid, tid: int(values, 2).unwrap_or(0) as i32 }),
        op::DISMOUNT => Some(RideEvent::Dismounted { cid }),
        _ => None,
    }
}

/// Monté d'après l'octet d'état (champ 18 du personnage, 15 de la vue).
pub fn is_riding(state: i64) -> bool {
    state & state::RIDING != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet, sparse};
    use archrust_protocol::schema::main;

    fn packet(values: &[(usize, Value)]) -> Vec<Option<Value>> {
        let s = main::RIDE;
        let bytes = build_packet(s, &sparse(s, values)).unwrap();
        assert_eq!(bytes[5], 0x4d);
        decode_packet(s, &bytes).unwrap().values
    }

    #[test]
    fn annonces_du_serveur() {
        let v = packet(&[(0, Value::I8(2)), (1, Value::I32(42)), (2, Value::I32(9001)), (3, Value::I32(0))]);
        assert_eq!(translate(0x4d, &v), Some(RideEvent::Mounted { cid: 42, iid: 9001 }));
        let v = packet(&[(0, Value::I8(6)), (1, Value::I32(42)), (2, Value::I32(1295)), (3, Value::I32(0))]);
        assert_eq!(translate(0x4d, &v), Some(RideEvent::MountedTid { cid: 42, tid: 1295 }));
        let v = packet(&[(0, Value::I8(4)), (1, Value::I32(42)), (2, Value::I32(0)), (3, Value::I32(0))]);
        assert_eq!(translate(0x4d, &v), Some(RideEvent::Dismounted { cid: 42 }));
        let v = packet(&[(0, Value::I8(5)), (1, Value::I32(42)), (2, Value::I32(0)), (3, Value::I32(30))]);
        assert_eq!(translate(0x4d, &v), None, "op 5 : sans effet");
        assert_eq!(translate(0x02, &v), None);
    }

    #[test]
    fn octet_d_etat() {
        assert!(is_riding(2) && is_riding(3));
        assert!(!is_riding(1) && !is_riding(0) && !is_riding(4));
    }
}
