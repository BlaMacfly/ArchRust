//! Échange entre joueurs (AgpmPrivateTrade, 0x07 ; réception d'Archonia `FUN_140661c10`).
//!
//! Paquet principal : opération, CID, CID de l'autre, IID, TID, argent, case d'origine et case visée (mini
//! `ITEM_GRID`), argument. Relevé dans Archonia (constructeurs `FUN_140662c20`…`FUN_1406630c0`) :
//!
//! ```text
//! op 1  demande          C → S : 1 soi, 2 cible           S → C (cible) : op 3, demandeur au champ 2 (Archonia)
//! op 4  accepter         C → S : 1 soi                    S → C : op 6 (1 soi, 2 l'autre) aux deux
//! op 5  annuler/refuser  C → S : 1 soi                    S → C : op 5 (fin), aussi au demandeur refusé
//! op 9  proposer         C → S : 1 soi, 3 IID, 6 case du sac, 7 case de la grille d'échange, 8 genre de grille
//! op 10 reprendre        C → S : 1 soi, 3 IID, 5 statut d'arrivée, 6 case d'échange, 7 case du sac
//! op 16 / 18             C → S : verrouiller / déverrouiller ; S → C : son propre verrou
//! op 17 / 19             S → C : verrou de l'autre posé / levé
//! op 20                  S → C : les deux sont verrouillés (« Trade » actif)
//! op 21 prêt             C → S : 1 soi ; S → C : son propre « prêt » ; op 22 : l'autre est prêt
//! op 23 or               C → S : 1 soi, 5 montant ; S → C : op 23 (le sien), op 24 (celui de l'autre)
//! op 25                  S → C : la cible refuse les échanges (`Option_Trade_Off_User`)
//! op 26 / 27             S → C : pièces Chantra de soi / de l'autre (champ 5)
//! op 0  résultat         S → C : champ 8 = 8 : échange fait
//! ```
//!
//! Les objets proposés passent par AGPMITEM statut 6 (grille d'échange, 1 × 3 × 4) : op 2 pour les siens, op 0
//! (propriétaire = l'autre) pour ceux de l'autre.
//!
//! Grille Ex (`CONTROL_PT_Grid_Ex`, une colonne de 3 cases) : op 9 genre 2 avec un Marvel Scroll of Release (usage
//! 35) de l'inventaire Cash ; le serveur le retire de l'inventaire Cash (AGPMITEM op 1) et le renvoie au statut 14
//! (`TRADE_OPTION_GRID`, op 0). Un objet lié se propose sur la grille ordinaire avec le genre 1. Échange fait :
//! parchemin consommé ; annulé : rendu à l'inventaire Cash.

use crate::items::Grid;
use archrust_protocol::packet::{build_mini, decode_mini, sparse, Value};
use archrust_protocol::schema::{main, mini, packet_type, Schema};

pub mod op {
    pub const RESULT: i8 = 0;
    pub const REQUEST: i8 = 1;
    pub const REQUEST_CONFIRM: i8 = 3;
    pub const CONFIRM: i8 = 4;
    pub const CANCEL: i8 = 5;
    pub const START: i8 = 6;
    pub const ADD: i8 = 9;
    pub const REMOVE: i8 = 10;
    pub const LOCK: i8 = 16;
    pub const TARGET_LOCKED: i8 = 17;
    pub const UNLOCK: i8 = 18;
    pub const TARGET_UNLOCKED: i8 = 19;
    pub const ACTIVE_READY: i8 = 20;
    pub const READY: i8 = 21;
    pub const TARGET_READY: i8 = 22;
    pub const UPDATE_MONEY: i8 = 23;
    pub const TARGET_UPDATE_MONEY: i8 = 24;
    pub const REFUSE: i8 = 25;
    pub const UPDATE_CC: i8 = 26;
    pub const TARGET_UPDATE_CC: i8 = 27;
}

