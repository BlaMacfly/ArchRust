//! Consommables, résurrection par objet et oubli des compétences. Relevé dans Archonia (répartiteurs
//! d'AgpmItem `FUN_1401b0e10`, d'AgpmCharacter `FUN_140173f10`, d'AgpmSkill `FUN_1402f5b70` ; AgcmUIItem,
//! AgcmUISkill2) :
//!
//! ```text
//! AGPMITEM op 3 (2 IID)                         client → serveur : utiliser l'objet (orbe, Gem of origin…) ; voir `items`
//! AGPMITEM op 4 (3 TID, 4 CID, 18 recharge)     objet utilisé (rappel 0x1c → FUN_14053f540) ; voir `world::ItemUsed`
//! AGPMITEM op 5 (3 TID, 4 CID)                  échec (rappel 0x1d → FUN_14053f740 : orbe → « Regeneration has failed »)
//! AGPMITEM op 18 (4 CID, 18 restant en ms)      recharge de l'orbe : début = maintenant + restant − 900 000 (rappel 0x46)
//! AGPMITEM op 28 (3 TID, 4 CID, 5 entier, 18 restant en ms) recharge partagée par modèle (FUN_1401c5b90)
//! AGPMITEM op 7 (4 CID)                         parchemin de retour refusé (rappel 0x21, sans abonné d'interface relevé)
//! AGPMITEM op 16 (2 IID, 4 CID)                 objet utilisé (rappel 0x1e, objet cherché par IID ; rien d'affiché relevé)
//! AGPMITEM op 27 code 3 (1 code, 4 CID)         `ReverseOrbError` (décodé par `morph`)
//! AGPMCHARACTER op 0 champ 19 (CHAR[32])        chaîne « skill init » du personnage (`+0xd0`, FUN_140175bb0)
//! AGPMCHARACTER op 23 (1 CID, 19 chaîne)        chaîne « skill init » mise à jour (strncpy dans `+0xd0`)
//! AGPMSKILL op 1 (1 identifiant)                compétence retirée (FUN_1402f8d20) ; voir `skills::SkillEvent::Removed`
//! 0x3B op 8 (2 CID, sans événement)             client → serveur : oubli des compétences chez le maître (FUN_1402b88e0)
//! 0x3B op 9 (5 résultat)                        réponse (rappel 0xd → FUN_1405dcc40) : 0 réussi, 1 échec, 2 pas de place
//! AGPMSKILL op 13 (1 identifiant, 2 base)       client → serveur : Return Orb, remise à zéro d'une compétence (`Skill_Rollback`,
//!                                               FUN_14069dd20 → FUN_14033dd30) ; l'IID de l'orbe n'est pas envoyé
//! AGPMITEMCONVERT op 12 (1 CID, 2 IID, 3 IID)   client → serveur : Release of Agreement (usage 28) glissé sur un équipement
//!                                               (FUN_1406d2300 → FUN_140318610) : champ 2 l'équipement, champ 3 l'objet
//! AGPMCHARACTER op 0 / 1 champ 27 (CHAR[16])    signature (Signature of Glory, `/nick`) : `+0xf1` du personnage
//!                                               (FUN_140175bb0), dessinée sur sa plaque de nom (FUN_140826450)
//! ```
//!
//! L'op 1 d'AGPMCHARACTER ne touche pas au champ 19 (FUN_140176730 ne le reçoit pas) : seuls l'ajout (op 0) et
//! l'op 23 le posent.

use archrust_protocol::packet::{build_mini, sparse, Value};
use archrust_protocol::schema::{main, mini, packet_type, Schema};

/// Opérations d'AgpmItem propres à ce module.
pub mod item_op {
    pub const USE_ITEM_FAILED_BY_TID: i64 = 5;
    pub const USE_RETURN_SCROLL_FAILED: i64 = 7;
    pub const USE_ITEM_SUCCESS: i64 = 16;
    pub const UPDATE_REUSE_TIME_FOR_REVERSE_ORB: i64 = 18;
    pub const UPDATE_COOLDOWN: i64 = 28;
}

/// Opérations d'AgpmCharacter propres à ce module.
pub mod character_op {
    pub const ADD: i64 = 0;
    pub const UPDATE: i64 = 1;
    pub const UPDATE_SKILLINIT_STRING: i64 = 23;
}

/// Opérations d'AgpmEventSkillMaster (0x3B) propres à ce module.
pub mod master_op {
    pub const SKILL_INITIALIZE: i8 = 8;
    pub const SKILL_INITIALIZE_RESULT: i64 = 9;
}

