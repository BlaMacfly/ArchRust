//! Production : récolte sur cadavre, cuisine, alchimie (AgpmProduct 0x17) et PNJ de production
//! (AgpmEventProduct 0x3A, fonction 24 : aventurier, cuisinier, alchimiste).
//!
//! Relevé dans Archonia (constructeurs `FUN_1402a5d10`, `FUN_1402d0180`) :
//!
//! ```text
//! 0x17 op 0 (1 CID, 2 compétence, 3 recette, 4 IID de l'objet recette)   fabriquer (C → S)
//! 0x17 op 1 (1 CID, 7 recettes connues : INT32 bout à bout)              à l'entrée en jeu (S → C)
//! 0x17 op 2 (1 CID, 2 compétence, 3 CID du cadavre)                       récolter (C → S)
//! 0x17 op 3 (1 CID, 2 compétence, 3 TID obtenu, 4 quantité, 5 code, 6 expérience)   résultat (S → C)
//! 0x3A op 0 (1 EVENT_BASE, 2 CID)            → op 1 : `UI_PDSkillBuy`
//! 0x3A op 2 (1 EVENT_BASE, 2 CID, 3 TID)     → op 3 (3 TID, 4 code) : compétence apprise
//! 0x3A op 4 (1 EVENT_BASE, 2 CID, 3 recette) → op 5 (3 recette, 4 code) : recette achetée
//! ```
//!
//! Le mini EVENT_BASE désigne la fonction 24 du PNJ ; sa donnée (mini à un entier) est la catégorie
//! (0 aventurier, 1 cuisinier, 2 alchimiste), lue avec les autres fonctions du PNJ (`npc`).

use archrust_protocol::packet::{decode_mini, sparse, Value};
use archrust_protocol::schema::{main, mini, packet_type, Schema};

/// Fonction d'événement des PNJ de production (`npc.ini` : `ProductCategory=`).
pub const FUNCTION: i32 = 24;

/// Opérations d'AgpmProduct.
pub mod op {
    pub const COMPOSE: i8 = 0;
    pub const COMPOSE_INFO: i8 = 1;
    pub const GATHER: i8 = 2;
    pub const RESULT: i8 = 3;
}

/// Opérations d'AgpmEventProduct.
pub mod event_op {
    pub const REQUEST: i8 = 0;
    pub const GRANT: i8 = 1;
    pub const LEARN_SKILL: i8 = 2;
    pub const LEARN_SKILL_RESULT: i8 = 3;
    pub const BUY_COMPOSE: i8 = 4;
    pub const BUY_COMPOSE_RESULT: i8 = 5;
}

/// Codes du résultat (0x17 op 3), mêmes valeurs que les contrôles du client.
pub mod result {
    pub const SUCCESS: i32 = 0;
    pub const FAIL: i32 = 1;
    pub const NO_TOOL: i32 = 2;
    pub const INVALID_TARGET: i32 = 3;
    pub const TOO_FAR: i32 = 4;
    pub const INVENTORY_FULL: i32 = 5;
    pub const NOT_ENOUGH_MATERIAL: i32 = 6;
    pub const NOTHING_LEFT: i32 = 7;
    /// Contrôles du client seulement (`FUN_1402a7920`) : niveau de compétence, recette inconnue.
    pub const LOW_LEVEL: i32 = 8;
    pub const UNKNOWN_RECIPE: i32 = 9;
}

/// Codes des réponses du PNJ (0x3A op 3 et 5) et de ses contrôles côté client (`FUN_1402d0900`,
/// `FUN_1402d0be0`).
pub mod learn_result {
    pub const SUCCESS: i32 = 0;
    pub const ALREADY: i32 = 2;
    pub const NOT_ENOUGH_MONEY: i32 = 3;
    pub const LOW_LEVEL: i32 = 4;
    pub const NO_SKILL: i32 = 5;
}

/// Demandes de production.
#[derive(Debug, Clone, PartialEq)]
pub enum ProductCommand {
    /// Fabriquer la recette `recipe` (TID de l'objet produit) avec la compétence `skill_id` (identifiant
    /// d'instance), et l'objet recette `recipe_item` (IID, 0 sans).
    Compose { skill_id: i32, recipe: i32, recipe_item: i32 },
    /// Récolter le cadavre `target` avec la compétence `skill_id`.
    Gather { skill_id: i32, target: i32 },
    /// Ouvrir la fenêtre du PNJ de production `npc`.
    Open(i32),
    /// Apprendre la compétence `tid` auprès du PNJ `npc`.
    LearnSkill { npc: i32, tid: i32 },
    /// Acheter la recette `recipe` auprès du PNJ `npc`.
    BuyCompose { npc: i32, recipe: i32 },
}

