//! Inventaire Cash (statut d'objet 51, fenêtre `UI_CashInven`) et sac du familier (statut 13, `UI_SubInventory`).
//!
//! Dialogue validé avec ArchCore (scénario `sim petbag`) :
//!
//! ```text
//! ← AGPMITEM op 0 statut 51, sans case (le client range lui-même la liste, FUN_14030bd50), champ 23 = mini Cash
//! ← AGPMITEM op 26 (champ 2 IID, 4 CID, 23 mini Cash)      objet « en cours d'utilisation » ou non ; le client
//!                                                            rafraîchit l'inventaire Cash et la disponibilité du sac
//! AGPMITEM op 3 (champ 2 IID)                               bouton « Use » / « Stoppable » d'une ligne (familier
//!                                                            sorti ou rappelé par le serveur)
//! AGPMITEM op 2 statut 13, champ 7 = [sac, ligne, colonne]  déposer dans le sac depuis l'inventaire, l'équipement ou
//!                                                            la banque, ou déplacer dans le sac ; [sac, -1, -1] :
//!                                                            onglet « Bag n » (première case libre de ce sac)
//!   ← op 2 statut 13 avec la case (ou réunion de piles : op 1 + op 2) ; refus : op 2 à la place d'origine
//! AGPMITEM op 2 statut 2 (champ 7) / 100 (champ 8)          sortir du sac (voir `items`, `npc`)
//! AGPMITEM op 30 (2 IID du familier, 4 CID, 20 = {INT16})   bouton « Auto Pick Up ON/OFF » ← op 30 en écho
//! AGPMITEM op 21                                            détruire (statuts 2, 13, 15, 51 ; voir `items`)
//! ```
//!
//! Mini Cash (champ 23, `mini::ITEM_CASH`, drapeaux 1) : 0 INT8 « en cours d'utilisation » (0, 1, 2 en pause),
//! 1 INT64 temps restant en ms, 2 UINT64 date d'expiration, 3 INT32, 4 INT64 (non lu par le client).
//! Écartés (ajouts d'Archonia) : marquage des sacs par Ctrl+clic (ops 37/38) et « Sell All ».

use archrust_protocol::packet::{build_mini, decode_mini, sparse, Value};
use archrust_protocol::schema::{main, mini, packet_type, Schema};

/// Opérations AgpmItem de ce module.
pub mod op {
    pub const UPDATE: u8 = 2;
    /// Mini Cash d'un objet mis à jour (serveur → client).
    pub const UPDATE_USE_TIME: u8 = 26;
    /// Ramassage automatique du sac du familier.
    pub const AUTO_PICK: u8 = 30;
}

/// Statuts AgpmItem.
pub mod status {
    /// Sac du familier.
    pub const SUB_INVENTORY: i8 = 13;
    /// Inventaire Cash.
    pub const CASH_INVENTORY: i8 = 51;
}

/// Valeurs de l'octet « en cours d'utilisation » (`+0x4f4`).
pub mod in_use {
    pub const NO: i8 = 0;
    pub const YES: i8 = 1;
    pub const PAUSED: i8 = 2;
}

/// Mini Cash d'un objet (champ 23) ; champ absent : `None`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CashState {
    pub in_use: Option<i8>,
    pub remaining_ms: Option<i64>,
    pub expire: Option<u64>,
    pub field3: Option<i32>,
}

impl CashState {
    /// Fusion : un champ reçu remplace l'ancien.
    pub fn merge(&mut self, other: &CashState) {
        self.in_use = other.in_use.or(self.in_use);
        self.remaining_ms = other.remaining_ms.or(self.remaining_ms);
        self.expire = other.expire.or(self.expire);
        self.field3 = other.field3.or(self.field3);
    }
}

/// Événements de l'inventaire Cash et du sac du familier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PetBagEvent {
    /// Mini Cash d'un objet (op 0 et 2 portant le champ 23, op 26 : `refresh`).
    Cash { iid: i32, state: CashState, refresh: bool },
    /// Ramassage automatique (op 30).
    AutoPick { iid: i32, on: bool },
}

