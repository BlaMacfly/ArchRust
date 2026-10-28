//! Production côté moteur : récolte sur cadavre, cuisine, alchimie, compétences de production et PNJ de
//! production. Protocole : `archrust_netclient::product` (AgpmProduct 0x17, AgpmEventProduct 0x3A) ;
//! fenêtres d'Archonia : `hud_product.rs`.
//!
//! Comportement relevé dans Archonia (sha256 e9c7609b…) :
//! * données du client : `productcategory.txt` (catégorie 0 Hunt, 1 Cook, 2 Alchemy → compétence),
//!   `productcompose.txt` (recettes, clé = TID de l'objet produit), `productexp.txt` (niveaux 1 à 15),
//!   `productgather.txt` (monstres récoltables par classe), `SKILL_SPEC.txt` (attribut 8 = production,
//!   outil exigé `requirement` 4 → `arg1`), `SKILL_CONST.txt` (niveau et prix d'apprentissage) ;
//! * lancer une compétence de production (`FUN_1406850c0`) : cuisine ou alchimie (catégorie ≥ 1) →
//!   « Using %s. » (`sysstr` 522), rien n'est envoyé et `UI_Compose` s'ouvre (`FUN_1406c2b10`) ;
//!   récolte → contrôles de `FUN_1402a7e00` (outil en inventaire : code 2 ; cadavre de monstre récoltable
//!   par la classe du joueur avec cette compétence : code 3 ; distance au sol ≤ 250 : code 4, qui
//!   n'empêche pas le lancement), puis le lancement part ; au retour de son action (op 7 du lanceur,
//!   `FUN_140684fc0`) la barre d'`UI_Cooldown` dure 5,4 s, puis AgpmProduct op 2 ;
//! * fabriquer (`FUN_1406c20f0`) : barre de 3 s (durée de l'animation, 3 s par défaut), puis op 0 avec la
//!   recette et l'objet recette pris dans l'inventaire (`FUN_1406c1540`) ;
//! * résultat (op 3) : messages de `FUN_14063f330` (récolte) ou `FUN_1406c24b0` (fabrication) selon la
//!   catégorie de la compétence ; recettes connues reçues par l'op 1 ;
//! * PNJ (fonction 24, bouton `sysstr` 329 « Crafting », `FUN_1406ca6a0`) : 0x3A op 0 → op 1 ouvre
//!   `UI_PDSkillBuy` (compétences puis recettes de la catégorie, `FUN_14063f960`), « Apprendre » après
//!   `PDSK_EVENT_BUY_CONFIRM` : op 2 (compétence) ou op 4 (recette) ; réponses op 3 et 5
//!   (`FUN_14063e4c0`, `FUN_14063d6a0`) ;
//! * `UI_PDSkill` (touche J, menu `MenuBtnProductSkill`) : niveau = « dirt » 4, expérience cumulée =
//!   « dirt » 6 de la compétence ; barre = expérience dans le niveau / expérience du niveau
//!   (`FUN_1402a8540`, `FUN_1402a8640`).
//!
//! Écarts : ni sons (`U_CR_*.wav`), ni animation de la compétence pendant la fabrication, ni infobulles ;
//! le titre requis d'une recette (colonne `UsingMonster`, vide partout) refuse la fabrication,
//! ArchRust ne suivant pas le titre actif.

use super::App;
use crate::assets::Assets;
use crate::hud::{ComposeUi, ComposeView, HudState, PdBuyEntry, PdBuyView, PdSkillEntry, PdSkillUi};
use crate::net::NetLink;
use crate::scene::item_name_for_tid;
use crate::ui::UiState;
use archrust_netclient::product::{learn_result, result, ProductCommand, ProductEvent, FUNCTION};
use archrust_netclient::skills::SkillEvent;
use archrust_netclient::world::ItemRecord;
use archrust_netclient::{Command, WorldEvent};
use egui::Color32;
use glam::Vec3;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

/// Barre de la récolte (`FUN_140684fc0` : 0x1518 ms).
const GATHER_PROGRESS: Duration = Duration::from_millis(5400);
/// Barre de la fabrication sans durée d'animation connue (`DAT_140e49f98` = 3,0 s).
const COMPOSE_PROGRESS: Duration = Duration::from_secs(3);
/// Distance au sol maximale de la récolte (`DAT_140e4a1b8` = 62 500 = 250²).
const GATHER_RANGE: f32 = 250.0;
/// Distance au PNJ au-delà de laquelle sa fenêtre se ferme.
const CLOSE_RANGE: f32 = 2200.0;
/// Statut AgpmItem d'un objet du sac.
const STATUS_INVENTORY: i8 = 2;
/// Cases du sac acceptées par le serveur (4 sacs de 4 × 4, voir `app_items.rs`).
const INVENTORY_CELLS: usize = 64;
/// Éléments de `UI_PDSkillBuy` au plus (`FUN_14063f960` : 60 cases).
const BUY_ENTRIES: usize = 60;
/// Recettes de `UI_Compose` au plus (`FUN_1402a6fa0` : 100).
const COMPOSE_ENTRIES: usize = 100;
/// `SKILL_SPEC.txt` : attribut des compétences de production, exigence « outil de production ».
const ATTRIBUTE_PRODUCT: i64 = 8;
const REQUIREMENT_TOOL: i64 = 4;
/// Couleur d'une recette irréalisable (`FUN_1406c0e20` : 0xFFAEA59C).
const UNAVAILABLE: Color32 = Color32::from_rgb(0xae, 0xa5, 0x9c);
/// Catégories d'`AgpmProduct` (`PTR_140e0c4b0`).
const CATEGORY_NAMES: [&str; 3] = ["Hunt", "Cook", "Alchemy"];
/// Textes de `sysstr.txt` utilisés.
const SYSSTR: [u32; 33] = [2, 3, 4, 10, 11, 12, 27, 28, 29, 31, 33, 36, 37, 38, 39, 40, 41, 44, 45, 46, 47, 48, 49, 50, 51, 262, 351, 426, 427, 429, 430, 517, 518];
const SYSSTR_USING: u32 = 522;

/// Une recette de `productcompose.txt` (`AgpaComposeTemplate`).
#[derive(Debug, Clone, Default, PartialEq)]
struct Recipe {
    category: i32,
    /// TID de l'objet produit, identifiant de la recette.
    result: i32,
    name: String,
    /// Titre requis (`UsingMonster`, 0 : aucun).
    title: i32,
    level: i32,
    price: i64,
    recipe_tid: i32,
    /// Matériaux : TID et quantité.
    materials: Vec<(i32, i32)>,
}

