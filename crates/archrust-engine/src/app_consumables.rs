//! Consommables, résurrection par objet et oubli des compétences côté moteur. Protocole :
//! `archrust_netclient::consumables` ; fenêtres : `hud_consumables.rs`.
//!
//! Comportement relevé dans Archonia (AgcmUIItem, AgcmUICharacter, AgcmUISkill2 ; sha256 e9c7609b…) :
//! * **Orbe de résurrection** (usage 14) : bouton « Use revival item » de `UI_Resurrection` (`Item_UseReverseOrb`,
//!   `FUN_14053f910`) qui utilise l'orbe trouvé par `FUN_1401c7660` (AGPMITEM op 3) : premier orbe convenable de
//!   l'inventaire Cash (statut 51), sinon le dernier de l'inventaire (2) puis du sous-inventaire (13), dans l'ordre des
//!   cases ; convenable = colonne `Level` sans bornes, ou niveau du joueur ≥ minimum et ≤ maximum (« 1;30 ») ;
//! * le bouton est actif (`IsActiveReverseOrb`, `FUN_14053eac0`) si le joueur est mort et qu'un orbe est trouvé ; en
//!   région de sûreté 2 (bits 2-3 du `Type` de RegionTemplate.ini) l'orbe doit être un objet Cash (`+0x254`) ; jamais
//!   pour l'ArchLord (`FUN_1402944c0` compare le nom à celui de l'ArchLord) ni dans une région sans le bit 0x100 ;
//! * une utilisation depuis l'inventaire ou la barre (`FUN_14053d0b0`, usage 14) est refusée sans message si le
//!   joueur est vivant, ou si l'orbe n'est pas Cash en région de sûreté 2 (D : `FUN_14017b410`, lu comme la zone de
//!   combat des passeports) ;
//! * op 4 (`FUN_14053f540`, pour soi) : orbe → `Item_UseSuccessReverseOrb` (« {nom} is regenerated ») ; usage 12 dont
//!   la compétence (`Use skill id` au `Use skill level`) a `movement` > 0 → `Item_UseAddMovementSkill`, sinon
//!   `atk_speed` > 0 → `Item_UseAddAtkSpeedSkill` ; objet à `Use Interval` ou d'usage 12 (orbe exclu) : recharge
//!   affichée sur la barre = `Use Interval` + `Apply Effect Count` × `Apply Effect Time` (`FUN_14029cf00`, la durée
//!   vient du modèle, pas du champ 18) ;
//! * op 5 (`FUN_14053f740`) : orbe → `Item_UseFailReverseOrb` (« Regeneration has failed ») ;
//! * op 18 : recharge de l'orbe, 15 min constantes du client ; op 27 code 3 (`ReverseOrbError`) : boîte « You cannot
//!   use the resurrection item. » (`UI_Resurrection` reste ouverte tant que le joueur est mort) ;
//! * **Gem of origin** (usage 27, `FUN_14053d0b0`) : refus sans message si mort ; Dragon Scion d'une autre classe que
//!   Scion → `IsEvolutionNotUseInitSkillItem` (laissé à `app_morph.rs`) ; aucune compétence payante apprise
//!   (`FUN_1402ba630`) → `SkillInitNoOwnSkill` dans la discussion ; sinon boîte modale `SkillInitConfirm`, OK → op 3 ;
//! * **Maître des compétences** (`Skill_TreeInitialize`, `FUN_1405dcb30`) : refus pendant l'évolution
//!   (`IsEvolutionNotUseInitSkillItem`), sinon `UI_SkillInitializeConfirm` (`Skill_ConfirmInitialize`) ; OK
//!   (`Skill_InitializeOK`, `FUN_1405dcbf0`) envoie 0x3B op 8 **seulement si** la chaîne « skill init » du personnage
//!   (AGPMCHARACTER champ 19) est vide ; réponse op 9 (`FUN_1405dcc40`) : 0 boîte `Skill_SkillInitialize_Success`,
//!   1 boîte `Skill_SkillInitialize_Failed`, 2 boîte `sysstr` 748.
//!
//! Choix d'ArchRust : aucun contrôle d'`UI_1024X768.ini` n'appelle `Skill_TreeInitialize` ; l'oubli chez le maître
//! se demande par `/oublier` (`/skillinit`). Écarts : le drapeau `+0x218 & 8` qui fait préférer un orbe de
//! l'inventaire, le bit 0x2000 de la compétence exclu du compte de `FUN_1402ba630` et la recharge des cases
//! précises `+0x6c40` / `+0x6c48` (ici : toutes les cases du même TID) ne sont pas identifiés.
//! `UI_SkillInitializeFail` (`Skill_FailSkillInitialize`) n'est ouverte par aucun chemin relevé : non dessinée.

