//! Usure des objets et réparation chez les forgerons (AgpmEventItemRepair 0x34, fonction de PNJ 17).
//!
//! Relevé dans Archonia (constructeur `FUN_1402b3b50`, réception `FUN_1402b4020`) :
//!
//! ```text
//! 0x34 op 3 (1 EVENT_BASE, 2 CID)                         demande d'ouverture (C → S)
//!   ← op 4 (1 EVENT_BASE, 2 CID)                          accord : `UI_Repair` et l'inventaire
//! 0x34 op 0 (1 EVENT_BASE, 2 CID, 3 IID ×20, 4 coût, 5 = 0)   réparation de la grille (C → S)
//!   ← op 1 (2 CID, 5 code)                                1 réussi, 2 or, 3 déjà au maximum, 4 échec,
//!                                                         5 objet non réparable
//! AGPMITEM op 0 / op 2, champ 10 : mini CHAR_FACTORS, sous-paquet 10 = FACTOR_ITEM
//!   champ 0 = durabilité actuelle (le maximum vient de la colonne `DUR` du modèle)
//! ```

use archrust_protocol::packet::{decode_mini, decode_packet, sparse, Value, GUARD_HEAD, HEADER_SIZE};
use archrust_protocol::schema::{main, mini, packet_type, Schema};

/// Fonction d'événement des forgerons et éleveurs (`npc.ini`, sans donnée).
pub const FUNCTION: i32 = 17;
/// Cases de la grille de réparation (5 lignes × 4 colonnes, indice = ligne × 4 + colonne).
pub const GRID_CELLS: usize = 20;
/// Sous-paquet « objet » des facteurs (type 10 d'AgpmFactors).
const FACTOR_ITEM_INDEX: usize = 10;

/// Opérations d'AgpmEventItemRepair.
pub mod op {
    pub const REPAIR: i8 = 0;
    pub const RESULT: i8 = 1;
    pub const REQUEST: i8 = 3;
    pub const GRANT: i8 = 4;
}

/// Codes du résultat (op 1), `FUN_140651540`.
pub mod result {
    pub const SUCCESS: i32 = 1;
    pub const NOT_ENOUGH_MONEY: i32 = 2;
    pub const ALREADY_MAX: i32 = 3;
    pub const FAILED: i32 = 4;
    pub const CANNOT_REPAIR: i32 = 5;
}

/// Demandes de réparation.
#[derive(Debug, Clone, PartialEq)]
pub enum RepairCommand {
    /// Ouvrir la réparation auprès du PNJ `npc` (op 3).
    Open(i32),
    /// Réparer les objets de la grille (op 0), coût total calculé par le client.
    Repair { npc: i32, grid: [i32; GRID_CELLS], cost: i64 },
}

/// Réponses du serveur et durabilité des objets.
#[derive(Debug, Clone, PartialEq)]
pub enum RepairEvent {
    /// Accord (op 4) : PNJ et CID du joueur.
    Granted { npc: i32, cid: i32 },
    /// Résultat d'une réparation (op 1).
    Result { cid: i32, code: i32 },
    /// Durabilité actuelle d'un objet (AGPMITEM, facteur objet).
    Durability { iid: i32, current: i32 },
}

fn int(values: &[Option<Value>], i: usize) -> Option<i64> {
    values.get(i).and_then(Option::as_ref).and_then(Value::as_i64)
}

/// Paquet d'une demande pour le personnage `me`.
pub fn command_packet(cmd: &RepairCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::EVENT_ITEM_REPAIR;
    match cmd {
        RepairCommand::Open(npc) => (s, sparse(s, &[(0, Value::I8(op::REQUEST)), (1, crate::npc::event_base(*npc, FUNCTION)), (2, Value::I32(me))])),
        RepairCommand::Repair { npc, grid, cost } => (
            s,
            sparse(
                s,
                &[
                    (0, Value::I8(op::REPAIR)),
                    (1, crate::npc::event_base(*npc, FUNCTION)),
                    (2, Value::I32(me)),
                    (3, Value::Ints(grid.iter().map(|i| *i as i64).collect())),
                    (4, Value::I64(*cost)),
                    (5, Value::I32(0)),
                ],
            ),
        ),
    }
}