/// Données de production lues dans le client.
#[derive(Default)]
struct ProductData {
    /// (catégorie, TID de compétence) de `productcategory.txt`.
    categories: Vec<(i32, i32)>,
    recipes: Vec<Recipe>,
    /// Par compétence : niveau → (expérience du niveau, expérience cumulée à sa fin).
    exp: HashMap<i32, BTreeMap<i32, (i64, i64)>>,
    /// Par modèle de monstre : (classe, compétence de récolte).
    gather: HashMap<i32, Vec<(i32, i32)>>,
    /// Compétences à l'attribut de production et outil exigé.
    product_skills: HashSet<i32>,
    tools: HashMap<i32, i32>,
    /// Nom de chaque compétence (`SKILLTEMPLATE.ini`).
    names: HashMap<i32, String>,
    /// Niveau du personnage et prix pour apprendre une compétence (niveau 1 de `SKILL_CONST.txt`).
    learn: HashMap<i32, (i32, i64)>,
    sysstr: HashMap<u32, String>,
    buy_confirm: String,
    title_missing: String,
}

impl ProductData {
    fn sys(&self, id: u32) -> String {
        self.sysstr.get(&id).cloned().unwrap_or_default()
    }

    fn name(&self, tid: i32) -> String {
        self.names.get(&tid).cloned().unwrap_or_else(|| tid.to_string())
    }

    /// Catégorie d'une compétence de production (`FUN_1402a7120`).
    fn category_of(&self, skill: i32) -> Option<i32> {
        self.categories.iter().find(|(_, s)| *s == skill).map(|(c, _)| *c)
    }

    fn recipe(&self, id: i32) -> Option<&Recipe> {
        self.recipes.iter().find(|r| r.result == id)
    }

    /// Nom de la catégorie (`sysstr` 27 à 29).
    fn category_name(&self, category: i32) -> String {
        if (0..3).contains(&category) {
            self.sys(27 + category as u32)
        } else {
            String::new()
        }
    }

    /// Expérience dans le niveau et expérience du niveau (`FUN_1402a8540`, `FUN_1402a8640`).
    fn level_exp(&self, skill: i32, level: i32, exp: i32) -> (i64, i64) {
        let Some(table) = self.exp.get(&skill).filter(|_| (1..=15).contains(&level)) else { return (0, 0) };
        let before = table.get(&(level - 1)).map_or(0, |e| e.1);
        let max = table.get(&level).map_or(0, |e| e.0);
        ((exp.max(0) as i64 - before).max(0), max)
    }
}

/// Lignes d'un tableau TSV (en-tête compris).
fn rows(text: &str) -> Vec<Vec<&str>> {
    text.lines().map(|l| l.trim_end_matches('\r').split('\t').map(str::trim).collect()).collect()
}

fn num(row: &[&str], i: usize) -> i64 {
    row.get(i).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) as i64
}

/// `productcategory.txt` : lignes dont la première colonne est une catégorie (`FUN_14020e8f0`).
fn parse_categories(text: &str, by_name: &HashMap<String, Vec<i32>>) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for row in rows(text).iter().skip(1) {
        let Some(cat) = row.first().and_then(|c| CATEGORY_NAMES.iter().position(|n| n.eq_ignore_ascii_case(c))) else { continue };
        if let Some(tid) = row.get(1).and_then(|n| by_name.get(*n)).and_then(|t| t.first()) {
            out.push((cat as i32, *tid));
        }
    }
    out
}

/// `productcompose.txt` (`FUN_14020ebb0`) : 23 colonnes, ligne rejetée sans catégorie connue.
fn parse_compose(text: &str) -> Vec<Recipe> {
    let mut out = Vec::new();
    for row in rows(text).iter().skip(1) {
        let Some(category) = row.get(1).and_then(|c| CATEGORY_NAMES.iter().position(|n| n.eq_ignore_ascii_case(c))) else { continue };
        let result = num(row, 2) as i32;
        if result <= 0 {
            continue;
        }
        let materials = (0..4).map(|k| (num(row, 11 + 3 * k) as i32, num(row, 13 + 3 * k) as i32)).filter(|(t, n)| *t > 0 && *n > 0).collect();
        out.push(Recipe {
            category: category as i32,
            result,
            name: row.get(3).map(|s| s.to_string()).unwrap_or_default(),
            title: num(row, 4) as i32,
            level: (num(row, 5) as i32).max(1),
            price: num(row, 6),
            recipe_tid: num(row, 8) as i32,
            materials,
        });
    }
    out
}

/// `productexp.txt` (`FUN_14020f2b0`) : nom, niveau (1 à 15), …, expérience du niveau, cumulée.
fn parse_exp(text: &str, by_name: &HashMap<String, Vec<i32>>) -> HashMap<i32, BTreeMap<i32, (i64, i64)>> {
    let mut out: HashMap<i32, BTreeMap<i32, (i64, i64)>> = HashMap::new();
    for row in rows(text).iter().skip(1) {
        let level = num(row, 1) as i32;
        let Some(tids) = row.first().and_then(|n| by_name.get(*n)).filter(|_| (1..=15).contains(&level)) else { continue };
        for tid in tids {
            out.entry(*tid).or_default().insert(level, (num(row, 5), num(row, 6)));
        }
    }
    out
}

/// `productgather.txt` (`FUN_14020f5c0`) : modèle de monstre, classes `1;2;3;4`, compétence.
fn parse_gather(text: &str) -> HashMap<i32, Vec<(i32, i32)>> {
    let mut out: HashMap<i32, Vec<(i32, i32)>> = HashMap::new();
    for row in rows(text).iter().skip(1) {
        let (monster, skill) = (num(row, 1) as i32, num(row, 3) as i32);
        if monster <= 0 || skill <= 0 {
            continue;
        }
        for class in row.get(2).unwrap_or(&"").split(';').filter_map(|c| c.trim().parse::<i32>().ok()).filter(|c| (1..=4).contains(c)) {
            out.entry(monster).or_default().push((class, skill));
        }
    }
    out
}