use super::App;
use crate::assets::Assets;
use crate::hud::{HudState, ItemUi};
use crate::net::NetLink;
use crate::ui::UiState;
use archrust_netclient::consumables::{init_result, ConsumableCommand, ConsumableEvent, REVERSE_ORB_REUSE_MS};
use archrust_netclient::items::ItemCommand;
use archrust_netclient::morph::{use_result, MorphEvent};
use archrust_netclient::world::{factor_part, ItemRecord};
use archrust_netclient::{Command, WorldEvent};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Usages (`Type` d'itemdatatable, objets d'`ItemType` 1).
const USE_SKILL_SCROLL: i64 = 12;
const USE_REVERSE_ORB: i64 = 14;
/// Rappel de groupe (`RECALL_PARTY` : Summoning Orb, Bead of Summoning…).
const USE_RECALL_PARTY: i64 = 15;
const USE_SKILL_INIT: i64 = 27;
/// Statuts AgpmItem parcourus par `FUN_1401c7660`.
const STATUS_INVENTORY: i8 = 2;
const STATUS_SUB_INVENTORY: i8 = 13;
const STATUS_CASH_INVENTORY: i8 = 51;
/// Bit « résurrection permise » du `Type` de RegionTemplate.ini (`+0x7d & 1`).
const REGION_RESURRECTION: i64 = 0x100;
/// Sûreté (bits 2-3 du `Type`) où seul un orbe Cash sert.
const SAFETY_ORB_CASH_ONLY: i64 = 2;
/// « The Skill Reset failed because you do not have enough room to store your Refund. »
const SYSSTR_NO_ROOM: u32 = 748;

/// Ligne d'itemdatatable.txt utile ici.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ItemRow {
    pub item_type: i64,
    pub use_type: i64,
    /// Colonne `Cash` (valeur − 100 si ≥ 100, `+0x254`) non nulle.
    pub cash: bool,
    /// Colonne `Level` : « min;max » ou « min » (maximum `+0x1f0`, 0 : sans maximum).
    pub min_level: i32,
    pub max_level: i32,
    /// `Use Interval` (`+0x3d0`), `Apply Effect Count` (`+0x458`), `Apply Effect Time` (`+0x45c`), en ms.
    pub use_interval_ms: u64,
    pub effect_count: u64,
    pub effect_time_ms: u64,
    /// Premier `Use skill id` (`+0x460`) et `Use skill level` (`+0x488`).
    pub skill_tid: i32,
    pub skill_level: i32,
}

impl ItemRow {
    /// Niveau du joueur admis par l'orbe (`FUN_1401c7660`) ; −1 : inconnu.
    pub fn level_ok(&self, level: i32) -> bool {
        (self.min_level < 1 && self.max_level == 0) || (self.min_level <= level && (self.max_level == 0 || level <= self.max_level))
    }

    /// Recharge affichée sur la barre après l'op 4 (`FUN_14053f540`) : objet à `Use Interval` ou d'usage 12, l'orbe
    /// excepté (la fonction s'arrête à son message).
    pub fn shown_reuse_ms(&self) -> Option<u64> {
        if self.item_type != 1 || self.use_type == USE_REVERSE_ORB || (self.use_interval_ms == 0 && self.use_type != USE_SKILL_SCROLL) {
            return None;
        }
        Some(self.use_interval_ms + self.effect_count * self.effect_time_ms).filter(|ms| *ms > 0)
    }
}

/// « 1;30 » → (1, 30), « 20 » → (20, 0).
pub fn parse_level(text: &str) -> (i32, i32) {
    let mut it = text.split(';').map(|v| v.trim().parse::<f64>().map(|v| v as i32).unwrap_or(0));
    let min = it.next().unwrap_or(0);
    (min, it.next().unwrap_or(0))
}

/// Objets d'itemdatatable.txt (en-tête ASCII, une ligne par TID).
pub fn parse_items(text: &str) -> HashMap<i32, ItemRow> {
    let mut out = HashMap::new();
    let mut lines = text.lines();
    let Some(header) = lines.next() else { return out };
    let cols: Vec<&str> = header.split('\t').map(str::trim).collect();
    let col = |name: &str| cols.iter().position(|c| c.eq_ignore_ascii_case(name));
    let (tid, item_type, use_type, level, cash) = (col("TID"), col("ItemType"), col("Type"), col("Level"), col("Cash"));
    let (interval, count, time, skill, skill_level) = (col("Use Interval"), col("Apply Effect Count"), col("Apply Effect Time"), col("Use skill id"), col("Use skill level"));
    for line in lines {
        let cells: Vec<&str> = line.split('\t').collect();
        let cell = |c: Option<usize>| c.and_then(|i| cells.get(i)).map(|v| v.trim()).unwrap_or("");
        // Listes « a;b » : la première valeur.
        let num = |c: Option<usize>| cell(c).split(';').next().and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(0.0) as i64;
        let t = num(tid) as i32;
        if t <= 0 {
            continue;
        }
        let c = num(cash);
        let (min_level, max_level) = parse_level(cell(level));
        out.insert(
            t,
            ItemRow {
                item_type: num(item_type),
                use_type: num(use_type),
                cash: (if c >= 100 { c - 100 } else { c }) != 0,
                min_level,
                max_level,
                use_interval_ms: num(interval).max(0) as u64,
                effect_count: num(count).max(0) as u64,
                effect_time_ms: num(time).max(0) as u64,
                skill_tid: num(skill) as i32,
                skill_level: num(skill_level) as i32,
            },
        );
    }
    out
}