/// Statut AGPMITEM d'un objet de la grille d'échange.
pub const STATUS_TRADE_GRID: i8 = 6;
/// Statut AGPMITEM d'un objet de la grille Ex (`TRADE_OPTION_GRID`, Marvel Scroll of Release).
pub const STATUS_TRADE_OPTION_GRID: i8 = 14;
/// Champ 8 de l'op 9 : grille ordinaire, objet lié sur la grille ordinaire, grille Ex (`FUN_1407bf0f0`).
pub mod grid_kind {
    pub const NORMAL: i32 = 0;
    pub const BOUND: i32 = 1;
    pub const EX: i32 = 2;
}
/// Grille Ex : 1 sac de 3 lignes × 1 colonne (`CONTROL_PT_Grid_Ex`, 54 × 158, cases de 50 + 2).
pub const GRID_EX: (i16, i16, i16) = (1, 3, 1);
/// Grille d'échange : 1 sac de 3 lignes × 4 colonnes (`CONTROL_PT_Grid`, 210 × 158, cases de 50 + 2).
pub const GRID: (i16, i16, i16) = (1, 3, 4);
/// Champ 8 de l'op 0 : échange fait (`FUN_140661c10`, `PTMSG_TradeComplete`).
pub const RESULT_DONE: i32 = 8;

/// Demandes d'échange du moteur.
#[derive(Debug, Clone, PartialEq)]
pub enum TradeCommand {
    /// Proposer un échange au joueur `target` (menu « Trade » du joueur ciblé).
    Request(i32),
    /// « Yes » de `UI_PrivateTradeYesNo`.
    Confirm,
    /// Annulation : demande, refus (« No »), croix de `UI_PrivateTrade`.
    Cancel,
    /// Objet du sac (case `from`) posé sur la case `to` de la grille d'échange ; `kind` : 0 grille, 2 grille Ex.
    Add { iid: i32, from: Grid, to: Grid, kind: i32 },
    /// Objet de la grille d'échange (case `from`) repris dans le sac (statut `status`, case `to`).
    Remove { iid: i32, status: i8, from: Grid, to: Grid },
    Lock,
    Unlock,
    /// « Trade » : prêt à échanger.
    Ready,
    /// Or proposé (`UI_PrivateTrade_Money`).
    Money(i64),
}

/// Paquets reçus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TradeEvent {
    /// Op 0 : résultat (`RESULT_DONE` : échange fait).
    Result { cid: i32, code: i32 },
    /// Op 3 : demande reçue (champs 1 et 2 tels quels, voir `app_trade.rs`).
    RequestConfirm { cid: i32, other: i32 },
    /// Op 5 : échange ou demande annulé.
    Cancelled { cid: i32 },
    /// Op 6 : échange ouvert avec `other`.
    Started { cid: i32, other: i32 },
    /// Ops 16 / 18 : son propre verrou (CID au champ 1 chez Archonia, absent chez ArchCore).
    Locked { cid: i32, locked: bool },
    /// Ops 17 / 19 : verrou de l'autre.
    TargetLocked(bool),
    /// Op 20 : les deux sont verrouillés.
    ActiveReady,
    /// Op 21 : son propre « prêt ».
    Ready,
    /// Op 22 : l'autre est prêt.
    TargetReady,
    /// Ops 23 / 24 : or proposé par soi (`mine`) ou par l'autre.
    Money { mine: bool, amount: i64 },
    /// Ops 26 / 27 : pièces Chantra.
    Cash { mine: bool, amount: i64 },
    /// Op 25 : la cible refuse les échanges.
    Refused { cid: i32, other: i32 },
}

fn grid_value(grid: Grid) -> Value {
    let values: Vec<Option<Value>> = grid.iter().map(|v| Some(Value::I16(*v))).collect();
    Value::Packet(build_mini(mini::ITEM_GRID, &values).unwrap_or_default())
}

/// Case d'un mini `ITEM_GRID`.
pub fn decode_grid(bytes: &[u8]) -> Option<Grid> {
    let (_, v) = decode_mini(mini::ITEM_GRID, bytes).ok()?;
    let at = |i: usize| v.get(i).and_then(Option::as_ref).and_then(Value::as_i64).map(|x| x as i16);
    Some([at(0)?, at(1)?, at(2)?])
}

