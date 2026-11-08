//! Compétences : liste du personnage, apprentissage et amélioration chez les instructeurs, lancement.
//!
//! Dialogue validé avec ArchCore (scénarios `sim skill`, `sim effects`) :
//!
//! ```text
//! vue optimisée op 2, champ 8 : paquets AGPMSKILL op 0 complets mis bout à bout (compétences connues)
//! AGPMSKILL op 0 (1 identifiant, 2 propriétaire, 3 facteurs : « dirt » indice 4 = niveau, 4 TID)
//! AGPMSKILL op 1 (1 identifiant) : compétence retirée (oubli des compétences, voir `consumables`)
//! AGPMSKILL op 2 (1 identifiant, 3 facteurs) : nouveau niveau
//! AGPMCHARACTER op 1, champ 7 : facteurs du joueur, « dirt » indice 5 = points de compétence
//! 0x3B op 4 (1 EVENT_BASE, 2 CID)            → op 5 : fenêtre de l'instructeur
//! 0x3B op 1 (4 TID)                          → AGPMSKILL op 0, puis op 6 (4 TID, 5 résultat : 0 réussi,
//!                                              1 échec ou argent, 2 déjà apprise, 3 points, 4 niveau, 5 classe)
//! 0x3B op 2 (3 identifiant)                  → AGPMSKILL op 2, puis op 3
//! AGPMSKILL op 7 (1 identifiant, 6 action : 0 type, 1 cible en mini SKILL_BASE (3, CID), 3 point visé d'une
//!     compétence au sol)
//!     → op 7 diffusé (2 lanceur ; action : 1 cible, 5 incantation, 6 durée, 7 relance en ms)
//!     → op 8 (action 2 : 1 cible, 2 facteurs du résultat comme pour le combat)
//! AGPMSKILL op 3 (2 base (3, CID), 9 bloc de 220 octets) : part des compétences des caractéristiques secondaires
//!     (voir [`SkillAttach`]) ; Archonia copie le champ 9 de tout paquet AGPMSKILL qui le porte.
//! AGPMSKILL S → C (réception 0x1402f5b70) :
//!     op 6 (2 base, 8 minis SKILL_BUFF) : tous les effets du personnage (remplace sa liste, rappel 12)
//!     op 10 (1 genre d'effet, 2 base du lanceur, 4 CID de la cible) : effet secondaire (1 explosion de fin
//!         d'effet, 2 téléport, 3 / 4 absorption PM / PV, 5 / 6 conversion, 7 libération, 8 pierre de lentille,
//!         9 parchemin de détection, 10 soin) ; rappel 9 : effet commun sur le personnage du champ 2
//!         (`FUN_140568d70`, voir `app_effects.rs`), 7 fait lâcher ce personnage comme cible (`FUN_140698c30`)
//!     op 14 (1 bonus, 2 base) : bonus de niveau de toutes les compétences (attache+0x854, < 31)
//!     op 15 (1 identifiant, 2 base, 4 TID) : recharge de la compétence remise à zéro (FUN_1402ff450)
//!     op 16 (1 identifiant, 2 base, 4 niveau) : bonus de niveau d'une compétence (compétence+0x188)
//!     (op 9 ignorée ; op 17, recharge imposée, ajout d'Archonia hors épisode 8 : non reprise)
//! ```

use crate::world::{ActionFactorPoints, WorldEvent};
use archrust_protocol::packet::{build_mini, decode_mini, decode_packet, sparse, Value};
use archrust_protocol::schema::{main, mini, packet_type, Schema};

/// Opérations d'AGPMSKILL.
pub mod op {
    pub const ADD: i64 = 0;
    /// Compétence retirée (1 identifiant ; `FUN_1402f5b70` cas 1 → `FUN_1402f8d20`), après un oubli par exemple.
    pub const REMOVE: i64 = 1;
    pub const UPDATE: i64 = 2;
    /// Données attachées du personnage (champ 9 seul ; FUN_1402f5b70 n'a pas de cas 3).
    pub const ATTACH: i64 = 3;
    pub const ALL_BUFFED: i64 = 6;
    pub const CAST: i64 = 7;
    pub const CAST_RESULT: i64 = 8;
    pub const ADDITIONAL_EFFECT: i64 = 10;
    pub const MODIFIED_SKILL_LEVEL: i64 = 14;
    pub const INIT_COOLTIME: i64 = 15;
    pub const SPECIFIC_SKILL_LEVEL: i64 = 16;
}