/// Réponses du serveur.
#[derive(Debug, Clone, PartialEq)]
pub enum ProductEvent {
    /// Recettes connues du personnage (op 1).
    Recipes { cid: i32, recipes: Vec<i32> },
    /// Résultat d'une récolte ou d'une fabrication (op 3).
    Result { cid: i32, skill_id: i32, item_tid: i32, count: i32, code: i32, exp: i32 },
    /// Fenêtre du PNJ accordée (0x3A op 1).
    Granted { npc: i32 },
    /// Compétence apprise ou refusée (0x3A op 3).
    SkillLearned { tid: i32, code: i32 },
    /// Recette achetée ou refusée (0x3A op 5).
    RecipeBought { recipe: i32, code: i32 },
}

/// Paquet d'une demande de production pour le personnage `me`.
pub fn command_packet(cmd: &ProductCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let p = main::PRODUCT;
    let e = main::EVENT_PRODUCT;
    let event = |op: i8, npc: i32, arg: Option<i32>| {
        let mut v = vec![(0, Value::I8(op)), (1, crate::npc::event_base(npc, FUNCTION)), (2, Value::I32(me))];
        if let Some(a) = arg {
            v.push((3, Value::I32(a)));
        }
        (e, sparse(e, &v))
    };
    match cmd {
        ProductCommand::Compose { skill_id, recipe, recipe_item } => (
            p,
            sparse(p, &[(0, Value::I8(op::COMPOSE)), (1, Value::I32(me)), (2, Value::I32(*skill_id)), (3, Value::I32(*recipe)), (4, Value::I32(*recipe_item))]),
        ),
        ProductCommand::Gather { skill_id, target } => {
            (p, sparse(p, &[(0, Value::I8(op::GATHER)), (1, Value::I32(me)), (2, Value::I32(*skill_id)), (3, Value::I32(*target))]))
        }
        ProductCommand::Open(npc) => event(event_op::REQUEST, *npc, None),
        ProductCommand::LearnSkill { npc, tid } => event(event_op::LEARN_SKILL, *npc, Some(*tid)),
        ProductCommand::BuyCompose { npc, recipe } => event(event_op::BUY_COMPOSE, *npc, Some(*recipe)),
    }
}