/// `SKILL_SPEC.txt` : compétences de production (attribut 8) et outil exigé (`requirement` 4 → `arg1`).
fn parse_spec(text: &str, by_name: &HashMap<String, Vec<i32>>) -> (HashSet<i32>, HashMap<i32, i32>) {
    let table = rows(text);
    let Some(header) = table.first() else { return Default::default() };
    let col = |n: &str| header.iter().position(|h| h.eq_ignore_ascii_case(n));
    let (Some(attr), Some(req), Some(arg1)) = (col("attribute"), col("requirement"), col("arg1")) else { return Default::default() };
    let (mut skills, mut tools) = (HashSet::new(), HashMap::new());
    for row in table.iter().skip(1) {
        if num(row, attr) != ATTRIBUTE_PRODUCT {
            continue;
        }
        for tid in row.first().and_then(|n| by_name.get(*n)).into_iter().flatten() {
            skills.insert(*tid);
            if num(row, req) == REQUIREMENT_TOOL && num(row, arg1) > 0 {
                tools.insert(*tid, num(row, arg1) as i32);
            }
        }
    }
    (skills, tools)
}

/// `SKILL_CONST.txt` : niveau du personnage et prix du niveau 1 de chaque compétence nommée.
fn parse_learn(text: &str, by_name: &HashMap<String, Vec<i32>>) -> HashMap<i32, (i32, i64)> {
    let table = rows(text);
    let Some(header) = table.first() else { return HashMap::new() };
    let col = |n: &str| header.iter().position(|h| h.eq_ignore_ascii_case(n));
    let (Some(lv), Some(level), Some(cost)) = (col("skill_level"), col("require_level"), col("skill_cost")) else { return HashMap::new() };
    let mut out = HashMap::new();
    let mut current = "";
    for row in table.iter().skip(1) {
        if let Some(n) = row.first().filter(|n| !n.is_empty()) {
            current = n;
        }
        if num(row, lv) != 1 {
            continue;
        }
        for tid in by_name.get(current).into_iter().flatten() {
            out.entry(*tid).or_insert((num(row, level) as i32, num(row, cost)));
        }
    }
    out
}

fn load_data(assets: &Assets) -> ProductData {
    let text = |f: &str| super::npc_glue::client_text_in(assets, "ini", f);
    let mut d = ProductData::default();
    let mut by_name: HashMap<String, Vec<i32>> = HashMap::new();
    if let Ok(t) = assets.table("ini", "SKILLTEMPLATE.ini") {
        let name_col = t.section("").and_then(|h| h.entries.iter().find(|(_, v)| v.eq_ignore_ascii_case("Name")).map(|(k, _)| k.clone()));
        for s in &t.sections {
            let (Ok(tid), Some(name)) = (s.name.parse::<i32>(), name_col.as_deref().and_then(|c| s.get(c))) else { continue };
            d.names.insert(tid, name.trim().to_string());
            by_name.entry(name.trim().to_string()).or_default().push(tid);
        }
    }
    for tids in by_name.values_mut() {
        tids.sort_unstable();
    }
    d.categories = parse_categories(&text("productcategory.txt"), &by_name);
    d.recipes = parse_compose(&text("productcompose.txt"));
    d.exp = parse_exp(&text("productexp.txt"), &by_name);
    d.gather = parse_gather(&text("productgather.txt"));
    (d.product_skills, d.tools) = parse_spec(&text("SKILL_SPEC.txt"), &by_name);
    d.learn = parse_learn(&text("SKILL_CONST.txt"), &by_name);
    for id in SYSSTR.into_iter().chain([SYSSTR_USING]) {
        d.sysstr.insert(id, super::npc_glue::sysstr(assets, id).unwrap_or_default());
    }
    d.buy_confirm = super::portal_glue::ui_event_text(assets, "PDSK_EVENT_BUY_CONFIRM");
    d.title_missing = super::npc_glue::client_text(assets, "uimessage.txt").lines().find_map(|l| l.strip_prefix("NotActivateRequireTitle\t")).map(|t| t.trim().to_string()).unwrap_or_default();
    log::info!(
        "production : {} catégorie(s), {} recette(s), {} monstre(s) récoltable(s), {} compétence(s)",
        d.categories.len(),
        d.recipes.len(),
        d.gather.len(),
        d.product_skills.len()
    );
    d
}

/// Messages du résultat (op 3, ou contrôle du client) : récolte (`FUN_14063f330`) ou fabrication
/// (`FUN_1406c24b0`). `item` : nom de l'objet obtenu ou visé ; `tool` : nom de l'outil exigé.
#[allow(clippy::too_many_arguments)]
fn result_lines(d: &ProductData, compose: bool, code: i32, skill: &str, item: Option<&str>, count: i32, exp: i32, tool: Option<&str>) -> Vec<String> {
    let fill = super::pvp_glue::fill;
    let mut out = Vec::new();
    match code {
        result::SUCCESS => {
            out.push(format!("{skill} {}", d.sys(41)));
            if let Some(item) = item {
                out.push(format!("{item} {} {count} {}", d.sys(351), d.sys(517)));
            }
            if exp > 0 {
                // La fabrication colle le nom de la compétence au texte (« %s%s %d %s »).
                let sep = if compose { "" } else { " " };
                out.push(format!("{skill}{sep}{} {exp} {}", d.sys(44), d.sys(518)));
            }
        }
        result::FAIL => out.push(format!("{skill} {}", d.sys(45))),
        result::NO_TOOL => out.extend(tool.map(|t| fill(&d.sys(46), &[skill.to_string(), t.to_string()]))),
        result::INVALID_TARGET => out.push(fill(&d.sys(47), &[skill.to_string()])),
        result::TOO_FAR => out.push(format!("{skill} {}", d.sys(48))),
        result::INVENTORY_FULL => {
            out.extend(item.map(|i| format!("{i} {}", d.sys(49))));
            // Récolte : le cas 5 d'Archonia se poursuit dans le cas 7.
            if !compose {
                out.push(d.sys(51));
            }
        }
        result::NOT_ENOUGH_MATERIAL if compose => out.extend(item.map(|i| format!("{i} {}", d.sys(50)))),
        result::NOTHING_LEFT => out.push(d.sys(51)),
        _ => {}
    }
    out
}

/// Contrôle d'une fabrication (`FUN_1402a7920`) : 0 possible, sinon `result::UNKNOWN_RECIPE`,
/// `NOT_ENOUGH_MATERIAL`, `LOW_LEVEL`, ou -1 (inventaire plein, refus sans code).
fn check_compose(r: &Recipe, known: &[i32], stock: &HashMap<i32, i64>, skill_level: Option<i32>, bag_full: bool) -> i32 {
    if bag_full {
        return -1;
    }
    if !known.contains(&r.result) {
        return result::UNKNOWN_RECIPE;
    }
    if r.materials.iter().any(|(tid, n)| stock.get(tid).copied().unwrap_or(0) < *n as i64) {
        return result::NOT_ENOUGH_MATERIAL;
    }
    if r.level > 0 && skill_level.unwrap_or(0) < r.level {
        return result::LOW_LEVEL;
    }
    0
}