/// Types d'action du mini SKILL_ACTION (champ 0) : 0 lancer, 4 annuler le lancement.
pub mod action_type {
    pub const CAST: i8 = 0;
    pub const CANCEL: i8 = 4;
    /// Refus reçus : 5 manquée, 6 échec, 7 cible invalide, 8 condition (arme), 9 PV, 10 PM, 11 PC, 12 flèches,
    /// 13 carreaux, 14 déjà en cours, 15 pas prête, 16 cible de récolte invalide, 17 plus rien à récolter.
    pub const REFUSED: std::ops::RangeInclusive<i32> = 5..=17;
}

/// Opérations d'AGPMEVENTSKILLMASTER (0x3B).
pub mod master_op {
    pub const LEARN: i8 = 1;
    pub const UPGRADE: i8 = 2;
    pub const UPGRADE_RESULT: i64 = 3;
    pub const OPEN: i8 = 4;
    pub const OPENED: i64 = 5;
    pub const LEARN_RESULT: i64 = 6;
}

/// Résultats d'un apprentissage (0x3B op 6, champ 5).
pub mod learn_result {
    pub const SUCCESS: i32 = 0;
    pub const FAIL: i32 = 1;
    pub const ALREADY_LEARNED: i32 = 2;
    pub const NOT_ENOUGH_POINTS: i32 = 3;
    pub const LOW_LEVEL: i32 = 4;
    pub const WRONG_CLASS: i32 = 5;
    /// Codes du contrôle d'Archonia, que sa fenêtre d'apprentissage n'affiche pas.
    pub const NOT_ENOUGH_HEROIC_POINTS: i32 = 6;
    pub const NOT_ENOUGH_CHARISMA: i32 = 7;
}

/// Résultats d'une amélioration (0x3B op 3, champ 5 ; Archonia FUN_1405dd9c0).
pub mod upgrade_result {
    pub const SUCCESS: i32 = 0;
    pub const NOT_ENOUGH_MONEY: i32 = 1;
    pub const NOT_ENOUGH_POINTS: i32 = 2;
    pub const NOT_ENOUGH_HEROIC_POINTS: i32 = 3;
    pub const NOT_ENOUGH_CHARISMA: i32 = 4;
}

/// Indices du sous-paquet « dirt » des facteurs : niveau d'une compétence, points du personnage.
const DIRT_SKILL_LEVEL: usize = 4;
pub const DIRT_SKILL_POINT: usize = 5;
/// Points héroïques restants du personnage (fenêtres `UI_HeroicSkill`…).
pub const DIRT_HEROIC_POINT: usize = 7;
/// Expérience cumulée d'une compétence de production (SKILL_EXP, lue par Archonia `FUN_1402a8540`).
const DIRT_SKILL_EXP: usize = 6;
/// Sous-paquet « dirt » d'AGPMCHAR_FACTORS.
const FACTORS_DIRT: usize = 11;

/// Fonction « maître des compétences » d'un PNJ.
const SKILL_MASTER_FUNCTION: i32 = 25;

/// Taille du bloc du champ 9 d'AGPMSKILL que copie Archonia (FUN_1402f5850 : 0xDC octets vers
/// `AgpdSkillAttachData + 0x6b0`, sans contrôle de longueur).
pub const ATTACH_BLOCK_SIZE: usize = 0xdc;

/// Part des compétences des caractéristiques secondaires de la fenêtre du personnage, lue dans le champ 9
/// d'AGPMSKILL (i32 petit-boutistes ; les autres octets du bloc ne sont lus par aucune fenêtre). Décalage dans
/// le bloc → champ d'`AgpdSkillAttachData` → ligne :
/// 0x08 → 0x6b8 taux critique, 0x0c → 0x6bc dégâts critiques, 0x2c → 0x6dc et 0x3c → 0x6ec « renvoi de
/// dégâts » (probabilité et quantité d'un coup reçu changé en PV), 0xa4 → 0x754 défense ignorée,
/// 0xa8 → 0x758 résistance élémentaire ignorée, 0xb0 → 0x760 résistance aux critiques, 0xbc → 0x76c résistance
/// à l'étourdissement.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SkillAttach {
    pub critical_rate: i32,
    pub critical_damage: i32,
    pub reflect_rate: i32,
    pub reflect_amount: i32,
    pub ignore_defense: i32,
    pub ignore_resistance: i32,
    pub critical_resistance: i32,
    pub stun_resistance: i32,
}

impl SkillAttach {
    /// Décalages dans le bloc, dans l'ordre des champs.
    pub const OFFSETS: [usize; 8] = [0x08, 0x0c, 0x2c, 0x3c, 0xa4, 0xa8, 0xb0, 0xbc];