/// Demandes du moteur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PetBagCommand {
    /// Ranger un objet dans le sac du familier : case, `[sac, -1, -1]` (onglet), ou place choisie par le serveur.
    Store { iid: i32, grid: Option<[i16; 3]> },
    /// Ramassage automatique (`iid` : objet du familier en cours d'utilisation).
    AutoPick { iid: i32, on: bool },
}

fn int(values: &[Option<Value>], i: usize) -> Option<i64> {
    values.get(i).and_then(Option::as_ref).and_then(Value::as_i64)
}

/// Mini Cash (tampon sans sa longueur).
pub fn decode_cash(bytes: &[u8]) -> Option<CashState> {
    let (_, v) = decode_mini(mini::ITEM_CASH, bytes).ok()?;
    let f = |i: usize| v.get(i).and_then(Option::as_ref);
    Some(CashState {
        in_use: f(0).and_then(Value::as_i64).map(|x| x as i8),
        remaining_ms: f(1).and_then(Value::as_i64),
        expire: f(2).and_then(|x| match x {
            Value::U64(u) => Some(*u),
            o => o.as_i64().map(|i| i as u64),
        }),
        field3: f(3).and_then(Value::as_i64).map(|x| x as i32),
    })
}

/// Mini Cash à envoyer (tests, outils).
pub fn cash_mini(state: &CashState) -> Vec<u8> {
    let m = mini::ITEM_CASH;
    let mut v = Vec::new();
    if let Some(x) = state.in_use {
        v.push((0, Value::I8(x)));
    }
    if let Some(x) = state.remaining_ms {
        v.push((1, Value::I64(x)));
    }
    if let Some(x) = state.expire {
        v.push((2, Value::U64(x)));
    }
    if let Some(x) = state.field3 {
        v.push((3, Value::I32(x)));
    }
    build_mini(m, &sparse(m, &v)).unwrap_or_default()
}

fn auto_pick_value(on: bool) -> Value {
    let m = mini::ITEM_AUTO_PICK;
    Value::Packet(build_mini(m, &sparse(m, &[(0, Value::I16(i16::from(on)))])).unwrap_or_default())
}

/// Mini Cash d'un AGPMITEM op 0 ou 2 (champ 23), à part de l'objet lui-même.
pub fn cash_of(values: &[Option<Value>]) -> Option<PetBagEvent> {
    if !matches!(int(values, 0), Some(0 | 2)) {
        return None;
    }
    let bytes = values.get(23).and_then(Option::as_ref).and_then(Value::as_bytes)?;
    Some(PetBagEvent::Cash { iid: int(values, 2)? as i32, state: decode_cash(bytes)?, refresh: false })
}

/// Ops 26 et 30 d'AGPMITEM.
pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<PetBagEvent> {
    if ptype != packet_type::ITEM {
        return None;
    }
    let iid = int(values, 2)? as i32;
    match int(values, 0)? as u8 {
        op::UPDATE_USE_TIME => {
            let state = values.get(23).and_then(Option::as_ref).and_then(Value::as_bytes).and_then(decode_cash).unwrap_or_default();
            Some(PetBagEvent::Cash { iid, state, refresh: true })
        }
        op::AUTO_PICK => {
            let bytes = values.get(20).and_then(Option::as_ref).and_then(Value::as_bytes)?;
            let (_, v) = decode_mini(mini::ITEM_AUTO_PICK, bytes).ok()?;
            Some(PetBagEvent::AutoPick { iid, on: int(&v, 0).unwrap_or(1) != 0 })
        }
        _ => None,
    }
}

fn grid_value(g: [i16; 3]) -> Value {
    let m = mini::ITEM_GRID;
    Value::Packet(build_mini(m, &g.iter().map(|v| Some(Value::I16(*v))).collect::<Vec<_>>()).unwrap_or_default())
}