/// Quantités par TID des objets du sac.
fn bag_stock(items: &HashMap<i32, ItemRecord>) -> HashMap<i32, i64> {
    let mut have = HashMap::new();
    for i in items.values().filter(|i| i.status == Some(STATUS_INVENTORY)) {
        if let Some(t) = i.template_id {
            *have.entry(t).or_insert(0) += i.count.unwrap_or(1).max(1) as i64;
        }
    }
    have
}

fn bag_full(items: &HashMap<i32, ItemRecord>) -> bool {
    items.values().filter(|i| i.status == Some(STATUS_INVENTORY)).count() >= INVENTORY_CELLS
}

/// Fenêtre de fabrication ouverte.
struct Compose {
    category: i32,
    /// Identifiant (instance) de la compétence lancée.
    skill_id: i32,
    /// Recette choisie (`0x6808`) et objet recette trouvé dans le sac (`0x680c`).
    selected: Option<i32>,
    recipe_item: i32,
    first_row: usize,
    /// Fabrication en cours jusqu'à l'envoi (`0x6810`).
    busy: bool,
}

/// Fenêtre du PNJ ouverte.
struct Buy {
    npc: i32,
    category: i32,
    first: usize,
    selected: Option<usize>,
}

#[derive(Default)]
pub struct ProductGlue {
    data: Option<ProductData>,
    /// Compétences du joueur : identifiant → (TID, niveau, expérience cumulée).
    skills: HashMap<i32, (i32, i32, i32)>,
    /// Recettes connues (op 1, achats réussis).
    known: Vec<i32>,
    compose: Option<Compose>,
    buy: Option<Buy>,
    /// Achat confirmé en attente : compétence (TID) ou recette.
    confirm: Option<(i32, bool)>,
    pub show_skills: bool,
    skills_first: usize,
    /// Récolte armée par le lancement : compétence (identifiant) et cadavre.
    gather: Option<(i32, i32)>,
    /// Barre d'attente : début, durée, demande à envoyer.
    progress: Option<(Instant, Duration, ProductCommand)>,
    /// Compétences à lancer depuis `UI_PDSkill`, traitées à l'image suivante.
    pending_cast: Vec<i32>,
}

impl ProductGlue {
    fn data(&mut self, assets: &Assets) -> &ProductData {
        self.data.get_or_insert_with(|| load_data(assets))
    }

    /// Échap : ferme les fenêtres de production ; rend `true` si l'une était ouverte.
    pub(super) fn close_windows(&mut self) -> bool {
        let open = self.compose.is_some() || self.buy.is_some() || self.show_skills;
        self.compose = None;
        self.buy = None;
        self.confirm = None;
        self.show_skills = false;
        open
    }

    /// Recettes connues de la catégorie, dans l'ordre reçu (`FUN_1402a6fa0`).
    fn compose_list(&mut self, assets: &Assets, category: i32) -> Vec<Recipe> {
        let known = self.known.clone();
        let d = self.data(assets);
        known.iter().filter_map(|id| d.recipe(*id)).filter(|r| r.category == category).take(COMPOSE_ENTRIES).cloned().collect()
    }

    /// Contrôle de la recette `id` avec la compétence de la fenêtre.
    fn check(&mut self, assets: &Assets, id: i32, items: &HashMap<i32, ItemRecord>) -> i32 {
        let Some(skill_id) = self.compose.as_ref().map(|c| c.skill_id) else { return -1 };
        let level = self.skills.get(&skill_id).map(|s| s.1);
        let known = self.known.clone();
        let Some(r) = self.data(assets).recipe(id).cloned() else { return result::UNKNOWN_RECIPE };
        check_compose(&r, &known, &bag_stock(items), level, bag_full(items))
    }

    /// Éléments d'`UI_PDSkillBuy` : compétences de la catégorie, puis toutes ses recettes.
    fn buy_entries(&mut self, assets: &Assets, category: i32, level: i32, money: i64) -> Vec<PdBuyEntry> {
        let owned: Vec<(i32, i32)> = self.skills.values().map(|s| (s.0, s.1)).collect();
        let known = self.known.clone();
        let d = self.data(assets);
        let level_of = |tid: i32| owned.iter().filter(|(t, _)| *t == tid).map(|(_, l)| *l).max();
        let category_skills: Vec<i32> = d.categories.iter().filter(|(c, _)| *c == category).map(|(_, s)| *s).collect();
        let mut out = Vec::new();
        // Compétences (`FUN_1402d0900` : niveau du personnage, puis gelds).
        for tid in &category_skills {
            let (need, cost) = d.learn.get(tid).copied().unwrap_or((0, 0));
            // Taxe de la région du joueur (`FUN_1402d0900`…, voir `app_tax.rs`).
            let cost = super::tax_glue::taxed(cost);
            let state = if level_of(*tid).is_some() {
                learn_result::ALREADY
            } else if level < need {
                learn_result::LOW_LEVEL
            } else if money < cost {
                learn_result::NOT_ENOUGH_MONEY
            } else {
                learn_result::SUCCESS
            };
            let price = if state == learn_result::ALREADY { d.sys(426) } else { format!("{}{need:2} {}{cost:4}{}", d.sys(262), d.sys(3), d.sys(10)) };
            out.push(PdBuyEntry { skill_tid: Some(*tid), item_tid: None, name: format!("{} ({})", d.name(*tid), d.sys(2)), price, state });
        }
        // Recettes (`FUN_1402d0be0` : connue, compétence absente, niveau de compétence, gelds).
        for r in d.recipes.iter().filter(|r| r.category == category) {
            let skill_level = category_skills.iter().filter_map(|t| level_of(*t)).min();
            let state = if known.contains(&r.result) {
                learn_result::ALREADY
            } else if category_skills.iter().any(|t| level_of(*t).is_none()) {
                learn_result::NO_SKILL
            } else if skill_level.unwrap_or(0) < r.level {
                learn_result::LOW_LEVEL
            } else if money < super::tax_glue::taxed(r.price) {
                learn_result::NOT_ENOUGH_MONEY
            } else {
                learn_result::SUCCESS
            };
            let price = if state == learn_result::ALREADY { d.sys(426) } else { format!("{}{:2} {}{:4}{}", d.sys(4), r.level, d.sys(3), super::tax_glue::taxed(r.price), d.sys(10)) };
            out.push(PdBuyEntry { skill_tid: None, item_tid: Some(r.result), name: r.name.clone(), price, state });
        }
        out.truncate(BUY_ENTRIES);
        out
    }
}