    /// Valeurs d'un bloc ; un bloc trop court se lit à 0 au-delà de sa fin (Archonia lirait au-delà).
    pub fn decode(block: &[u8]) -> Self {
        let at = |o: usize| block.get(o..o + 4).map_or(0, |b| i32::from_le_bytes(b.try_into().unwrap()));
        let [a, b, c, d, e, f, g, h] = Self::OFFSETS.map(at);
        SkillAttach { critical_rate: a, critical_damage: b, reflect_rate: c, reflect_amount: d, ignore_defense: e, ignore_resistance: f, critical_resistance: g, stun_resistance: h }
    }

    /// Bloc de 220 octets nuls hormis les 8 valeurs (ce que doit envoyer le serveur).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![0u8; ATTACH_BLOCK_SIZE];
        let v = [self.critical_rate, self.critical_damage, self.reflect_rate, self.reflect_amount, self.ignore_defense, self.ignore_resistance, self.critical_resistance, self.stun_resistance];
        for (o, x) in Self::OFFSETS.iter().zip(v) {
            out[*o..*o + 4].copy_from_slice(&x.to_le_bytes());
        }
        out
    }
}

/// Champ 9 d'un paquet AGPMSKILL dont la base (champ 2) désigne un personnage (type 3), quelle que soit
/// l'opération (FUN_1402f5850 le copie avant de traiter l'opération).
pub fn attach_of(values: &[Option<Value>]) -> Option<SkillEvent> {
    let block = bytes(values, 9)?;
    let (_, b) = decode_mini(mini::SKILL_BASE, bytes(values, 2)?).ok()?;
    if int(&b, 0)? != 3 {
        return None;
    }
    Some(SkillEvent::Attach { cid: int(&b, 1)? as i32, data: SkillAttach::decode(block) })
}

/// Demandes de compétences.
#[derive(Debug, Clone, PartialEq)]
pub enum SkillCommand {
    /// Lancer une compétence (identifiant) sur une cible (CID ; le joueur pour lui-même).
    /// `forced` : attaque forcée (Ctrl, champ 4 du mini SKILL_ACTION), nécessaire contre un joueur qui n'est pas
    /// un ennemi déclaré.
    Cast { id: i32, target: i32, forced: bool },
    /// Lancer une compétence au sol (`range` 7, 701, 702 de SKILL_SPEC) : comme `Cast`, avec le point visé au
    /// champ 3 du mini SKILL_ACTION (`FUN_14069a0c0` → `FUN_14033c440`) ; après un clic, la cible est le joueur.
    CastGround { id: i32, target: i32, pos: [f32; 3], forced: bool },
    /// Annuler une compétence en cours (action de type 4, cible = soi) : clic sur l'icône d'annulation de
    /// l'évolution (`FUN_1404b6ee0` → `FUN_14033c440`), avec l'identifiant de la compétence possédée.
    CancelCast { id: i32 },
    /// Ouvrir la fenêtre d'un instructeur.
    MasterOpen(i32),
    /// Apprendre une compétence (TID) auprès d'un instructeur.
    Learn { npc: i32, tid: i32 },
    /// Améliorer une compétence (identifiant) auprès d'un instructeur.
    Upgrade { npc: i32, id: i32 },
}

fn base(cid: i32) -> Value {
    let s = mini::SKILL_BASE;
    Value::Packet(build_mini(s, &[Some(Value::I8(3)), Some(Value::I32(cid))]).unwrap_or_default())
}

/// Paquet d'une demande de compétence pour le personnage `me`.
pub fn command_packet(cmd: &SkillCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let master = |op: i8, npc: i32, extra: &[(usize, Value)]| {
        let mut v = vec![(0, Value::I8(op)), (1, crate::npc::event_base(npc, SKILL_MASTER_FUNCTION)), (2, Value::I32(me))];
        v.extend_from_slice(extra);
        (main::EVENT_SKILL_MASTER, sparse(main::EVENT_SKILL_MASTER, &v))
    };
    match cmd {
        SkillCommand::Cast { id, target, forced } => {
            let a = mini::SKILL_ACTION;
            let action = Value::Packet(build_mini(a, &sparse(a, &[(0, Value::I8(0)), (1, base(*target)), (4, Value::I8(*forced as i8))])).unwrap_or_default());
            (main::SKILL, sparse(main::SKILL, &[(0, Value::I8(op::CAST as i8)), (1, Value::I32(*id)), (2, base(me)), (6, action)]))
        }
        SkillCommand::CastGround { id, target, pos, forced } => {
            let a = mini::SKILL_ACTION;
            let action = Value::Packet(build_mini(a, &sparse(a, &[(0, Value::I8(action_type::CAST)), (1, base(*target)), (3, Value::Vec3(*pos)), (4, Value::I8(*forced as i8))])).unwrap_or_default());
            (main::SKILL, sparse(main::SKILL, &[(0, Value::I8(op::CAST as i8)), (1, Value::I32(*id)), (2, base(me)), (6, action)]))
        }
        SkillCommand::CancelCast { id } => {
            let a = mini::SKILL_ACTION;
            let action = Value::Packet(build_mini(a, &sparse(a, &[(0, Value::I8(action_type::CANCEL)), (1, base(me))])).unwrap_or_default());
            (main::SKILL, sparse(main::SKILL, &[(0, Value::I8(op::CAST as i8)), (1, Value::I32(*id)), (2, base(me)), (6, action)]))
        }
        SkillCommand::MasterOpen(npc) => master(master_op::OPEN, *npc, &[]),
        SkillCommand::Learn { npc, tid } => master(master_op::LEARN, *npc, &[(4, Value::I32(*tid))]),
        SkillCommand::Upgrade { npc, id } => master(master_op::UPGRADE, *npc, &[(3, Value::I32(*id))]),
    }
}

