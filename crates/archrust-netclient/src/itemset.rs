//! Commutateur d'équipement (AgpmItemSet, 0x6A ; fenêtre `UI_SwitchEditor`).
//!
//! Relevé dans Archonia (constructeur `FUN_14024fb10`, réception `FUN_14024ff60`, AgcmItemSet `FUN_1406b3e80`) :
//! 100 emplacements par personnage, `slot = (profil × 10 + config) × 2 + jeu` (jeu 0 = A, 1 = B).
//!
//! ```text
//! op 0 (1 slot)                    capturer l'équipement porté dans le jeu (C → S)
//! op 1 (1 slot)                    équiper le jeu (vide : tout déséquiper)
//! op 2 (1 slot)                    vider le jeu
//! op 3 (1 slot, 2 drapeau)         le jeu contient des objets ou non (S → C)
//! op 4 (1 slot, -1 : aucun)        jeu actif (S → C)
//! op 5 (1 slot, 3 objets)          contenu du jeu (S → C), jetons `E,%d,%llu` séparés par `:`
//! op 6 (1 slot, 2 (de << 4) | vers) déplacer une case de la grille
//! op 7 (1 slot, 2 case)            retirer une case
//! ```
//!
//! Le client envoie toujours les quatre champs (objets : chaîne vide). Le `%llu` des jetons est l'identifiant
//! 64 bits de l'objet, champ 26 d'AGPMITEM (`AgpdItem+0x58`), lu ici à part de l'objet ([`item_key_of`]).

use archrust_protocol::packet::{decode_packet, sparse, Value, GUARD_HEAD, HEADER_SIZE};
use archrust_protocol::schema::{main, packet_type, Schema};

/// Emplacements, profils, configurations par profil, cases d'une grille.
pub const SLOTS: i32 = 100;
pub const PROFILES: usize = 5;
pub const CONFIGS: usize = 10;
pub const GRID: usize = 10;

pub mod op {
    pub const SAVE: i8 = 0;
    pub const EQUIP: i8 = 1;
    pub const CLEAR: i8 = 2;
    pub const FLAG: i8 = 3;
    pub const ACTIVE: i8 = 4;
    pub const CONTENT: i8 = 5;
    pub const MOVE: i8 = 6;
    pub const REMOVE: i8 = 7;
}

/// Emplacement d'un jeu (`set` 0 = A, 1 = B).
pub fn slot(profile: usize, config: usize, set: usize) -> i32 {
    ((profile * CONFIGS + config) * 2 + set) as i32
}

/// Demandes du commutateur.
#[derive(Debug, Clone, PartialEq)]
pub enum ItemSetCommand {
    Save(i32),
    Equip(i32),
    Clear(i32),
    Move { slot: i32, from: u8, to: u8 },
    Remove { slot: i32, pos: u8 },
}

/// Réponses du serveur.
#[derive(Debug, Clone, PartialEq)]
pub enum ItemSetEvent {
    Flag { slot: i32, filled: bool },
    Active(i32),
    Content { slot: i32, items: String },
    /// Identifiant 64 bits d'un objet (AGPMITEM champ 26).
    ItemKey { iid: i32, key: u64 },
}

/// Paquet d'une demande (le CID n'est pas transmis : le serveur le connaît).
pub fn command_packet(cmd: &ItemSetCommand) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::ITEM_SET;
    let (op, slot, arg) = match *cmd {
        ItemSetCommand::Save(slot) => (op::SAVE, slot, 0u8),
        ItemSetCommand::Equip(slot) => (op::EQUIP, slot, 0),
        ItemSetCommand::Clear(slot) => (op::CLEAR, slot, 0),
        ItemSetCommand::Move { slot, from, to } => (op::MOVE, slot, (from << 4) | (to & 0x0f)),
        ItemSetCommand::Remove { slot, pos } => (op::REMOVE, slot, pos),
    };
    (s, sparse(s, &[(0, Value::I8(op)), (1, Value::I32(slot)), (2, Value::I8(arg as i8)), (3, Value::Str(Vec::new()))]))
}

/// Jetons `E,<partie>,<id64>` d'une liste d'objets (10 au plus, `FUN_1406ba5a0`).
pub fn parse_items(text: &str) -> Vec<(i32, u64)> {
    text.split(':')
        .filter(|t| t.starts_with('E'))
        .take(GRID)
        .filter_map(|t| {
            let mut it = t.split(',').skip(1);
            let part = it.next()?.trim().parse().ok()?;
            let key = it.next()?.trim().parse().ok()?;
            Some((part, key))
        })
        .collect()
}

/// Liste d'objets d'après ses jetons (envoi du serveur, essais).
pub fn format_items(items: &[(i32, u64)]) -> String {
    items.iter().map(|(p, k)| format!("E,{p},{k}")).collect::<Vec<_>>().join(":")
}

fn int(v: &[Option<Value>], i: usize) -> Option<i64> {
    v.get(i).and_then(Option::as_ref).and_then(Value::as_i64)
}

/// Paquet AgpmItemSet traduit, sinon `None` (rejet d'Archonia : op ≥ 8 ou slot ≥ 100).
pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<ItemSetEvent> {
    if ptype != packet_type::ITEM_SET {
        return None;
    }
    let op = int(values, 0)?;
    let slot = int(values, 1).unwrap_or(0) as i32;
    if !(0..8).contains(&op) || slot >= SLOTS {
        return None;
    }
    let arg = int(values, 2).unwrap_or(0);
    match op as i8 {
        op::FLAG if slot >= 0 => Some(ItemSetEvent::Flag { slot, filled: arg != 0 }),
        op::ACTIVE => Some(ItemSetEvent::Active(slot.max(-1))),
        op::CONTENT if slot >= 0 => {
            let items = values.get(3).and_then(Option::as_ref).and_then(Value::as_text).unwrap_or_default();
            Some(ItemSetEvent::Content { slot, items })
        }
        _ => None,
    }
}