/// Vitesses de déplacement et d'attaque par niveau d'une compétence (SKILL_CONST.txt, nom en première colonne de
/// la première ligne, `skill_level` ensuite). Les colonnes `movement` / `atk_speed` reviennent trois fois dans
/// l'en-tête : la deuxième est celle que remplissent les potions et charmes (`+0x2c7c` / `+0x30d8`, D).
pub fn parse_skill_speeds(text: &str) -> HashMap<String, Vec<(i32, i32)>> {
    let mut out: HashMap<String, Vec<(i32, i32)>> = HashMap::new();
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap_or("").split('\t').map(str::trim).collect();
    let second = |name: &str| header.iter().enumerate().filter(|(_, h)| h.eq_ignore_ascii_case(name)).map(|(i, _)| i).nth(1);
    let (movement, atk_speed, level) = (second("movement"), second("atk_speed"), header.iter().position(|h| h.eq_ignore_ascii_case("skill_level")));
    let mut current: Option<String> = None;
    for line in lines {
        let f: Vec<&str> = line.split('\t').collect();
        let num = |c: Option<usize>| c.and_then(|i| f.get(i)).and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(0.0) as i32;
        let name = f.first().map(|s| s.trim()).unwrap_or("");
        if !name.is_empty() {
            current = Some(name.to_lowercase());
        }
        let (Some(n), lv) = (&current, num(level)) else { continue };
        if lv < 1 {
            continue;
        }
        let levels = out.entry(n.clone()).or_default();
        if levels.len() < lv as usize {
            levels.resize(lv as usize, (0, 0));
        }
        levels[lv as usize - 1] = (num(movement), num(atk_speed));
    }
    out
}

/// Orbe que le bouton utiliserait (`FUN_1401c7660`) : premier orbe convenable de l'inventaire Cash, sinon le dernier
/// convenable de l'inventaire puis du sous-inventaire, dans l'ordre des cases.
pub fn find_orb(items: &HashMap<i32, ItemRecord>, rows: &HashMap<i32, ItemRow>, level: i32) -> Option<i32> {
    let usable = |i: &&ItemRecord| i.template_id.and_then(|t| rows.get(&t)).is_some_and(|r| r.item_type == 1 && r.use_type == USE_REVERSE_ORB && r.level_ok(level));
    let ordered = |status: i8| {
        let mut v: Vec<&ItemRecord> = items.values().filter(|i| i.status == Some(status)).filter(usable).collect();
        v.sort_by_key(|i| (i.grid.unwrap_or([i16::MAX; 3]), i.id));
        v
    };
    if let Some(i) = ordered(STATUS_CASH_INVENTORY).first() {
        return Some(i.id);
    }
    ordered(STATUS_INVENTORY).into_iter().chain(ordered(STATUS_SUB_INVENTORY)).last().map(|i| i.id)
}

/// `IsActiveReverseOrb` (`FUN_14053eac0`) : `region_type` = `Type` de la région du joueur (inconnue : sans contrôle).
pub fn orb_active(dead: bool, orb: Option<ItemRow>, region_type: Option<i64>, archlord: bool) -> bool {
    let Some(orb) = orb.filter(|_| dead) else { return false };
    let safety_ok = region_type.is_none_or(|t| (t >> 2) & 3 != SAFETY_ORB_CASH_ONLY || orb.cash);
    let region_ok = region_type.is_none_or(|t| t & REGION_RESURRECTION != 0);
    safety_ok && region_ok && !archlord
}

/// Décision du client avant l'op 3 (`FUN_14053d0b0`, usages 14 et 27).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseCheck {
    /// Envoyée telle quelle (ou laissée aux autres contrôles).
    Send,
    /// Refusée sans message.
    Silent,
    /// Gem of origin sans compétence payante : `SkillInitNoOwnSkill`.
    NoOwnSkill,
    /// Gem of origin : boîte `SkillInitConfirm` avant l'envoi.
    Confirm,
}