fn send(net: Option<&NetLink>, cmd: ProductCommand) {
    if let Some(net) = net {
        net.send(Command::Product(cmd));
    }
}

/// Fenêtres de production, confirmation d'achat et barre d'attente.
pub fn panel(ctx: &egui::Context, glue: &mut ProductGlue, items: &HashMap<i32, ItemRecord>, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>, ui: &mut UiState) {
    let level = hud.stats.level.unwrap_or(1);
    let money = hud.stats.money.unwrap_or(0);

    if let Some(category) = glue.compose.as_ref().map(|c| c.category) {
        compose_panel(ctx, glue, category, items, hud, assets, ui);
    }

    if let Some((npc, category, mut first, selected)) = glue.buy.as_ref().map(|b| (b.npc, b.category, b.first, b.selected)) {
        let entries = glue.buy_entries(assets, category, level, money);
        let title = glue.data(assets).category_name(category);
        let mut out = Vec::new();
        crate::hud::draw_pdskill_buy(ctx, hud, assets, PdBuyView { title: &title, entries: &entries, selected, first: &mut first, money }, &mut out);
        if let Some(b) = glue.buy.as_mut() {
            b.first = first;
        }
        for r in out {
            match r {
                PdSkillUi::CloseBuy => {
                    glue.buy = None;
                    glue.confirm = None;
                }
                PdSkillUi::SelectBuy(i) => {
                    if let Some(b) = glue.buy.as_mut() {
                        b.selected = Some(i);
                    }
                }
                // `PDSkill_BuyBuy` : élément possible seulement, puis `PDSK_EVENT_BUY_CONFIRM`.
                PdSkillUi::Buy => {
                    if let Some(e) = selected.and_then(|i| entries.get(i)).filter(|e| e.state == learn_result::SUCCESS) {
                        glue.confirm = match (e.skill_tid, e.item_tid) {
                            (Some(tid), _) => Some((tid, true)),
                            (None, Some(recipe)) => Some((recipe, false)),
                            _ => None,
                        };
                    }
                }
                _ => {}
            }
        }
        if let Some((id, skill)) = glue.confirm {
            let text = glue.data(assets).buy_confirm.clone();
            if let Some(yes) = crate::hud::confirm_box(ctx, hud, assets, "pdskill-buy-confirm", &text) {
                glue.confirm = None;
                if yes {
                    send(net, if skill { ProductCommand::LearnSkill { npc, tid: id } } else { ProductCommand::BuyCompose { npc, recipe: id } });
                }
            }
        }
    }

    if glue.show_skills {
        let mut list: Vec<(i32, i32, i32)> = glue.skills.values().copied().collect();
        list.sort_by_key(|s| s.0);
        let d = glue.data(assets);
        let entries: Vec<PdSkillEntry> = list
            .into_iter()
            .filter(|(tid, _, _)| d.product_skills.contains(tid))
            .map(|(tid, level, exp)| {
                let (degree, max) = d.level_exp(tid, level, exp);
                let pct = if max > 0 { (degree as f32 * 100.0 / max as f32).min(100.0) } else { 0.0 };
                // `FUN_14063e180` : « %3d%s %s:%4.1f%% » (niveau, `sysstr` 12, `sysstr` 4, pourcentage).
                let level_exp = format!("{level:3}{} {}:{pct:4.1}%", d.sys(12), d.sys(4));
                PdSkillEntry { tid, name: d.name(tid), level_exp, fraction: pct / 100.0 }
            })
            .collect();
        let mut out = Vec::new();
        crate::hud::draw_pdskill(ctx, hud, assets, &entries, &mut glue.skills_first, &mut out);
        for r in out {
            match r {
                PdSkillUi::CloseSkills => glue.show_skills = false,
                PdSkillUi::Cast(tid) => glue.pending_cast.push(tid),
                _ => {}
            }
        }
    }

    if let Some((since, total, cmd)) = glue.progress.clone() {
        crate::hud::draw_cooldown(ctx, hud, assets, (since.elapsed().as_secs_f32() / total.as_secs_f32()).min(1.0), "");
        if since.elapsed() >= total {
            glue.progress = None;
            // `FUN_1406c07d0` : la fabrication n'est plus verrouillée à l'envoi.
            if let Some(c) = glue.compose.as_mut() {
                c.busy = false;
            }
            send(net, cmd);
        } else {
            ctx.request_repaint();
        }
    }
}