/// Identifiant 64 bits d'un paquet AGPMITEM (op 0 ou 2) qui porte son champ 26.
pub fn item_key_of(values: &[Option<Value>]) -> Option<ItemSetEvent> {
    if !matches!(int(values, 0), Some(0 | 2)) {
        return None;
    }
    let iid = int(values, 2)? as i32;
    let key = match values.get(26).and_then(Option::as_ref)? {
        Value::U64(k) => *k,
        other => other.as_i64()? as u64,
    };
    Some(ItemSetEvent::ItemKey { iid, key })
}

/// Valeurs des paquets AGPMITEM complets mis bout à bout (champ 6 de la vue optimisée op 2).
pub fn item_packets_in_block(mut bytes: &[u8]) -> Vec<Vec<Option<Value>>> {
    let mut out = Vec::new();
    while bytes.len() > HEADER_SIZE && bytes[0] == GUARD_HEAD {
        let Ok(len) = bytes[1..5].try_into().map(u32::from_le_bytes) else { break };
        let len = len as usize;
        if len < HEADER_SIZE || len > bytes.len() {
            break;
        }
        let (one, rest) = bytes.split_at(len);
        bytes = rest;
        if one[5] == main::ITEM.id {
            if let Ok(p) = decode_packet(main::ITEM, one) {
                out.push(p.values);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::build_packet;

    fn roundtrip(s: &'static Schema, v: &[Option<Value>]) -> Vec<Option<Value>> {
        let bytes = build_packet(s, v).unwrap();
        decode_packet(s, &bytes).unwrap().values
    }

    #[test]
    fn demandes() {
        assert_eq!((slot(0, 0, 0), slot(0, 0, 1), slot(1, 2, 1), slot(4, 9, 1)), (0, 1, 25, 99));
        let (s, v) = command_packet(&ItemSetCommand::Move { slot: 25, from: 9, to: 3 });
        assert_eq!(s.id, 0x6a);
        let bytes = build_packet(s, &v).unwrap();
        // Drapeaux 1 octet, les quatre champs présents, chaîne de 4000 octets.
        assert_eq!(bytes[HEADER_SIZE], 0x0f);
        let v = decode_packet(s, &bytes).unwrap().values;
        assert_eq!((int(&v, 0), int(&v, 1), int(&v, 2).map(|a| a as u8)), (Some(6), Some(25), Some(0x93)));
        let v = roundtrip(s, &command_packet(&ItemSetCommand::Save(4)).1);
        assert_eq!((int(&v, 0), int(&v, 1), int(&v, 2)), (Some(0), Some(4), Some(0)));
        let v = roundtrip(s, &command_packet(&ItemSetCommand::Remove { slot: 1, pos: 7 }).1);
        assert_eq!((int(&v, 0), int(&v, 2)), (Some(7), Some(7)));
    }

    #[test]
    fn reponses() {
        let s = main::ITEM_SET;
        let items = format_items(&[(2, 10234), (9, 10240), (12, 10251)]);
        assert_eq!(items, "E,2,10234:E,9,10240:E,12,10251");
        let v = roundtrip(s, &sparse(s, &[(0, Value::I8(5)), (1, Value::I32(3)), (3, Value::str(items.as_bytes()))]));
        assert_eq!(translate(0x6a, &v), Some(ItemSetEvent::Content { slot: 3, items: items.clone() }));
        assert_eq!(parse_items(&items), vec![(2, 10234), (9, 10240), (12, 10251)]);
        assert_eq!(parse_items("X,1,2:E,3,4:bad:E,5"), vec![(3, 4)]);
        let v = roundtrip(s, &sparse(s, &[(0, Value::I8(3)), (1, Value::I32(3)), (2, Value::I8(1))]));
        assert_eq!(translate(0x6a, &v), Some(ItemSetEvent::Flag { slot: 3, filled: true }));
        let v = roundtrip(s, &sparse(s, &[(0, Value::I8(4)), (1, Value::I32(-1))]));
        assert_eq!(translate(0x6a, &v), Some(ItemSetEvent::Active(-1)));
        // Rejets d'Archonia.
        let v = roundtrip(s, &sparse(s, &[(0, Value::I8(4)), (1, Value::I32(100))]));
        assert_eq!(translate(0x6a, &v), None);
        let v = roundtrip(s, &sparse(s, &[(0, Value::I8(8)), (1, Value::I32(1))]));
        assert_eq!(translate(0x6a, &v), None);
    }

    #[test]
    fn identifiant_des_objets() {
        let item = main::ITEM;
        let v = roundtrip(item, &sparse(item, &[(0, Value::U8(0)), (1, Value::I8(2)), (2, Value::I32(9)), (18, Value::U64(1500)), (22, Value::U64(7)), (26, Value::U64(10234))]));
        assert_eq!(item_key_of(&v), Some(ItemSetEvent::ItemKey { iid: 9, key: 10234 }));
        assert_eq!(int(&v, 18), Some(1500));
        let mut block = build_packet(item, &v).unwrap();
        block.extend(build_packet(item, &sparse(item, &[(0, Value::U8(0)), (2, Value::I32(10))])).unwrap());
        let all = item_packets_in_block(&block);
        assert_eq!(all.len(), 2);
        assert_eq!(item_key_of(&all[1]), None);
    }
}