/// Paquet d'une demande d'échange pour le personnage `me`.
pub fn command_packet(cmd: &TradeCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::PRIVATE_TRADE;
    let simple = |o: i8| sparse(s, &[(0, Value::I8(o)), (1, Value::I32(me))]);
    let values = match cmd {
        TradeCommand::Request(target) => sparse(s, &[(0, Value::I8(op::REQUEST)), (1, Value::I32(me)), (2, Value::I32(*target))]),
        TradeCommand::Confirm => simple(op::CONFIRM),
        TradeCommand::Cancel => simple(op::CANCEL),
        TradeCommand::Add { iid, from, to, kind } => sparse(
            s,
            &[(0, Value::I8(op::ADD)), (1, Value::I32(me)), (3, Value::I32(*iid)), (6, grid_value(*from)), (7, grid_value(*to)), (8, Value::I32(*kind))],
        ),
        TradeCommand::Remove { iid, status, from, to } => sparse(
            s,
            &[(0, Value::I8(op::REMOVE)), (1, Value::I32(me)), (3, Value::I32(*iid)), (5, Value::I64(*status as i64)), (6, grid_value(*from)), (7, grid_value(*to))],
        ),
        TradeCommand::Lock => simple(op::LOCK),
        TradeCommand::Unlock => simple(op::UNLOCK),
        TradeCommand::Ready => simple(op::READY),
        TradeCommand::Money(amount) => sparse(s, &[(0, Value::I8(op::UPDATE_MONEY)), (1, Value::I32(me)), (5, Value::I64(*amount))]),
    };
    (s, values)
}

