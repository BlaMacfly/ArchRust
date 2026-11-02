//! Métamorphose (potions d'usage 7, compétences « transform_target ») et évolution du Dragon Scion côté moteur.
//! Protocole : `archrust_netclient::morph` ; icônes : `hud_wanted.rs` (`draw_system_grid_icon`).
//!
//! Comportement relevé dans Archonia (AgpmCharacter, AgcmCharacter, AgcmUICharacter ; sha256 e9c7609b…) :
//! * op 11 (`FUN_140173f10` → `FUN_1401698d0`) : forme au gabarit inconnu ignorée ; même TID : rien (l'état
//!   métamorphosé reste celui que donne le TID d'origine sauvé) ; sinon TID, visage et coiffure d'origine sauvés
//!   **une fois** (`+0x608…+0x610`), modèle, animations et objets portés rechargés ;
//! * op 12 (`FUN_140169550`) : seulement métamorphosé ; TID, visage et coiffure d'origine rétablis puis oubliés ;
//! * op 26 / 27 : refusées pour un TID nul ou inconnu, une évolution déjà faite (ou absente pour 27) ou le même
//!   TID ; race et classe du nouveau gabarit recopiées dans les facteurs (`FUN_14019e640`, type 2) ; objets portés
//!   par `ItemEvolutionTable.txt` (voir `scene::evolution_item`) ;
//! * champ 18 de l'op 0 : bit 0 métamorphosé, bit 2 évolué (pas de TID d'origine connu dans ce cas) ;
//! * icônes de `UI_SystemMessageGrid` pour soi, pas pour l'ArchLord (`FUN_1404fdda0`, `FUN_1404fdbe0`) :
//!   `Cancel_Transform.png` (clic → op 13, `FUN_1404b71e0`) et `UI_MainGrid_BtnRestoreEvolution.png` (clic →
//!   AgpmSkill op 7 action 4 sur la compétence d'évolution dont l'effet est actif, `FUN_1404b6ee0`) ; infobulle
//!   `sysstr` 665 ; retirer l'icône de métamorphose remet le compte à rebours à zéro ;
//! * compte à rebours (`FUN_1405006b0`, `FUN_1404fd3e0`) : à l'op 4 d'une potion d'usage 7, `Transform_Remain_Notice`
//!   (durée en minutes), puis chaque minute tant que le joueur est métamorphosé ou évolué, `Transform_Restore_Notice`
//!   à l'échéance ;
//! * AgpmItem op 27 : messages `UseItemFailedBySiegewar`, `UseItemCannotTransformCreature`,
//!   `UseItemCannotTransformPotion` (code 3, `ReverseOrbError` : voir `app_consumables.rs`) ; ops 19 / 20 : recharge commune des
//!   potions de métamorphose (10 min), affichée sur la barre de raccourcis ;
//! * refus du client métamorphosé, sauf l'ArchLord (`FUN_1402944c0`) : compétences (`FUN_14069a0c0`, sans
//!   message), dialogue des PNJ (`FUN_140574e80`, sans message), passeports (`Item_TryUseTelScrollOnTransform`,
//!   `FUN_14053d0b0`) ; coiffeur `CustomizeNeedRestoreTransform` métamorphosé **ou évolué** (`app_customize.rs`) ;
//!   DragonScion d'une autre classe que Scion : objet d'oubli des compétences (usage 27) refusé
//!   (`IsEvolutionNotUseInitSkillItem`). Potions 346 à 348 : voir `app_ride.rs` (inventaire) et
//!   [`refused_item`] (barre de raccourcis).
//!
//!
//! Pendant la transformation (relevé suite, `SPEC-METAMORPHOSE-2`) :
//! * l'équipement se change **normalement** ; sous métamorphose **ou** évolution, sauf une monture (genre 0x20),
//!   le client ajoute le contrôle `FUN_1401bca10` ([`equip_permitted`]) contre les facteurs **courants** (race,
//!   sexe, classe ; d'origine sous métamorphose, de la forme sous évolution) et le drapeau DISARMAMENT : complet,
//!   niveau compris, pour porter (`FUN_140311b60`), sans le niveau pour retirer (`FUN_140312b00`, `FUN_1401c4330`) ;
//!   une demande refusée n'est pas envoyée (sans message, comme les erreurs « EquipItem() Error ») et un
//!   déplacement de l'op 2 d'AGPMITEM qui échoue (porté → 2, 13, 51 ; → 3) est **ignoré** (`FUN_1401b4d00`) ;
//! * métamorphosé (`+0x600`), un changement d'équipement **n'est pas redessiné** (`FUN_1407523a0`,
//!   `FUN_140754c60`) : tout est rattaché au modèle à l'op 11 / 12 (`FUN_1404b43a0`) ; visage et coiffure ne sont
//!   pas posés (`FUN_1404b48d0`) ;
//! * attaque d'un PJ (`FUN_1402d8e80`) : refusée si l'attaquant est métamorphosé dans une forme de `Type` & 8
//!   (animal : Mutation, potions 408 à 416), ou si la cible l'est ; une cible non joueur doit être attaquable
//!   (0x10000, gabarit **courant**, `FUN_140170d10`, voir `app_combat.rs`) ;
//! * compétence sur un ennemi (`target` 1 de SKILL_SPEC, `FUN_1402f8e20`) : refusée sur une cible métamorphosée
//!   dont la forme n'a pas 0x10000 et 0x20000 ;
//! * « même race » (`FUN_14017fbf0`) : gabarit d'origine d'un personnage métamorphosé ([`App::morph_origin_tid`],
//!   couleurs des Shrines dans `app_shrine.rs`) ;
//! * menu d'échange privé (`FUN_14053e080`) inactif si l'un des deux est métamorphosé, sauf l'ArchLord
//!   ([`private_trade_active`]) ;
//! * ops 11 / 12 pour tout personnage en vue, monstres compris (le modèle du monstre change).
//!
//! Écarts : le rappel 0x35 de l'op 13 reçue et le champ `+0xb38` qui bloque l'op 26 ne sont pas reproduits ; les
//! actions sociales n'existent pas encore dans ArchRust ; l'infobulle des icônes n'a que le texte 665. Dans
//! `FUN_1401bca10`, le niveau maximal du gabarit d'objet (`+0x1f0`, colonne inconnue) et les règles de région des
//! montures ne sont pas repris ; le bit GM (0x10) de l'attaquant n'est pas connu du client ; `UI_TargetInfo` n'affiche ni niveau ni couleur
//! (`FUN_1404fe410` : gabarit d'origine) et ArchRust ne calcule pas la croissance par niveau.

