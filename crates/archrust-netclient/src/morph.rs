//! Métamorphose (potions et compétences de transformation) et évolution du Dragon Scion (Slayer, Orbiter,
//! Summoner of Evolution). Relevé dans Archonia (répartiteur d'AgpmCharacter `FUN_140173f10`, AgpmItem
//! `FUN_1401b0e10`) :
//!
//! ```text
//! AGPMCHARACTER op 0, champ 18     bit 0 métamorphosé (+0x600), bit 2 évolué (+0x604), lus à l'ajout seulement
//! AGPMCHARACTER op 11 (1 CID, 2 TID de la forme)    métamorphose : TID, visage, coiffure d'origine sauvés une fois
//! AGPMCHARACTER op 12 (1 CID)                        fin de la métamorphose : retour au TID d'origine
//! AGPMCHARACTER op 13 (1 CID du joueur)              client → serveur : clic sur l'icône d'annulation
//! AGPMCHARACTER op 26 (1 CID, 2 TID évolué)          évolution (refusée si déjà évolué ou même TID)
//! AGPMCHARACTER op 27 (1 CID, 2 TID à rétablir)      fin d'évolution (refusée si non évolué ou même TID)
//! AGPMSKILL op 7, action de type 4                   client → serveur : annuler l'évolution (voir `skills`)
//! AGPMITEM op 27 (1 code, 4 CID)                     refus : 1 siège, 2 créature de siège, 3 fenêtre, 4 potion
//! AGPMITEM op 19 (4 CID, 22 temps restant en ms)     recharge commune des potions de métamorphose (10 min)
//! AGPMITEM op 20 (4 CID)                             recharge remise à zéro
//! ```
//!
//! L'op 13 reçue du serveur (rappel 0x35 d'Archonia, abonné inconnu) n'est pas traduite.
//!
//! Les ops 11 et 12 valent pour **n'importe quel personnage**, monstres compris (`FUN_1401698d0` cherche le CID
//! quel que soit son type ; la Mutation lancée sur un monstre en change le modèle) : aucun filtre ici ni dans
//! `app_morph.rs`. Sous métamorphose ou évolution, le client contrôle aussi les déplacements d'objets de l'op 2
//! d'AGPMITEM (`FUN_1401b4d00` → `FUN_1401bca10`, drapeau DISARMAMENT `auction::item_flag::DISARMAMENT` du
//! champ 19) : voir `app_morph.rs`.

use archrust_protocol::packet::Value;
use archrust_protocol::schema::{main, packet_type, Schema};

/// Opérations d'AgpmCharacter.
pub mod op {
    pub const TRANSFORM: i64 = 11;
    pub const RESTORE_TRANSFORM: i64 = 12;
    pub const CANCEL_TRANSFORM: i8 = 13;
    pub const EVOLUTION: i64 = 26;
    pub const RESTORE_EVOLUTION: i64 = 27;
}

/// Opérations d'AgpmItem.
pub mod item_op {
    /// Même traitement que l'op 19 dans le répartiteur d'Archonia.
    pub const UPDATE_REUSE_TIME: i64 = 8;
    pub const UPDATE_REUSE_TIME_FOR_TRANSFORM: i64 = 19;
    pub const INIT_TIME_FOR_TRANSFORM: i64 = 20;
    pub const USE_ITEM_RESULT: i64 = 27;
}

/// Codes de l'op 27 d'AgpmItem (champ 1) et clés d'uimessage affichées (`FUN_14053f7c0`) ; le code 3
/// (`ReverseOrbError` : `UI_Resurrection` rouverte et boîte « You cannot use the resurrection item. ») est traité
/// par le moteur avec l'orbe de résurrection (`app_consumables.rs`).
pub mod use_result {
    pub const FAILED_BY_SIEGEWAR: i32 = 1;
    pub const CANNOT_TRANSFORM_CREATURE: i32 = 2;
    pub const REVERSE_ORB_ERROR: i32 = 3;
    pub const CANNOT_TRANSFORM_POTION: i32 = 4;

    pub fn message(code: i32) -> Option<&'static str> {
        match code {
            FAILED_BY_SIEGEWAR => Some("UseItemFailedBySiegewar"),
            CANNOT_TRANSFORM_CREATURE => Some("UseItemCannotTransformCreature"),
            CANNOT_TRANSFORM_POTION => Some("UseItemCannotTransformPotion"),
            _ => None,
        }
    }
}

/// Recharge commune des potions de métamorphose, constante du client (op 19 : début = maintenant + restant − 600 000).
pub const TRANSFORM_REUSE_MS: u64 = 600_000;