/// Réponses liées aux compétences.
#[derive(Debug, Clone, PartialEq)]
pub enum SkillEvent {
    /// Compétence connue (entrée en jeu ou apprise) : identifiant, TID, niveau, expérience de production
    /// cumulée (« dirt » indice 6) si le serveur l'envoie.
    Known { owner: i32, id: i32, tid: i32, level: i32, exp: Option<i32> },
    /// Compétence retirée (AGPMSKILL op 1).
    Removed { id: i32 },
    /// Niveau d'une compétence changé (et son expérience de production, si elle est jointe).
    Level { id: i32, level: i32, exp: Option<i32> },
    /// Expérience de production seule changée (AGPMSKILL op 2 sans niveau).
    Exp { id: i32, exp: i32 },
    /// Lancement commencé (op 7) : lanceur, compétence, cible, durées en ms.
    Cast { caster: i32, id: i32, tid: i32, target: i32, cast_ms: u32, recast_ms: u32 },
    /// Lancement refusé (op 7, type d'action 5 à 17, seul le lanceur le reçoit) : voir [`action_type`].
    CastRefused { caster: i32, id: i32, kind: i32 },
    /// Fenêtre d'un instructeur ouverte (0x3B op 5).
    MasterOpened { npc: i32 },
    /// Résultat d'un apprentissage (0x3B op 6).
    Learned { tid: i32, result: i32 },
    /// Résultat d'une amélioration (0x3B op 3) : 0 faite, sinon `upgrade_result`.
    Upgraded { id: i32, result: i32 },
    /// Part des compétences des caractéristiques secondaires d'un personnage (AGPMSKILL champ 9).
    Attach { cid: i32, data: SkillAttach },
    /// Tous les effets actifs d'un personnage (op 6) : (TID de l'effet, durée en ms, TID du modèle du lanceur).
    Buffs { owner: i32, buffs: Vec<(i32, u64, i32)> },
    /// Effet secondaire (op 10) : genre, lanceur, cible.
    AdditionalEffect { kind: i32, caster: i32, target: i32 },
    /// Bonus de niveau de toutes les compétences du personnage (op 14).
    LevelBonus { owner: i32, bonus: i32 },
    /// Recharge remise à zéro (op 15) : identifiant et TID de la compétence.
    CooldownReset { owner: i32, id: i32, tid: i32 },
    /// Bonus de niveau d'une compétence (op 16).
    SkillBonus { owner: i32, id: i32, level: i32 },
}

fn int(values: &[Option<Value>], i: usize) -> Option<i64> {
    values.get(i).and_then(Option::as_ref).and_then(Value::as_i64)
}

fn bytes(values: &[Option<Value>], i: usize) -> Option<&[u8]> {
    values.get(i).and_then(Option::as_ref).and_then(Value::as_bytes)
}

/// Indice `index` du sous-paquet « dirt » d'un AGPMCHAR_FACTORS.
pub fn factors_dirt(factors: &[u8], index: usize) -> Option<i32> {
    let (_, cf) = decode_mini(mini::CHAR_FACTORS, factors).ok()?;
    let (_, dirt) = decode_mini(mini::FACTOR_DIRT, bytes(&cf, FACTORS_DIRT)?).ok()?;
    int(&dirt, index).map(|v| v as i32)
}

/// CID d'un mini SKILL_BASE.
fn base_cid(b: Option<&[u8]>) -> Option<i32> {
    let (_, v) = decode_mini(mini::SKILL_BASE, b?).ok()?;
    int(&v, 1).map(|v| v as i32)
}