use super::App;
use crate::assets::Assets;
use crate::hud::{HudState, ItemUi};
use crate::scene::{entity_meshes_with_head, Head};
use crate::ui::UiState;
use archrust_netclient::morph::{use_result, MorphCommand, MorphEvent};
use archrust_netclient::skills::SkillCommand;
use archrust_netclient::world::{factor_part, CharacterStats, ItemRecord};
use archrust_netclient::{Command, WorldEvent};
use egui::Color32;
use glam::Mat4;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Usages (`Type` d'itemdatatable, objets d'`ItemType` 1) relevés par `FUN_14053d0b0`.
const USE_TELEPORT_SCROLL: i64 = 4;
const USE_TRANSFORM: i64 = 7;
const USE_SPEED_POTION: i64 = 12;
const USE_SKILL_INIT: i64 = 27;
/// `transform_target` de SKILL_SPEC des compétences d'évolution (bits `0x8000000001000`, `FUN_1402fef60`).
const TRANSFORM_TARGET_EVOLUTION: i64 = 5;
/// Intervalle des avis du compte à rebours.
const NOTICE_EVERY: Duration = Duration::from_secs(60);
const TEXTURE_TRANSFORM: &str = "Cancel_Transform.png";
const TEXTURE_EVOLUTION: &str = "UI_MainGrid_BtnRestoreEvolution.png";
/// `ItemType` d'itemdatatable : objet qui se porte.
const ITEM_TYPE_EQUIP: i64 = 0;
/// Genre (« Kind », gabarit d'objet `+0x3cc`) d'une monture : hors du contrôle sous métamorphose.
const KIND_MOUNT: i64 = 0x20;
/// Statuts d'objet d'AgpmItem contrôlés par `FUN_1401b4d00` (op 2).
const STATUS_INVENTORY: i8 = 2;
const STATUS_EQUIP: i8 = 3;
const STATUS_SUB_INVENTORY: i8 = 13;
const STATUS_CASH_INVENTORY: i8 = 51;
/// `target` de SKILL_SPEC : compétence lancée sur un ennemi.
const SKILL_TARGET_ENEMY: i64 = 1;

/// Bits du type de personnage (colonne `Type` de CharacterDataTable.txt, gabarit `+0x258`).
pub(super) mod char_type {
    /// CREATURE : formes animales (Mutation, potions 408 à 416).
    pub const CREATURE: u32 = 0x8;
    pub const ATTACKABLE: u32 = 0x10000;
    pub const TARGETABLE: u32 = 0x20000;
}

/// État d'un personnage (`AgpdCharacter` : `+0x600` métamorphosé, `+0x604` évolué, `+0x608…+0x610` origine).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct MorphState {
    pub transformed: bool,
    pub evolved: bool,
    /// TID, visage et coiffure d'avant la première métamorphose (op 11).
    pub origin: Option<(i32, Head)>,
}

impl MorphState {
    /// Op 11 : nouveau TID à poser, ou rien. `known` : gabarit de la forme connu du client.
    pub fn transform(&mut self, current: i32, head: Head, tid: i32, known: bool) -> Option<i32> {
        if !known {
            return None;
        }
        if tid == current {
            // FUN_1401698d0 échoue : +0x600 = (+0x608 ≠ 0).
            self.transformed = self.origin.is_some();
            return None;
        }
        if self.origin.is_none() {
            self.origin = Some((current, head));
        }
        self.transformed = true;
        Some(tid)
    }

    /// Op 12 : TID et tête d'origine à rétablir, ou rien (l'état métamorphosé est levé dans les deux cas).
    pub fn restore(&mut self, current: i32) -> Option<(i32, Head)> {
        if !self.transformed {
            return None;
        }
        self.transformed = false;
        match self.origin {
            Some((tid, head)) if tid != 0 && tid != current => {
                self.origin = None;
                Some((tid, head))
            }
            _ => None,
        }
    }

    /// Op 26 : TID évolué à poser, ou rien.
    pub fn evolve(&mut self, current: i32, tid: i32, known: bool) -> Option<i32> {
        if tid == 0 || !known || self.evolved || tid == current {
            return None;
        }
        self.evolved = true;
        Some(tid)
    }

    /// Op 27 : TID à rétablir, ou rien.
    pub fn restore_evolution(&mut self, current: i32, tid: i32, known: bool) -> Option<i32> {
        if tid == 0 || !known || !self.evolved || tid == current {
            return None;
        }
        self.evolved = false;
        Some(tid)
    }
}

/// Avis du compte à rebours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Notice {
    /// `Transform_Remain_Notice` : minutes restantes.
    Remain(u64),
    /// `Transform_Restore_Notice`.
    Restore,
}

/// Compte à rebours d'AgcmUICharacter (`+0x26b8` fin, `+0x26c0` restant, `+0x26c8` prochain avis).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Countdown {
    end: Instant,
    remain_ms: i64,
    next: Instant,
}

impl Countdown {
    /// `FUN_1405006b0` : départ, avec le premier avis.
    fn start(now: Instant, duration_ms: u64) -> (Self, Notice) {
        let c = Countdown { end: now + Duration::from_millis(duration_ms), remain_ms: duration_ms as i64, next: now + NOTICE_EVERY };
        (c, Notice::Remain(duration_ms / 60_000))
    }

    /// `FUN_1404fd3e0` : avis de cette image ; `None` dans le second membre : compte à rebours fini.
    fn tick(mut self, now: Instant, active: bool) -> (Option<Self>, Option<Notice>) {
        if !active {
            return (Some(self), None);
        }
        if now >= self.end {
            return (None, Some(Notice::Restore));
        }
        if self.next <= now {
            self.remain_ms -= 60_000;
            self.next = now + NOTICE_EVERY;
            return (Some(self), Some(Notice::Remain(self.remain_ms.max(0) as u64 / 60_000)));
        }
        (Some(self), None)
    }
}

/// Colonnes utiles d'itemdatatable.txt : `ItemType`, `Type`, `SubType`, `Polymorph Dur`, et les restrictions
/// d'un équipement (`Kind`, `Level`, `Race`, `Class`, `Gender` ; races et classes en masques `1 << (n − 1)`).
#[derive(Debug, Clone, Copy, Default)]
struct ItemRow {
    item_type: i64,
    use_type: i64,
    sub_type: i64,
    duration_ms: u64,
    kind: i64,
    level: i64,
    race_mask: u32,
    class_mask: u32,
    gender: i64,
}

/// Facteurs courants d'un personnage (`+0x470`, bloc CharType : race, sexe, classe ; niveau du bloc statut) ;
/// −1 : inconnu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CharFactors {
    race: i32,
    gender: i32,
    class: i32,
    level: i32,
}

/// Bit de la race ou de la classe `n` dans un masque de restriction (`1 << (n − 1)`, sur 5 bits comme le client :
/// −1 inconnu → bit 30).
fn restriction_bit(n: i32) -> u32 {
    1u32 << ((n - 1) & 0x1f)
}

/// `FUN_1401bca10` pour un PJ : classe, race, sexe, niveau minimal (sauf `without_level`), DISARMAMENT.
fn equip_permitted(row: &ItemRow, c: CharFactors, flags: u32, without_level: bool) -> bool {
    if row.class_mask != 0 && row.class_mask & restriction_bit(c.class) == 0 {
        return false;
    }
    if row.race_mask != 0 && row.race_mask & restriction_bit(c.race) == 0 {
        return false;
    }
    if row.gender > 0 && row.gender != c.gender as i64 {
        return false;
    }
    if !without_level && row.level > 0 && row.level > c.level as i64 {
        return false;
    }
    flags & archrust_netclient::auction::item_flag::DISARMAMENT == 0
}

/// Contrôle ajouté sous métamorphose ou évolution (`changed_form`) : `None` hors de ce cas et pour une monture,
/// sinon `FUN_1401bca10` complet pour porter (`equip`), sans le niveau pour retirer.
fn morph_equip_check(row: &ItemRow, changed_form: bool, c: CharFactors, flags: u32, equip: bool) -> Option<bool> {
    (changed_form && row.kind != KIND_MOUNT).then(|| equip_permitted(row, c, flags, !equip))
}