/// Fonction et PNJ d'un mini EVENT_BASE.
fn event_of(values: &[Option<Value>]) -> Option<(i32, i32)> {
    let bytes = values.get(1).and_then(Option::as_ref).and_then(Value::as_bytes)?;
    let (_, v) = decode_mini(mini::EVENT_BASE, bytes).ok()?;
    Some((int(&v, 1).unwrap_or(0) as i32, int(&v, 3).unwrap_or(0) as i32))
}

/// Paquet AgpmEventItemRepair traduit, sinon `None`. Comme `FUN_140304370`, un CID absent ou nul et
/// un événement d'une autre fonction sont écartés.
pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<RepairEvent> {
    if ptype != packet_type::EVENT_ITEM_REPAIR {
        return None;
    }
    let cid = int(values, 2).filter(|c| *c != 0)? as i32;
    let event = event_of(values);
    if event.is_some_and(|(_, f)| f != FUNCTION) {
        return None;
    }
    match int(values, 0)? as i8 {
        op::GRANT => Some(RepairEvent::Granted { npc: event?.0, cid }),
        op::RESULT => Some(RepairEvent::Result { cid, code: int(values, 5).unwrap_or(0) as i32 }),
        _ => None,
    }
}

/// Durabilité actuelle d'un paquet AGPMITEM (op 0 ou 2) qui porte ses facteurs (champ 10).
pub fn durability_of(values: &[Option<Value>]) -> Option<RepairEvent> {
    if !matches!(int(values, 0), Some(0 | 2)) {
        return None;
    }
    let iid = int(values, 2)? as i32;
    let factors = values.get(10).and_then(Option::as_ref).and_then(Value::as_bytes)?;
    let (_, f) = decode_mini(mini::CHAR_FACTORS, factors).ok()?;
    let item = f.get(FACTOR_ITEM_INDEX).and_then(Option::as_ref).and_then(Value::as_bytes)?;
    let (_, v) = decode_mini(mini::FACTOR_ITEM, item).ok()?;
    Some(RepairEvent::Durability { iid, current: int(&v, 0)? as i32 })
}

/// Durabilités des paquets AGPMITEM complets du champ 6 de la vue optimisée op 2 (entrée en jeu).
pub fn durabilities_in_block(mut bytes: &[u8]) -> Vec<RepairEvent> {
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
            out.extend(decode_packet(main::ITEM, one).ok().and_then(|p| durability_of(&p.values)));
        }
    }
    out
}

/// Mini CHAR_FACTORS réduit au facteur objet {durabilité, …, maximum d'ini} (envoi du serveur, essais).
pub fn item_factors(current: i32, max: i32) -> Value {
    let fi = mini::FACTOR_ITEM;
    let item = archrust_protocol::packet::build_mini(fi, &sparse(fi, &[(0, Value::I32(current)), (4, Value::I32(max))])).unwrap_or_default();
    let cf = mini::CHAR_FACTORS;
    Value::Packet(archrust_protocol::packet::build_mini(cf, &sparse(cf, &[(FACTOR_ITEM_INDEX, Value::Packet(item))])).unwrap_or_default())
}

/// Pourcentage de durabilité d'Archonia (`FUN_1401be960`) : tronqué, 1 entre 0 et 1, -1 sans
/// durabilité (maximum du modèle ≤ 0), 0 à 0.
pub fn durability_percent(current: i32, max: i32) -> i32 {
    if max <= 0 {
        return -1;
    }
    if current <= 0 {
        return 0;
    }
    let p = current as f32 / max as f32 * 100.0;
    if p > 0.0 && p < 1.0 {
        1
    } else {
        p as i32
    }
}

