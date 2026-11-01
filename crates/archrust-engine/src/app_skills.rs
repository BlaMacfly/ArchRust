//! Compétences côté moteur : livre (`K`, `/competences`), instructeurs, barre de raccourcis (touches
//! 1 à =) et lancement. Protocole : `archrust_netclient::skills` ; fenêtres : `hud_skills.rs`.
//!
//! * Données du client : noms et TID dans `SKILLTEMPLATE.ini`, niveaux dans `SKILL_CONST.txt` (classes
//!   admises `require_class_*`, niveau et points exigés, prix d'apprentissage et d'amélioration).
//! * Un instructeur enseigne les compétences dont le niveau 1 admet la classe de sa vue (race et
//!   classe de la fonction 25), comme la règle du serveur.
//! * Lancement : sur la cible choisie, sinon sur soi ; le serveur mène la marche jusqu'à portée et
//!   annonce le lancement (op 7), d'où vient le délai de relance affiché.
//! * Barre : le serveur ne garde pas son contenu (module UI_STATUS absent d'ArchCore), elle est
//!   enregistrée par personnage dans `saved-variables/raccourcis-<personnage>.txt`.

use super::App;
use crate::assets::Assets;
use crate::hud::{BookEntry, BookView, HudState, Shortcut, SkillUi, SlotView, TrainerEntry, TrainerView, EXTRA_BELTS, QUICK_SLOTS};
use egui::Color32;
use crate::net::NetLink;
use archrust_netclient::items::ItemCommand;
use archrust_netclient::npc::function;
use archrust_netclient::skills::{learn_result, upgrade_result, SkillCommand, SkillEvent};
use archrust_netclient::Command;
use glam::Vec3;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use winit::keyboard::KeyCode;

/// « Normal Attack » : connue de tous, pas dans le livre.
const NORMAL_ATTACK: i32 = 222;
/// Colonnes « require_class_* » de SKILL_CONST, dans l'ordre.
const CLASS_COLUMNS: [&str; 13] =
    ["knight", "archer", "wizard", "berserker", "hunter", "sorcerer", "swashbuckler", "ranger", "elementaler", "slayer", "obiter", "summoner", "scion"];
/// Distance à laquelle l'instructeur reste ouvert.
const CLOSE_RANGE: f32 = 2200.0;

/// Colonne de classe (race 1 humain, 2 orc, 3 elfe de lune, 4 scion ; classe 1 guerrier, 2 archer,
/// 4 mage).
fn class_column(race: i32, class: i32) -> Option<usize> {
    let col = match (race, class) {
        (1, 1) => "knight",
        (1, 2) => "archer",
        (1, 4) => "wizard",
        (2, 1) => "berserker",
        (2, 2) => "hunter",
        (2, 4) => "sorcerer",
        (3, 1) => "swashbuckler",
        (3, 2) => "ranger",
        (3, 4) => "elementaler",
        (4, 1) => "slayer",
        (4, 2) => "obiter",
        (4, 4) => "summoner",
        (4, _) => "scion",
        _ => return None,
    };
    CLASS_COLUMNS.iter().position(|c| *c == col)
}

/// Un niveau de compétence (`SKILL_CONST.txt`).
#[derive(Debug, Clone, Default)]
struct Level {
    classes: u16,
    require_level: i32,
    require_point: i32,
    cost: i64,
    upgrade_cost: i64,
    /// Coûts (PV, PM, flèches), temps d'incantation, de relance et durée en ms (`cost_hp`, `cost_mp`,
    /// `cost_arrow`, `cast_time`, `recast_time`, `duration`), pour l'infobulle.
    cost_hp: i64,
    cost_mp: i64,
    cost_arrow: i64,
    cast_ms: i64,
    recast_ms: i64,
    duration_ms: i64,
}

#[derive(Debug, Clone, Default)]
struct SkillData {
    name: String,
    /// Niveaux 1, 2… dans l'ordre.
    levels: Vec<Level>,
    /// `skill_type` de SKILL_CONST (9 : compétence d'ArchLord, `FUN_1402fa5c0`).
    kind: i32,
}

#[derive(Default)]
struct SkillTables {
    by_tid: HashMap<i32, SkillData>,
    /// `skilltooltip.txt` par nom de compétence (en minuscules) : description et colonnes marquées « V ».
    tips: HashMap<String, TipRow>,
    /// Même table dans le dossier de langue (description traduite), si ses noms correspondent.
    tips_local: HashMap<String, TipRow>,
}

/// Ligne de `skilltooltip.txt` : ce que l'infobulle d'Archonia montre pour une compétence.
#[derive(Debug, Clone, Default)]
struct TipRow {
    description: String,
    shown: std::collections::HashSet<String>,
}

/// `skilltooltip.txt` : en-tête (`Name`, `Description`, `Level`, …, `RequiredLevel`, `CostMP`,
/// `CastTime`…), puis une ligne par compétence où « V » marque les lignes à montrer.
fn parse_tips(text: &str) -> HashMap<String, TipRow> {
    let mut lines = text.lines();
    let header: Vec<String> = lines.next().unwrap_or("").split('\t').map(|h| h.trim().to_string()).collect();
    let mut out = HashMap::new();
    for line in lines {
        let f: Vec<&str> = line.split('\t').collect();
        let name = f.first().map(|s| s.trim()).unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let description = f.get(1).map(|s| s.trim()).filter(|d| !d.eq_ignore_ascii_case("V")).unwrap_or("").to_string();
        let shown = f.iter().enumerate().skip(2).filter(|(_, v)| v.trim().eq_ignore_ascii_case("V")).filter_map(|(i, _)| header.get(i).cloned()).collect();
        out.entry(name.to_lowercase()).or_insert(TipRow { description, shown });
    }
    out
}