/// Op 2 d'AGPMITEM sur un objet du joueur (`FUN_1401b4d00`) : `true` si le client l'ignore (objet porté rangé en
/// 2, 13 ou 51, ou objet porté, refusés par [`morph_equip_check`]).
fn ignores_update(row: &ItemRow, changed_form: bool, c: CharFactors, flags: u32, current: Option<i8>, new: Option<i8>) -> bool {
    let equip = match (current, new) {
        (Some(STATUS_EQUIP), Some(STATUS_INVENTORY | STATUS_SUB_INVENTORY | STATUS_CASH_INVENTORY)) => false,
        (cur, Some(STATUS_EQUIP)) if cur != Some(STATUS_EQUIP) => true,
        _ => return false,
    };
    morph_equip_check(row, changed_form, c, flags, equip) == Some(false)
}

/// Attaque d'un PJ (hors GM) permise par `FUN_1402d8e80` : ni lui ni la cible métamorphosés dans une forme de
/// `Type` & 8 (`me_form`, `target_form` : type de la forme, `None` hors métamorphose).
fn attack_permitted(me_form: Option<u32>, target_form: Option<u32>) -> bool {
    let animal = |f: Option<u32>| f.is_some_and(|t| t & char_type::CREATURE != 0);
    !animal(me_form) && !animal(target_form)
}

/// Compétence sur un ennemi permise sur une cible (`FUN_1402f8e20`) : une forme (`target_form`) doit être
/// attaquable et ciblable.
fn skill_target_permitted(enemy_skill: bool, target_form: Option<u32>) -> bool {
    let both = char_type::ATTACKABLE | char_type::TARGETABLE;
    !enemy_skill || target_form.is_none_or(|t| t & both == both)
}

/// Menu d'échange privé actif (`FUN_14053e080`) : chacun des deux, s'il est métamorphosé, doit être l'ArchLord
/// (`(métamorphosé, ArchLord)` pour soi puis la cible).
fn private_trade_active(me: (bool, bool), target: (bool, bool)) -> bool {
    (!me.0 || me.1) && (!target.0 || target.1)
}

#[derive(Default)]
struct MorphData {
    items: HashMap<i32, ItemRow>,
    /// Compétences d'évolution (SKILL_SPEC `transform_target` 5).
    evolution_skills: HashSet<i32>,
    /// Compétences lancées sur un ennemi (SKILL_SPEC `target` 1).
    enemy_skills: HashSet<i32>,
    sysstr: HashMap<u32, String>,
}

impl MorphData {
    fn item(&self, tid: Option<i32>) -> ItemRow {
        tid.and_then(|t| self.items.get(&t).copied()).unwrap_or_default()
    }
}

fn load_data(assets: &Assets) -> MorphData {
    let mut d = MorphData::default();
    let text = super::npc_glue::client_text_in(assets, "ini", "itemdatatable.txt");
    let mut lines = text.lines();
    if let Some(header) = lines.next() {
        let cols: Vec<&str> = header.split('\t').map(str::trim).collect();
        let col = |name: &str| cols.iter().position(|c| c.eq_ignore_ascii_case(name));
        let (tid, item_type, use_type, sub_type, dur) = (col("TID"), col("ItemType"), col("Type"), col("SubType"), col("Polymorph Dur"));
        let (kind, level, race, class, gender) = (col("Kind"), col("Level"), col("Race"), col("Class"), col("Gender"));
        for line in lines {
            let cells: Vec<&str> = line.split('\t').collect();
            let num = |c: Option<usize>| c.and_then(|i| cells.get(i)).and_then(|v| v.trim().parse::<f64>().ok()).map(|v| v as i64).unwrap_or(0);
            // Liste « 1;2;3 » → masque.
            let mask = |c: Option<usize>| {
                let list = c.and_then(|i| cells.get(i)).copied().unwrap_or_default();
                list.split(';').filter_map(|v| v.trim().parse::<i32>().ok()).filter(|&v| v > 0).fold(0u32, |m, v| m | restriction_bit(v))
            };
            let t = num(tid) as i32;
            if t > 0 {
                d.items.insert(
                    t,
                    ItemRow {
                        item_type: num(item_type),
                        use_type: num(use_type),
                        sub_type: num(sub_type),
                        duration_ms: num(dur).max(0) as u64,
                        kind: num(kind),
                        level: num(level),
                        race_mask: mask(race),
                        class_mask: mask(class),
                        gender: num(gender),
                    },
                );
            }
        }
    }
    // SKILL_SPEC.txt (par nom) → TID par SKILLTEMPLATE.ini.
    let mut by_name: HashMap<String, Vec<i32>> = HashMap::new();
    if let Ok(t) = assets.table("ini", "SKILLTEMPLATE.ini") {
        let name_col = t.section("").and_then(|h| h.entries.iter().find(|(_, v)| v.eq_ignore_ascii_case("Name")).map(|(k, _)| k.clone()));
        for s in &t.sections {
            if let (Ok(tid), Some(name)) = (s.name.parse::<i32>(), name_col.as_deref().and_then(|c| s.get(c))) {
                by_name.entry(name.trim().to_string()).or_default().push(tid);
            }
        }
    }
    let spec = super::npc_glue::client_text_in(assets, "ini", "SKILL_SPEC.txt");
    let mut rows = spec.lines().map(|l| l.split('\t').map(str::trim).collect::<Vec<_>>());
    if let Some(header) = rows.next() {
        let transform = header.iter().position(|h| h.eq_ignore_ascii_case("transform_target"));
        let target = header.iter().position(|h| h.eq_ignore_ascii_case("target"));
        for row in rows {
            let value = |c: Option<usize>| c.and_then(|i| row.get(i)).and_then(|v| v.parse::<i64>().ok());
            let tids = row.first().and_then(|n| by_name.get(*n)).into_iter().flatten();
            if value(transform) == Some(TRANSFORM_TARGET_EVOLUTION) {
                d.evolution_skills.extend(tids.clone());
            }
            if value(target) == Some(SKILL_TARGET_ENEMY) {
                d.enemy_skills.extend(tids);
            }
        }
    }
    for id in [346, 347, 348, 665] {
        d.sysstr.insert(id, super::npc_glue::sysstr(assets, id).unwrap_or_default());
    }
    log::info!("métamorphose : {} potion(s), {} compétence(s) d'évolution", d.items.values().filter(|i| i.item_type == 1 && i.use_type == USE_TRANSFORM).count(), d.evolution_skills.len());
    d
}

/// Objet refusé par le client (`FUN_14053d0b0`) : clé d'uimessage (`Err`) ou numéro de `sysstr` (`Ok`).
/// `riding_cases` : potions 346 à 348, que l'inventaire laisse à `app_ride.rs`.
fn refused_item(row: ItemRow, transformed: bool, archlord: bool, race_class: Option<(i32, i32)>, riding_cases: bool) -> Option<Result<u32, &'static str>> {
    if row.item_type != 1 {
        return None;
    }
    match row.use_type {
        USE_TRANSFORM if riding_cases && transformed => Some(Ok(346)),
        USE_SPEED_POTION if riding_cases && transformed && !archlord => match row.sub_type {
            1 => Some(Ok(347)),
            2 => Some(Ok(348)),
            _ => None,
        },
        USE_TELEPORT_SCROLL if transformed && !archlord => Some(Err("Item_TryUseTelScrollOnTransform")),
        USE_SKILL_INIT if matches!(race_class, Some((4, class)) if class != 3) => Some(Err("IsEvolutionNotUseInitSkillItem")),
        _ => None,
    }
}