/// Bits de l'octet « transformation » (champ 18) propres à ce module (le bit 1, monté, est dans `ride`).
pub mod state {
    pub const TRANSFORMED: i64 = 0x01;
    pub const EVOLVED: i64 = 0x04;
}

/// Annonces du serveur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MorphEvent {
    /// Octet d'état d'un personnage ajouté (op 0, champ 18).
    State { cid: i32, transformed: bool, evolved: bool },
    /// Métamorphose dans le modèle `tid` (op 11).
    Transform { cid: i32, tid: i32 },
    /// Fin de la métamorphose (op 12).
    Restore { cid: i32 },
    /// Évolution dans le modèle `tid` (op 26).
    Evolve { cid: i32, tid: i32 },
    /// Fin de l'évolution, retour au modèle `tid` (op 27).
    RestoreEvolution { cid: i32, tid: i32 },
    /// Refus d'utilisation d'un objet (AgpmItem op 27) : code et CID visé.
    UseItemResult { cid: i32, code: i32 },
    /// Recharge des potions de métamorphose : temps restant en ms (AgpmItem op 19).
    ReuseTime { cid: i32, remain_ms: u64 },
    /// Recharge remise à zéro (AgpmItem op 20).
    ReuseReset { cid: i32 },
}

/// Demandes du client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MorphCommand {
    /// Annuler la métamorphose (op 13, `FUN_1404b71e0`).
    CancelTransform,
}

fn int(values: &[Option<Value>], i: usize) -> Option<i64> {
    values.get(i).and_then(Option::as_ref).and_then(Value::as_i64)
}