/// `UI_Compose` : recettes connues, détail, matériaux, fabrication.
#[allow(clippy::too_many_arguments)]
fn compose_panel(ctx: &egui::Context, glue: &mut ProductGlue, category: i32, items: &HashMap<i32, ItemRecord>, hud: &mut HudState, assets: &Assets, ui: &mut UiState) {
    let recipes = glue.compose_list(assets, category);
    let checks: Vec<i32> = recipes.iter().map(|r| glue.check(assets, r.result, items)).collect();
    let rows: Vec<(String, Color32)> = recipes.iter().zip(&checks).map(|(r, c)| (item_name_for_tid(assets, r.result).unwrap_or_else(|| r.name.clone()), if *c == 0 { Color32::WHITE } else { UNAVAILABLE })).collect();
    let Some(c) = glue.compose.as_ref() else { return };
    let (selected, first_row, busy, skill_id) = (c.selected, c.first_row, c.busy, c.skill_id);
    let can_compose = selected.is_some_and(|id| skill_id != 0 && glue.check(assets, id, items) == 0) && !busy;
    let stock = bag_stock(items);
    let d = glue.data(assets);
    let title = d.category_name(category);
    // `COMPOSE_DISP_SEL_EDIT` : `sysstr` 31 (cuisine) ou 33 (alchimie), nom de la catégorie.
    let prompt = match category {
        1 => super::pvp_glue::fill(&d.sys(31), &[d.sys(28)]),
        2 => super::pvp_glue::fill(&d.sys(33), &[d.sys(29)]),
        _ => String::new(),
    };
    let recipe = selected.and_then(|id| d.recipe(id)).cloned();
    let mut detail = String::new();
    let mut grid: Vec<(i32, i32)> = Vec::new();
    let mut recipe_item = 0;
    if let Some(r) = &recipe {
        // `COMPOSE_DISP_MAKE_EDIT` (`FUN_1406c0a50`).
        let name = |tid: i32| item_name_for_tid(assets, tid).unwrap_or_else(|| tid.to_string());
        detail = format!("______[{}]______- {} -___", name(r.result), d.sys(38));
        for (tid, n) in &r.materials {
            detail.push_str(&format!("{} {n}{}___", name(*tid), d.sys(11)));
        }
        if r.recipe_tid > 0 {
            detail.push_str(&super::pvp_glue::fill(&d.sys(39), &[name(r.recipe_tid)]));
        }
        detail.push_str(&super::pvp_glue::fill(&d.sys(40), &[title.clone()]));
        // `COMPOSE_UD_MAKE_GRID` (`FUN_1406c1540`) : les matériaux s'ils sont tous là, puis l'objet recette du sac.
        if r.materials.iter().all(|(tid, n)| stock.get(tid).copied().unwrap_or(0) >= *n as i64) {
            grid.extend(r.materials.iter().copied());
            if r.recipe_tid > 0 {
                if let Some(it) = items.values().filter(|i| i.status == Some(STATUS_INVENTORY) && i.template_id == Some(r.recipe_tid)).min_by_key(|i| i.id) {
                    recipe_item = it.id;
                    grid.push((r.recipe_tid, 1));
                }
            }
        }
    }
    let (ok, cancel, title_missing) = (d.sys(36), d.sys(37), d.title_missing.clone());
    let selected_row = selected.and_then(|id| recipes.iter().position(|r| r.result == id));
    let mut out = Vec::new();
    let view = ComposeView { title: &title, prompt: &prompt, rows: &rows, selected: selected_row, first_row, detail: &detail, grid: &grid, ok: &ok, cancel: &cancel, can_compose };
    crate::hud::draw_compose(ctx, hud, assets, view, &mut out);
    if let Some(c) = glue.compose.as_mut() {
        c.recipe_item = recipe_item;
    }
    for r in out {
        match r {
            ComposeUi::Close => {
                glue.compose = None;
                return;
            }
            ComposeUi::Scroll(n) => {
                if let Some(c) = glue.compose.as_mut() {
                    c.first_row = (c.first_row as i32 + n).clamp(0, rows.len().saturating_sub(1) as i32) as usize;
                }
            }
            // `COMPOSE_LIST_ITEM_CLICK` : une recette irréalisable n'est pas retenue.
            ComposeUi::Select(k) => {
                if busy {
                    continue;
                }
                let ok = checks.get(k).copied() == Some(0);
                if let Some(c) = glue.compose.as_mut() {
                    c.selected = recipes.get(k).map(|r| r.result).filter(|_| ok);
                }
            }
            // `COMPOSE_OKBUTTON_CLICK` (`FUN_1406c20f0`).
            ComposeUi::Compose => {
                let (Some(r), Some(c)) = (recipe.as_ref(), glue.compose.as_mut()) else { continue };
                if c.busy {
                    continue;
                }
                if r.title != 0 {
                    if !title_missing.is_empty() {
                        hud.push_system(&title_missing);
                        ui.push_chat(title_missing.clone());
                    }
                    continue;
                }
                c.busy = true;
                let cmd = ProductCommand::Compose { skill_id: c.skill_id, recipe: r.result, recipe_item: c.recipe_item };
                glue.progress = Some((Instant::now(), COMPOSE_PROGRESS, cmd));
            }
        }
    }
}

impl App {
    /// Relevé des compétences du joueur (niveau, expérience de production) et du lancement d'une récolte.
    pub(super) fn product_observe(&mut self, ev: &WorldEvent) {
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        match ev {
            WorldEvent::Connected => {
                self.product.skills.clear();
                self.product.known.clear();
                self.product.gather = None;
                self.product.progress = None;
                self.product.compose = None;
                self.product.buy = None;
            }
            WorldEvent::Skill(SkillEvent::Known { owner, id, tid, level, exp }) if *owner == 0 || *owner == me || me == 0 => {
                self.product.skills.insert(*id, (*tid, *level, exp.unwrap_or(0)));
            }
            WorldEvent::Skill(SkillEvent::Level { id, level, exp }) => {
                if let Some(s) = self.product.skills.get_mut(id) {
                    s.1 = *level;
                    s.2 = exp.unwrap_or(s.2);
                }
            }
            WorldEvent::Skill(SkillEvent::Exp { id, exp }) => {
                if let Some(s) = self.product.skills.get_mut(id) {
                    s.2 = *exp;
                }
            }
            // Action de lancement de la compétence armée (`FUN_140684fc0`) : barre de 5,4 s, puis op 2.
            WorldEvent::Skill(SkillEvent::Cast { caster, id, .. }) if *caster == me && me != 0 => {
                if let Some((skill_id, target)) = self.product.gather.filter(|(s, _)| s == id) {
                    self.product.gather = None;
                    self.product.progress = Some((Instant::now(), GATHER_PROGRESS, ProductCommand::Gather { skill_id, target }));
                }
            }
            _ => {}
        }
    }

    /// Avant l'envoi d'un lancement (`FUN_1406850c0`) : rend `false` s'il ne doit pas partir.
    pub(super) fn product_before_cast(&mut self, tid: i32, id: i32, target: i32) -> bool {
        let assets = self.assets.clone();
        let d = self.product.data(&assets);
        if !d.product_skills.contains(&tid) {
            return true;
        }
        let name = d.name(tid);
        let using = super::pvp_glue::fill(&d.sys(SYSSTR_USING), &[name.clone()]);
        match d.category_of(tid).filter(|c| *c >= 1) {
            // Cuisine, alchimie : la fenêtre de fabrication, sans envoi.
            Some(category) => {
                self.ui.push_chat(using);
                self.product.compose = Some(Compose { category, skill_id: id, selected: None, recipe_item: 0, first_row: 0, busy: false });
                false
            }
            None => {
                let code = self.product_check_gather(tid, target);
                if code == 0 || code == result::TOO_FAR {
                    self.ui.push_chat(using);
                    self.product.gather = Some((id, target));
                    return true;
                }
                let d = self.product.data(&assets);
                let tool = d.tools.get(&tid).and_then(|t| item_name_for_tid(&assets, *t));
                for line in result_lines(d, false, code, &name, None, 0, 0, tool.as_deref()) {
                    self.ui.push_chat(line);
                }
                false
            }
        }
    }