/// Clics sur les icônes, traités par `App::morph_frame`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Click {
    Transform,
    Evolution,
}

#[derive(Default)]
pub struct MorphGlue {
    data: Option<MorphData>,
    states: HashMap<i32, MorphState>,
    /// CID du joueur.
    me: i32,
    /// Le joueur est l'ArchLord (voir `app_archlord.rs`), relevé à chaque image.
    archlord: bool,
    /// TID de la compétence d'évolution dont l'effet (AgpmSkill op 4) est actif sur le joueur.
    evolution_effect: Option<i32>,
    countdown: Option<Countdown>,
    /// Fin de la recharge commune des potions de métamorphose (AgpmItem op 19) et sa durée.
    reuse_end: Option<Instant>,
    clicks: Vec<Click>,
}

impl MorphGlue {
    fn data(&mut self, assets: &Assets) -> &MorphData {
        self.data.get_or_insert_with(|| load_data(assets))
    }

    pub fn transformed(&self, cid: i32) -> bool {
        self.states.get(&cid).is_some_and(|s| s.transformed)
    }

    pub fn evolved(&self, cid: i32) -> bool {
        self.states.get(&cid).is_some_and(|s| s.evolved)
    }
}

/// Infobulle des deux icônes : `sysstr` 665 « Cancel Metamorphosis ».
fn tooltip(glue: &mut MorphGlue, assets: &Assets) -> Vec<(String, Color32)> {
    let text = glue.data(assets).sysstr.get(&665).cloned().unwrap_or_default();
    vec![(text, Color32::WHITE)]
}

/// Icônes d'annulation dans `UI_SystemMessageGrid`, après l'avis de recherche (`wanted_shown`) s'il est affiché.
pub fn panel(ctx: &egui::Context, glue: &mut MorphGlue, wanted_shown: bool, hud: &mut HudState, assets: &Assets) {
    if glue.me == 0 || glue.archlord {
        return;
    }
    let mut index = wanted_shown as usize;
    let tip = tooltip(glue, assets);
    for (on, texture, click) in [(glue.transformed(glue.me), TEXTURE_TRANSFORM, Click::Transform), (glue.evolved(glue.me), TEXTURE_EVOLUTION, Click::Evolution)] {
        if !on {
            continue;
        }
        if crate::hud::draw_system_grid_icon(ctx, hud, assets, index, texture, &tip) {
            glue.clicks.push(click);
        }
        index += 1;
    }
}

/// Race et classe du joueur dans ses facteurs (`FACTOR_TYPE` 0 et 2).
fn race_class(hud: &HudState) -> Option<(i32, i32)> {
    let f = &hud.stats.factors;
    Some((*f.get(&(factor_part::TYPE, 0))?, *f.get(&(factor_part::TYPE, 2))?))
}

/// Facteurs courants du joueur (`FACTOR_TYPE` 0 race, 1 sexe, 2 classe ; niveau).
fn char_factors(hud: &HudState) -> CharFactors {
    let f = &hud.stats.factors;
    let get = |part: u8, i: u8| f.get(&(part, i)).copied();
    CharFactors {
        race: get(factor_part::TYPE, 0).unwrap_or(-1),
        gender: get(factor_part::TYPE, 1).unwrap_or(-1),
        class: get(factor_part::TYPE, 2).unwrap_or(-1),
        level: hud.stats.level.or(get(factor_part::STATUS, 7)).unwrap_or(-1),
    }
}

/// Demande de l'interface qui porte (`Some(true)`) ou retire (`Some(false)`) un objet, avec son identifiant.
fn equip_request(r: &ItemUi, items: &HashMap<i32, ItemRecord>, d: &MorphData) -> Option<(i32, bool)> {
    match *r {
        ItemUi::Activate { iid, worn: true } => Some((iid, false)),
        ItemUi::DropOnBag { drag, .. } if drag.worn => Some((drag.iid, false)),
        ItemUi::Activate { iid, worn: false } if d.item(items.get(&iid).and_then(|i| i.template_id)).item_type == ITEM_TYPE_EQUIP => Some((iid, true)),
        ItemUi::DropOnEquip(drag) if !drag.worn => Some((drag.iid, true)),
        _ => None,
    }
}

/// Clic droit sur un objet du sac refusé métamorphosé ou évolué (passeport, objet d'oubli des compétences), et
/// équipement porté ou retiré refusé par `FUN_1401bca10` sous métamorphose ou évolution (`flags` : drapeaux
/// d'état des objets, AGPMITEM champ 19). Avant `app_items::panel`.
#[allow(clippy::too_many_arguments)]
pub fn item_requests(glue: &mut MorphGlue, items: &HashMap<i32, ItemRecord>, flags: &HashMap<i32, u32>, requests: &mut Vec<ItemUi>, hud: &mut HudState, assets: &Assets, _ui: &mut UiState) {
    let (transformed, archlord, race_class) = (glue.transformed(glue.me), glue.archlord, race_class(hud));
    let changed_form = transformed || glue.evolved(glue.me);
    let factors = char_factors(hud);
    let d = glue.data(assets);
    requests.retain(|r| {
        if let Some((iid, equip)) = equip_request(r, items, d) {
            let row = d.item(items.get(&iid).and_then(|i| i.template_id));
            if morph_equip_check(&row, changed_form, factors, flags.get(&iid).copied().unwrap_or(0), equip) == Some(false) {
                log::info!("objet {iid} : {} refusé sous la forme courante (FUN_1401bca10)", if equip { "équipement" } else { "retrait" });
                return false;
            }
            return true;
        }
        let ItemUi::Activate { iid, worn: false } = *r else { return true };
        let row = d.item(items.get(&iid).and_then(|i| i.template_id));
        let Some(refusal) = refused_item(row, transformed, archlord, race_class, false) else { return true };
        let text = match refusal {
            Ok(id) => d.sysstr.get(&id).cloned().unwrap_or_default(),
            Err(key) => crate::hud::ui_message(hud, assets, key),
        };
        hud.push_system(&text);
        false
    });
}

impl App {
    fn morph_me(&self) -> i32 {
        self.player.as_ref().map_or(0, |p| p.cid)
    }


    /// Le gabarit `tid` est connu du client (`CharacterTemplatePublic.ini` ; sans client, tous).
    fn morph_known(&self, tid: i32) -> bool {
        !self.assets.has_client() || self.assets.table("ini", "CharacterTemplatePublic.ini").is_ok_and(|t| t.section(&tid.to_string()).is_some())
    }

    /// TID et tête courants d'un personnage en vue (le joueur compris).
    fn morph_current(&self, cid: i32) -> Option<(i32, Head)> {
        self.player.iter().chain(self.entities.iter()).find(|e| e.cid == cid).map(|e| (e.tid, e.head))
    }