/// Durée en ms écrite en secondes (« 2 s », « 7,45 s »).
fn seconds(ms: i64) -> String {
    let text = format!("{:.2}", ms as f64 / 1000.0);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{} s", text.replace('.', ","))
}

fn load_tables(assets: &Assets) -> SkillTables {
    // SKILL_CONST.txt : une ligne par niveau, le nom sur la première.
    let text = assets
        .read_file("ini/SKILL_CONST.txt")
        .map(|raw| {
            let plain = archrust_data::ini::decrypt_ini(&raw);
            if plain.starts_with("name") { plain } else { String::from_utf8_lossy(&raw).into_owned() }
        })
        .unwrap_or_default();
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap_or("").split('\t').map(str::trim).collect();
    let col = |n: &str| header.iter().position(|h| h.eq_ignore_ascii_case(n));
    let class_cols: Vec<Option<usize>> = CLASS_COLUMNS.iter().map(|c| col(&format!("require_class_{c}"))).collect();
    let (c_level, c_point, c_cost, c_up) = (col("require_level"), col("require_point"), col("skill_cost"), col("skill_upgrade_cost"));
    let mut by_name: HashMap<String, Vec<Level>> = HashMap::new();
    let mut kinds: HashMap<String, i32> = HashMap::new();
    let mut current: Option<String> = None;
    for line in lines {
        let f: Vec<&str> = line.split('\t').collect();
        let get = |c: Option<usize>| c.and_then(|i| f.get(i)).map(|s| s.trim()).unwrap_or("");
        let num = |c: Option<usize>| get(c).parse::<f64>().unwrap_or(0.0);
        let name = f.first().map(|s| s.trim()).unwrap_or("");
        if !name.is_empty() {
            current = Some(name.to_string());
            kinds.entry(name.to_string()).or_insert(num(col("skill_type")) as i32);
        }
        let Some(n) = &current else { continue };
        if get(col("skill_level")).is_empty() {
            continue;
        }
        let mut classes = 0u16;
        for (i, c) in class_cols.iter().enumerate() {
            if num(*c) != 0.0 {
                classes |= 1 << i;
            }
        }
        by_name.entry(n.clone()).or_default().push(Level {
            classes,
            require_level: num(c_level) as i32,
            require_point: num(c_point) as i32,
            cost: num(c_cost) as i64,
            upgrade_cost: num(c_up) as i64,
            cost_hp: num(col("cost_hp")) as i64,
            cost_mp: num(col("cost_mp")) as i64,
            cost_arrow: num(col("cost_arrow")) as i64,
            cast_ms: num(col("cast_time")) as i64,
            recast_ms: num(col("recast_time")) as i64,
            duration_ms: num(col("duration")) as i64,
        });
    }
    // SKILLTEMPLATE.ini : TID → nom.
    let mut tables = SkillTables::default();
    if let Ok(t) = assets.table("ini", "SKILLTEMPLATE.ini") {
        let name_col = t.section("").and_then(|h| h.entries.iter().find(|(_, v)| v.eq_ignore_ascii_case("Name")).map(|(k, _)| k.clone()));
        for s in &t.sections {
            let (Ok(tid), Some(name)) = (s.name.parse::<i32>(), name_col.as_deref().and_then(|c| s.get(c))) else { continue };
            let name = name.trim().to_string();
            let levels = by_name.get(&name).cloned().unwrap_or_default();
            let kind = kinds.get(&name).copied().unwrap_or(0);
            tables.by_tid.insert(tid, SkillData { name, levels, kind });
        }
    }
    // Infobulles : `skilltooltip.txt` anglais (noms de SKILLTEMPLATE) et celui du dossier de langue.
    tables.tips = parse_tips(&super::npc_glue::client_text_in(assets, "ini", "skilltooltip.txt"));
    tables.tips_local = parse_tips(&super::npc_glue::client_text(assets, "skilltooltip.txt"));
    log::info!("compétences : {} modèle(s), {} décrite(s) par SKILL_CONST, {} infobulle(s)", tables.by_tid.len(), by_name.len(), tables.tips.len());
    tables
}

#[derive(Default)]
pub struct SkillsGlue {
    tables: Option<SkillTables>,
    /// Compétences du joueur : identifiant → (TID, niveau).
    known: HashMap<i32, (i32, i32)>,
    pub show_book: bool,
    book_first: usize,
    /// Instructeur ouvert et race/classe enseignées.
    trainer: Option<(i32, (i32, i32))>,
    trainer_first: usize,
    trainer_selected: Option<i32>,
    /// Fin du délai de relance par identifiant (et durée totale).
    cooldowns: HashMap<i32, (Instant, Duration)>,
    /// Cases : potions, barre principale, puis les barres en plus (`QUICK_SLOTS`).
    slots: Vec<Option<Shortcut>>,
    /// Barres en plus affichées (`UI_QuickBelt_1…`, flèches de la barre).
    belts: usize,
    /// Personnage dont la barre est chargée.
    slots_owner: Option<String>,
    /// Demandes des fenêtres, traitées à l'image suivante (la passe egui tient le GPU emprunté).
    pending: Vec<SkillUi>,
    /// Seuils de prise automatique des potions des cases 1 (vie) et 2 (mana), en % ; 0 : désactivé.
    pub auto: [u8; 2],
    /// Dernière prise automatique de chaque potion.
    auto_at: [Option<Instant>; 2],
    /// Potions de vie et de mana gardées par le serveur, à poser sur les cases 1 et 2 si elles sont vides.
    server_potions: Option<(i32, i32)>,
}