/// Contrôle d'une utilisation : `evolution_lock` = Dragon Scion d'une autre classe que Scion (refus laissé à
/// `app_morph.rs`, qui l'affiche) ; `region_type` comme [`orb_active`].
pub fn use_check(row: ItemRow, dead: bool, evolution_lock: bool, paid_skills: usize, region_type: Option<i64>) -> UseCheck {
    if row.item_type != 1 {
        return UseCheck::Send;
    }
    match row.use_type {
        USE_REVERSE_ORB if !dead => UseCheck::Silent,
        USE_REVERSE_ORB if !row.cash && region_type.is_some_and(|t| (t >> 2) & 3 == SAFETY_ORB_CASH_ONLY) => UseCheck::Silent,
        USE_SKILL_INIT if dead => UseCheck::Silent,
        USE_SKILL_INIT if evolution_lock => UseCheck::Send,
        USE_SKILL_INIT if paid_skills < 1 => UseCheck::NoOwnSkill,
        USE_SKILL_INIT => UseCheck::Confirm,
        _ => UseCheck::Send,
    }
}

/// Message de l'op 4 d'un objet d'usage 12 (`FUN_14053f540`) : clé de l'événement et pourcentage.
pub fn speed_message(row: ItemRow, speeds: Option<&Vec<(i32, i32)>>) -> Option<(&'static str, i32)> {
    if row.item_type != 1 || row.use_type != USE_SKILL_SCROLL {
        return None;
    }
    let (movement, atk) = *speeds?.get((row.skill_level.max(1) - 1) as usize)?;
    if movement > 0 {
        Some(("Item_UseAddMovementSkill", movement))
    } else if atk > 0 {
        Some(("Item_UseAddAtkSpeedSkill", atk))
    } else {
        None
    }
}

#[derive(Default)]
struct Data {
    items: HashMap<i32, ItemRow>,
    /// `Type` de RegionTemplate.ini par région.
    regions: HashMap<u16, i64>,
    /// Vitesses par niveau, par TID de compétence (SKILLTEMPLATE.ini → SKILL_CONST.txt).
    speeds: HashMap<i32, Vec<(i32, i32)>>,
}

fn load_data(assets: &Assets) -> Data {
    let items = parse_items(&super::npc_glue::client_text_in(assets, "ini", "itemdatatable.txt"));
    let mut regions = HashMap::new();
    if let Ok(t) = assets.table("ini", "RegionTemplate.ini") {
        // Clé `Type` (ou colonne d'un en-tête `[]` si le fichier en a un).
        let column = t.section("").and_then(|h| h.entries.iter().find(|(_, v)| v.eq_ignore_ascii_case("Type")).map(|(k, _)| k.clone())).unwrap_or_else(|| "Type".to_string());
        for s in &t.sections {
            let (Ok(region), Some(ty)) = (s.name.trim().parse::<u16>(), s.get(&column).and_then(|v| v.trim().parse::<i64>().ok())) else { continue };
            regions.insert(region, ty);
        }
    }
    let by_name = parse_skill_speeds(&super::npc_glue::client_text_in(assets, "ini", "SKILL_CONST.txt"));
    let mut speeds = HashMap::new();
    if let Ok(t) = assets.table("ini", "SKILLTEMPLATE.ini") {
        let name_col = t.section("").and_then(|h| h.entries.iter().find(|(_, v)| v.eq_ignore_ascii_case("Name")).map(|(k, _)| k.clone()));
        for s in &t.sections {
            let (Ok(tid), Some(name)) = (s.name.parse::<i32>(), name_col.as_deref().and_then(|c| s.get(c))) else { continue };
            if let Some(v) = by_name.get(&name.trim().to_lowercase()).filter(|v| v.iter().any(|(m, a)| *m > 0 || *a > 0)) {
                speeds.insert(tid, v.clone());
            }
        }
    }
    log::info!("consommables : {} objet(s), {} région(s), {} compétence(s) de vitesse", items.len(), regions.len(), speeds.len());
    Data { items, regions, speeds }
}

/// Joueur, relevé à chaque image pour la passe egui.
#[derive(Debug, Clone, Default)]
struct Me {
    dead: bool,
    level: i32,
    archlord: bool,
    region: Option<u16>,
    evolution_lock: bool,
    paid_skills: usize,
}

#[derive(Default)]
pub struct ConsumablesGlue {
    data: Option<Data>,
    me: Me,
    /// Chaîne « skill init » du joueur (AGPMCHARACTER champ 19).
    skill_init: String,
    /// Gem of origin en attente de `SkillInitConfirm` (IID).
    gem_confirm: Option<i32>,
    /// `UI_SkillInitializeConfirm` ouverte.
    master_confirm: bool,
    /// Boîte d'information (`Action=4:0`, `FUN_1405b3740`).
    info: Option<String>,
    /// Recharge affichée par TID (op 4) : fin et durée.
    reuse: HashMap<i32, (Instant, Duration)>,
    /// Fin de la recharge de l'orbe (op 18).
    orb_reuse_end: Option<Instant>,
}