    /// Pose le modèle `tid` (et la tête d'origine) d'un personnage : corps, tête, objets portés rechargés, animations
    /// reconstruites avec les maillages.
    fn morph_set_model(&mut self, cid: i32, tid: i32, head: Option<Head>) {
        if self.player.as_ref().is_some_and(|p| p.cid == cid) {
            let Some(p) = self.player.as_mut() else { return };
            p.tid = tid;
            if let Some(h) = head {
                p.head = h;
            }
            // Métamorphosé : ni visage ni coiffure (FUN_1404b48d0).
            let (tid, head, worn) = (p.tid, (!self.morph.transformed(cid)).then_some(p.head), p.equipment.clone());
            let Some(gpu) = self.gpu.as_mut() else { return };
            let fallback = gpu.player_marker.clone();
            let meshes = entity_meshes_with_head(&mut gpu.renderer, &self.assets, tid, head, &worn, &fallback).unwrap_or_else(|err| {
                log::warn!("métamorphose du joueur (TID {tid}) : {err:#}");
                vec![(fallback.clone(), Mat4::IDENTITY)]
            });
            if let Some(p) = self.player.as_mut() {
                p.meshes = meshes;
            }
        } else if let Some(index) = self.entities.iter().position(|e| e.cid == cid) {
            self.entities[index].tid = tid;
            if let Some(h) = head {
                self.entities[index].head = h;
            }
            self.reload_entity_meshes(index);
        }
    }

    /// Race et classe du gabarit recopiées dans les facteurs du joueur (évolution et retour).
    fn morph_copy_type(&mut self, cid: i32, tid: i32) {
        if cid != self.morph_me() {
            return;
        }
        if let Some((race, class)) = crate::scene::template_race_class(&self.assets, tid) {
            let mut s = CharacterStats::default();
            s.factors.insert((factor_part::TYPE, 0), race);
            s.factors.insert((factor_part::TYPE, 2), class);
            self.hud.merge_stats(&s);
        }
    }

    fn morph_message(&mut self, text: &str) {
        if !text.trim().is_empty() {
            self.hud.push_system(text);
        }
    }

    /// Paquets qui touchent à la métamorphose sans lui être propres : connexion, effets de compétence, objet
    /// utilisé (compte à rebours), personnage sorti de la vue.
    pub(super) fn morph_observe(&mut self, ev: &WorldEvent) {
        let me = self.morph_me();
        match ev {
            WorldEvent::Connected => {
                // Changement de personnage (`FUN_1405a7180`) : icônes retirées, tout est oublié.
                let data = self.morph.data.take();
                self.morph = MorphGlue { data, ..Default::default() };
            }
            WorldEvent::CharacterRemoved { cid, .. } if *cid != me => {
                self.morph.states.remove(cid);
            }
            WorldEvent::BuffAdded(b) if b.owner == me => {
                let assets = self.assets.clone();
                if self.morph.data(&assets).evolution_skills.contains(&b.skill_tid) {
                    self.morph.evolution_effect = Some(b.skill_tid);
                }
            }
            WorldEvent::BuffRemoved { owner, skill_tid } if *owner == me && self.morph.evolution_effect == Some(*skill_tid) => {
                self.morph.evolution_effect = None;
            }
            WorldEvent::ItemUsed { tid: Some(tid), user, .. } if user.is_none_or(|u| u == me) => {
                let assets = self.assets.clone();
                let row = self.morph.data(&assets).item(Some(*tid));
                if row.item_type == 1 && row.use_type == USE_TRANSFORM {
                    let (c, notice) = Countdown::start(Instant::now(), row.duration_ms);
                    self.morph.countdown = Some(c);
                    self.morph_notice(notice);
                }
            }
            _ => {}
        }
    }

    fn morph_notice(&mut self, notice: Notice) {
        let text = match notice {
            Notice::Remain(min) => super::chat_glue::printf(&crate::hud::ui_message(&mut self.hud, &self.assets, "Transform_Remain_Notice"), &[min.to_string().as_str()]),
            Notice::Restore => crate::hud::ui_message(&mut self.hud, &self.assets, "Transform_Restore_Notice"),
        };
        self.morph_message(&text);
    }

    /// Annonces de `morph`.
    pub(super) fn on_morph_event(&mut self, ev: MorphEvent) {
        let me = self.morph_me();
        match ev {
            MorphEvent::State { cid, transformed, evolved } => {
                let st = self.morph.states.entry(cid).or_default();
                st.transformed = transformed;
                st.evolved = evolved;
            }
            MorphEvent::Transform { cid, tid } => {
                let Some((current, head)) = self.morph_current(cid) else { return };
                let known = self.morph_known(tid);
                let st = self.morph.states.entry(cid).or_default();
                let new = st.transform(current, head, tid, known);
                let transformed = st.transformed;
                if let Some(tid) = new {
                    log::info!("métamorphose : personnage {cid} en TID {tid} (d'origine {current})");
                    self.morph_set_model(cid, tid, None);
                }
                self.on_world_event(WorldEvent::Transformed { cid, transformed });
            }
            MorphEvent::Restore { cid } => {
                let current = self.morph_current(cid).map_or(0, |c| c.0);
                let Some(st) = self.morph.states.get_mut(&cid) else { return };
                let was = st.transformed;
                if let Some((tid, head)) = st.restore(current) {
                    log::info!("fin de la métamorphose : personnage {cid} rendu au TID {tid}");
                    self.morph_set_model(cid, tid, Some(head));
                }
                if was {
                    // Icône retirée : compte à rebours remis à zéro (FUN_1404fdda0).
                    if cid == me {
                        self.morph.countdown = None;
                    }
                    self.on_world_event(WorldEvent::Transformed { cid, transformed: false });
                }
            }
            MorphEvent::Evolve { cid, tid } | MorphEvent::RestoreEvolution { cid, tid } => {
                let evolve = matches!(ev, MorphEvent::Evolve { .. });
                let Some((current, _)) = self.morph_current(cid) else { return };
                let known = self.morph_known(tid);
                let st = self.morph.states.entry(cid).or_default();
                let new = if evolve { st.evolve(current, tid, known) } else { st.restore_evolution(current, tid, known) };
                if let Some(tid) = new {
                    log::info!("{} : personnage {cid} en TID {tid}", if evolve { "évolution" } else { "fin de l'évolution" });
                    self.morph_set_model(cid, tid, None);
                    self.morph_copy_type(cid, tid);
                }
            }
            MorphEvent::UseItemResult { cid, code } if cid == me || cid == 0 => match use_result::message(code) {
                Some(key) => {
                    let text = crate::hud::ui_message(&mut self.hud, &self.assets, key);
                    self.morph_message(&text);
                }
                // Code 3 (`ReverseOrbError`) : relevé par `consumables_observe` (voir `app_consumables.rs`).
                None => {}
            },
            MorphEvent::ReuseTime { cid, remain_ms } if cid == me || cid == 0 => {
                self.morph.reuse_end = Some(Instant::now() + Duration::from_millis(remain_ms));
            }
            MorphEvent::ReuseReset { cid } if cid == me || cid == 0 => self.morph.reuse_end = None,
            MorphEvent::UseItemResult { .. } | MorphEvent::ReuseTime { .. } | MorphEvent::ReuseReset { .. } => {}
        }
    }