impl SkillsGlue {
    fn tables(&mut self, assets: &Assets) -> &SkillTables {
        self.tables.get_or_insert_with(|| load_tables(assets))
    }

    fn name(&mut self, assets: &Assets, tid: i32) -> String {
        self.tables(assets).by_tid.get(&tid).map(|d| d.name.clone()).unwrap_or_else(|| format!("Compétence {tid}"))
    }

    /// Infobulle d'une compétence au niveau `level` (1 et plus ; 0 : le premier niveau), comme celle
    /// d'Archonia : nom, niveau, description, puis les lignes que `skilltooltip.txt` marque (libellés
    /// de `sysstr.txt` 67 à 79 : niveau requis, points, incantation, relance, durée, coûts).
    fn tooltip(&mut self, assets: &Assets, tid: i32, level: i32) -> Vec<(String, Color32)> {
        let name = self.name(assets, tid);
        let tables = self.tables(assets);
        let data = tables.by_tid.get(&tid);
        let key = data.map(|d| d.name.to_lowercase()).unwrap_or_default();
        let row = tables.tips.get(&key).cloned();
        let description = tables.tips_local.get(&key).map(|r| r.description.clone()).filter(|d| !d.is_empty()).or_else(|| row.as_ref().map(|r| r.description.clone())).unwrap_or_default();
        let lv = data.and_then(|d| d.levels.get((level.max(1) - 1) as usize).or(d.levels.last())).cloned();
        let mut lines = vec![(name, Color32::from_rgb(255, 210, 110))];
        if level > 0 {
            lines.push((format!("Niveau {level}"), Color32::from_rgb(255, 235, 170)));
        }
        if !description.is_empty() {
            lines.push((description, Color32::from_rgb(225, 225, 225)));
        }
        let (Some(row), Some(lv)) = (row, lv) else { return lines };
        let rows: [(&str, u32, Option<String>); 8] = [
            ("RequiredLevel", 67, (lv.require_level > 0).then(|| lv.require_level.to_string())),
            ("RequiredPoints", 68, (lv.require_point > 0).then(|| lv.require_point.to_string())),
            ("CastTime", 69, (lv.cast_ms > 0).then(|| seconds(lv.cast_ms))),
            ("RecastTime", 70, (lv.recast_ms > 0).then(|| seconds(lv.recast_ms))),
            ("Duration", 71, (lv.duration_ms > 0).then(|| seconds(lv.duration_ms))),
            ("CostHP", 77, (lv.cost_hp > 0).then(|| lv.cost_hp.to_string())),
            ("CostMP", 78, (lv.cost_mp > 0).then(|| lv.cost_mp.to_string())),
            ("CostArrow", 79, (lv.cost_arrow > 0).then(|| lv.cost_arrow.to_string())),
        ];
        for (column, label, value) in rows {
            let Some(value) = value.filter(|_| row.shown.contains(column)) else { continue };
            let label = super::npc_glue::sysstr(assets, label).unwrap_or_else(|| column.to_string());
            lines.push((format!("{label} : {value}"), Color32::from_rgb(200, 220, 255)));
        }
        lines
    }

    fn id_of(&self, tid: i32) -> Option<i32> {
        self.known.iter().find(|(_, (t, _))| *t == tid).map(|(id, _)| *id)
    }

    /// Secondes de relance restantes.
    fn left_s(&self, id: i32) -> u32 {
        self.cooldowns.get(&id).map(|(end, _)| end.saturating_duration_since(Instant::now()).as_secs_f32().ceil() as u32).unwrap_or(0)
    }

    fn cooldown(&self, id: i32) -> f32 {
        match self.cooldowns.get(&id) {
            Some((end, total)) if *end > Instant::now() && !total.is_zero() => (*end - Instant::now()).as_secs_f32() / total.as_secs_f32(),
            _ => 0.0,
        }
    }
}

/// Intervalle minimal entre deux prises automatiques de la même potion.
const AUTO_POTION_GAP: Duration = Duration::from_secs(1);

fn slots_path(dir: &Path, name: &str) -> PathBuf {
    let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    dir.join(format!("raccourcis-{safe}.txt"))
}

/// Barre enregistrée : une ligne « case sorte TID » (sorte : compétence ou objet ; cases 12 et
/// plus : barres en plus) et une ligne « barres n » (barres en plus affichées).
fn load_slots(dir: &Path, name: &str) -> (Vec<Option<Shortcut>>, usize) {
    let mut slots = vec![None; QUICK_SLOTS];
    let mut belts = 0;
    for line in std::fs::read_to_string(slots_path(dir, name)).unwrap_or_default().lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if let ["barres", n] = f.as_slice() {
            belts = n.parse::<usize>().unwrap_or(0).min(EXTRA_BELTS);
        }
        if let [slot, kind, tid] = f.as_slice() {
            if let (Ok(slot), Ok(tid)) = (slot.parse::<usize>(), tid.parse::<i32>()) {
                if slot < QUICK_SLOTS {
                    slots[slot] = match *kind {
                        "competence" => Some(Shortcut::Skill(tid)),
                        "objet" => Some(Shortcut::Item(tid)),
                        _ => None,
                    };
                }
            }
        }
    }
    (slots, belts)
}