/// Résultats de l'op 9 (champ 5, `FUN_1405dcc40`).
pub mod init_result {
    pub const SUCCESS: i32 = 0;
    pub const FAILED: i32 = 1;
    /// Pas assez de place pour le remboursement : `sysstr` 748.
    pub const NO_ROOM: i32 = 2;
}

/// Champ « skill init » d'AGPMCHARACTER.
pub const SKILL_INIT_FIELD: usize = 19;
/// Champ « signature » d'AGPMCHARACTER (CHAR[16]).
pub const SIGNATURE_FIELD: usize = 27;

/// Opération d'AgpmSkill de la Return Orb (`REQUEST_SKILL_ROLLBACK`).
pub const SKILL_ROLLBACK: i8 = 13;
/// Opération d'AgpmItemConvert du Release of Agreement (`SOCKET_INITIALIZE`).
pub const SOCKET_INIT: i8 = 12;

/// Recharge de l'orbe de résurrection, constante du client (op 18 : début = maintenant + restant − 900 000).
pub const REVERSE_ORB_REUSE_MS: u64 = 900_000;

/// Annonces du serveur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsumableEvent {
    /// Échec d'utilisation d'un objet (AgpmItem op 5).
    UseFailed { tid: i32, cid: i32 },
    /// Recharge de l'orbe : temps restant en ms (AgpmItem op 18).
    ReverseOrbReuse { cid: i32, remain_ms: u64 },
    /// Chaîne « skill init » d'un personnage (AgpmCharacter op 0 ou 23, champ 19) ; vide : oubli encore permis.
    SkillInit { cid: i32, value: String },
    /// Résultat de l'oubli des compétences chez le maître (0x3B op 9, champ 5).
    SkillInitResult { result: i32 },
    /// Signature d'un personnage (AgpmCharacter op 0 ou 1, champ 27) ; vide : aucune.
    Signature { cid: i32, text: String },
    /// Recharge d'un modèle d'objet (AgpmItem op 28) : temps restant en ms.
    Cooldown { tid: i32, cid: i32, remain_ms: u64 },
    /// Parchemin de retour refusé (op 7) ou objet utilisé (op 16, IID) : sans effet visible relevé.
    ReturnScrollFailed { cid: i32 },
    UseSucceeded { iid: i32, cid: i32 },
}

/// Demandes du client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumableCommand {
    /// Oublier toutes les compétences chez le maître (0x3B op 8, champ 2 = CID, champ 1 nul).
    SkillInitialize,
    /// Return Orb : remettre à zéro la compétence d'identifiant `id` (AGPMSKILL op 13).
    SkillRollback { id: i32 },
    /// Release of Agreement `item` glissé sur l'équipement `equip` (AGPMITEMCONVERT op 12).
    SocketInit { equip: i32, item: i32 },
}

fn int(values: &[Option<Value>], i: usize) -> Option<i64> {
    values.get(i).and_then(Option::as_ref).and_then(Value::as_i64)
}

/// Chaîne « skill init » (champ 19) d'un paquet AgpmCharacter op 0 ou 23 qui la porte.
pub fn skill_init_of(ptype: u8, values: &[Option<Value>]) -> Option<ConsumableEvent> {
    if ptype != packet_type::CHARACTER || !matches!(int(values, 0)?, character_op::ADD | character_op::UPDATE_SKILLINIT_STRING) {
        return None;
    }
    let value = values.get(SKILL_INIT_FIELD).and_then(Option::as_ref).and_then(Value::as_text)?;
    Some(ConsumableEvent::SkillInit { cid: int(values, 1)? as i32, value })
}

/// Signature (champ 27) d'un paquet AgpmCharacter op 0 ou 1 qui la porte. Archonia ne recopie qu'une chaîne non
/// vide (`strncpy` sous condition, FUN_140175bb0) : une chaîne vide est rendue telle quelle, au moteur d'en juger.
pub fn signature_of(ptype: u8, values: &[Option<Value>]) -> Option<ConsumableEvent> {
    if ptype != packet_type::CHARACTER || !matches!(int(values, 0)?, character_op::ADD | character_op::UPDATE) {
        return None;
    }
    let text = values.get(SIGNATURE_FIELD).and_then(Option::as_ref).and_then(Value::as_text)?;
    Some(ConsumableEvent::Signature { cid: int(values, 1)? as i32, text })
}