    /// À chaque image : CID et titre d'ArchLord du joueur, clics des icônes, compte à rebours.
    pub(super) fn morph_frame(&mut self) {
        let me = self.morph_me();
        self.morph.me = me;
        self.morph.archlord = self.archlord_is_me();
        for click in std::mem::take(&mut self.morph.clicks) {
            match click {
                Click::Transform if self.morph.transformed(me) => {
                    if let Some(net) = &self.net {
                        net.send(Command::Morph(MorphCommand::CancelTransform));
                    }
                }
                // L'effet de la compétence d'évolution (op 4) doit avoir été reçu, sinon rien n'est envoyé.
                Click::Evolution if self.morph.evolved(me) => {
                    let id = self.morph.evolution_effect.and_then(|tid| self.skills_id_of(tid));
                    match (id, &self.net) {
                        (Some(id), Some(net)) => net.send(Command::Skill(SkillCommand::CancelCast { id })),
                        (None, _) => log::info!("évolution : aucun effet de compétence d'évolution actif, annulation non envoyée"),
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        if let Some(c) = self.morph.countdown {
            let active = self.morph.transformed(me) || self.morph.evolved(me);
            let (next, notice) = c.tick(Instant::now(), active);
            self.morph.countdown = next;
            if let Some(n) = notice {
                self.morph_notice(n);
            }
        }
    }

    /// Lancement d'une compétence permis : pas métamorphosé, sauf l'ArchLord (`FUN_14069a0c0`, refus sans message).
    pub(super) fn morph_allows_skill(&self, tid: i32) -> bool {
        let ok = !self.morph.transformed(self.morph_me()) || self.archlord_is_me();
        if !ok {
            log::info!("compétence {tid} : refusée métamorphosé");
        }
        ok
    }

    /// Compétence d'évolution : lancée sur soi (`target` 100 de SKILL_SPEC).
    pub(super) fn morph_is_evolution_skill(&mut self, tid: i32) -> bool {
        let assets = self.assets.clone();
        self.morph.data(&assets).evolution_skills.contains(&tid)
    }

    /// Dialogue des PNJ permis : pas métamorphosé, sauf l'ArchLord (`FUN_140574e80`, retour sans message).
    pub(super) fn morph_allows_npc(&self) -> bool {
        !self.morph.transformed(self.morph_me()) || self.archlord_is_me()
    }

    /// Joueur métamorphosé ou évolué (coiffeur : `CustomizeNeedRestoreTransform`, `FUN_1406bd4b0`).
    pub(super) fn morph_changed_form(&self) -> bool {
        let me = self.morph_me();
        self.morph.transformed(me) || self.morph.evolved(me)
    }

    /// Personnage métamorphosé (`+0x600`) : son équipement n'est pas redessiné et sa tête n'est pas posée
    /// (`FUN_1407523a0`, `FUN_140754c60`, `FUN_1404b48d0`) ; tout est rechargé à l'op 12.
    pub(super) fn morph_frozen_model(&self, cid: i32) -> bool {
        self.morph.transformed(cid)
    }

    /// TID qui donne la race « d'origine » (`FUN_14017fbf0`) : celui d'avant la métamorphose s'il est connu, sinon
    /// `current`.
    pub(super) fn morph_origin_tid(&self, cid: i32, current: i32) -> i32 {
        match self.morph.states.get(&cid) {
            Some(MorphState { transformed: true, origin: Some((tid, _)), .. }) if *tid != 0 => *tid,
            _ => current,
        }
    }

    /// Type (`CharacterDataTable.txt`) de la forme d'un personnage métamorphosé en vue, sinon `None`.
    fn morph_form_type(&mut self, cid: i32) -> Option<u32> {
        if !self.morph.transformed(cid) {
            return None;
        }
        let (tid, _) = self.morph_current(cid)?;
        Some(self.combat_char_type(tid))
    }

    /// Attaque de `target` permise (`FUN_1402d8e80`) : ni le joueur ni la cible dans une forme de `Type` & 8.
    pub(super) fn morph_allows_attack(&mut self, target: i32) -> bool {
        let me = self.morph_me();
        let (mine, theirs) = (self.morph_form_type(me), self.morph_form_type(target));
        let ok = attack_permitted(mine, theirs);
        if !ok {
            log::info!("attaque de {target} : refusée (forme animale, FUN_1402d8e80)");
        }
        ok
    }

    /// Compétence `tid` sur `target` permise (`FUN_1402f8e20`) : une compétence sur un ennemi ne vise pas une forme
    /// ni attaquable ni ciblable.
    pub(super) fn morph_allows_skill_target(&mut self, tid: i32, target: i32) -> bool {
        if target == self.morph_me() {
            return true;
        }
        let assets = self.assets.clone();
        let enemy = self.morph.data(&assets).enemy_skills.contains(&tid);
        let form = self.morph_form_type(target);
        let ok = skill_target_permitted(enemy, form);
        if !ok {
            log::info!("compétence {tid} sur {target} : cible métamorphosée non ciblable (FUN_1402f8e20)");
        }
        ok
    }

    /// Menu d'échange privé actif avec `target` (`FUN_14053e080`, « IsActivePrivateTradeMemu ») : entrée « Trade »
    /// de `UI_TargetCharacterMenu` (voir `app_trade.rs`).
    pub(super) fn morph_allows_private_trade(&self, target: i32) -> bool {
        let name = self.entities.iter().find(|e| e.cid == target).map(|e| e.name.as_str()).unwrap_or_default();
        let me = (self.morph.transformed(self.morph_me()), self.archlord_is_me());
        private_trade_active(me, (self.morph.transformed(target), !name.is_empty() && self.archlord.is(name)))
    }

    /// `/porter` (`equip`) ou `/ranger` refusé sous métamorphose ou évolution (même contrôle que [`item_requests`]).
    pub(super) fn morph_refuses_equip(&mut self, item: &ItemRecord, equip: bool) -> bool {
        let assets = self.assets.clone();
        let (changed_form, factors) = (self.morph_changed_form(), char_factors(&self.hud));
        let flags = self.auction.item_flags.get(&item.id).copied().unwrap_or(0);
        let row = self.morph.data(&assets).item(item.template_id);
        morph_equip_check(&row, changed_form, factors, flags, equip) == Some(false)
    }

    /// Op 2 d'AGPMITEM sur un objet du joueur ignorée sous métamorphose ou évolution (`FUN_1401b4d00`).
    pub(super) fn morph_ignores_item(&mut self, item: &ItemRecord) -> bool {
        if item.operation != 2 || !self.morph_changed_form() {
            return false;
        }
        let assets = self.assets.clone();
        let current = self.items.get(&item.id);
        let tid = item.template_id.or(current.and_then(|i| i.template_id));
        let flags = self.auction.item_flags.get(&item.id).copied().unwrap_or(0);
        let factors = char_factors(&self.hud);
        let current = current.and_then(|i| i.status);
        let row = self.morph.data(&assets).item(tid);
        let ignored = ignores_update(&row, true, factors, flags, current, item.status);
        if ignored {
            log::info!("objet {} : déplacement {:?} → {:?} ignoré sous la forme courante (FUN_1401b4d00)", item.id, current, item.status);
        }
        ignored
    }

    /// Objet de la barre de raccourcis refusé (mêmes règles que l'inventaire, potions 346 à 348 comprises) : rend
    /// `true` après avoir affiché le message.
    pub(super) fn morph_refuses_item(&mut self, tid: i32) -> bool {
        let assets = self.assets.clone();
        let me = self.morph_me();
        let (transformed, archlord, race_class) = (self.morph.transformed(me), self.archlord_is_me(), race_class(&self.hud));
        let row = self.morph.data(&assets).item(Some(tid));
        let Some(refusal) = refused_item(row, transformed, archlord, race_class, true) else { return false };
        let text = match refusal {
            Ok(id) => self.morph.data(&assets).sysstr.get(&id).cloned().unwrap_or_default(),
            Err(key) => crate::hud::ui_message(&mut self.hud, &assets, key),
        };
        self.morph_message(&text);
        true
    }

    /// Recharge commune d'une potion de métamorphose `tid` pour la barre : part restante et secondes.
    pub(super) fn morph_item_cooldown(&mut self, tid: i32) -> Option<(f32, u32)> {
        let assets = self.assets.clone();
        let row = self.morph.data(&assets).item(Some(tid));
        if row.item_type != 1 || row.use_type != USE_TRANSFORM {
            return None;
        }
        let left = self.morph.reuse_end?.checked_duration_since(Instant::now())?;
        let total = Duration::from_millis(archrust_netclient::morph::TRANSFORM_REUSE_MS);
        Some(((left.as_secs_f32() / total.as_secs_f32()).min(1.0), left.as_secs_f32().ceil() as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: Head = Head { face: 3, hair: 5 };

    #[test]
    fn metamorphose_comme_archonia() {
        let mut s = MorphState::default();
        // Gabarit inconnu : ignoré.
        assert_eq!(s.transform(96, HEAD, 272, false), None);
        assert!(!s.transformed);
        // Origine sauvée à la première forme seulement.
        assert_eq!(s.transform(96, HEAD, 272, true), Some(272));
        assert_eq!(s.transform(272, Head::default(), 456, true), Some(456));
        assert_eq!(s.origin, Some((96, HEAD)));
        // Même TID : rien, reste métamorphosé (origine connue).
        assert_eq!(s.transform(456, Head::default(), 456, true), None);
        assert!(s.transformed);
        // Fin : TID, visage et coiffure d'origine, puis oubliés.
        assert_eq!(s.restore(456), Some((96, HEAD)));
        assert!(!s.transformed && s.origin.is_none());
        // Pas métamorphosé : l'op 12 ne fait rien.
        assert_eq!(s.restore(96), None);
        // Ajouté métamorphosé par l'op 0 (bit 0) : pas d'origine, l'op 12 lève seulement l'état.
        let mut s = MorphState { transformed: true, ..Default::default() };
        assert_eq!(s.restore(272), None);
        assert!(!s.transformed);
    }

    #[test]
    fn evolution_comme_archonia() {
        let mut s = MorphState::default();
        assert_eq!(s.restore_evolution(1724, 1724, true), None, "non évolué");
        assert_eq!(s.evolve(1724, 0, true), None, "TID nul");
        assert_eq!(s.evolve(1724, 1722, false), None, "gabarit inconnu");
        assert_eq!(s.evolve(1724, 1722, true), Some(1722));
        assert_eq!(s.evolve(1722, 1723, true), None, "déjà évolué : retour au Scion d'abord");
        assert_eq!(s.restore_evolution(1722, 1722, true), None, "même TID");
        assert_eq!(s.restore_evolution(1722, 1724, true), Some(1724));
        assert!(!s.evolved);
    }

    #[test]
    fn compte_a_rebours() {
        let t0 = Instant::now();
        let (c, n) = Countdown::start(t0, 240_000);
        assert_eq!(n, Notice::Remain(4));
        // Rien avant la minute ; pas d'avis si le joueur n'est plus métamorphosé.
        assert_eq!(c.tick(t0 + Duration::from_secs(30), true).1, None);
        assert_eq!(c.tick(t0 + Duration::from_secs(90), false), (Some(c), None));
        let (c2, n) = c.tick(t0 + Duration::from_secs(60), true);
        assert_eq!(n, Some(Notice::Remain(3)));
        let (c3, n) = c2.unwrap().tick(t0 + Duration::from_secs(240), true);
        assert_eq!((c3, n), (None, Some(Notice::Restore)));
        // Durée 0 (objet 3909) : fin aussitôt.
        let (c, n) = Countdown::start(t0, 0);
        assert_eq!(n, Notice::Remain(0));
        assert_eq!(c.tick(t0, true), (None, Some(Notice::Restore)));
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : potions, compétences d'évolution, gabarits, table
    /// d'évolution des objets, textures des icônes.
    #[test]
    fn donnees_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let d = load_data(&assets);
        let spider = d.item(Some(406));
        assert_eq!((spider.item_type, spider.use_type, spider.duration_ms), (1, USE_TRANSFORM, 240_000));
        assert_eq!(d.item(Some(198)).use_type, USE_TELEPORT_SCROLL);
        assert_eq!(d.item(Some(4876)).use_type, USE_SKILL_INIT);
        for tid in [1189, 1192, 1195] {
            assert!(d.evolution_skills.contains(&tid), "compétence d'évolution {tid}");
        }
        assert_eq!(d.evolution_skills.len(), 3);
        // Mutation (339) : compétence sur un ennemi ; restrictions d'équipement (Bastard Sword : races 1 et 2,
        // chevalier, niveau 23) ; monture (White Horse) hors contrôle.
        assert!(d.enemy_skills.contains(&339), "Mutation");
        let sword = d.item(Some(2));
        assert_eq!((sword.kind, sword.level, sword.race_mask, sword.class_mask), (2, 23, 0b11, 0b1));
        assert_eq!(d.item(Some(3002)).kind, KIND_MOUNT);
        assert!(d.sysstr.get(&665).is_some_and(|t| !t.is_empty()), "sysstr 665 (« Cancel Metamorphosis »)");
        assert_eq!(crate::scene::template_race_class(&assets, 1722), Some((4, 1)));
        assert_eq!(crate::scene::template_race_class(&assets, 1724), Some((4, 3)));
        assert_eq!(crate::scene::template_race_class(&assets, 1732), Some((4, 4)));
        let evo = crate::scene::parse_evolution_table(&super::super::npc_glue::client_text_in(&assets, "ini", "itemevolutiontable.txt"));
        assert!(evo.len() > 800, "{} lignes", evo.len());
        assert_eq!((crate::scene::evolution_item(&evo, 1, 7829), crate::scene::evolution_item(&evo, 2, 7829)), (6661, 6686));
        assert_eq!(crate::scene::evolution_item(&evo, 4, 7831), 8276);
        let mut hud = HudState::default();
        for key in ["Transform_Remain_Notice", "Transform_Restore_Notice", "Item_TryUseTelScrollOnTransform", "IsEvolutionNotUseInitSkillItem", "UseItemCannotTransformPotion"] {
            assert!(!crate::hud::ui_message(&mut hud, &assets, key).is_empty(), "{key}");
        }
        for t in [TEXTURE_TRANSFORM, TEXTURE_EVOLUTION] {
            assert!(assets.image("texture/ui", &t.to_ascii_lowercase()).is_ok(), "{t}");
        }
    }

    #[test]
    fn objets_refuses() {
        let row = |use_type, sub_type| ItemRow { item_type: 1, use_type, sub_type, ..Default::default() };
        // Passeport métamorphosé, sauf l'ArchLord.
        assert_eq!(refused_item(row(4, 0), true, false, None, false), Some(Err("Item_TryUseTelScrollOnTransform")));
        assert_eq!(refused_item(row(4, 0), true, true, None, false), None);
        assert_eq!(refused_item(row(4, 0), false, false, None, false), None);
        // Objet d'oubli : DragonScion d'une autre classe que Scion.
        assert_eq!(refused_item(row(27, 0), false, false, Some((4, 1)), false), Some(Err("IsEvolutionNotUseInitSkillItem")));
        assert_eq!(refused_item(row(27, 0), false, false, Some((4, 3)), false), None);
        assert_eq!(refused_item(row(27, 0), false, false, Some((1, 1)), false), None);
        // Potions de la barre : 346 même pour l'ArchLord, 347 / 348 sauf l'ArchLord.
        assert_eq!(refused_item(row(7, 0), true, true, None, true), Some(Ok(346)));
        assert_eq!(refused_item(row(7, 0), true, true, None, false), None, "inventaire : voir app_ride.rs");
        assert_eq!(refused_item(row(12, 2), true, false, None, true), Some(Ok(348)));
        assert_eq!(refused_item(row(12, 1), true, true, None, true), None);
    }

    /// Arme de Scion (race 4, classes 1 à 4, niveau 40) et armure d'humain chevalier.
    fn scion_weapon() -> ItemRow {
        ItemRow { kind: 2, level: 40, race_mask: restriction_bit(4), class_mask: [1, 2, 3, 4].into_iter().fold(0, |m, c| m | restriction_bit(c)), ..Default::default() }
    }

    #[test]
    fn controle_de_l_equipement_sous_la_forme() {
        let knight_armor = ItemRow { kind: 1, level: 10, race_mask: restriction_bit(1) | restriction_bit(2), class_mask: restriction_bit(1), ..Default::default() };
        let human_knight = CharFactors { race: 1, gender: 1, class: 1, level: 20 };
        // Hors métamorphose et évolution : pas de contrôle ajouté (le serveur garde le sien).
        assert_eq!(morph_equip_check(&knight_armor, false, CharFactors { race: 3, ..human_knight }, 0, true), None);
        // Métamorphosé : facteurs d'origine (humain chevalier) → permis, niveau compris.
        assert_eq!(morph_equip_check(&knight_armor, true, human_knight, 0, true), Some(true));
        assert_eq!(morph_equip_check(&knight_armor, true, CharFactors { level: 5, ..human_knight }, 0, true), Some(false), "niveau à l'équipement");
        assert_eq!(morph_equip_check(&knight_armor, true, CharFactors { level: 5, ..human_knight }, 0, false), Some(true), "pas au retrait");
        // Race, classe, sexe.
        assert_eq!(morph_equip_check(&knight_armor, true, CharFactors { race: 3, ..human_knight }, 0, false), Some(false));
        assert_eq!(morph_equip_check(&knight_armor, true, CharFactors { class: 2, ..human_knight }, 0, false), Some(false));
        let female = ItemRow { gender: 2, ..knight_armor };
        assert_eq!(morph_equip_check(&female, true, human_knight, 0, false), Some(false));
        // Facteurs inconnus (−1 → bit 30) : refusé si l'objet a une restriction.
        assert_eq!(morph_equip_check(&knight_armor, true, CharFactors { race: -1, gender: -1, class: -1, level: -1 }, 0, false), Some(false));
        assert_eq!(morph_equip_check(&ItemRow::default(), true, CharFactors { race: -1, gender: -1, class: -1, level: -1 }, 0, false), Some(true));
        // DISARMAMENT.
        assert_eq!(morph_equip_check(&ItemRow::default(), true, human_knight, 0x1000, false), Some(false));
        // Monture (genre 0x20) : jamais refusée par ce contrôle.
        let horse = ItemRow { kind: KIND_MOUNT, race_mask: restriction_bit(3), ..Default::default() };
        assert_eq!(morph_equip_check(&horse, true, human_knight, 0x1000, true), None);
        // Évolution (facteurs de la forme : Slayer race 4 classe 1) : l'arme de Scion reste permise, l'armure
        // d'humain ne peut être ni portée ni retirée.
        let slayer = CharFactors { race: 4, gender: 1, class: 1, level: 50 };
        assert_eq!(morph_equip_check(&scion_weapon(), true, slayer, 0, true), Some(true));
        assert_eq!(morph_equip_check(&knight_armor, true, slayer, 0, false), Some(false));
    }

    #[test]
    fn deplacements_du_serveur_ignores() {
        let slayer = CharFactors { race: 4, gender: 1, class: 1, level: 30 };
        let w = scion_weapon();
        // Porté → sac, sous-inventaire, inventaire cash : contrôle sans niveau (niveau 30 < 40 sans effet).
        for to in [STATUS_INVENTORY, STATUS_SUB_INVENTORY, STATUS_CASH_INVENTORY] {
            assert!(!ignores_update(&w, true, slayer, 0, Some(STATUS_EQUIP), Some(to)));
            assert!(ignores_update(&w, true, CharFactors { race: 1, ..slayer }, 0, Some(STATUS_EQUIP), Some(to)));
        }
        // Sac → porté : contrôle complet, niveau compris.
        assert!(ignores_update(&w, true, slayer, 0, Some(STATUS_INVENTORY), Some(STATUS_EQUIP)));
        assert!(!ignores_update(&w, true, CharFactors { level: 40, ..slayer }, 0, Some(STATUS_INVENTORY), Some(STATUS_EQUIP)));
        // Autres déplacements (sac → sac, porté → banque) et hors forme : appliqués.
        assert!(!ignores_update(&w, true, CharFactors { race: 1, ..slayer }, 0, Some(STATUS_INVENTORY), Some(STATUS_INVENTORY)));
        assert!(!ignores_update(&w, true, CharFactors { race: 1, ..slayer }, 0, Some(STATUS_EQUIP), Some(100)));
        assert!(!ignores_update(&w, false, CharFactors { race: 1, ..slayer }, 0, Some(STATUS_EQUIP), Some(STATUS_INVENTORY)));
    }

    #[test]
    fn formes_animales_et_echange_prive() {
        let (monster, animal, big_rage) = (Some(0x30004), Some(0x8), Some(0x30004));
        // Attaque : ni soi ni la cible en animal (Type & 8) ; une forme de monstre ne change rien.
        assert!(attack_permitted(None, None));
        assert!(attack_permitted(monster, None) && attack_permitted(None, big_rage));
        assert!(!attack_permitted(animal, None), "soi en animal : n'attaque personne");
        assert!(!attack_permitted(None, animal), "cible en animal : inattaquable");
        // Compétence sur un ennemi : forme attaquable et ciblable exigée ; sur soi ou un allié, rien.
        assert!(skill_target_permitted(true, None) && skill_target_permitted(true, monster));
        assert!(!skill_target_permitted(true, animal));
        assert!(!skill_target_permitted(true, Some(0x10004)), "sans 0x20000");
        assert!(skill_target_permitted(false, animal));
        // Échange privé : métamorphosé seulement si ArchLord, pour soi comme pour la cible.
        assert!(private_trade_active((false, false), (false, false)));
        assert!(!private_trade_active((true, false), (false, false)));
        assert!(!private_trade_active((false, false), (true, false)));
        assert!(private_trade_active((true, true), (false, false)));
        assert!(private_trade_active((false, false), (true, true)));
    }
}