fn save_slots(dir: &Path, name: &str, slots: &[Option<Shortcut>], belts: usize) {
    let mut text = String::from("# Barre de raccourcis d'ArchRust : case (0 à 11, puis 12 et plus pour les barres en plus) sorte TID\n");
    text.push_str(&format!("barres {belts}\n"));
    for (i, s) in slots.iter().enumerate() {
        match s {
            Some(Shortcut::Skill(t)) => text.push_str(&format!("{i} competence {t}\n")),
            Some(Shortcut::Item(t)) => text.push_str(&format!("{i} objet {t}\n")),
            None => {}
        }
    }
    let _ = std::fs::create_dir_all(dir);
    if let Err(e) = std::fs::write(slots_path(dir, name), text) {
        log::warn!("barre de raccourcis non enregistrée : {e}");
    }
}

/// Données des fenêtres pour cette image.
#[derive(Default)]
pub struct SkillViews {
    book: Vec<BookEntry>,
    trainer: Option<(String, Vec<TrainerEntry>)>,
    slots: Vec<SlotView>,
    belts: usize,
    points: Option<i32>,
    auto: [u8; 2],
}

/// Fenêtres et barre ; les demandes attendent `App::skills_frame`.
pub fn panel(ctx: &egui::Context, glue: &mut SkillsGlue, views: &SkillViews, hud: &mut HudState, assets: &Assets) {
    let mut out = Vec::new();
    crate::hud::draw_quickbar(ctx, hud, assets, &views.slots, views.belts, views.auto, &mut out);
    if glue.show_book {
        crate::hud::draw_skill_book(ctx, hud, assets, BookView { entries: &views.book, points: views.points, first: &mut glue.book_first }, &mut out);
    }
    if let Some((title, entries)) = &views.trainer {
        let view = TrainerView { title: title.clone(), entries, selected: glue.trainer_selected, points: views.points, first: &mut glue.trainer_first };
        crate::hud::draw_trainer(ctx, hud, assets, view, &mut out);
    }
    glue.pending.extend(out);
}

fn send(net: Option<&NetLink>, cmd: SkillCommand) {
    if let Some(net) = net {
        net.send(Command::Skill(cmd));
    }
}

impl App {
    /// Compétences du joueur et réponses des instructeurs.
    pub(super) fn on_skill_event(&mut self, ev: SkillEvent) {
        let assets = self.assets.clone();
        let me = self.player.as_ref().map(|p| p.cid).unwrap_or(0);
        match ev {
            SkillEvent::Known { owner, id, tid, level, .. } => {
                if owner == 0 || owner == me || me == 0 {
                    self.skills.known.insert(id, (tid, level));
                }
            }
            SkillEvent::Level { id, level, .. } => {
                if let Some(tid) = self.skills.known.get_mut(&id).map(|s| {
                    s.1 = level;
                    s.0
                }) {
                    let name = self.skills.name(&assets, tid);
                    self.ui.push_chat(format!("[compétence] {name} passe au niveau {level}."));
                }
            }
            SkillEvent::Cast { caster, id, tid, cast_ms, recast_ms, .. } if caster == me => {
                let total = Duration::from_millis(cast_ms as u64 + recast_ms as u64);
                self.skills.cooldowns.insert(id, (Instant::now() + total, total));
                let name = self.skills.name(&assets, tid);
                self.ui.push_chat(format!("[compétence] {name}."));
            }
            SkillEvent::Cast { .. } => {}
            // Expérience de production : relevée par `product_observe` (voir `app_product.rs`).
            SkillEvent::Exp { .. } => {}
            SkillEvent::MasterOpened { npc } => {
                let teach = self.npc_teach(npc);
                match teach {
                    Some(t) => {
                        self.skills.trainer = Some((npc, t));
                        self.skills.trainer_first = 0;
                        self.skills.trainer_selected = None;
                    }
                    None => self.ui.push_chat("[compétence] Cet instructeur n'enseigne rien que le client connaisse."),
                }
            }
            // Points héroïques ou charisme manquants : la fenêtre d'Archonia n'affiche rien, le serveur le dit
            // par un message système.
            SkillEvent::Learned { result: learn_result::NOT_ENOUGH_HEROIC_POINTS | learn_result::NOT_ENOUGH_CHARISMA, .. } => {}
            SkillEvent::Learned { tid, result } => {
                let name = self.skills.name(&assets, tid);
                self.ui.push_chat(format!(
                    "[compétence] {}",
                    match result {
                        learn_result::SUCCESS => format!("Vous apprenez {name} !"),
                        learn_result::ALREADY_LEARNED => format!("Vous connaissez déjà {name}."),
                        learn_result::NOT_ENOUGH_POINTS => format!("Pas assez de points de compétence pour {name}."),
                        learn_result::LOW_LEVEL => format!("Niveau trop bas pour {name}."),
                        learn_result::WRONG_CLASS => format!("{name} n'est pas pour votre classe."),
                        _ => format!("Impossible d'apprendre {name} (or insuffisant ?)."),
                    }
                ));
            }
            // Messages de l'[EventMap] d'Archonia (FUN_1405dd9c0 : Skill_UpgradeSuccess, Skill_ErrorNotEnoughMoney,
            // …SkillPoint, …HeroicPoint, …CharismaPoint).
            SkillEvent::Upgraded { result, .. } => {
                let text = match result {
                    upgrade_result::SUCCESS => "Skill Upgraded",
                    upgrade_result::NOT_ENOUGH_MONEY => "You do not have enough Gold",
                    upgrade_result::NOT_ENOUGH_POINTS => "You do not have enough skill points",
                    upgrade_result::NOT_ENOUGH_HEROIC_POINTS => "You don't have enough Heroic Points.",
                    upgrade_result::NOT_ENOUGH_CHARISMA => "You do not have enough charisma.",
                    _ => return,
                };
                self.ui.push_chat(format!("[compétence] {text}"));
            }
        }
    }