/// Paquet AGPMSKILL (op 0, 2, 3, 7, 8) ou 0x3B traduit, sinon `None`. Le champ 9 des autres opérations est
/// relevé à part (`world::extra_events`, [`decode_skill_block`]).
pub fn translate(t: u8, values: &[Option<Value>]) -> Option<WorldEvent> {
    match t {
        packet_type::SKILL => match int(values, 0)? {
            op::ATTACH => attach_of(values).map(WorldEvent::Skill),
            op::ADD => Some(WorldEvent::Skill(SkillEvent::Known {
                owner: base_cid(bytes(values, 2)).unwrap_or(0),
                id: int(values, 1)? as i32,
                tid: int(values, 4)? as i32,
                level: bytes(values, 3).and_then(|f| factors_dirt(f, DIRT_SKILL_LEVEL)).unwrap_or(1),
                exp: bytes(values, 3).and_then(|f| factors_dirt(f, DIRT_SKILL_EXP)),
            })),
            op::REMOVE => Some(WorldEvent::Skill(SkillEvent::Removed { id: int(values, 1)? as i32 })),
            op::ALL_BUFFED => {
                let list: Vec<&[u8]> = match values.get(8).and_then(Option::as_ref) {
                    Some(Value::PacketList(l)) => l.iter().map(Vec::as_slice).collect(),
                    Some(Value::Packet(b)) => vec![b.as_slice()],
                    _ => Vec::new(),
                };
                let entries = list.len();
                let buffs: Vec<_> = list
                    .into_iter()
                    .filter_map(|b| decode_mini(mini::SKILL_BUFF, b).ok())
                    .filter_map(|(_, v)| {
                        let ms = match v.get(1).and_then(Option::as_ref) {
                            Some(Value::U64(x)) => *x,
                            Some(x) => x.as_i64().unwrap_or(0).max(0) as u64,
                            None => 0,
                        };
                        Some((int(&v, 0)? as i32, ms, int(&v, 2).unwrap_or(0) as i32)).filter(|(t, _, _)| *t != 0)
                    })
                    .collect();
                let owner = base_cid(bytes(values, 2))?;
                log::debug!(target: "archrust_buffs", "reseau_decode op=6 porteur={owner} entrees={entries} retenues={} buffs_tid_ms_modele={buffs:?}", buffs.len());
                Some(WorldEvent::Skill(SkillEvent::Buffs { owner, buffs }))
            }
            op::ADDITIONAL_EFFECT => Some(WorldEvent::Skill(SkillEvent::AdditionalEffect {
                kind: int(values, 1)? as i32,
                caster: base_cid(bytes(values, 2)).unwrap_or(0),
                target: int(values, 4).unwrap_or(0) as i32,
            })),
            op::MODIFIED_SKILL_LEVEL => Some(WorldEvent::Skill(SkillEvent::LevelBonus { owner: base_cid(bytes(values, 2)).unwrap_or(0), bonus: int(values, 1)? as i32 })),
            op::INIT_COOLTIME => Some(WorldEvent::Skill(SkillEvent::CooldownReset {
                owner: base_cid(bytes(values, 2)).unwrap_or(0),
                id: int(values, 1).unwrap_or(0) as i32,
                tid: int(values, 4).unwrap_or(0) as i32,
            })),
            op::SPECIFIC_SKILL_LEVEL => Some(WorldEvent::Skill(SkillEvent::SkillBonus {
                owner: base_cid(bytes(values, 2)).unwrap_or(0),
                id: int(values, 1)? as i32,
                level: int(values, 4).unwrap_or(0) as i32,
            })),
            op::UPDATE => {
                let id = int(values, 1)? as i32;
                let exp = bytes(values, 3).and_then(|f| factors_dirt(f, DIRT_SKILL_EXP));
                Some(WorldEvent::Skill(match bytes(values, 3).and_then(|f| factors_dirt(f, DIRT_SKILL_LEVEL)) {
                    Some(level) => SkillEvent::Level { id, level, exp },
                    None => SkillEvent::Exp { id, exp: exp? },
                }))
            }
            op::CAST => {
                let (_, a) = decode_mini(mini::SKILL_ACTION, bytes(values, 6)?).ok()?;
                // Type d'action : 0 / 1 lancement ; 5 à 17 refus (`action_type::REFUSED`), qui ne touche ni
                // l'incantation ni la recharge ; 2 à 4 sans lancement à annoncer ici.
                match int(&a, 0).unwrap_or(1) as i32 {
                    0 | 1 => {}
                    k if action_type::REFUSED.contains(&k) => {
                        return Some(WorldEvent::Skill(SkillEvent::CastRefused { caster: base_cid(bytes(values, 2)).unwrap_or(0), id: int(values, 1)? as i32, kind: k }));
                    }
                    _ => return None,
                }
                Some(WorldEvent::Skill(SkillEvent::Cast {
                    caster: base_cid(bytes(values, 2)).unwrap_or(0),
                    id: int(values, 1)? as i32,
                    tid: int(values, 4).unwrap_or(0) as i32,
                    target: base_cid(bytes(&a, 1)).unwrap_or(0),
                    cast_ms: int(&a, 5).unwrap_or(0) as u32,
                    recast_ms: int(&a, 7).unwrap_or(0) as u32,
                }))
            }
            // Effet de la compétence : présenté comme un coup (dégâts, PV restants), comme le 0x41.
            op::CAST_RESULT => {
                let (_, a) = decode_mini(mini::SKILL_ACTION, bytes(values, 6)?).ok()?;
                let factor_packet = bytes(&a, 2).map(<[u8]>::to_vec);
                let hit = factor_packet.as_deref().and_then(crate::combat::decode_hit_factors);
                let factors: Option<ActionFactorPoints> = hit.as_ref().map(|h| h.0.clone());
                let damage = hit.as_ref().and_then(|h| h.1);
                Some(WorldEvent::CharacterAction {
                    cid: base_cid(bytes(values, 2)).unwrap_or(0),
                    target_cid: base_cid(bytes(&a, 1)).unwrap_or(0),
                    result: Some(crate::combat::outcome::SUCCESS),
                    target_hp: factors.as_ref().and_then(|f| f.hp),
                    factors,
                    damage,
                    factor_packet,
                    combo: None,
                    force_attack: None,
                    additional_effect: None,
                    hit_index: None,
                })
            }
            _ => None,
        },
        packet_type::EVENT_SKILL_MASTER => {
            let npc = bytes(values, 1)
                .and_then(|b| decode_mini(mini::EVENT_BASE, b).ok())
                .and_then(|(_, v)| int(&v, 1))
                .unwrap_or(0) as i32;
            Some(WorldEvent::Skill(match int(values, 0)? {
                master_op::OPENED => SkillEvent::MasterOpened { npc },
                master_op::LEARN_RESULT => SkillEvent::Learned { tid: int(values, 4).unwrap_or(0) as i32, result: int(values, 5).unwrap_or(-1) as i32 },
                master_op::UPGRADE_RESULT => SkillEvent::Upgraded { id: int(values, 3).unwrap_or(0) as i32, result: int(values, 5).unwrap_or(0) as i32 },
                _ => return None,
            }))
        }
        _ => None,
    }
}