    /// Contrôles d'une récolte (`FUN_1402a7e00`) : outil, cadavre, distance.
    fn product_check_gather(&mut self, tid: i32, target: i32) -> i32 {
        let assets = self.assets.clone();
        let Some((me, my_tid, at)) = self.player.as_ref().map(|p| (p.cid, p.tid, p.position)) else { return result::INVALID_TARGET };
        let tool = self.product.data(&assets).tools.get(&tid).copied();
        if let Some(tool) = tool {
            let owned = self.items.values().any(|i| i.template_id == Some(tool) && i.status == Some(STATUS_INVENTORY) && i.owner_cid.is_none_or(|o| o == me));
            if !owned {
                return result::NO_TOOL;
            }
        }
        let Some((monster, pos)) = self.entities.iter().find(|e| e.cid == target && !e.is_player && e.hp == Some(0)).map(|e| (e.tid, e.position)) else {
            return result::INVALID_TARGET;
        };
        // Classe du joueur (1 à 4) ; inconnue (0) : pas de contrôle de la classe.
        let class = super::convert_glue::Wearer::of(&assets, my_tid, 1).class as i32;
        let gatherable = self.product.data(&assets).gather.get(&monster).is_some_and(|l| l.iter().any(|(c, s)| *s == tid && (class == 0 || *c == class)));
        if !gatherable {
            return result::INVALID_TARGET;
        }
        if Vec3::new(pos.x - at.x, 0.0, pos.z - at.z).length() > GATHER_RANGE {
            return result::TOO_FAR;
        }
        0
    }

    /// Paquets AgpmProduct et AgpmEventProduct.
    pub(super) fn on_product_event(&mut self, ev: ProductEvent) {
        let assets = self.assets.clone();
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        match ev {
            ProductEvent::Recipes { cid, recipes } if cid == me || cid == 0 || me == 0 => self.product.known = recipes,
            ProductEvent::Recipes { .. } => {}
            ProductEvent::Result { cid, skill_id, item_tid, count, code, exp } => {
                if cid != me && cid != 0 {
                    return;
                }
                let tid = self.product.skills.get(&skill_id).map(|s| s.0).unwrap_or(0);
                let d = self.product.data(&assets);
                let compose = d.category_of(tid).is_some_and(|c| c >= 1);
                let item = (item_tid > 0).then(|| item_name_for_tid(&assets, item_tid)).flatten();
                let tool = d.tools.get(&tid).and_then(|t| item_name_for_tid(&assets, *t));
                for line in result_lines(d, compose, code, &d.name(tid), item.as_deref(), count, exp, tool.as_deref()) {
                    self.ui.push_chat(line);
                }
                // Fabrication : objet recette oublié, recette gardée si elle reste réalisable.
                if compose {
                    let selected = self.product.compose.as_ref().and_then(|c| c.selected);
                    if let Some(id) = selected {
                        let ok = self.product.check(&assets, id, &self.items) == 0;
                        if let Some(c) = self.product.compose.as_mut() {
                            c.recipe_item = 0;
                            if !ok {
                                c.selected = None;
                            }
                        }
                    }
                }
            }
            // `PDSK_EVENT_OPEN_NPCDIALOG` : catégorie de la fonction 24 du PNJ.
            ProductEvent::Granted { npc } => {
                let category = self.npc_functions(npc).iter().find(|f| f.function == FUNCTION).and_then(|f| f.data).unwrap_or(0);
                self.product.buy = Some(Buy { npc, category, first: 0, selected: None });
                self.product.confirm = None;
            }
            ProductEvent::SkillLearned { tid, code } => {
                let name = self.product.data(&assets).name(tid);
                self.product_learn_message(code, &name);
            }
            ProductEvent::RecipeBought { recipe, code } => {
                if code == learn_result::SUCCESS && !self.product.known.contains(&recipe) {
                    self.product.known.push(recipe);
                }
                let name = self.product.data(&assets).recipe(recipe).map(|r| r.name.clone()).unwrap_or_default();
                self.product_learn_message(code, &name);
            }
        }
    }

    /// Réponse du PNJ (`FUN_14063e4c0`, `FUN_14063d6a0`) : 0 appris, 3 gelds, 5 compétence absente.
    fn product_learn_message(&mut self, code: i32, name: &str) {
        let assets = self.assets.clone();
        let d = self.product.data(&assets);
        let line = match code {
            learn_result::SUCCESS => format!("{name} {}", d.sys(427)),
            learn_result::NOT_ENOUGH_MONEY => d.sys(429),
            learn_result::NO_SKILL => super::pvp_glue::fill(&d.sys(430), &[name.to_string()]),
            _ => return,
        };
        self.ui.push_chat(line);
        if let Some(b) = self.product.buy.as_mut() {
            b.selected = None;
        }
    }

    /// À chaque image : compétences lancées depuis `UI_PDSkill`, fenêtre du PNJ fermée en s'éloignant.
    pub(super) fn product_frame(&mut self) {
        for tid in std::mem::take(&mut self.product.pending_cast) {
            self.skills_cast(tid);
        }
        let Some(me) = self.player.as_ref().map(|p| p.position) else { return };
        if let Some(npc) = self.product.buy.as_ref().map(|b| b.npc) {
            let far = self.entities.iter().find(|e| e.cid == npc).is_none_or(|e| Vec3::new(e.position.x - me.x, 0.0, e.position.z - me.z).length() > CLOSE_RANGE);
            if far {
                self.product.buy = None;
                self.product.confirm = None;
            }
        }
    }