    /// Identifiant de la compétence possédée de TID `tid` (annulation de l'évolution, voir `app_morph.rs`).
    pub(super) fn skills_id_of(&self, tid: i32) -> Option<i32> {
        self.skills.id_of(tid)
    }

    /// Race et classe enseignées par un PNJ (fonction 25 de sa vue).
    fn npc_teach(&self, npc: i32) -> Option<(i32, i32)> {
        self.npc_functions(npc).iter().find(|f| f.function == function::SKILL_MASTER).and_then(|f| f.teach)
    }

    /// Lance la compétence `tid` sur la cible choisie, sinon sur soi.
    pub(super) fn skills_cast(&mut self, tid: i32) {
        let assets = self.assets.clone();
        let name = self.skills.name(&assets, tid);
        let Some(id) = self.skills.id_of(tid) else {
            self.ui.push_chat(format!("[compétence] Vous ne connaissez pas {name}."));
            return;
        };
        if self.skills.cooldown(id) > 0.0 {
            let left = self.skills.cooldowns.get(&id).map(|(e, _)| (*e - Instant::now()).as_secs()).unwrap_or(0);
            self.ui.push_chat(format!("[compétence] {name} sera prête dans {left} s."));
            return;
        }
        let Some(me) = self.player.as_ref().map(|p| p.cid) else { return };
        // Compétences d'évolution du Dragon Scion (`target` 100 de SKILL_SPEC) : sur soi, quelle que soit la cible.
        let target = if self.morph_is_evolution_skill(tid) { me } else { self.addons.target_cid().unwrap_or(me) };
        // Métamorphosé : aucune compétence, sauf pour l'ArchLord (voir `app_morph.rs`).
        if !self.morph_allows_skill(tid) {
            return;
        }
        // Compétence d'ArchLord (`skill_type` 9) : réservée à l'ArchLord (`FUN_1402fa5c0`, voir `app_archlord.rs`).
        if self.skills.tables(&assets).by_tid.get(&tid).is_some_and(|d| d.kind == 9) && !self.archlord_is_me() {
            log::info!("compétence {tid} ({name}) : réservée à l'ArchLord, non envoyée");
            return;
        }
        // À cheval : `auto_attack` seulement (voir `app_ride.rs`).
        if !self.ride_allows_skill(tid) {
            return;
        }
        // Compétences de production (AgcmProductMed) : contrôles de la récolte, ou fenêtre de fabrication
        // sans envoi pour la cuisine et l'alchimie (voir `app_product.rs`).
        if !self.product_before_cast(tid, id, target) {
            return;
        }
        // Le serveur mène la marche jusqu'à portée : pas d'arrêt du client, qui interromprait.
        self.walking = false;
        // Ctrl : attaque forcée sur un joueur (combat entre joueurs, voir `app_pvp.rs`).
        let forced = self.addon_glue.modifiers.control_key();
        send(self.net.as_ref(), SkillCommand::Cast { id, target, forced });
    }

    fn skills_use_slot(&mut self, slot: usize) {
        match self.skills.slots.get(slot).copied().flatten() {
            Some(Shortcut::Skill(tid)) if tid == NORMAL_ATTACK => {
                if let Some(cid) = self.addons.target_cid() {
                    self.combat_attack(cid);
                }
            }
            Some(Shortcut::Skill(tid)) => self.skills_cast(tid),
            Some(Shortcut::Item(tid)) => {
                // Refus du client métamorphosé ou évolué (FUN_14053d0b0, voir `app_morph.rs`).
                if self.morph_refuses_item(tid) {
                    return;
                }
                let item = self.items.values().filter(|i| i.template_id == Some(tid) && i.status == Some(2)).min_by_key(|i| i.id).map(|i| i.id);
                match (item, &self.net) {
                    (Some(iid), Some(net)) => net.send(Command::Item(ItemCommand::Use(iid))),
                    (None, _) => self.ui.push_chat("[objet] Plus aucun exemplaire dans l'inventaire."),
                    _ => {}
                }
            }
            None => {}
        }
    }

    /// Statut d'interface reçu à l'entrée en jeu : seuils de prise automatique et potions de la barre.
    pub(super) fn on_ui_status(&mut self, st: archrust_netclient::ui_status::UiStatus) {
        self.skills.auto = [st.auto_hp.clamp(0, 100) as u8, st.auto_mp.clamp(0, 100) as u8];
        self.skills.server_potions = Some((st.hp_potion_tid, st.mp_potion_tid));
    }