/// Paquet AgpmPrivateTrade traduit, sinon `None`.
pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<TradeEvent> {
    if ptype != packet_type::PRIVATE_TRADE {
        return None;
    }
    let int = |i: usize| values.get(i).and_then(Option::as_ref).and_then(Value::as_i64);
    let (cid, other) = (int(1).unwrap_or(0) as i32, int(2).unwrap_or(0) as i32);
    let money = int(5).unwrap_or(0);
    Some(match int(0)? as i8 {
        op::RESULT => TradeEvent::Result { cid, code: int(8).unwrap_or(0) as i32 },
        op::REQUEST_CONFIRM => TradeEvent::RequestConfirm { cid, other },
        op::CANCEL => TradeEvent::Cancelled { cid },
        op::START => TradeEvent::Started { cid, other },
        op::LOCK => TradeEvent::Locked { cid, locked: true },
        op::UNLOCK => TradeEvent::Locked { cid, locked: false },
        op::TARGET_LOCKED => TradeEvent::TargetLocked(true),
        op::TARGET_UNLOCKED => TradeEvent::TargetLocked(false),
        op::ACTIVE_READY => TradeEvent::ActiveReady,
        op::READY => TradeEvent::Ready,
        op::TARGET_READY => TradeEvent::TargetReady,
        op::UPDATE_MONEY => TradeEvent::Money { mine: true, amount: money },
        op::TARGET_UPDATE_MONEY => TradeEvent::Money { mine: false, amount: money },
        op::UPDATE_CC => TradeEvent::Cash { mine: true, amount: money },
        op::TARGET_UPDATE_CC => TradeEvent::Cash { mine: false, amount: money },
        op::REFUSE => TradeEvent::Refused { cid, other },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet};

    fn roundtrip(s: &'static Schema, v: &[Option<Value>]) -> Vec<Option<Value>> {
        let bytes = build_packet(s, v).unwrap();
        decode_packet(s, &bytes).unwrap().values
    }

    fn int(v: &[Option<Value>], i: usize) -> Option<i64> {
        v.get(i).and_then(Option::as_ref).and_then(Value::as_i64)
    }

    fn grid(v: &[Option<Value>], i: usize) -> Option<Grid> {
        v.get(i).and_then(Option::as_ref).and_then(Value::as_bytes).and_then(decode_grid)
    }

    #[test]
    fn schema_d_archonia() {
        let s = main::PRIVATE_TRADE;
        assert_eq!((s.id, s.fields.len()), (0x07, 9));
        assert_eq!(main::by_id(0x07).map(|s| s.name), Some("AGPMPRIVATETRADE"));
    }

    #[test]
    fn demandes() {
        let (s, v) = command_packet(&TradeCommand::Request(77), 42);
        let v = roundtrip(s, &v);
        assert_eq!((int(&v, 0), int(&v, 1), int(&v, 2)), (Some(1), Some(42), Some(77)));
        for (cmd, o) in [(TradeCommand::Confirm, 4), (TradeCommand::Cancel, 5), (TradeCommand::Lock, 16), (TradeCommand::Unlock, 18), (TradeCommand::Ready, 21)] {
            let v = roundtrip(s, &command_packet(&cmd, 42).1);
            assert_eq!((int(&v, 0), int(&v, 1), int(&v, 2)), (Some(o), Some(42), None), "{cmd:?}");
        }
        let v = roundtrip(s, &command_packet(&TradeCommand::Money(500), 42).1);
        assert_eq!((int(&v, 0), int(&v, 5)), (Some(23), Some(500)));
    }

    #[test]
    fn objets_proposes_et_repris() {
        let s = main::PRIVATE_TRADE;
        let v = roundtrip(s, &command_packet(&TradeCommand::Add { iid: 9001, from: [1, 2, 3], to: [0, 1, 2], kind: 0 }, 42).1);
        assert_eq!((int(&v, 0), int(&v, 1), int(&v, 3), int(&v, 8)), (Some(9), Some(42), Some(9001), Some(0)));
        assert_eq!((grid(&v, 6), grid(&v, 7)), (Some([1, 2, 3]), Some([0, 1, 2])));
        // Marvel Scroll de l'inventaire Cash sur la grille Ex (genre 2), objet lié sur la grille ordinaire (genre 1).
        let v = roundtrip(s, &command_packet(&TradeCommand::Add { iid: 77, from: [0, 0, 0], to: [0, 0, 0], kind: grid_kind::EX }, 42).1);
        assert_eq!((int(&v, 0), int(&v, 3), int(&v, 8)), (Some(9), Some(77), Some(2)));
        let v = roundtrip(s, &command_packet(&TradeCommand::Add { iid: 78, from: [0, 0, 1], to: [0, 2, 3], kind: grid_kind::BOUND }, 42).1);
        assert_eq!((int(&v, 8), grid(&v, 7)), (Some(1), Some([0, 2, 3])));
        let v = roundtrip(s, &command_packet(&TradeCommand::Remove { iid: 9001, status: 2, from: [0, 1, 2], to: [0, 0, 0] }, 42).1);
        assert_eq!((int(&v, 0), int(&v, 3), int(&v, 5)), (Some(10), Some(9001), Some(2)));
        assert_eq!((grid(&v, 6), grid(&v, 7)), (Some([0, 1, 2]), Some([0, 0, 0])));
    }

    #[test]
    fn reponses_du_serveur() {
        let s = main::PRIVATE_TRADE;
        let p = |pairs: &[(usize, Value)]| roundtrip(s, &sparse(s, pairs));
        // Op 3 telle qu'ArchCore l'envoie : demandeur au champ 1, cible au champ 2.
        assert_eq!(translate(7, &p(&[(0, Value::I8(3)), (1, Value::I32(10)), (2, Value::I32(20))])), Some(TradeEvent::RequestConfirm { cid: 10, other: 20 }));
        assert_eq!(translate(7, &p(&[(0, Value::I8(6)), (1, Value::I32(20)), (2, Value::I32(10))])), Some(TradeEvent::Started { cid: 20, other: 10 }));
        assert_eq!(translate(7, &p(&[(0, Value::I8(5)), (1, Value::I32(20))])), Some(TradeEvent::Cancelled { cid: 20 }));
        assert_eq!(translate(7, &p(&[(0, Value::I8(16))])), Some(TradeEvent::Locked { cid: 0, locked: true }));
        assert_eq!(translate(7, &p(&[(0, Value::I8(19))])), Some(TradeEvent::TargetLocked(false)));
        assert_eq!(translate(7, &p(&[(0, Value::I8(20))])), Some(TradeEvent::ActiveReady));
        assert_eq!(translate(7, &p(&[(0, Value::I8(22))])), Some(TradeEvent::TargetReady));
        assert_eq!(translate(7, &p(&[(0, Value::I8(24)), (5, Value::I64(100))])), Some(TradeEvent::Money { mine: false, amount: 100 }));
        assert_eq!(translate(7, &p(&[(0, Value::I8(25)), (1, Value::I32(20)), (2, Value::I32(10))])), Some(TradeEvent::Refused { cid: 20, other: 10 }));
        assert_eq!(translate(7, &p(&[(0, Value::I8(0)), (1, Value::I32(20)), (8, Value::I32(8))])), Some(TradeEvent::Result { cid: 20, code: RESULT_DONE }));
        // Ops d'envoi seulement, autre type de paquet : rien.
        assert_eq!(translate(7, &p(&[(0, Value::I8(9))])), None);
        assert_eq!(translate(8, &p(&[(0, Value::I8(5))])), None);
    }
}