impl ConsumablesGlue {
    fn data(&mut self, assets: &Assets) -> &Data {
        self.data.get_or_insert_with(|| load_data(assets))
    }

    fn row(&mut self, assets: &Assets, tid: Option<i32>) -> ItemRow {
        tid.and_then(|t| self.data(assets).items.get(&t).copied()).unwrap_or_default()
    }

    fn region_type(&mut self, assets: &Assets) -> Option<i64> {
        let region = self.me.region?;
        self.data(assets).regions.get(&region).copied()
    }

    /// Contrôle d'une utilisation de l'objet `iid` : `true` si elle est envoyée telle quelle.
    fn check(&mut self, iid: i32, items: &HashMap<i32, ItemRecord>, hud: &mut HudState, assets: &Assets, ui: &mut UiState) -> bool {
        let row = self.row(assets, items.get(&iid).and_then(|i| i.template_id));
        let region_type = self.region_type(assets);
        match use_check(row, self.me.dead, self.me.evolution_lock, self.me.paid_skills, region_type) {
            UseCheck::Send => true,
            UseCheck::Silent => {
                log::info!("objet {iid} : utilisation refusée par le client (FUN_14053d0b0)");
                false
            }
            UseCheck::NoOwnSkill => {
                // `FUN_140515040` : message SYSTEM de la discussion.
                let text = crate::hud::ui_message(hud, assets, "SkillInitNoOwnSkill");
                // En-tête « Chat_System_Header » d'uimessage.
                ui.push_chat(format!("{} {text}", crate::hud::ui_message(hud, assets, "Chat_System_Header")));
                false
            }
            UseCheck::Confirm => {
                self.gem_confirm = Some(iid);
                false
            }
        }
    }
}

fn send(net: Option<&NetLink>, cmd: Command) {
    if let Some(net) = net {
        net.send(cmd);
    }
}

/// Demandes de l'inventaire : refus et confirmation de l'orbe et de la Gem of origin, avant `app_morph.rs` (le
/// refus d'un mort passe avant celui de l'évolution, comme dans `FUN_14053d0b0`).
pub fn item_requests(glue: &mut ConsumablesGlue, items: &HashMap<i32, ItemRecord>, requests: &mut Vec<ItemUi>, hud: &mut HudState, assets: &Assets, ui: &mut UiState) {
    requests.retain(|r| match *r {
        ItemUi::Activate { iid, worn: false } => glue.check(iid, items, hud, assets, ui),
        _ => true,
    });
}

/// Dessin egui : bouton de l'orbe dans `UI_Resurrection`, `SkillInitConfirm`, `UI_SkillInitializeConfirm`, boîtes.
pub fn panel(ctx: &egui::Context, glue: &mut ConsumablesGlue, items: &HashMap<i32, ItemRecord>, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>) {
    if glue.me.dead {
        let level = glue.me.level;
        let orb = find_orb(items, &glue.data(assets).items, level);
        let row = orb.map(|iid| glue.row(assets, items.get(&iid).and_then(|i| i.template_id)));
        let region_type = glue.region_type(assets);
        let active = orb_active(true, row, region_type, glue.me.archlord);
        if crate::hud::draw_resurrection_orb(ctx, hud, assets, active) {
            if let Some(iid) = orb {
                send(net, Command::Item(ItemCommand::Use(iid)));
            }
        }
    }
    if let Some(iid) = glue.gem_confirm {
        let text = crate::hud::ui_message(hud, assets, "SkillInitConfirm");
        if let Some(answer) = crate::hud::confirm_box(ctx, hud, assets, "archrust-skill-init-gem", &text) {
            glue.gem_confirm = None;
            if answer && items.contains_key(&iid) {
                send(net, Command::Item(ItemCommand::Use(iid)));
            }
        }
    }
    if glue.master_confirm {
        let text = crate::hud::ui_message(hud, assets, "Skill_ConfirmInitialize");
        if let Some(answer) = crate::hud::draw_skill_init_confirm(ctx, hud, assets, &text) {
            glue.master_confirm = false;
            // `Skill_InitializeOK` : rien si la chaîne « skill init » est déjà posée (oubli déjà fait).
            if answer && glue.skill_init.is_empty() {
                send(net, Command::Consumable(ConsumableCommand::SkillInitialize));
            } else if answer {
                log::info!("oubli des compétences : chaîne « skill init » déjà posée ({:?}), rien n'est envoyé", glue.skill_init);
            }
        }
    }
    if let Some(text) = glue.info.clone() {
        if crate::hud::info_box(ctx, hud, assets, &text) {
            glue.info = None;
        }
    }
}