    /// Prise automatique des potions des cases 1 (vie) et 2 (mana) quand la jauge passe sous son seuil,
    /// une fois par seconde au plus ; le serveur refuse la potion encore en recharge.
    fn skills_auto_potions(&mut self) {
        let st = &self.hud.stats;
        let gauges = [(st.hp, st.max_hp), (st.mp, st.max_mp)];
        // Mort : rien.
        if st.hp.is_some_and(|hp| hp <= 0) {
            return;
        }
        for (k, (cur, max)) in gauges.into_iter().enumerate() {
            let threshold = self.skills.auto[k] as i64;
            let (Some(cur), Some(max)) = (cur, max) else { continue };
            if threshold == 0 || max <= 0 || cur as i64 * 100 >= threshold * max as i64 {
                continue;
            }
            if self.skills.auto_at[k].is_some_and(|t| t.elapsed() < AUTO_POTION_GAP) {
                continue;
            }
            let Some(Some(Shortcut::Item(tid))) = self.skills.slots.get(k).copied() else { continue };
            // Sans potion dans le sac : rien (pas de message à chaque image).
            if !self.items.values().any(|i| i.template_id == Some(tid) && i.status == Some(2)) {
                continue;
            }
            self.skills.auto_at[k] = Some(Instant::now());
            self.skills_use_slot(k);
        }
    }

    /// Touches 1 à = : cases de la barre (potions sur 1 et 2). Maj, Ctrl et Alt + 3 à = : cases des
    /// barres en plus 1, 2 et 3 (`Hotkey=1:4…`, `2:4…`, `3:4…` « Quick Slot 2-1… 4-10 » d'Archonia ;
    /// la barre 4 n'a pas de touches).
    pub(super) fn skills_key(&mut self, code: KeyCode) -> bool {
        let m = self.addon_glue.modifiers;
        let belt = if m.shift_key() {
            1
        } else if m.control_key() {
            2
        } else if m.alt_key() {
            3
        } else {
            0
        };
        let slot = match code {
            KeyCode::Digit1 => 0,
            KeyCode::Digit2 => 1,
            KeyCode::Digit3 => 2,
            KeyCode::Digit4 => 3,
            KeyCode::Digit5 => 4,
            KeyCode::Digit6 => 5,
            KeyCode::Digit7 => 6,
            KeyCode::Digit8 => 7,
            KeyCode::Digit9 => 8,
            KeyCode::Digit0 => 9,
            KeyCode::Minus => 10,
            KeyCode::Equal => 11,
            _ => return false,
        };
        let slot = match belt {
            0 => slot,
            // Potions : pas de touche avec modificateur (`HotkeyFlag=0`).
            _ if slot < 2 => return true,
            b => 12 + (b - 1) * 10 + (slot - 2),
        };
        self.skills_use_slot(slot);
        true
    }

    /// Demandes des fenêtres et de la barre.
    fn skills_apply(&mut self, requests: Vec<SkillUi>) {
        let mut changed = false;
        for r in requests {
            match r {
                SkillUi::CloseBook => self.skills.show_book = false,
                SkillUi::CloseTrainer => self.skills.trainer = None,
                SkillUi::Cast(tid) => self.skills_cast(tid),
                SkillUi::SelectTrainer(tid) => self.skills.trainer_selected = Some(tid),
                SkillUi::Learn(tid) => {
                    if let Some((npc, _)) = self.skills.trainer {
                        send(self.net.as_ref(), SkillCommand::Learn { npc, tid });
                    }
                }
                SkillUi::Upgrade(tid) => {
                    if let (Some((npc, _)), Some(id)) = (self.skills.trainer, self.skills.id_of(tid)) {
                        send(self.net.as_ref(), SkillCommand::Upgrade { npc, id });
                    }
                }
                SkillUi::SlotSet { slot, shortcut } => {
                    let Some(cell) = self.skills.slots.get_mut(slot) else { continue };
                    *cell = Some(shortcut);
                    changed = true;
                    // Potions des cases 1 et 2 : gardées aussi par le serveur (statut d'interface).
                    if let (0 | 1, Shortcut::Item(tid), Some(net)) = (slot, shortcut, &self.net) {
                        let mut u = archrust_netclient::ui_status::UiStatusUpdate::default();
                        if slot == 0 {
                            u.hp_potion_tid = Some(tid);
                        } else {
                            u.mp_potion_tid = Some(tid);
                        }
                        net.send(Command::UiStatus(u));
                    }
                }
                SkillUi::SlotSwap(a, b) => {
                    if a < self.skills.slots.len() && b < self.skills.slots.len() {
                        self.skills.slots.swap(a, b);
                        changed = true;
                    }
                }
                SkillUi::Belts(delta) => {
                    self.skills.belts = (self.skills.belts as i64 + delta as i64).clamp(0, EXTRA_BELTS as i64) as usize;
                    changed = true;
                }
                SkillUi::SlotClear(slot) => {
                    if let Some(cell) = self.skills.slots.get_mut(slot) {
                        *cell = None;
                    }
                    changed = true;
                }
                SkillUi::SlotUse(slot) => self.skills_use_slot(slot),
                SkillUi::AutoGauge { hp, value } => {
                    self.skills.auto[if hp { 0 } else { 1 }] = value.min(100);
                    let mut u = archrust_netclient::ui_status::UiStatusUpdate::default();
                    if hp {
                        u.auto_hp = Some(value.min(100) as i8);
                    } else {
                        u.auto_mp = Some(value.min(100) as i8);
                    }
                    if let Some(net) = &self.net {
                        net.send(Command::UiStatus(u));
                    }
                }
            }
        }
        if changed {
            if let Some(name) = self.skills.slots_owner.clone() {
                save_slots(&self.config.saved_dir, &name, &self.skills.slots, self.skills.belts);
            }
        }
    }