/// Paquet AgpmItem (ops 5, 18), AgpmCharacter (op 23) ou 0x3B (op 9) traduit, sinon `None`. L'op 0
/// d'AgpmCharacter est relevée à part ([`skill_init_of`], `world::extra_events`).
pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<ConsumableEvent> {
    match ptype {
        packet_type::ITEM => {
            let cid = int(values, 4).unwrap_or(0) as i32;
            match int(values, 0)? {
                item_op::USE_ITEM_FAILED_BY_TID => Some(ConsumableEvent::UseFailed { tid: int(values, 3)? as i32, cid }),
                o @ (item_op::UPDATE_REUSE_TIME_FOR_REVERSE_ORB | item_op::UPDATE_COOLDOWN) => {
                    let remain_ms = match values.get(18).and_then(Option::as_ref) {
                        Some(Value::U64(v)) => *v,
                        Some(v) => v.as_i64().unwrap_or(0).max(0) as u64,
                        None => 0,
                    };
                    if o == item_op::UPDATE_COOLDOWN {
                        return Some(ConsumableEvent::Cooldown { tid: int(values, 3)? as i32, cid, remain_ms });
                    }
                    Some(ConsumableEvent::ReverseOrbReuse { cid, remain_ms })
                }
                item_op::USE_RETURN_SCROLL_FAILED => Some(ConsumableEvent::ReturnScrollFailed { cid }),
                item_op::USE_ITEM_SUCCESS => Some(ConsumableEvent::UseSucceeded { iid: int(values, 2).unwrap_or(0) as i32, cid }),
                _ => None,
            }
        }
        packet_type::CHARACTER if int(values, 0)? == character_op::UPDATE_SKILLINIT_STRING => skill_init_of(ptype, values),
        packet_type::EVENT_SKILL_MASTER if int(values, 0)? == master_op::SKILL_INITIALIZE_RESULT => {
            Some(ConsumableEvent::SkillInitResult { result: int(values, 5).unwrap_or(-1) as i32 })
        }
        _ => None,
    }
}