/// Paquet AGPMITEM d'une demande pour le personnage `me`.
pub fn command_packet(cmd: &PetBagCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::ITEM;
    let values = match cmd {
        PetBagCommand::Store { iid, grid } => {
            let mut v = vec![(0, Value::U8(op::UPDATE)), (1, Value::I8(status::SUB_INVENTORY)), (2, Value::I32(*iid)), (4, Value::I32(me))];
            if let Some(g) = grid {
                v.push((7, grid_value(*g)));
            }
            sparse(s, &v)
        }
        PetBagCommand::AutoPick { iid, on } => sparse(s, &[(0, Value::U8(op::AUTO_PICK)), (2, Value::I32(*iid)), (4, Value::I32(me)), (20, auto_pick_value(*on))]),
    };
    (s, values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mini_cash_aller_retour() {
        let full = CashState { in_use: Some(in_use::PAUSED), remaining_ms: Some(90_000_000), expire: Some(1_798_000_000), field3: Some(3) };
        assert_eq!(decode_cash(&cash_mini(&full)), Some(full));
        // ArchCore n'envoie que l'octet « en cours d'utilisation » : drapeaux 0x01, puis l'INT8.
        let only = CashState { in_use: Some(1), ..Default::default() };
        assert_eq!(cash_mini(&only), vec![0x01, 0x01]);
        assert_eq!(decode_cash(&[0x01, 0x00]), Some(CashState { in_use: Some(0), ..Default::default() }));
        let mut s = full;
        s.merge(&CashState { in_use: Some(1), ..Default::default() });
        assert_eq!((s.in_use, s.remaining_ms), (Some(1), Some(90_000_000)));
    }

    #[test]
    fn paquets_cash_et_sac() {
        // Op 0 statut 51 avec mini Cash ; op 26.
        let cash = Value::Packet(cash_mini(&CashState { in_use: Some(0), ..Default::default() }));
        let add = sparse(main::ITEM, &[(0, Value::U8(0)), (1, Value::I8(51)), (2, Value::I32(42)), (3, Value::I32(7564)), (23, cash.clone())]);
        assert_eq!(cash_of(&add), Some(PetBagEvent::Cash { iid: 42, state: CashState { in_use: Some(0), ..Default::default() }, refresh: false }));
        let upd = sparse(main::ITEM, &[(0, Value::U8(26)), (2, Value::I32(42)), (4, Value::I32(7)), (23, Value::Packet(cash_mini(&CashState { in_use: Some(1), ..Default::default() })))]);
        assert_eq!(cash_of(&upd), None, "op 26 : traduit par translate");
        assert_eq!(translate(packet_type::ITEM, &upd), Some(PetBagEvent::Cash { iid: 42, state: CashState { in_use: Some(1), ..Default::default() }, refresh: true }));
        // Op 30 : aller (client) et écho (serveur).
        let (sch, v) = command_packet(&PetBagCommand::AutoPick { iid: 42, on: false }, 7);
        assert_eq!((sch.id, int(&v, 0), int(&v, 2), int(&v, 4)), (packet_type::ITEM, Some(30), Some(42), Some(7)));
        let Some(Value::Packet(b)) = v[20].as_ref() else { panic!("champ 20 absent") };
        assert_eq!(b, &vec![0x01, 0x00, 0x00]);
        assert_eq!(translate(packet_type::ITEM, &v), Some(PetBagEvent::AutoPick { iid: 42, on: false }));
        // Dépôt : case, onglet, sans case.
        let (_, v) = command_packet(&PetBagCommand::Store { iid: 5, grid: Some([1, -1, -1]) }, 7);
        assert_eq!((int(&v, 0), int(&v, 1), int(&v, 2), int(&v, 4)), (Some(2), Some(13), Some(5), Some(7)));
        let Some(Value::Packet(b)) = v[7].as_ref() else { panic!("case absente") };
        let (_, g) = decode_mini(mini::ITEM_GRID, b).unwrap();
        assert_eq!((int(&g, 0), int(&g, 1), int(&g, 2)), (Some(1), Some(-1), Some(-1)));
        assert!(command_packet(&PetBagCommand::Store { iid: 5, grid: None }, 7).1[7].is_none());
    }
}