/// Race et classe courantes (`FACTOR_TYPE` 0 et 2).
fn race_class(hud: &HudState) -> Option<(i32, i32)> {
    let f = &hud.stats.factors;
    Some((*f.get(&(factor_part::TYPE, 0))?, *f.get(&(factor_part::TYPE, 2))?))
}

impl App {
    /// À chaque image : joueur pour la passe egui, confirmation du retour au village (voir `app_combat.rs`).
    pub(super) fn consumables_frame(&mut self) {
        let dead = self.combat.player_dead;
        let level = self.hud.stats.level.unwrap_or(-1);
        let paid_skills = self.skills_paid_count();
        self.consumables.me = Me {
            dead,
            level,
            archlord: self.archlord_is_me(),
            region: self.zone_region.as_ref().map(|z| z.region),
            evolution_lock: matches!(race_class(&self.hud), Some((4, class)) if class != 3),
            paid_skills,
        };
        // `FUN_1404f86a0` : niveau > 5 et hors guerre de siège de sa guilde (`+0x760` ≠ 2 : attache au sens inconnu,
        // supposée vraie).
        self.combat.town_confirm = dead && level > 5 && !self.siege_war_participant();
    }

    /// Objet de la barre de raccourcis : `true` si son utilisation est refusée ou attend une confirmation.
    pub(super) fn consumables_intercepts(&mut self, iid: i32) -> bool {
        let assets = self.assets.clone();
        !self.consumables.check(iid, &self.items, &mut self.hud, &assets, &mut self.ui)
    }

    /// Recharge d'un objet `tid` pour la barre : part restante et secondes.
    pub(super) fn consumables_item_cooldown(&mut self, tid: i32) -> Option<(f32, u32)> {
        let assets = self.assets.clone();
        let row = self.consumables.row(&assets, Some(tid));
        let now = Instant::now();
        let (end, total) = if row.item_type == 1 && row.use_type == USE_REVERSE_ORB {
            (self.consumables.orb_reuse_end?, Duration::from_millis(REVERSE_ORB_REUSE_MS))
        } else {
            *self.consumables.reuse.get(&tid)?
        };
        let left = end.checked_duration_since(now)?;
        Some(((left.as_secs_f32() / total.as_secs_f32().max(0.001)).min(1.0), left.as_secs_f32().ceil() as u32))
    }

    fn consumables_message(&mut self, text: &str) {
        if !text.trim().is_empty() {
            self.hud.push_system(text);
        }
    }

    /// Paquets qui touchent aux consommables sans leur être propres : connexion, objet utilisé (op 4), refus 3 de
    /// l'op 27.
    pub(super) fn consumables_observe(&mut self, ev: &WorldEvent) {
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        match ev {
            WorldEvent::Connected => {
                let data = self.consumables.data.take();
                self.consumables = ConsumablesGlue { data, ..Default::default() };
            }
            WorldEvent::ItemUsed { tid: Some(tid), user, .. } if user.is_none_or(|u| u == me || me == 0) => {
                let assets = self.assets.clone();
                let row = self.consumables.row(&assets, Some(*tid));
                if row.item_type == 1 && row.use_type == USE_REVERSE_ORB {
                    let name = self.player.as_ref().map(|p| p.name.clone()).unwrap_or_default();
                    let text = super::guild_glue::fill_braces(&super::portal_glue::ui_event_text(&assets, "Item_UseSuccessReverseOrb"), &name);
                    self.consumables_message(&text);
                    return;
                }
                // Orbe de rappel (rappel 0x1c d'AgpmItem → FUN_1405e5330) : `PartyUseRecallSuccess`.
                if row.item_type == 1 && row.use_type == USE_RECALL_PARTY {
                    let text = super::portal_glue::ui_event_text(&assets, "PartyUseRecallSuccess");
                    self.consumables_message(&text);
                }
                let speeds = self.consumables.data(&assets).speeds.get(&row.skill_tid).cloned();
                if let Some((key, percent)) = speed_message(row, speeds.as_ref()) {
                    let text = super::guild_glue::fill_braces(&super::portal_glue::ui_event_text(&assets, key), &percent.to_string());
                    self.consumables_message(&text);
                }
                if let Some(ms) = row.shown_reuse_ms() {
                    let total = Duration::from_millis(ms);
                    self.consumables.reuse.insert(*tid, (Instant::now() + total, total));
                }
            }
            WorldEvent::Morph(MorphEvent::UseItemResult { cid, code: use_result::REVERSE_ORB_ERROR }) if *cid == me || *cid == 0 => {
                let text = super::portal_glue::ui_event_text(&self.assets, "ReverseOrbError");
                self.consumables.info = Some(text).filter(|t| !t.is_empty());
            }
            _ => {}
        }
    }