/// Paquet d'une demande pour le personnage `me`.
pub fn command_packet(cmd: &ConsumableCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    match cmd {
        ConsumableCommand::SkillInitialize => {
            let s = main::EVENT_SKILL_MASTER;
            (s, sparse(s, &[(0, Value::I8(master_op::SKILL_INITIALIZE)), (2, Value::I32(me))]))
        }
        ConsumableCommand::SkillRollback { id } => {
            // Mini « base » du propriétaire : type du personnage (`+0x8`, 3) et CID (FUN_14033dd30).
            let b = mini::SKILL_BASE;
            let base = Value::Packet(build_mini(b, &[Some(Value::I8(3)), Some(Value::I32(me))]).unwrap_or_default());
            let s = main::SKILL;
            (s, sparse(s, &[(0, Value::I8(SKILL_ROLLBACK)), (1, Value::I32(*id)), (2, base)]))
        }
        ConsumableCommand::SocketInit { equip, item } => {
            let s = main::ITEM_CONVERT;
            (s, sparse(s, &[(0, Value::I8(SOCKET_INIT)), (1, Value::I32(me)), (2, Value::I32(*equip)), (3, Value::I32(*item))]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet};

    fn roundtrip(schema: &'static Schema, pairs: &[(usize, Value)]) -> Vec<Option<Value>> {
        let bytes = build_packet(schema, &sparse(schema, pairs)).unwrap();
        decode_packet(schema, &bytes).unwrap().values
    }

    #[test]
    fn recharge_par_modele() {
        let v = roundtrip(main::ITEM, &[(0, Value::U8(28)), (3, Value::I32(190)), (4, Value::I32(42)), (5, Value::I32(1)), (18, Value::U64(4500))]);
        assert_eq!(translate(packet_type::ITEM, &v), Some(ConsumableEvent::Cooldown { tid: 190, cid: 42, remain_ms: 4500 }));
        let v = roundtrip(main::ITEM, &[(0, Value::U8(16)), (2, Value::I32(9)), (4, Value::I32(42))]);
        assert_eq!(translate(packet_type::ITEM, &v), Some(ConsumableEvent::UseSucceeded { iid: 9, cid: 42 }));
    }

    #[test]
    fn oubli_chez_le_maitre() {
        let (s, v) = command_packet(&ConsumableCommand::SkillInitialize, 42);
        let bytes = build_packet(s, &v).unwrap();
        let back = decode_packet(main::EVENT_SKILL_MASTER, &bytes).unwrap().values;
        assert_eq!((int(&back, 0), int(&back, 2)), (Some(8), Some(42)));
        assert!(back[1].is_none(), "sans événement");

        for (code, expected) in [(0, init_result::SUCCESS), (1, init_result::FAILED), (2, init_result::NO_ROOM)] {
            let v = roundtrip(main::EVENT_SKILL_MASTER, &[(0, Value::I8(9)), (5, Value::I8(code))]);
            assert_eq!(translate(0x3b, &v), Some(ConsumableEvent::SkillInitResult { result: expected }));
        }
        // Les autres opérations du maître restent à `skills`.
        let v = roundtrip(main::EVENT_SKILL_MASTER, &[(0, Value::I8(6)), (5, Value::I8(0))]);
        assert_eq!(translate(0x3b, &v), None);
    }

    #[test]
    fn objets_orbe_et_echec() {
        let v = roundtrip(main::ITEM, &[(0, Value::U8(5)), (3, Value::I32(842)), (4, Value::I32(42))]);
        assert_eq!(translate(0x04, &v), Some(ConsumableEvent::UseFailed { tid: 842, cid: 42 }));
        let v = roundtrip(main::ITEM, &[(0, Value::U8(18)), (4, Value::I32(42)), (18, Value::U64(60_000))]);
        assert_eq!(translate(0x04, &v), Some(ConsumableEvent::ReverseOrbReuse { cid: 42, remain_ms: 60_000 }));
        // Op 4 (objet utilisé) : laissée à `world`.
        let v = roundtrip(main::ITEM, &[(0, Value::U8(4)), (3, Value::I32(842)), (4, Value::I32(42))]);
        assert_eq!(translate(0x04, &v), None);
    }

    #[test]
    fn return_orb_et_release_of_agreement() {
        let (s, v) = command_packet(&ConsumableCommand::SkillRollback { id: 77 }, 42);
        let back = decode_packet(s, &build_packet(s, &v).unwrap()).unwrap().values;
        assert_eq!((s.id, int(&back, 0), int(&back, 1)), (0x05, Some(13), Some(77)));
        let base = back[2].as_ref().and_then(Value::as_bytes).unwrap();
        let (_, b) = archrust_protocol::packet::decode_mini(mini::SKILL_BASE, base).unwrap();
        assert_eq!((int(&b, 0), int(&b, 1)), (Some(3), Some(42)));
        assert!(back[4].is_none(), "ni TID ni IID de l'orbe");

        let (s, v) = command_packet(&ConsumableCommand::SocketInit { equip: 900, item: 901 }, 42);
        let back = decode_packet(s, &build_packet(s, &v).unwrap()).unwrap().values;
        assert_eq!((s.id, int(&back, 0), int(&back, 1), int(&back, 2), int(&back, 3)), (0x16, Some(12), Some(42), Some(900), Some(901)));
    }

    #[test]
    fn signature_du_personnage() {
        let v = roundtrip(main::CHARACTER, &[(0, Value::I8(1)), (1, Value::I32(42)), (27, Value::Str(b"Gloire".to_vec()))]);
        assert_eq!(signature_of(0x02, &v), Some(ConsumableEvent::Signature { cid: 42, text: "Gloire".into() }));
        let v = roundtrip(main::CHARACTER, &[(0, Value::I8(0)), (1, Value::I32(42)), (27, Value::Str(b"0123456789abcdef".to_vec()))]);
        assert_eq!(signature_of(0x02, &v), Some(ConsumableEvent::Signature { cid: 42, text: "0123456789abcdef".into() }));
        // Sans champ 27, ou autre opération : rien.
        let v = roundtrip(main::CHARACTER, &[(0, Value::I8(1)), (1, Value::I32(42))]);
        assert_eq!(signature_of(0x02, &v), None);
        let v = roundtrip(main::CHARACTER, &[(0, Value::I8(23)), (1, Value::I32(42)), (27, Value::Str(b"x".to_vec()))]);
        assert_eq!(signature_of(0x02, &v), None);
    }

    #[test]
    fn chaine_skill_init() {
        let v = roundtrip(main::CHARACTER, &[(0, Value::I8(23)), (1, Value::I32(42)), (19, Value::Str(b"1".to_vec()))]);
        assert_eq!(translate(0x02, &v), Some(ConsumableEvent::SkillInit { cid: 42, value: "1".into() }));
        let v = roundtrip(main::CHARACTER, &[(0, Value::I8(0)), (1, Value::I32(42)), (19, Value::Str(Vec::new()))]);
        assert_eq!(skill_init_of(0x02, &v), Some(ConsumableEvent::SkillInit { cid: 42, value: String::new() }));
        // L'ajout (op 0) passe par `skill_init_of`, pas par `translate` ; l'op 1 ne porte pas la chaîne.
        assert_eq!(translate(0x02, &v), None);
        let v = roundtrip(main::CHARACTER, &[(0, Value::I8(1)), (1, Value::I32(42)), (19, Value::Str(b"1".to_vec()))]);
        assert_eq!(skill_init_of(0x02, &v), None);
    }
}