/// Paquet AgpmCharacter (ops 11, 12, 26, 27) ou AgpmItem (ops 8, 19, 20, 27) traduit, sinon `None`.
pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<MorphEvent> {
    match ptype {
        packet_type::CHARACTER => {
            let cid = int(values, 1)? as i32;
            let tid = int(values, 2).unwrap_or(0) as i32;
            match int(values, 0)? {
                op::TRANSFORM => Some(MorphEvent::Transform { cid, tid }),
                op::RESTORE_TRANSFORM => Some(MorphEvent::Restore { cid }),
                op::EVOLUTION => Some(MorphEvent::Evolve { cid, tid }),
                op::RESTORE_EVOLUTION => Some(MorphEvent::RestoreEvolution { cid, tid }),
                _ => None,
            }
        }
        packet_type::ITEM => {
            let cid = int(values, 4).unwrap_or(0) as i32;
            match int(values, 0)? {
                item_op::USE_ITEM_RESULT => Some(MorphEvent::UseItemResult { cid, code: int(values, 1)? as i32 }),
                item_op::UPDATE_REUSE_TIME_FOR_TRANSFORM | item_op::UPDATE_REUSE_TIME => {
                    let remain_ms = match values.get(22).and_then(Option::as_ref)? {
                        Value::U64(v) => *v,
                        v => v.as_i64()?.max(0) as u64,
                    };
                    Some(MorphEvent::ReuseTime { cid, remain_ms })
                }
                item_op::INIT_TIME_FOR_TRANSFORM => Some(MorphEvent::ReuseReset { cid }),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Octet d'état d'un paquet AgpmCharacter op 0 (`FUN_140175bb0`) ; l'op 1 ne touche ni l'un ni l'autre bit.
pub fn state_of(ptype: u8, values: &[Option<Value>]) -> Option<MorphEvent> {
    if ptype != packet_type::CHARACTER || int(values, 0)? != archrust_protocol::schema::character_op::ADD as i64 {
        return None;
    }
    let (cid, t) = (int(values, 1)? as i32, int(values, 18)?);
    Some(MorphEvent::State { cid, transformed: t & state::TRANSFORMED != 0, evolved: t & state::EVOLVED != 0 })
}

/// Paquet d'une demande pour le personnage `me`.
pub fn command_packet(cmd: &MorphCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    match cmd {
        MorphCommand::CancelTransform => {
            let s = main::CHARACTER;
            (s, archrust_protocol::packet::sparse(s, &[(0, Value::I8(op::CANCEL_TRANSFORM)), (1, Value::I32(me))]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet, sparse};

    fn decoded(s: &'static Schema, values: &[(usize, Value)]) -> Vec<Option<Value>> {
        let bytes = build_packet(s, &sparse(s, values)).unwrap();
        decode_packet(s, &bytes).unwrap().values
    }

    #[test]
    fn operations_du_personnage() {
        let c = main::CHARACTER;
        let v = decoded(c, &[(0, Value::I8(11)), (1, Value::I32(42)), (2, Value::I32(272))]);
        assert_eq!(translate(0x02, &v), Some(MorphEvent::Transform { cid: 42, tid: 272 }));
        let v = decoded(c, &[(0, Value::I8(12)), (1, Value::I32(42))]);
        assert_eq!(translate(0x02, &v), Some(MorphEvent::Restore { cid: 42 }));
        let v = decoded(c, &[(0, Value::I8(26)), (1, Value::I32(42)), (2, Value::I32(1722))]);
        assert_eq!(translate(0x02, &v), Some(MorphEvent::Evolve { cid: 42, tid: 1722 }));
        let v = decoded(c, &[(0, Value::I8(27)), (1, Value::I32(42)), (2, Value::I32(1724))]);
        assert_eq!(translate(0x02, &v), Some(MorphEvent::RestoreEvolution { cid: 42, tid: 1724 }));
        // Monstre (CID quelconque) : mêmes ops 11 / 12 que pour un joueur (Mutation, 101 Rooster).
        let v = decoded(c, &[(0, Value::I8(11)), (1, Value::I32(900_123)), (2, Value::I32(101))]);
        assert_eq!(translate(0x02, &v), Some(MorphEvent::Transform { cid: 900_123, tid: 101 }));
        let v = decoded(c, &[(0, Value::I8(12)), (1, Value::I32(900_123))]);
        assert_eq!(translate(0x02, &v), Some(MorphEvent::Restore { cid: 900_123 }));
        // L'op 13 reçue n'est pas traduite ; l'op 1 non plus.
        let v = decoded(c, &[(0, Value::I8(13)), (1, Value::I32(42))]);
        assert_eq!(translate(0x02, &v), None);
        // Octet d'état : à l'ajout seulement.
        let v = decoded(c, &[(0, Value::I8(0)), (1, Value::I32(42)), (2, Value::I32(1724)), (18, Value::I8(0x07))]);
        assert_eq!(state_of(0x02, &v), Some(MorphEvent::State { cid: 42, transformed: true, evolved: true }));
        let v = decoded(c, &[(0, Value::I8(1)), (1, Value::I32(42)), (18, Value::I8(0x01))]);
        assert_eq!(state_of(0x02, &v), None);
    }

    #[test]
    fn deplacement_et_drapeau_desarme() {
        // AGPMITEM op 2 : objet 77 porté (3) rangé dans le sac (2), drapeaux d'état (champ 19) avec DISARMAMENT.
        let i = main::ITEM;
        let v = decoded(i, &[(0, Value::U8(2)), (2, Value::I32(77)), (4, Value::I32(42)), (19, Value::I32(0x1000))]);
        assert_eq!(translate(0x04, &v), None, "laissé à `world` (contrôle dans app_morph.rs)");
        match crate::auction::item_flags_of(&v) {
            Some(crate::auction::AuctionEvent::ItemFlags { iid: 77, flags }) => assert_ne!(flags & crate::auction::item_flag::DISARMAMENT, 0),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn annulation_envoyee() {
        let (s, v) = command_packet(&MorphCommand::CancelTransform, 42);
        let bytes = build_packet(s, &v).unwrap();
        assert_eq!(bytes[5], 0x02);
        let back = decode_packet(s, &bytes).unwrap().values;
        assert_eq!((int(&back, 0), int(&back, 1), int(&back, 2)), (Some(13), Some(42), None));
    }

    #[test]
    fn recharge_et_refus_des_objets() {
        let i = main::ITEM;
        let v = decoded(i, &[(0, Value::U8(19)), (4, Value::I32(42)), (22, Value::U64(540_000))]);
        assert_eq!(translate(0x04, &v), Some(MorphEvent::ReuseTime { cid: 42, remain_ms: 540_000 }));
        let v = decoded(i, &[(0, Value::U8(20)), (4, Value::I32(42))]);
        assert_eq!(translate(0x04, &v), Some(MorphEvent::ReuseReset { cid: 42 }));
        let v = decoded(i, &[(0, Value::U8(27)), (1, Value::I8(4)), (4, Value::I32(42))]);
        assert_eq!(translate(0x04, &v), Some(MorphEvent::UseItemResult { cid: 42, code: 4 }));
        assert_eq!(use_result::message(4), Some("UseItemCannotTransformPotion"));
        assert_eq!(use_result::message(3), None);
        // Op 8 sans temps restant : rien.
        let v = decoded(i, &[(0, Value::U8(8)), (4, Value::I32(42))]);
        assert_eq!(translate(0x04, &v), None);
        // Utilisation (op 4) : laissée à `world`.
        let v = decoded(i, &[(0, Value::U8(4)), (3, Value::I32(406)), (4, Value::I32(42))]);
        assert_eq!(translate(0x04, &v), None);
    }
}