    /// Annonces de `consumables`.
    pub(super) fn on_consumable_event(&mut self, ev: ConsumableEvent) {
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        let assets = self.assets.clone();
        match ev {
            ConsumableEvent::UseFailed { tid, cid } if cid == me || cid == 0 => {
                let row = self.consumables.row(&assets, Some(tid));
                if row.item_type == 1 && row.use_type == USE_REVERSE_ORB {
                    let text = super::portal_glue::ui_event_text(&assets, "Item_UseFailReverseOrb");
                    self.consumables_message(&text);
                }
            }
            ConsumableEvent::ReverseOrbReuse { cid, remain_ms } if cid == me || cid == 0 => {
                self.consumables.orb_reuse_end = Some(Instant::now() + Duration::from_millis(remain_ms.min(REVERSE_ORB_REUSE_MS)));
            }
            ConsumableEvent::SkillInit { cid, value } if cid == me || me == 0 => self.consumables.skill_init = value,
            ConsumableEvent::SkillInitResult { result } => {
                let text = match result {
                    init_result::SUCCESS => super::portal_glue::ui_event_text(&assets, "Skill_SkillInitialize_Success"),
                    init_result::FAILED => super::portal_glue::ui_event_text(&assets, "Skill_SkillInitialize_Failed"),
                    init_result::NO_ROOM => super::npc_glue::sysstr(&assets, SYSSTR_NO_ROOM).unwrap_or_default(),
                    _ => String::new(),
                };
                log::info!("oubli des compétences : résultat {result}");
                self.consumables.info = Some(text).filter(|t| !t.is_empty());
            }
            ConsumableEvent::UseFailed { .. } | ConsumableEvent::ReverseOrbReuse { .. } | ConsumableEvent::SkillInit { .. } => {}
        }
    }