/// Liste d'INT32 d'un bloc mémoire (petit-boutiste).
pub fn decode_list(block: &[u8]) -> Vec<i32> {
    block.chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

/// Bloc mémoire d'une liste d'INT32 (envoi du serveur, essais).
pub fn encode_list(list: &[i32]) -> Vec<u8> {
    list.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// Paquet AgpmProduct ou AgpmEventProduct traduit, sinon `None`.
pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<ProductEvent> {
    let int = |i: usize| values.get(i).and_then(Option::as_ref).and_then(Value::as_i64);
    match ptype {
        packet_type::PRODUCT => match int(0)? as i8 {
            op::COMPOSE_INFO => {
                let block = values.get(7).and_then(Option::as_ref).and_then(Value::as_bytes).unwrap_or(&[]);
                Some(ProductEvent::Recipes { cid: int(1).unwrap_or(0) as i32, recipes: decode_list(block) })
            }
            op::RESULT => Some(ProductEvent::Result {
                cid: int(1).unwrap_or(0) as i32,
                skill_id: int(2).unwrap_or(0) as i32,
                item_tid: int(3).unwrap_or(0) as i32,
                count: int(4).unwrap_or(0) as i32,
                code: int(5).unwrap_or(result::FAIL as i64) as i32,
                exp: int(6).unwrap_or(0) as i32,
            }),
            _ => None,
        },
        packet_type::EVENT_PRODUCT => {
            // Valeurs par défaut du décodage d'Archonia (FUN_1402d05a0) : a = -1, b = 1.
            let a = int(3).unwrap_or(-1) as i32;
            let b = int(4).unwrap_or(1) as i32;
            match int(0)? as i8 {
                event_op::GRANT => {
                    let npc = values
                        .get(1)
                        .and_then(Option::as_ref)
                        .and_then(Value::as_bytes)
                        .and_then(|bytes| decode_mini(mini::EVENT_BASE, bytes).ok())
                        .and_then(|(_, v)| v.get(1).and_then(Option::as_ref).and_then(Value::as_i64))
                        .unwrap_or(0) as i32;
                    Some(ProductEvent::Granted { npc })
                }
                event_op::LEARN_SKILL_RESULT => Some(ProductEvent::SkillLearned { tid: a, code: b }),
                event_op::BUY_COMPOSE_RESULT => Some(ProductEvent::RecipeBought { recipe: a, code: b }),
                _ => None,
            }
        }
        _ => None,
    }
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

    #[test]
    fn fabriquer_et_recolter() {
        let (s, v) = command_packet(&ProductCommand::Compose { skill_id: 12, recipe: 644, recipe_item: 9001 }, 42);
        assert_eq!(s.id, 0x17);
        let v = roundtrip(s, &v);
        assert_eq!((int(&v, 0), int(&v, 1), int(&v, 2), int(&v, 3), int(&v, 4)), (Some(0), Some(42), Some(12), Some(644), Some(9001)));
        let (s, v) = command_packet(&ProductCommand::Gather { skill_id: 13, target: 777 }, 42);
        let v = roundtrip(s, &v);
        assert_eq!((int(&v, 0), int(&v, 2), int(&v, 3), int(&v, 4)), (Some(2), Some(13), Some(777), None));
    }

    #[test]
    fn pnj_de_production() {
        let (s, v) = command_packet(&ProductCommand::BuyCompose { npc: 5, recipe: 406 }, 42);
        assert_eq!(s.id, 0x3a);
        let v = roundtrip(s, &v);
        assert_eq!((int(&v, 0), int(&v, 2), int(&v, 3)), (Some(4), Some(42), Some(406)));
        let (_, base) = decode_mini(mini::EVENT_BASE, v[1].as_ref().and_then(Value::as_bytes).unwrap()).unwrap();
        assert_eq!((int(&base, 1), int(&base, 3)), (Some(5), Some(24)));
        let (_, v) = command_packet(&ProductCommand::LearnSkill { npc: 5, tid: 203 }, 42);
        assert_eq!((int(&v, 0), int(&v, 3)), (Some(2), Some(203)));

        // Réponses : accord (EVENT_BASE du PNJ), compétence apprise, recette sans code (défaut 1).
        let e = main::EVENT_PRODUCT;
        let grant = roundtrip(e, &sparse(e, &[(0, Value::I8(1)), (1, crate::npc::event_base(5, FUNCTION)), (2, Value::I32(42))]));
        assert_eq!(translate(0x3a, &grant), Some(ProductEvent::Granted { npc: 5 }));
        let learned = roundtrip(e, &sparse(e, &[(0, Value::I8(3)), (2, Value::I32(42)), (3, Value::I32(203)), (4, Value::I32(0))]));
        assert_eq!(translate(0x3a, &learned), Some(ProductEvent::SkillLearned { tid: 203, code: 0 }));
        let bought = roundtrip(e, &sparse(e, &[(0, Value::I8(5)), (3, Value::I32(406))]));
        assert_eq!(translate(0x3a, &bought), Some(ProductEvent::RecipeBought { recipe: 406, code: 1 }));
    }

    #[test]
    fn recettes_et_resultat() {
        let p = main::PRODUCT;
        let info = roundtrip(p, &sparse(p, &[(0, Value::I8(1)), (1, Value::I32(42)), (7, Value::Block(encode_list(&[644, 406, 4990])))]));
        assert_eq!(translate(0x17, &info), Some(ProductEvent::Recipes { cid: 42, recipes: vec![644, 406, 4990] }));
        let empty = roundtrip(p, &sparse(p, &[(0, Value::I8(1)), (1, Value::I32(42))]));
        assert_eq!(translate(0x17, &empty), Some(ProductEvent::Recipes { cid: 42, recipes: vec![] }));
        let res = roundtrip(p, &sparse(p, &[(0, Value::I8(3)), (1, Value::I32(42)), (2, Value::I32(13)), (3, Value::I32(4963)), (4, Value::I32(1)), (5, Value::I32(0)), (6, Value::I32(11))]));
        assert_eq!(translate(0x17, &res), Some(ProductEvent::Result { cid: 42, skill_id: 13, item_tid: 4963, count: 1, code: 0, exp: 11 }));
        // Op d'envoi (C → S) : pas un événement.
        let (_, gather) = command_packet(&ProductCommand::Gather { skill_id: 13, target: 777 }, 42);
        assert_eq!(translate(0x17, &gather), None);
    }
}