    /// `/production` : fenêtre des compétences de production ; `/recettes` : recettes connues.
    pub(super) fn product_slash(&mut self, cmd: &str) -> bool {
        match cmd {
            "/production" | "/craft" => self.product.show_skills = !self.product.show_skills,
            "/recettes" | "/recipes" => {
                let assets = self.assets.clone();
                let known = self.product.known.clone();
                let names: Vec<String> = known.iter().map(|id| item_name_for_tid(&assets, *id).unwrap_or_else(|| id.to_string())).collect();
                self.ui.push_chat(format!("[production] {} recette(s) : {}", names.len(), names.join(", ")));
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> HashMap<String, Vec<i32>> {
        [("Collection Skill", 663), ("Cooking", 203), ("Alchemy", 285)].into_iter().map(|(n, t)| (n.to_string(), vec![t])).collect()
    }

    #[test]
    fn tables_de_production() {
        let by_name = names();
        let cats = parse_categories("분류\tSkill\tNemu\r\nHunt\tCollection Skill\t\t100\t1\t\tAdventurer\r\nCook\tCooking\t\t1000\t15\tMastery Level\tCook\r\n\t\tBroiled Meat\t500\t1\r\nAlchemy\tAlchemy\t\t2000\t20\r\n", &by_name);
        assert_eq!(cats, vec![(0, 663), (1, 203), (2, 285)]);

        let compose = parse_compose("MakeId\tCategory\tResult_TID\r\n\tCook\t644\tBroiled Meat of Vitality\t\t1\t500\t70\t623\tSalty Seasoning\t5\t4960\tBug Meat\t3\t4963\tSpider Web\t1\t641\tPotato\t1\r\n\tNone\t1\tx\r\n");
        assert_eq!(compose.len(), 1);
        let r = &compose[0];
        assert_eq!((r.category, r.result, r.level, r.price, r.recipe_tid, r.title), (1, 644, 1, 500, 623, 0));
        assert_eq!(r.materials, vec![(4960, 3), (4963, 1), (641, 1)]);

        let exp = parse_exp("h\r\nCollection Skill\t1\t5\t7\t0\t103\t103\r\nCollection Skill\t2\t7\t9\t2\t260\t363\r\nCollection Skill\t3\t10\t13\t4\t597\t960\r\nCooking\t16\t1\t1\t1\t1\t1\r\n", &by_name);
        assert_eq!(exp[&663].len(), 3);
        assert!(!exp.contains_key(&203), "niveau hors bornes ignoré");
        let d = ProductData { exp, ..Default::default() };
        // Niveau 3, 1000 points cumulés : 1000 − 363 = 637 sur 597 (la barre est bornée à 100 % ensuite).
        assert_eq!(d.level_exp(663, 3, 1000), (637, 597));
        assert_eq!(d.level_exp(663, 1, 50), (50, 103));
        assert_eq!(d.level_exp(663, 2, 10), (0, 260));

        let gather = parse_gather("Target Type\tTarget TID\tClass\tSkill TID\tProbability\tItem Set\r\n\t33\t1;2;3;4\t663\t90\t2\r\n\t34\t\t\t\t\r\n");
        assert_eq!(gather[&33].len(), 4);
        assert!(!gather.contains_key(&34));

        let (skills, tools) = parse_spec("Name\tattribute\tupgradable\tcheck_range\ttarget\trange\trequirement\tcondition\tother_condition\targ1\r\nCooking\t8\t1\t2\t100\t\t4\t\t\t655\r\nCollection Skill\t8\t2\t1\t1\t202\t4\t\t\t654\r\nAlchemy\t3\t1\r\n", &by_name);
        assert_eq!(skills, HashSet::from([203, 663]));
        assert_eq!((tools[&203], tools[&663]), (655, 654));

        let learn = parse_learn("name\tskill_level\trequire_level\tskill_cost\r\nCooking\t1\t10\t500\r\n\t2\t12\t900\r\nAlchemy\t1\t15\t1000\r\n", &by_name);
        assert_eq!((learn[&203], learn[&285]), ((10, 500), (15, 1000)));
    }

    #[test]
    fn controle_de_fabrication() {
        let r = Recipe { category: 1, result: 644, level: 3, materials: vec![(4960, 3), (641, 1)], ..Default::default() };
        let stock: HashMap<i32, i64> = [(4960, 3), (641, 2)].into_iter().collect();
        assert_eq!(check_compose(&r, &[644], &stock, Some(3), false), 0);
        assert_eq!(check_compose(&r, &[], &stock, Some(3), false), result::UNKNOWN_RECIPE);
        assert_eq!(check_compose(&r, &[644], &stock, Some(2), false), result::LOW_LEVEL);
        assert_eq!(check_compose(&r, &[644], &[(4960, 2)].into_iter().collect(), Some(3), false), result::NOT_ENOUGH_MATERIAL);
        assert_eq!(check_compose(&r, &[644], &stock, Some(3), true), -1);
    }

    #[test]
    fn messages_de_resultat() {
        let mut d = ProductData::default();
        for (id, t) in [(41, "has succeeded."), (44, "Mastery Level"), (45, "has failed."), (46, "You do not have %s item needed to use %s."), (47, "This object cannot be used with %s."), (49, "Not enough space to store the item."), (51, "Cannot collect any more."), (351, "Item"), (517, "Obtained"), (518, "has increased")] {
            d.sysstr.insert(id, t.to_string());
        }
        assert_eq!(
            result_lines(&d, false, 0, "Collection Skill", Some("Spider Web"), 1, 12, None),
            vec!["Collection Skill has succeeded.", "Spider Web Item 1 Obtained", "Collection Skill Mastery Level 12 has increased"]
        );
        assert_eq!(result_lines(&d, true, 0, "Cooking", None, 0, 3, None), vec!["Cooking has succeeded.", "CookingMastery Level 3 has increased"]);
        assert_eq!(result_lines(&d, false, 2, "Collection Skill", None, 0, 0, Some("Gathering Knife")), vec!["You do not have Collection Skill item needed to use Gathering Knife."]);
        assert_eq!(result_lines(&d, false, 5, "Collection Skill", Some("Skin"), 0, 0, None), vec!["Skin Not enough space to store the item.", "Cannot collect any more."]);
        assert_eq!(result_lines(&d, true, 5, "Cooking", Some("Soup"), 0, 0, None), vec!["Soup Not enough space to store the item."]);
        assert!(result_lines(&d, false, 6, "Collection Skill", Some("x"), 0, 0, None).is_empty());
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : catégories, recettes, monstres et outils.
    #[test]
    fn donnees_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let d = load_data(&assets);
        assert_eq!(d.categories, vec![(0, 663), (1, 203), (2, 285)]);
        assert_eq!(d.recipes.len(), 95);
        assert_eq!(d.recipe(644).map(|r| (r.category, r.price, r.materials.len())), Some((1, 500, 3)));
        assert!(d.gather.get(&33).is_some_and(|l| l.contains(&(1, 663))));
        assert_eq!((d.tools.get(&663), d.tools.get(&203), d.tools.get(&285)), (Some(&654), Some(&655), Some(&1016)));
        assert!(d.product_skills.contains(&663));
        assert_eq!(d.exp.get(&663).map(|e| e.len()), Some(15));
        // Fenêtres d'Archonia et contrôles dessinés.
        let ui = assets.table("ini", "UI_1024X768.ini").unwrap();
        for (name, controls) in [("UI_Compose", ["CTL_LIST_SEL", "CTL_GRID_SRC", "CTL_BTN_OK"]), ("UI_PDSkillBuy", ["CTL_LIST", "CTL_LIST_ITEM", "CTL_ST_DISABLE"]), ("UI_PDSkill", ["CTL_LIST", "CTL_LISTITEM", "CTL_BAR_SKILL"])] {
            let w = crate::hud::UiWindow::parse(&ui, None, name).unwrap();
            assert!(controls.iter().all(|c| w.control(c).is_some()), "{name}");
        }
    }
}
