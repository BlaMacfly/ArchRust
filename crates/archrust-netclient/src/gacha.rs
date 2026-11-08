//! Gacha (« Fortune Gate ») des socles des villes : type 0x57 (AgpmEventGacha).
//!
//! Dialogue relevé dans Archonia (`FUN_14027e9f0`, `FUN_14027ecb0`, réception `FUN_14027e640`) et validé avec
//! ArchCore (scénario `sim gacha`) :
//!
//! ```text
//! op 0 (1 EVENT_BASE : objet du monde, fonction 38 ; 2 CID ; 3 valeur de l'événement)   clic sur le socle
//!   → op 1 : 3 code (0 à 4 : fenêtre UI_Gacha ; 5, 6 : message seul), 4 TID et 5 nombre de l'objet demandé,
//!            6 or, 7 charisme, 8 TID de la bande (u32 à la suite)
//! op 2 (1 EVENT_BASE, 2 CID, 3 à 7 nuls)                                                bouton « Start »
//!   → op 3 : 3 TID tiré, 4 code (0 réussite)
//! codes : 1 objets, 2 or, 3 charisme manquants, 4 inventaire plein, 5 erreur inconnue, 6 niveau hors table
//! ```

use archrust_protocol::packet::{build_mini, decode_mini, sparse, Value};
use archrust_protocol::schema::{main, mini, Schema};

/// Fonction d'événement des socles (`Function=38` des `obj*.ini`).
pub const FUNCTION_GACHA: i32 = 38;
/// Type d'objet « objet du monde » d'une désignation d'événement.
const BASE_TYPE_OBJECT: i8 = 1;

/// Opérations d'AGPMEVENTGACHA.
pub mod op {
    pub const REQUEST: i8 = 0;
    pub const OPEN: i8 = 1;
    pub const ROLL: i8 = 2;
    pub const RESULT: i8 = 3;
}

/// Codes (op 1 champ 3, op 3 champ 4) ; messages `sysstr` de `FUN_1405ffe00`.
pub mod code {
    pub const SUCCESS: i32 = 0;
    pub const NOT_ENOUGH_ITEM: i32 = 1;
    pub const NOT_ENOUGH_GOLD: i32 = 2;
    pub const NOT_ENOUGH_CHARISMA: i32 = 3;
    pub const INVENTORY_FULL: i32 = 4;
    pub const UNKNOWN: i32 = 5;
    pub const LEVEL_LIMIT: i32 = 6;

    /// Message `sysstr` d'un code, ou `None` (réussite, code inconnu).
    pub fn sysstr(code: i32) -> Option<u32> {
        match code {
            NOT_ENOUGH_ITEM => Some(729),
            NOT_ENOUGH_GOLD => Some(730),
            NOT_ENOUGH_CHARISMA => Some(739),
            INVENTORY_FULL => Some(731),
            UNKNOWN => Some(732),
            LEVEL_LIMIT => Some(733),
            _ => None,
        }
    }

    /// Codes de l'op 1 qui ouvrent la fenêtre (`FUN_1405ff970` : 0 à 4).
    pub fn opens(code: i32) -> bool {
        (0..=4).contains(&code)
    }
}

/// Demandes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GachaCommand {
    /// Clic sur le socle porté par l'objet du monde `object`.
    Request(u32),
    /// Bouton « Start » de la fenêtre ouverte sur `object`.
    Roll(u32),
}

/// Ce que demande un socle (op 1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GachaCost {
    pub item_tid: i32,
    pub item_count: i32,
    pub gold: i32,
    pub charisma: i32,
}

/// Réponses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GachaEvent {
    /// Op 1 : fenêtre (codes 0 à 4) ou message seul ; objet du socle, coût, bande.
    Open { object: Option<u32>, code: i32, cost: GachaCost, band: Vec<i32> },
    /// Op 3 : objet tiré, ou refus.
    Result { code: i32, tid: i32 },
}

fn int(values: &[Option<Value>], i: usize) -> Option<i64> {
    values.get(i).and_then(Option::as_ref).and_then(Value::as_i64)
}