    /// À chaque image : demandes des fenêtres, barre du personnage chargée, instructeur fermé en
    /// s'éloignant.
    pub(super) fn skills_frame(&mut self) {
        let requests = std::mem::take(&mut self.skills.pending);
        if !requests.is_empty() {
            self.skills_apply(requests);
        }
        let Some((name, me)) = self.player.as_ref().filter(|p| p.cid != 0 && !p.name.is_empty()).map(|p| (p.name.clone(), p.position)) else { return };
        if self.skills.slots_owner.as_deref() != Some(name.as_str()) {
            (self.skills.slots, self.skills.belts) = load_slots(&self.config.saved_dir, &name);
            self.skills.slots_owner = Some(name);
        }
        // Potions gardées par le serveur : sur les cases 1 et 2 restées vides (autre poste, barre effacée).
        if let Some((hp, mp)) = self.skills.server_potions.take() {
            let mut changed = false;
            for (slot, tid) in [(0, hp), (1, mp)] {
                if let Some(cell) = self.skills.slots.get_mut(slot).filter(|c| tid > 0 && c.is_none()) {
                    *cell = Some(Shortcut::Item(tid));
                    changed = true;
                }
            }
            if let (true, Some(owner)) = (changed, self.skills.slots_owner.clone()) {
                save_slots(&self.config.saved_dir, &owner, &self.skills.slots, self.skills.belts);
            }
        }
        self.skills_auto_potions();
        if let Some((npc, _)) = self.skills.trainer {
            let far = self.entities.iter().find(|e| e.cid == npc).is_none_or(|e| Vec3::new(e.position.x - me.x, 0.0, e.position.z - me.z).length() > CLOSE_RANGE);
            if far {
                self.skills.trainer = None;
            }
        }
    }

    /// Données des fenêtres et de la barre.
    pub(super) fn skills_views(&mut self) -> SkillViews {
        let assets = self.assets.clone();
        let mut v = SkillViews { points: self.hud.stats.skill_points, auto: self.skills.auto, ..Default::default() };
        let known: Vec<(i32, (i32, i32))> = self.skills.known.iter().map(|(id, s)| (*id, *s)).collect();
        if self.skills.show_book {
            for (id, (tid, level)) in &known {
                if *tid == NORMAL_ATTACK {
                    continue;
                }
                let name = self.skills.name(&assets, *tid);
                let tip = self.skills.tooltip(&assets, *tid, *level);
                v.book.push(BookEntry { tid: *tid, name, level: *level, cooldown: self.skills.cooldown(*id), left_s: self.skills.left_s(*id), tip });
            }
            v.book.sort_by(|a, b| a.name.cmp(&b.name));
        }
        if let Some((npc, (race, class))) = self.skills.trainer {
            let col = class_column(race, class);
            let (level, money, points) = (self.hud.stats.level.unwrap_or(1), self.hud.stats.money.unwrap_or(0), self.hud.stats.skill_points.unwrap_or(0));
            let mut seen_names = std::collections::HashSet::new();
            let mut tids: Vec<(i32, SkillData)> = self
                .skills
                .tables(&assets)
                .by_tid
                .iter()
                // Compétences ordinaires de la classe : niveau requis et prix renseignés (les
                // héroïques et spéciales n'en ont pas) ; les variantes « <ASY>… » sont écartées.
                .filter(|(tid, d)| {
                    **tid != NORMAL_ATTACK
                        && !d.name.starts_with('<')
                        && d.levels.first().is_some_and(|l| l.require_level > 0 && l.cost > 0 && col.is_some_and(|c| l.classes & (1 << c) != 0))
                })
                .map(|(t, d)| (*t, d.clone()))
                .collect();
            tids.sort_by_key(|(t, _)| *t);
            let mut entries = Vec::new();
            for (tid, d) in tids {
                if !seen_names.insert(d.name.clone()) {
                    continue;
                }
                let owned = known.iter().find(|(_, (t, _))| *t == tid).map(|(_, (_, l))| *l).unwrap_or(0);
                let next = d.levels.get(owned as usize);
                // Prix de l'instructeur avec la taxe de la région (`FUN_1402b8aa0`…, voir `app_tax.rs`).
                let cost = super::tax_glue::taxed(next.map(|l| if owned == 0 { l.cost } else { l.upgrade_cost }).unwrap_or(0));
                let require_level = next.map(|l| l.require_level).unwrap_or(0);
                let possible = next.is_some_and(|l| level >= l.require_level && points >= l.require_point && money >= cost);
                let tip = self.skills.tooltip(&assets, tid, (owned + 1).min(d.levels.len() as i32));
                entries.push(TrainerEntry { tid, name: d.name.clone(), level: owned, max_level: d.levels.len() as i32, require_level, cost, possible, tip });
            }
            entries.sort_by_key(|e| (e.require_level, e.tid));
            let npc_name = self.entities.iter().find(|e| e.cid == npc).map(|e| e.name.clone()).unwrap_or_default();
            v.trainer = Some((npc_name, entries));
        }
        v.belts = self.skills.belts;
        v.slots = vec![SlotView::default(); self.skills.slots.len()];
        let slots = self.skills.slots.clone();
        for (i, s) in slots.iter().enumerate() {
            let Some(sc) = s else { continue };
            let (count, cooldown, left_s) = match sc {
                Shortcut::Skill(t) => {
                    let id = self.skills.id_of(*t);
                    (None, id.map(|id| self.skills.cooldown(id)).unwrap_or(0.0), id.map(|id| self.skills.left_s(id)).unwrap_or(0))
                }
                Shortcut::Item(t) => {
                    let n: i32 = self.items.values().filter(|it| it.template_id == Some(*t) && it.status == Some(2)).map(|it| it.count.unwrap_or(1)).sum();
                    // Potion de métamorphose : recharge commune de 10 min (AgpmItem op 19, voir `app_morph.rs`).
                    let (cooldown, left_s) = self.morph_item_cooldown(*t).unwrap_or((0.0, 0));
                    (Some(n), cooldown, left_s)
                }
            };
            // Infobulle : compétence au niveau connu ; objet : celle du premier exemplaire du sac.
            let tip = match sc {
                Shortcut::Skill(t) => {
                    let level = self.skills.known.values().find(|(k, _)| k == t).map_or(0, |(_, l)| *l);
                    self.skills.tooltip(&assets, *t, level)
                }
                Shortcut::Item(t) => self
                    .items
                    .values()
                    .filter(|it| it.template_id == Some(*t) && it.status == Some(2))
                    .min_by_key(|it| it.id)
                    .and_then(|it| self.hud.item_tips.get(&it.id).cloned())
                    .or_else(|| crate::scene::item_name_for_tid(&assets, *t).map(|n| vec![(n, Color32::WHITE)]))
                    .unwrap_or_default(),
            };
            v.slots[i] = SlotView { shortcut: Some(*sc), count, cooldown, left_s, tip };
        }
        v
    }