    /// `/oublier` (`/skillinit`) : oubli des compétences chez le maître (`Skill_TreeInitialize`).
    pub(super) fn consumables_slash(&mut self, cmd: &str) -> bool {
        if !matches!(cmd, "/oublier" | "/skillinit") {
            return false;
        }
        if self.consumables.me.evolution_lock {
            self.consumables.info = Some(crate::hud::ui_message(&mut self.hud, &self.assets, "IsEvolutionNotUseInitSkillItem")).filter(|t| !t.is_empty());
        } else {
            self.consumables.master_confirm = true;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: i32, tid: i32, status: i8, grid: [i16; 3]) -> ItemRecord {
        ItemRecord { operation: 0, id, template_id: Some(tid), owner_cid: None, count: Some(1), status: Some(status), grid: Some(grid) }
    }

    #[test]
    fn colonnes_d_itemdatatable() {
        let text = "TID\tName\tLevel\tItemType\tType\tUse Interval\tApply Effect Count\tApply Effect Time\tUse skill id\tUse skill level\tCash\n\
                    842\tResurrection Orb\t\t1\t14\t90000\t\t\t\t\t1\n\
                    7387\tOrb for Novices\t1;30\t1\t14\t\t\t\t\t\t1\n\
                    1048\tPotion\t\t1\t12\t360000\t\t\t239\t1\t\n\
                    593\tSpeed\t\t1\t12\t\t3\t1000\t454;455\t1\t100\n";
        let rows = parse_items(text);
        assert_eq!(rows[&842].use_interval_ms, 90_000);
        assert!(rows[&842].cash && !rows[&593].cash, "Cash 100 → 0 (valeur − 100)");
        assert_eq!((rows[&7387].min_level, rows[&7387].max_level), (1, 30));
        assert_eq!((rows[&593].skill_tid, rows[&593].skill_level), (454, 1));
        // Recharge de la barre : `Use Interval` + compte × durée ; usage 12 même sans `Use Interval` ; pas l'orbe.
        assert_eq!(rows[&1048].shown_reuse_ms(), Some(360_000));
        assert_eq!(rows[&593].shown_reuse_ms(), Some(3_000));
        assert_eq!(rows[&842].shown_reuse_ms(), None);
        assert_eq!(parse_level("20"), (20, 0));
        assert!(rows[&7387].level_ok(30) && !rows[&7387].level_ok(31) && rows[&842].level_ok(-1));
    }

    #[test]
    fn orbe_choisi_et_bouton() {
        let rows = parse_items("TID\tLevel\tItemType\tType\tCash\n842\t\t1\t14\t1\n1054\t\t1\t14\t\n7387\t1;30\t1\t14\t1\n");
        let mut items = HashMap::new();
        items.insert(1, record(1, 1054, STATUS_INVENTORY, [0, 0, 1]));
        items.insert(2, record(2, 1054, STATUS_INVENTORY, [0, 2, 0]));
        items.insert(3, record(3, 7387, STATUS_INVENTORY, [0, 3, 0]));
        // Dernier orbe convenable de l'inventaire (celui « for Novices » ne sert plus au niveau 40).
        assert_eq!(find_orb(&items, &rows, 40), Some(2));
        assert_eq!(find_orb(&items, &rows, 10), Some(3));
        // L'inventaire Cash passe d'abord.
        items.insert(4, record(4, 842, STATUS_CASH_INVENTORY, [0, 0, 5]));
        assert_eq!(find_orb(&items, &rows, 40), Some(4));

        let (orb, cash) = (rows[&1054], rows[&842]);
        assert!(orb_active(true, Some(orb), None, false));
        assert!(!orb_active(false, Some(orb), None, false), "vivant");
        assert!(!orb_active(true, None, None, false), "sans orbe");
        assert!(!orb_active(true, Some(orb), None, true), "ArchLord");
        assert!(!orb_active(true, Some(orb), Some(0), false), "région sans le bit 0x100");
        assert!(orb_active(true, Some(orb), Some(0x100 | 0x4), false));
        assert!(!orb_active(true, Some(orb), Some(0x100 | 0x8), false), "sûreté 2 : orbe Cash seulement");
        assert!(orb_active(true, Some(cash), Some(0x100 | 0x8), false));
    }

    #[test]
    fn refus_avant_l_envoi() {
        let gem = ItemRow { item_type: 1, use_type: USE_SKILL_INIT, ..Default::default() };
        assert_eq!(use_check(gem, true, false, 3, None), UseCheck::Silent);
        assert_eq!(use_check(gem, false, true, 3, None), UseCheck::Send, "évolution : refus de app_morph");
        assert_eq!(use_check(gem, false, false, 0, None), UseCheck::NoOwnSkill);
        assert_eq!(use_check(gem, false, false, 1, None), UseCheck::Confirm);
        let orb = ItemRow { item_type: 1, use_type: USE_REVERSE_ORB, ..Default::default() };
        assert_eq!(use_check(orb, false, false, 0, None), UseCheck::Silent, "vivant");
        assert_eq!(use_check(orb, true, false, 0, Some(0x108)), UseCheck::Silent, "sûreté 2 sans Cash");
        assert_eq!(use_check(ItemRow { cash: true, ..orb }, true, false, 0, Some(0x108)), UseCheck::Send);
        assert_eq!(use_check(ItemRow { item_type: 1, use_type: 4, ..Default::default() }, true, false, 0, None), UseCheck::Send);
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : orbes 842 (Cash, 90 s) et 7387 (« for Novices », 1;30),
    /// « Extra-large Quickness Potion » (596 → compétence 455, déplacement + 25 %), régions 20 (Battle Square, sans
    /// le bit 0x100) et 1.
    #[test]
    fn donnees_d_archonia() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let d = load_data(&Assets::new(Some(dir.into()), &[]));
        let orb = d.items[&842];
        assert_eq!((orb.use_type, orb.cash, orb.use_interval_ms), (USE_REVERSE_ORB, true, 90_000));
        assert_eq!((d.items[&7387].min_level, d.items[&7387].max_level), (1, 30));
        let potion = d.items[&596];
        assert_eq!(speed_message(potion, d.speeds.get(&potion.skill_tid)), Some(("Item_UseAddMovementSkill", 25)));
        assert!(d.regions.get(&20).is_some_and(|t| t & REGION_RESURRECTION == 0));
        assert!(d.regions.values().any(|t| t & REGION_RESURRECTION != 0));
    }

    #[test]
    fn messages_de_vitesse() {
        let text = "name\tskill_level\tmovement\tatk_speed\tmovement\tatk_speed\n\
                    Extra-large Quickness Potion\t1\t\t\t25\t\n\
                    Extra-large Light Speed Potion\t1\t\t\t\t20\n\
                    Pet\t1\t\t\t\t\n\t2\t\t\t3\t\n";
        let speeds = parse_skill_speeds(text);
        let row = |lv| ItemRow { item_type: 1, use_type: USE_SKILL_SCROLL, skill_level: lv, ..Default::default() };
        assert_eq!(speed_message(row(1), speeds.get("extra-large quickness potion")), Some(("Item_UseAddMovementSkill", 25)));
        assert_eq!(speed_message(row(1), speeds.get("extra-large light speed potion")), Some(("Item_UseAddAtkSpeedSkill", 20)));
        assert_eq!(speed_message(row(1), speeds.get("pet")), None);
        assert_eq!(speed_message(row(2), speeds.get("pet")), Some(("Item_UseAddMovementSkill", 3)));
    }
}