/// Compétences du champ 8 de la vue optimisée op 2 : paquets AGPMSKILL complets mis bout à bout.
pub fn decode_skill_block(mut b: &[u8]) -> Vec<WorldEvent> {
    let mut out = Vec::new();
    while b.len() > 5 {
        let Ok(len) = b[1..5].try_into().map(u32::from_le_bytes) else { break };
        let len = len as usize;
        if len < archrust_protocol::packet::HEADER_SIZE || len > b.len() {
            break;
        }
        let (one, rest) = b.split_at(len);
        b = rest;
        let Ok(p) = decode_packet(main::SKILL, one) else { continue };
        out.extend(translate(packet_type::SKILL, &p.values));
        if int(&p.values, 0) != Some(op::ATTACH) {
            out.extend(attach_of(&p.values).map(WorldEvent::Skill));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::build_packet;

    fn factors(level: i32) -> Value {
        let d = mini::FACTOR_DIRT;
        let dirt = build_mini(d, &sparse(d, &[(4, Value::I32(level)), (5, Value::I32(level))])).unwrap();
        let cf = mini::CHAR_FACTORS;
        Value::Packet(build_mini(cf, &sparse(cf, &[(11, Value::Packet(dirt))])).unwrap())
    }

    #[test]
    fn liste_et_lancement() {
        let k = main::SKILL;
        let add = |id: i32, tid: i32, level: i32| {
            build_packet(k, &sparse(k, &[(0, Value::I8(0)), (1, Value::I32(id)), (2, base(42)), (3, factors(level)), (4, Value::I32(tid))])).unwrap()
        };
        let mut block = add(1, 222, 1);
        block.extend(add(2, 4, 3));
        let evs = decode_skill_block(&block);
        assert_eq!(evs.len(), 2);
        let WorldEvent::Skill(SkillEvent::Known { owner, id, tid, level, exp }) = &evs[1] else { panic!("{:?}", evs[1]) };
        assert_eq!((*owner, *id, *tid, *level, *exp), (42, 2, 4, 3, None));
        // Compétence de production : expérience cumulée au « dirt » indice 6.
        let d = mini::FACTOR_DIRT;
        let dirt = build_mini(d, &sparse(d, &[(4, Value::I32(3)), (6, Value::I32(960))])).unwrap();
        let cf = mini::CHAR_FACTORS;
        let f = Value::Packet(build_mini(cf, &sparse(cf, &[(11, Value::Packet(dirt))])).unwrap());
        let p = build_packet(k, &sparse(k, &[(0, Value::I8(0)), (1, Value::I32(9)), (2, base(42)), (3, f), (4, Value::I32(663))])).unwrap();
        let evs = decode_skill_block(&p);
        assert!(matches!(evs.as_slice(), [WorldEvent::Skill(SkillEvent::Known { owner: 42, id: 9, tid: 663, level: 3, exp: Some(960) })]));
        // Retrait (op 1) : identifiant seul.
        let p = decode_packet(k, &build_packet(k, &sparse(k, &[(0, Value::I8(1)), (1, Value::I32(9)), (2, base(42))])).unwrap()).unwrap();
        assert!(matches!(translate(packet_type::SKILL, &p.values), Some(WorldEvent::Skill(SkillEvent::Removed { id: 9 }))));

        let (s, v) = command_packet(&SkillCommand::Cast { id: 2, target: 77, forced: false }, 42);
        assert_eq!(s.id, packet_type::SKILL);
        let (_, a) = decode_mini(mini::SKILL_ACTION, bytes(&v, 6).unwrap()).unwrap();
        assert_eq!((int(&v, 0), int(&v, 1), base_cid(bytes(&a, 1))), (Some(7), Some(2), Some(77)));
        // Refus (type d'action 12) : pas un lancement, donc pas de recharge remise à zéro.
        let a = mini::SKILL_ACTION;
        let act = build_mini(a, &sparse(a, &[(0, Value::I8(12)), (1, base(42))])).unwrap();
        let v = sparse(main::SKILL, &[(0, Value::I8(op::CAST as i8)), (1, Value::I32(9)), (2, base(42)), (6, Value::Packet(act))]);
        assert!(matches!(translate(packet_type::SKILL, &v), Some(WorldEvent::Skill(SkillEvent::CastRefused { caster: 42, id: 9, kind: 12 }))));
        // Compétence au sol : le joueur pour cible, le point visé au champ 3 de l'action, après passage par le paquet complet.
        let (s, v) = command_packet(&SkillCommand::CastGround { id: 2, target: 42, pos: [100.5, -3.0, 2000.25], forced: false }, 42);
        let p = decode_packet(s, &build_packet(s, &v).unwrap()).unwrap();
        let (_, a) = decode_mini(mini::SKILL_ACTION, bytes(&p.values, 6).unwrap()).unwrap();
        assert_eq!((int(&p.values, 0), int(&p.values, 1), base_cid(bytes(&p.values, 2)), int(&a, 0), base_cid(bytes(&a, 1)), int(&a, 4)), (Some(7), Some(2), Some(42), Some(0), Some(42), Some(0)));
        assert!(matches!(a.get(3), Some(Some(Value::Vec3(pos))) if *pos == [100.5, -3.0, 2000.25]));
        // Annulation de l'évolution : action de type 4 sur la compétence possédée, cible = soi.
        let (_, v) = command_packet(&SkillCommand::CancelCast { id: 5 }, 42);
        let (_, a) = decode_mini(mini::SKILL_ACTION, bytes(&v, 6).unwrap()).unwrap();
        assert_eq!((int(&v, 0), int(&v, 1), base_cid(bytes(&v, 2)), int(&a, 0), base_cid(bytes(&a, 1))), (Some(7), Some(5), Some(42), Some(4), Some(42)));
        let (s, v) = command_packet(&SkillCommand::Learn { npc: 9, tid: 57 }, 42);
        assert_eq!((s.id, int(&v, 0), int(&v, 4)), (0x3b, Some(1), Some(57)));
        // Arbres de maîtrise : apprendre par TID (op 1), améliorer par identifiant (op 2, champ 3).
        let (s, v) = command_packet(&SkillCommand::Upgrade { npc: 9, id: 12 }, 42);
        assert_eq!((s.id, int(&v, 0), int(&v, 3), int(&v, 2)), (0x3b, Some(2), Some(12), Some(42)));
    }

    #[test]
    fn effets_bonus_et_recharges() {
        let k = main::SKILL;
        let d = |pairs: &[(usize, Value)]| decode_packet(k, &build_packet(k, &sparse(k, pairs)).unwrap()).unwrap().values;
        let b = mini::SKILL_BUFF;
        let buff = |tid: i32, ms: u64| build_mini(b, &sparse(b, &[(0, Value::I32(tid)), (1, Value::U64(ms)), (2, Value::I32(96))])).unwrap();
        let v = d(&[(0, Value::I8(6)), (2, base(42)), (8, Value::PacketList(vec![buff(57, 30_000), buff(58, 1_000)]))]);
        assert!(matches!(translate(packet_type::SKILL, &v), Some(WorldEvent::Skill(SkillEvent::Buffs { owner: 42, buffs })) if buffs == vec![(57, 30_000, 96), (58, 1_000, 96)]));
        let v = d(&[(0, Value::I8(10)), (1, Value::I32(8)), (2, base(42)), (4, Value::I32(77))]);
        assert!(matches!(translate(packet_type::SKILL, &v), Some(WorldEvent::Skill(SkillEvent::AdditionalEffect { kind: 8, caster: 42, target: 77 }))));
        let v = d(&[(0, Value::I8(14)), (1, Value::I32(2)), (2, base(42))]);
        assert!(matches!(translate(packet_type::SKILL, &v), Some(WorldEvent::Skill(SkillEvent::LevelBonus { owner: 42, bonus: 2 }))));
        let v = d(&[(0, Value::I8(15)), (1, Value::I32(5)), (2, base(42)), (4, Value::I32(222))]);
        assert!(matches!(translate(packet_type::SKILL, &v), Some(WorldEvent::Skill(SkillEvent::CooldownReset { owner: 42, id: 5, tid: 222 }))));
        let v = d(&[(0, Value::I8(16)), (1, Value::I32(5)), (2, base(42)), (4, Value::I32(3))]);
        assert!(matches!(translate(packet_type::SKILL, &v), Some(WorldEvent::Skill(SkillEvent::SkillBonus { owner: 42, id: 5, level: 3 }))));
    }

    #[test]
    fn points_heroiques() {
        // « dirt » indice 5 : points de compétence ; indice 7 : points héroïques (fenêtres héroïques).
        let d = mini::FACTOR_DIRT;
        let dirt = build_mini(d, &sparse(d, &[(5, Value::I32(12)), (7, Value::I32(41))])).unwrap();
        let cf = mini::CHAR_FACTORS;
        let f = build_mini(cf, &sparse(cf, &[(11, Value::Packet(dirt))])).unwrap();
        assert_eq!((factors_dirt(&f, DIRT_SKILL_POINT), factors_dirt(&f, DIRT_HEROIC_POINT)), (Some(12), Some(41)));
    }

    #[test]
    fn donnees_attachees_du_personnage() {
        let k = main::SKILL;
        // Bloc de 220 octets : i32 aux décalages 0x08, 0x0c, 0x2c, 0x3c, 0xa4, 0xa8, 0xb0, 0xbc.
        let mut block = vec![0u8; ATTACH_BLOCK_SIZE];
        for (i, o) in SkillAttach::OFFSETS.iter().enumerate() {
            block[*o..*o + 4].copy_from_slice(&(i as i32 + 1).to_le_bytes());
        }
        block[0x10..0x14].copy_from_slice(&99i32.to_le_bytes());
        let want = SkillAttach { critical_rate: 1, critical_damage: 2, reflect_rate: 3, reflect_amount: 4, ignore_defense: 5, ignore_resistance: 6, critical_resistance: 7, stun_resistance: 8 };
        assert_eq!(SkillAttach::decode(&block), want);
        assert_eq!(SkillAttach::decode(&want.encode()), want);
        // Bloc tronqué : ce qui manque vaut 0.
        assert_eq!(SkillAttach::decode(&block[..0x30]), SkillAttach { critical_rate: 1, critical_damage: 2, reflect_rate: 3, ..Default::default() });
        // Op 3 : base (3, CID) et champ 9.
        let p = build_packet(k, &sparse(k, &[(0, Value::I8(3)), (2, base(42)), (9, Value::Block(block.clone()))])).unwrap();
        let d = decode_packet(k, &p).unwrap();
        assert!(matches!(translate(packet_type::SKILL, &d.values), Some(WorldEvent::Skill(SkillEvent::Attach { cid: 42, data })) if data == want));
        // Base qui ne désigne pas un personnage : ignorée.
        let s = mini::SKILL_BASE;
        let other = Value::Packet(build_mini(s, &[Some(Value::I8(1)), Some(Value::I32(42))]).unwrap());
        let p = build_packet(k, &sparse(k, &[(0, Value::I8(3)), (2, other), (9, Value::Block(block.clone()))])).unwrap();
        assert!(translate(packet_type::SKILL, &decode_packet(k, &p).unwrap().values).is_none());
        // Autre opération qui porte le champ 9 (op 0 dans le bloc de la vue optimisée) : les deux événements.
        let p = build_packet(k, &sparse(k, &[(0, Value::I8(0)), (1, Value::I32(1)), (2, base(42)), (3, factors(2)), (4, Value::I32(222)), (9, Value::Block(block))])).unwrap();
        let evs = decode_skill_block(&p);
        assert_eq!(evs.len(), 2);
        assert!(matches!(evs[0], WorldEvent::Skill(SkillEvent::Known { id: 1, level: 2, .. })));
        assert!(matches!(evs[1], WorldEvent::Skill(SkillEvent::Attach { cid: 42, data }) if data == want));
    }
}