    /// `/competences` (`/skills`), `/lancer nom`.
    pub(super) fn skills_slash(&mut self, cmd: &str, arg: &str) -> bool {
        match cmd {
            "/competences" | "/compétences" | "/skills" => self.skills.show_book = !self.skills.show_book,
            // Seuil de prise automatique d'une potion, comme le curseur posé sur la jauge (essais pilotés).
            "/potionauto" | "/autopotion" => {
                let mut a = arg.split_whitespace();
                let hp = match a.next() {
                    Some("vie" | "pv" | "hp") => true,
                    Some("mana" | "pm" | "mp") => false,
                    _ => {
                        self.ui.push_chat("[potion] usage : /potionauto vie|mana 0-100");
                        return true;
                    }
                };
                let value = a.next().and_then(|v| v.parse::<u8>().ok()).unwrap_or(0).min(100);
                self.skills.pending.push(SkillUi::AutoGauge { hp, value });
                self.ui.push_chat(format!("[potion] prise automatique de la potion de {} sous {value} %", if hp { "vie" } else { "mana" }));
            }
            "/lancer" | "/cast" => {
                let wanted = arg.trim().to_lowercase();
                let assets = self.assets.clone();
                let known: Vec<i32> = self.skills.known.values().map(|(t, _)| *t).collect();
                let pick = known.into_iter().find(|t| t.to_string() == wanted || self.skills.name(&assets, *t).to_lowercase().contains(&wanted));
                match pick {
                    Some(tid) if !wanted.is_empty() => self.skills_cast(tid),
                    _ => self.ui.push_chat("[compétence] usage : /lancer nom de la compétence"),
                }
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_et_barre() {
        assert_eq!(class_column(1, 1), Some(0));
        assert_eq!(class_column(2, 4), Some(5));
        let dir = std::env::temp_dir().join("archrust-test-raccourcis");
        let mut slots = vec![None; QUICK_SLOTS];
        slots[2] = Some(Shortcut::Skill(4));
        slots[0] = Some(Shortcut::Item(190));
        // Barre en plus 2, case 3 (Ctrl+5).
        slots[12 + 10 + 2] = Some(Shortcut::Skill(7));
        save_slots(&dir, "Gm Test", &slots, 2);
        assert_eq!(load_slots(&dir, "Gm Test"), (slots, 2));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn infobulles_des_competences() {
        let text = "Name\tDescription\tLevel\tRequiredLevel\tCostMP\tCastTime\nDual Slash\tDouble Attack\tV\tV\tV\t\nPower Rush\t\t\t\t\t\n";
        let tips = parse_tips(text);
        let row = &tips["dual slash"];
        assert_eq!(row.description, "Double Attack");
        assert!(row.shown.contains("RequiredLevel") && row.shown.contains("CostMP") && !row.shown.contains("CastTime"));
        assert!(tips["power rush"].shown.is_empty());
        assert_eq!(seconds(2000), "2 s");
        assert_eq!(seconds(7450), "7,45 s");
        assert_eq!(seconds(2500), "2,5 s");
    }
}