/// Coût de réparation d'Archonia (`FUN_1402b43a0`) : `tronc(prix / max × (max − actuelle) × 0,5)`,
/// puis la taxe de la région (`taux` %, 0 sur ArchCore). Rien si l'objet est plein.
pub fn repair_cost(current: i32, max: i32, price: i64, tax_percent: i64) -> i64 {
    if current == max || max <= 0 {
        return 0;
    }
    let cost = (price as f64 / max as f64 * (max - current) as f64 * 0.5) as i64;
    cost + (cost as f64 * tax_percent as f64 / 100.0) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet};

    fn roundtrip(s: &'static Schema, v: &[Option<Value>]) -> Vec<Option<Value>> {
        let bytes = build_packet(s, v).unwrap();
        decode_packet(s, &bytes).unwrap().values
    }

    #[test]
    fn demandes_de_reparation() {
        let (s, v) = command_packet(&RepairCommand::Open(5), 42);
        assert_eq!(s.id, 0x34);
        let v = roundtrip(s, &v);
        assert_eq!((int(&v, 0), int(&v, 2)), (Some(3), Some(42)));
        assert_eq!(event_of(&v), Some((5, 17)));

        let mut grid = [0; GRID_CELLS];
        grid[0] = 900;
        grid[7] = 901;
        let (s, v) = command_packet(&RepairCommand::Repair { npc: 5, grid, cost: 5934 }, 42);
        let bytes = build_packet(s, &v).unwrap();
        let v = decode_packet(s, &bytes).unwrap().values;
        assert_eq!((int(&v, 0), int(&v, 4), int(&v, 5)), (Some(0), Some(5934), Some(0)));
        let Some(Value::Ints(ids)) = v[3].as_ref() else { panic!("grille absente") };
        assert_eq!((ids.len(), ids[0], ids[7], ids[19]), (20, 900, 901, 0));
    }

    #[test]
    fn reponses_du_serveur() {
        let s = main::EVENT_ITEM_REPAIR;
        let grant = roundtrip(s, &sparse(s, &[(0, Value::I8(4)), (1, crate::npc::event_base(5, FUNCTION)), (2, Value::I32(42))]));
        assert_eq!(translate(0x34, &grant), Some(RepairEvent::Granted { npc: 5, cid: 42 }));
        let done = roundtrip(s, &sparse(s, &[(0, Value::I8(1)), (2, Value::I32(42)), (5, Value::I32(3))]));
        assert_eq!(translate(0x34, &done), Some(RepairEvent::Result { cid: 42, code: 3 }));
        // CID nul, autre fonction : écartés.
        let nul = roundtrip(s, &sparse(s, &[(0, Value::I8(1)), (2, Value::I32(0)), (5, Value::I32(1))]));
        assert_eq!(translate(0x34, &nul), None);
        let other = roundtrip(s, &sparse(s, &[(0, Value::I8(4)), (1, crate::npc::event_base(5, 22)), (2, Value::I32(42))]));
        assert_eq!(translate(0x34, &other), None);
    }

    #[test]
    fn durabilite_dans_agpmitem() {
        let s = main::ITEM;
        let v = roundtrip(s, &sparse(s, &[(0, Value::U8(2)), (2, Value::I32(77)), (10, item_factors(30, 69))]));
        assert_eq!(durability_of(&v), Some(RepairEvent::Durability { iid: 77, current: 30 }));
        let bare = roundtrip(s, &sparse(s, &[(0, Value::U8(2)), (2, Value::I32(77)), (1, Value::I8(3))]));
        assert_eq!(durability_of(&bare), None);
        // Bloc de l'entrée en jeu : paquets complets bout à bout.
        let mut block = build_packet(s, &sparse(s, &[(0, Value::U8(0)), (2, Value::I32(1)), (10, item_factors(0, 93))])).unwrap();
        block.extend(build_packet(s, &sparse(s, &[(0, Value::U8(0)), (2, Value::I32(2))])).unwrap());
        assert_eq!(durabilities_in_block(&block), vec![RepairEvent::Durability { iid: 1, current: 0 }]);
    }

    #[test]
    fn pourcentage_et_cout_comme_archonia() {
        assert_eq!(durability_percent(30, 69), 43);
        assert_eq!(durability_percent(1, 252_000), 1);
        assert_eq!(durability_percent(0, 69), 0);
        assert_eq!(durability_percent(5, 0), -1);
        assert_eq!(durability_percent(14, 69), 20);
        // Bastard Sword (69, NPC Price 21000) à 30 : 5934 (spécification).
        assert_eq!(repair_cost(30, 69, 21_000, 0), 5934);
        assert_eq!(repair_cost(69, 69, 21_000, 0), 0);
        assert_eq!(repair_cost(30, 69, 21_000, 10), 5934 + 593);
    }
}