/// Mini EVENT_BASE du socle porté par l'objet `object`.
pub fn event_base(object: u32) -> Value {
    let s = mini::EVENT_BASE;
    let values = sparse(s, &[(0, Value::I8(BASE_TYPE_OBJECT)), (1, Value::I32(object as i32)), (3, Value::I32(FUNCTION_GACHA))]);
    Value::Packet(build_mini(s, &values).unwrap_or_default())
}

/// Paquet d'une demande pour le personnage `me`.
pub fn command_packet(cmd: &GachaCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::EVENT_GACHA;
    let (o, object) = match cmd {
        GachaCommand::Request(object) => (op::REQUEST, *object),
        GachaCommand::Roll(object) => (op::ROLL, *object),
    };
    (s, sparse(s, &[(0, Value::I8(o)), (1, event_base(object)), (2, Value::I32(me)), (3, Value::I32(0))]))
}

/// TID de la bande (u32 à la suite).
fn band_of(bytes: &[u8]) -> Vec<i32> {
    bytes.chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])).filter(|t| *t > 0).collect()
}

/// Traduit un paquet 0x57, sinon `None`.
pub fn translate(packet_type: u8, values: &[Option<Value>]) -> Option<GachaEvent> {
    if packet_type != archrust_protocol::schema::packet_type::EVENT_GACHA {
        return None;
    }
    let n = |i: usize| int(values, i).unwrap_or(0) as i32;
    match int(values, 0)? as i8 {
        op::OPEN => {
            let object = values.get(1).and_then(Option::as_ref).and_then(Value::as_bytes).and_then(object_of).map(|o| o as u32);
            let band = values.get(8).and_then(Option::as_ref).and_then(Value::as_bytes).map(band_of).unwrap_or_default();
            Some(GachaEvent::Open { object, code: n(3), cost: GachaCost { item_tid: n(4), item_count: n(5), gold: n(6), charisma: n(7) }, band })
        }
        op::RESULT => Some(GachaEvent::Result { code: n(4), tid: n(3) }),
        _ => None,
    }
}

/// Objet du monde désigné par un EVENT_BASE.
pub fn object_of(base: &[u8]) -> Option<i64> {
    decode_mini(mini::EVENT_BASE, base).ok().and_then(|(_, v)| int(&v, 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet};

    #[test]
    fn demandes_et_reponses() {
        let (s, v) = command_packet(&GachaCommand::Request(132_646_683), 7);
        assert_eq!(s.id, 0x57);
        let bytes = build_packet(s, &v).unwrap();
        let back = decode_packet(main::EVENT_GACHA, &bytes).unwrap();
        assert_eq!((back.op(), int(&back.values, 2)), (Some(0), Some(7)));
        let base = back.get(1).and_then(Value::as_bytes).unwrap();
        assert_eq!(object_of(base), Some(132_646_683));
        let (_, v) = command_packet(&GachaCommand::Roll(5), 7);
        assert_eq!(int(&v, 0), Some(2));

        // Op 1 du serveur : code 1, 1 × 7436, bande de deux TID.
        let mut band = Vec::new();
        band.extend_from_slice(&537u32.to_le_bytes());
        band.extend_from_slice(&4001u32.to_le_bytes());
        let open = sparse(main::EVENT_GACHA, &[(0, Value::I8(1)), (1, event_base(9)), (2, Value::I32(7)), (3, Value::I32(1)), (4, Value::I32(7436)), (5, Value::I32(1)), (8, Value::Block(band))]);
        let bytes = build_packet(main::EVENT_GACHA, &open).unwrap();
        let back = decode_packet(main::EVENT_GACHA, &bytes).unwrap();
        let ev = translate(0x57, &back.values).unwrap();
        assert_eq!(ev, GachaEvent::Open { object: Some(9), code: 1, cost: GachaCost { item_tid: 7436, item_count: 1, gold: 0, charisma: 0 }, band: vec![537, 4001] });
        let done = sparse(main::EVENT_GACHA, &[(0, Value::I8(3)), (3, Value::I32(537)), (4, Value::I32(0))]);
        assert_eq!(translate(0x57, &done), Some(GachaEvent::Result { code: 0, tid: 537 }));
        assert_eq!(translate(0x3f, &done), None);
        assert_eq!((code::sysstr(4), code::sysstr(0), code::opens(4), code::opens(5)), (Some(731), None, true, false));
    }
}
