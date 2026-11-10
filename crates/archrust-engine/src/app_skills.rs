//! Compétences côté moteur : livre (`K`, `/competences`), instructeurs, barre de raccourcis (touches
//! 1 à =) et lancement. Protocole : `archrust_netclient::skills` ; fenêtres : `hud_skills.rs`.
//!
//! * Données du client : noms et TID dans `SKILLTEMPLATE.ini`, niveaux dans `SKILL_CONST.txt` (classes
//!   admises `require_class_*`, niveau et points exigés, prix d'apprentissage et d'amélioration).
//! * Un instructeur enseigne les compétences dont le niveau 1 admet la classe de sa vue (race et
//!   classe de la fonction 25), comme la règle du serveur.
//! * Lancement : sur la cible choisie, sinon sur soi ; le serveur mène la marche jusqu'à portée et
//!   annonce le lancement (op 7), d'où vient le délai de relance affiché. Compétences au sol : repère puis
//!   clic, voir `app_groundcast.rs`.
//! * Maîtrises (`app_mastery.rs`, `hud_mastery.rs`) : onglets Combat… Passive filtrés par `SKILLMASTERY.txt`, AL
//!   (compétences d'ArchLord), arbres Combo (supérieur), Heroic et du Dragon Scion ; un instructeur d'un
//!   Dragon Scion (race 4) ouvre `UI_SkillUpDragonScion_Scion` (`Skill_OpenUpUIScion`), les autres
//!   `UI_SkillBuy`. « Apprendre » / « Améliorer » d'un arbre demande confirmation (`Skill_ConfirmLearnSkill`,
//!   `Skill_ConfirmUpgradeSkill`), puis envoie 0x3B op 1 (TID) ou op 2 (identifiant).
//! * Barre : gardée par le serveur (AgpmUIStatus 0x14, `archrust_netclient::ui_status`). Les 50 cases
//!   du serveur (couche × 10 + colonne) sont les cases locales 2 à 51 (barre principale puis barres en
//!   plus) ; les potions (cases 0 et 1) sont les champs 6 et 7. L'op 0 reçue remplace les 50 cases
//!   (objet : IID → TID par l'inventaire ; compétence : identifiant d'instance → TID par la liste des
//!   compétences) ; chaque pose ou retrait envoie l'op 1 (index + mini, ou index seul), une potion posée
//!   son TID (jamais 0), un seuil déplacé sa valeur une seconde après le dernier mouvement. Sans champ 2
//!   dans l'op 0 (serveur qui ne garde pas la barre), repli sur
//!   `saved-variables/raccourcis-<personnage>.txt`, qui ne garde sinon que le nombre de barres affichées.

use super::App;
use crate::assets::Assets;
use super::mastery_glue::{MasteryRow, MasteryTables};
use crate::hud::{BookEntry, BookView, HudState, Shortcut, SkillTab, SkillUi, SlotView, TrainerEntry, TrainerView, TreeSlot, TreeView, EXTRA_BELTS, QUICK_SLOTS};
use archrust_netclient::world::factor_part;
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

/// Événement `Skill_Cast_*` d'un type d'action refusé (AGPMSKILL op 7 ; énumération de la référence, noms de
/// `FUN_1405d5d40`). 13 (carreaux) : Archonia n'a pas de texte.
fn cast_refused_event(kind: i32) -> Option<&'static str> {
    Some(match kind {
        5 => "Skill_Cast_Miss",
        6 => "Skill_Cast_Failed",
        7 => "Skill_Cast_InvalidTarget",
        8 => "Skill_Cast_NeedRequirement",
        9 => "Skill_Cast_NotEnoughHP",
        10 => "Skill_Cast_NotEnoughMP",
        11 => "Skill_Cast_NotEnoughSP",
        12 => "Skill_Cast_NotEnoughArrow",
        14 => "Skill_Cast_AlreadyUse",
        15 => "Skill_Cast_NotReadyCast",
        16 => "Skill_Cast_InvalidTargetToGather",
        17 => "Skill_Cast_NotEnoughProduct",
        _ => return None,
    })
}

/// Un niveau de compétence (`SKILL_CONST.txt`).
#[derive(Debug, Clone, Default)]
struct Level {
    classes: u16,
    require_level: i32,
    require_point: i32,
    /// Points héroïques et charisme exigés (`require_heroic_point`, `require_charisma_point`).
    require_heroic: i32,
    require_charisma: i32,
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
    /// `area_r` × 100 (`+0x6148`) : largeur du repère du ciblage au sol (voir `app_groundcast.rs`).
    area_r: f32,
}

#[derive(Debug, Clone, Default)]
struct SkillData {
    name: String,
    /// Niveaux 1, 2… dans l'ordre.
    levels: Vec<Level>,
    /// `skill_type` de SKILL_CONST (9 : compétence d'ArchLord, `FUN_1402fa5c0`).
    kind: i32,
    /// Drapeaux de `SKILL_SPEC.txt` du décompte des maîtrises.
    spec: Spec,
}

/// Drapeaux de `SKILL_SPEC.txt` (une ligne par compétence, chargeur FUN_14023d850) lus par le décompte des
/// niveaux des maîtrises (FUN_1402fdc60, FUN_1402fd210).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Spec {
    /// Reste 9 d'`attribute` (bit 0x400) : passive.
    passive: bool,
    /// Reste 8 d'`attribute` (bit 0x2000) : production, niveau compté borné à 100 au lieu de 30.
    product: bool,
    /// `transform_target` 5 (« … of Evolution », drapeaux 0x8000000001000).
    evolution: bool,
    /// `auto_attack` rempli (attaque normale, `+0x13e60`).
    auto_attack: bool,
    /// `range` 7, 701 ou 702 : compétence au sol (drapeau 0x4 de `+0xf0`, `FUN_14023d850`, `FUN_1402fed30`).
    ground: bool,
}

/// Reste d'`attribute` une fois ôtés, dans l'ordre, 40000, 20000, 10000, 1000, 200, 100, 40, puis 20 ou 10
/// (FUN_14023d850) : 1 à 6 éléments, 8 production, 9 passive.
fn attribute_rest(attribute: i32) -> i32 {
    let mut a = attribute;
    for step in [40000, 20000, 10000, 1000, 200, 100, 40] {
        if a >= step {
            a -= step;
        }
    }
    if a >= 20 {
        a - 20
    } else if a >= 10 {
        a - 10
    } else {
        a
    }
}

/// `SKILL_SPEC.txt` : drapeaux par nom de compétence (première ligne de chaque nom).
fn parse_spec(text: &str) -> HashMap<String, Spec> {
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap_or("").split('\t').map(str::trim).collect();
    let col = |n: &str| header.iter().position(|h| h.eq_ignore_ascii_case(n));
    let (c_attr, c_transform, c_auto, c_range) = (col("attribute"), col("transform_target"), col("auto_attack"), col("range"));
    let mut out = HashMap::new();
    for line in lines {
        let f: Vec<&str> = line.split('\t').map(str::trim).collect();
        let get = |c: Option<usize>| c.and_then(|i| f.get(i)).copied().unwrap_or("");
        let num = |c: Option<usize>| get(c).parse::<i32>().unwrap_or(0);
        let name = f.first().copied().unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let rest = (!get(c_attr).is_empty()).then(|| attribute_rest(num(c_attr)));
        out.entry(name.to_string()).or_insert(Spec {
            passive: rest == Some(9),
            product: rest == Some(8),
            evolution: num(c_transform) == 5,
            auto_attack: !get(c_auto).is_empty(),
            ground: matches!(num(c_range), 7 | 701 | 702),
        });
    }
    out
}

/// Niveau compté d'une compétence apprise (FUN_1402fd210, « dirt » 4) : borné à 30, à 100 pour une production.
fn counted_level(d: &SkillData, level: i32) -> i32 {
    level.clamp(0, if d.spec.product { 100 } else { 30 })
}

/// Le niveau 1 de la compétence admet la race et la classe (colonne `require_class_*` remplie, FUN_1402fe170) :
/// races 1 à 3 en classes 1, 2, 4 ; race 4 en classes 1 (slayer), 2 (obiter), 3 (scion), 4 (summoner).
fn admits(d: &SkillData, race: i32, class: i32) -> bool {
    let col = match (race, class) {
        (1..=3, 1 | 2 | 4) | (4, 1..=4) => class_column(race, class),
        _ => None,
    };
    col.zip(d.levels.first()).is_some_and(|(c, l)| l.classes & (1 << c) != 0)
}

/// Niveaux cumulés des compétences apprises (FUN_1402fdc60) : de toutes sans race ni classe (ConditionD de l'arbre
/// supérieur, appel (-1, -1), sans exclusion) ; sinon de celles qu'admettent la race et la classe, hors passives,
/// évolutions et attaque normale (ConditionC).
/// Partie « arbre » du contrôle de la Return Orb (`FUN_1402bcc70` pour `UI_SkillHighLevel`, `FUN_1402bd5f0` pour les
/// fenêtres héroïques et du Scion) : la compétence `tid` est dans l'arbre `rows`, aucune autre compétence apprise de
/// l'arbre ne l'exige (ConditionA/B de niveau non nul) et le total qui resterait (`total_after`, niveaux cumulés ou
/// points héroïques dépensés sans elle) suffit encore à chaque autre case apprise (`FUN_1402be4b0`, ConditionD).
fn tree_refundable(rows: &[MasteryRow], tid: i32, known: &HashMap<i32, i32>, total_after: i32) -> bool {
    if !rows.iter().any(|r| r.tid == tid) {
        return false;
    }
    let learned = |r: &&MasteryRow| r.tid != tid && known.get(&r.tid).is_some_and(|l| *l > 0);
    !rows.iter().filter(learned).any(|r| r.requires.iter().any(|(t, n)| *t == tid && *n > 0) || total_after < r.total)
}

fn mastery_levels(by_tid: &HashMap<i32, SkillData>, known: &HashMap<i32, i32>, race_class: Option<(i32, i32)>) -> i32 {
    known
        .iter()
        .filter_map(|(tid, lv)| {
            let d = by_tid.get(tid)?;
            let counted = race_class.is_none_or(|(r, c)| admits(d, r, c) && !d.spec.passive && !d.spec.evolution && !d.spec.auto_attack);
            counted.then(|| counted_level(d, *lv))
        })
        .sum()
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

/// Durée en ms écrite en secondes comme FUN_140875040 : « %.1f %s » pour l'incantation, « %d %s » (secondes
/// entières) pour la relance et la durée ; `unit` est sysstr 8.
fn seconds(ms: i64, tenth: bool, unit: &str) -> String {
    if tenth {
        format!("{:.1} {unit}", ms as f64 / 1000.0)
    } else {
        format!("{} {unit}", ms / 1000)
    }
}

fn load_tables(assets: &Assets) -> SkillTables {
    // SKILL_CONST.txt : une ligne par niveau, le nom sur la première (en clair ou chiffrée, CP949 : les
    // noms doivent rester ceux de SKILLTEMPLATE.ini).
    let text = assets.read_text("ini", "SKILL_CONST.txt").unwrap_or_default();
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
            require_heroic: num(col("require_heroic_point")) as i32,
            require_charisma: num(col("require_charisma_point")) as i32,
            cost: num(c_cost) as i64,
            upgrade_cost: num(c_up) as i64,
            cost_hp: num(col("cost_hp")) as i64,
            cost_mp: num(col("cost_mp")) as i64,
            cost_arrow: num(col("cost_arrow")) as i64,
            cast_ms: num(col("cast_time")) as i64,
            recast_ms: num(col("recast_time")) as i64,
            duration_ms: num(col("duration")) as i64,
            area_r: num(col("area_r")) as f32 * 100.0,
        });
    }
    // SKILL_SPEC.txt : drapeaux du décompte des maîtrises.
    let spec = assets.read_text("ini", "SKILL_SPEC.txt").map(|t| parse_spec(&t)).unwrap_or_default();
    // SKILLTEMPLATE.ini : TID → nom.
    let mut tables = SkillTables::default();
    if let Ok(t) = assets.table("ini", "SKILLTEMPLATE.ini") {
        let name_col = t.section("").and_then(|h| h.entries.iter().find(|(_, v)| v.eq_ignore_ascii_case("Name")).map(|(k, _)| k.clone()));
        for s in &t.sections {
            let (Ok(tid), Some(name)) = (s.name.parse::<i32>(), name_col.as_deref().and_then(|c| s.get(c))) else { continue };
            let name = name.trim().to_string();
            let levels = by_name.get(&name).cloned().unwrap_or_default();
            let kind = kinds.get(&name).copied().unwrap_or(0);
            let spec = spec.get(&name).copied().unwrap_or_default();
            tables.by_tid.insert(tid, SkillData { name, levels, kind, spec });
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
    /// Blocage local du lancement et dernier ordre de marche demandé pendant celui-ci.
    cast_until: Option<Instant>,
    cast_move: Option<(i32, Vec3)>,
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
    /// Paquets 0x14 reçus (op 0 et op 1), appliqués par `skills_frame` après le chargement de la barre
    /// du personnage.
    server_status: Vec<archrust_netclient::ui_status::UiStatus>,
    /// La barre est gardée par le serveur (op 0 avec champ 2) : plus de cases dans le fichier local.
    server_belt: bool,
    /// Cases reçues dont l'objet ou la compétence n'est pas encore connu : (case locale, base).
    unresolved: Vec<(usize, archrust_netclient::ui_status::BeltBase)>,
    /// Seuils connus du serveur et dernier mouvement d'un curseur (envoi une seconde après).
    auto_sent: [i8; 2],
    auto_moved: [Option<Instant>; 2],
    /// Arbres et catégories des maîtrises (`app_mastery.rs`).
    mastery: Option<MasteryTables>,
    /// Infobulles déjà composées, par (TID, niveau connu, bonus de niveau).
    tips_cache: HashMap<(i32, i32, i32), Vec<(String, Color32)>>,
    /// Onglet du livre (`None` : celui d'ouverture, Combat ou Scion) et de l'instructeur.
    book_tab: Option<SkillTab>,
    trainer_tab: SkillTab,
    /// Case choisie dans un arbre (TID).
    tree_selected: Option<i32>,
    /// Compétence choisie dans la liste du livre (TID, `Skill_SelectGrid`).
    book_selected: Option<i32>,
    /// Return Orb : compétence (TID) en attente de `Skill_RollbackConfirm` / `Skill_Init_Confirm_Message`, puis
    /// confirmée (envoi à l'image suivante, `FUN_1405db3a0` refait les contrôles).
    rollback_confirm: Option<i32>,
    rollback_ok: Option<i32>,
    /// Confirmation en attente : TID et apprentissage (sinon amélioration).
    confirm: Option<(i32, bool)>,
    /// Cette confirmation vient de l'instructeur (`Skill_ConfirmBuySkill`) et non d'un arbre.
    confirm_buy: bool,
    /// Boîte d'information (`Action=4:…` des événements `Skill_*` de l'[EventMap]).
    info: Option<String>,
    /// Messages des événements d'interface (`[EventMap]`).
    events: Option<HashMap<String, String>>,
    /// Bonus de niveau de toutes les compétences (AGPMSKILL op 14, attache+0x854) et de chacune (op 16,
    /// compétence+0x188), par identifiant.
    level_bonus: i32,
    skill_bonus: HashMap<i32, i32>,
}

/// Bonus de niveau d'une compétence apprise (FUN_1408762b0) : bonus du personnage (+0x854 des données de
/// compétence, FUN_1402ff810, AGPMSKILL op 14), +1 si le titre porté est lié à la compétence (FUN_1401c3e90,
/// `TitleRelatedSkills.txt`, ajout d'Archonia écarté) et bonus propre à la compétence (+0x188, op 16).
fn skill_level_bonus(glue: &SkillsGlue, tid: i32) -> i32 {
    glue.level_bonus + glue.id_of(tid).and_then(|id| glue.skill_bonus.get(&id)).copied().unwrap_or(0)
}

/// En-tête « %s : %d », ou « %s : %d (+%d) » avec un bonus (FUN_1408762b0, FUN_14087cb70).
fn level_header(word: &str, level: i32, bonus: i32) -> String {
    if bonus == 0 {
        format!("{word} : {level}")
    } else {
        format!("{word} : {level} (+{bonus})")
    }
}

impl SkillsGlue {
    fn tables(&mut self, assets: &Assets) -> &SkillTables {
        self.tables.get_or_insert_with(|| load_tables(assets))
    }

    fn mastery(&mut self, assets: &Assets) -> &MasteryTables {
        self.mastery.get_or_insert_with(|| MasteryTables::load(assets))
    }

    /// Points héroïques dépensés dans les niveaux connus (ConditionD d'un arbre héroïque, FUN_1402b9200).
    fn heroic_spent(&mut self, assets: &Assets, known: &HashMap<i32, i32>) -> i32 {
        let tables = self.tables(assets);
        known.iter().filter_map(|(tid, lv)| tables.by_tid.get(tid).map(|d| d.levels.iter().take((*lv).max(0) as usize).map(|l| l.require_heroic).sum::<i32>())).sum()
    }

    fn name(&mut self, assets: &Assets, tid: i32) -> String {
        self.tables(assets).by_tid.get(&tid).map(|d| d.name.clone()).unwrap_or_default()
    }

    /// Infobulle d'une compétence dont le personnage a le niveau `owned` (0 : pas apprise), comme celle
    /// d'Archonia (FUN_14087c230) : nom, description (coupée aux « \n », FUN_1408777c0), puis
    /// * apprise : « Niveau actuel : %d » (sysstr 24, « %s : %d (+%d) » avec un bonus, FUN_1408762b0) et les
    ///   lignes de ce niveau, puis, sous le niveau maximal, « Niveau suivant : %d » (sysstr 25, FUN_14087cb70)
    ///   et les lignes du niveau suivant ;
    /// * pas apprise : les lignes du premier niveau, sans en-tête (FUN_14087cb70 sans compétence).
    /// Lignes (FUN_14087c340) : celles que `skilltooltip.txt` marque (libellés de `sysstr.txt` 67 à 79 : niveau
    /// requis, points, incantation, relance, durée, coûts).
    ///
    /// Gardée par (TID, niveau, bonus) : les vues sont refaites à chaque image pour toute la barre, l'arbre ou la
    /// liste de l'instructeur, et le texte ne dépend de rien d'autre.
    fn tooltip(&mut self, assets: &Assets, tid: i32, owned: i32) -> Vec<(String, Color32)> {
        let key = (tid, owned, if owned > 0 { skill_level_bonus(self, tid) } else { 0 });
        if let Some(lines) = self.tips_cache.get(&key) {
            return lines.clone();
        }
        let lines = self.tooltip_lines(assets, tid, owned);
        self.tips_cache.insert(key, lines.clone());
        lines
    }

    fn tooltip_lines(&mut self, assets: &Assets, tid: i32, owned: i32) -> Vec<(String, Color32)> {
        let name = self.name(assets, tid);
        let tables = self.tables(assets);
        let data = tables.by_tid.get(&tid).cloned();
        let key = data.as_ref().map(|d| d.name.to_lowercase()).unwrap_or_default();
        let row = tables.tips.get(&key).cloned();
        let description = tables.tips_local.get(&key).map(|r| r.description.clone()).filter(|d| !d.is_empty()).or_else(|| row.as_ref().map(|r| r.description.clone())).unwrap_or_default();
        let mut lines = vec![(name, Color32::from_rgb(255, 210, 110))];
        for part in description.split("\\n").filter(|p| !p.is_empty()) {
            lines.push((part.to_string(), Color32::from_rgb(225, 225, 225)));
        }
        let max = data.as_ref().map_or(0, |d| d.levels.len() as i32);
        if owned <= 0 {
            self.tooltip_rows(assets, data.as_ref(), row.as_ref(), 1, Color32::from_rgb(200, 220, 255), &mut lines);
            return lines;
        }
        let bonus = skill_level_bonus(self, tid);
        let current = owned + bonus;
        let [now_word, next_word] = [24, 25].map(|n| super::npc_glue::sysstr(assets, n).unwrap_or_default());
        lines.push((level_header(&now_word, current, bonus), Color32::from_rgb(255, 235, 170)));
        self.tooltip_rows(assets, data.as_ref(), row.as_ref(), current, Color32::from_rgb(200, 220, 255), &mut lines);
        if current < max {
            lines.push((level_header(&next_word, current + 1, bonus), Color32::from_rgb(200, 200, 200)));
            self.tooltip_rows(assets, data.as_ref(), row.as_ref(), current + 1, Color32::from_rgb(170, 180, 200), &mut lines);
        }
        lines
    }

    /// Lignes d'un niveau de l'infobulle (FUN_14087c340, niveau borné à 30 comme Archonia).
    fn tooltip_rows(&mut self, assets: &Assets, data: Option<&SkillData>, row: Option<&TipRow>, level: i32, color: Color32, lines: &mut Vec<(String, Color32)>) {
        let level = level.clamp(1, 30);
        let lv = data.and_then(|d| d.levels.get((level - 1) as usize).or(d.levels.last())).cloned();
        let (Some(row), Some(lv)) = (row, lv) else { return };
        let unit = super::npc_glue::sysstr(assets, 8).unwrap_or_default();
        // Points héroïques et charisme exigés : colonnes `RequiredHeroicPoints`, `RequireCharismaPoints` de
        // `skilltooltip.txt`, libellés sysstr 791 et 792.
        let rows: [(&str, u32, Option<String>); 10] = [
            ("RequiredLevel", 67, (lv.require_level > 0).then(|| lv.require_level.to_string())),
            ("RequiredPoints", 68, (lv.require_point > 0).then(|| lv.require_point.to_string())),
            ("RequiredHeroicPoints", 791, (lv.require_heroic > 0).then(|| lv.require_heroic.to_string())),
            ("RequireCharismaPoints", 792, (lv.require_charisma > 0).then(|| lv.require_charisma.to_string())),
            ("CastTime", 69, (lv.cast_ms > 0).then(|| seconds(lv.cast_ms, true, &unit))),
            ("RecastTime", 70, (lv.recast_ms > 0).then(|| seconds(lv.recast_ms, false, &unit))),
            ("Duration", 71, (lv.duration_ms > 0).then(|| seconds(lv.duration_ms, false, &unit))),
            ("CostHP", 77, (lv.cost_hp > 0).then(|| lv.cost_hp.to_string())),
            ("CostMP", 78, (lv.cost_mp > 0).then(|| lv.cost_mp.to_string())),
            ("CostArrow", 79, (lv.cost_arrow > 0).then(|| lv.cost_arrow.to_string())),
        ];
        for (column, label, value) in rows {
            let Some(value) = value.filter(|_| row.shown.contains(column)) else { continue };
            let label = super::npc_glue::sysstr(assets, label).unwrap_or_else(|| column.to_string());
            lines.push((format!("{label} : {value}"), color));
        }
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
/// Délai d'envoi d'un seuil après le dernier mouvement de son curseur (OnIdle FUN_140650b00).
const AUTO_SEND_DELAY: Duration = Duration::from_secs(1);
/// Cases locales avant la barre : les deux potions.
const POTION_SLOTS: usize = 2;

/// Case locale d'une case du serveur (couche × 10 + colonne) : la barre principale suit les potions,
/// les barres en plus la suivent (12 + (couche − 1) × 10 + colonne = 2 + index).
fn local_slot(index: usize) -> usize {
    index + POTION_SLOTS
}

/// Case du serveur d'une case locale (`None` : potions).
fn belt_index(slot: usize) -> Option<usize> {
    slot.checked_sub(POTION_SLOTS).filter(|i| *i < archrust_netclient::ui_status::BELT_SLOTS)
}

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
    book_tab: SkillTab,
    /// Cases de l'arbre du livre ou de l'instructeur ouvert, nom de la classe, points héroïques.
    book_tree: Vec<Option<TreeSlot>>,
    trainer_tree: Vec<Option<TreeSlot>>,
    title: String,
    heroic_points: Option<i32>,
    /// Onglets ouvrables (arbre connu pour la classe).
    allowed: Vec<SkillTab>,
    confirm: Option<String>,
    /// « Reset Selected » actif pour la compétence choisie du livre, et texte de sa confirmation.
    rollback: bool,
    rollback_text: Option<String>,
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
    let allowed = |t: SkillTab| views.allowed.contains(&t);
    if glue.show_book {
        let tab = views.book_tab;
        if tab.is_tree() {
            let points = if tab.heroic() { views.heroic_points } else { views.points };
            let view = TreeView { tab, trainer: false, title: views.title.clone(), slots: &views.book_tree, points, selected: glue.tree_selected, allowed: &allowed, rollback: views.rollback };
            crate::hud::draw_tree(ctx, hud, assets, view, &mut out);
        } else {
            let view = BookView { entries: &views.book, points: views.points, first: &mut glue.book_first, tab, allowed: &allowed, title: views.title.clone(), selected: glue.book_selected, rollback: views.rollback };
            crate::hud::draw_skill_book(ctx, hud, assets, view, &mut out);
        }
    }
    if let Some((title, entries)) = &views.trainer {
        let tab = glue.trainer_tab;
        if tab.is_tree() {
            let points = if tab.heroic() { views.heroic_points } else { views.points };
            let view = TreeView { tab, trainer: true, title: title.clone(), slots: &views.trainer_tree, points, selected: glue.tree_selected, allowed: &allowed, rollback: false };
            crate::hud::draw_tree(ctx, hud, assets, view, &mut out);
        } else {
            let view = TrainerView { title: title.clone(), entries, selected: glue.trainer_selected, points: views.points, first: &mut glue.trainer_first, tab, allowed: &allowed };
            crate::hud::draw_trainer(ctx, hud, assets, view, &mut out);
        }
    }
    if let Some(text) = glue.info.clone() {
        if crate::hud::info_box(ctx, hud, assets, &text) {
            glue.info = None;
        }
    }
    // Confirmation d'Archonia avant l'apprentissage ou l'amélioration d'une case d'arbre.
    if let (Some(text), Some((tid, learn))) = (&views.confirm, glue.confirm) {
        if let Some(yes) = crate::hud::confirm_box(ctx, hud, assets, ("skill-tree-confirm", tid), text) {
            glue.confirm = None;
            if yes {
                out.push(if learn { SkillUi::Learn(tid) } else { SkillUi::Upgrade(tid) });
            }
        }
    }
    // Return Orb : boîte modale de l'[EventMap] (`Skill_RollbackConfirm`) ou d'uimessage (`Skill_Init_Confirm_Message`).
    if let (Some(text), Some(tid)) = (&views.rollback_text, glue.rollback_confirm) {
        if let Some(yes) = crate::hud::confirm_box(ctx, hud, assets, ("skill-rollback-confirm", tid), text) {
            glue.rollback_confirm = None;
            if yes {
                glue.rollback_ok = Some(tid);
            }
        }
    }
    glue.pending.extend(out);
}

fn send(net: Option<&NetLink>, cmd: SkillCommand) {
    if let Some(net) = net {
        net.send(Command::Skill(cmd));
    }
}

impl App {
    /// Compétences apprises qui ont un prix (`FUN_1402ba630` avec `param_5` nul : prix du niveau 1, taxe comprise,
    /// non nul ; « Normal Attack » exclue) : la Gem of origin est refusée s'il n'y en a aucune (`SkillInitNoOwnSkill`).
    pub(super) fn skills_paid_count(&mut self) -> usize {
        let assets = self.assets.clone();
        let tids: Vec<i32> = self.skills.known.values().map(|(t, _)| *t).filter(|t| *t != NORMAL_ATTACK).collect();
        let tables = self.skills.tables(&assets);
        tids.iter().filter(|t| tables.by_tid.get(t).and_then(|d| d.levels.first()).is_some_and(|l| super::tax_glue::taxed(l.cost) != 0)).count()
    }

    /// Blocage local des actions jusqu'à `until`, hors lancement de compétence (ramassage sur un objet de quête,
    /// FUN_14017acd0 type 4 : voir `app_quest_object.rs`).
    pub(super) fn skills_lock_until(&mut self, until: Instant) {
        self.skills.cast_until = Some(until);
        self.skills.cast_move = None;
    }

    /// FUN_1404b1e40 : le dernier clic attend la fin du blocage local.
    pub(super) fn skills_defer_move(&mut self, destination: Vec3) -> bool {
        if !self.skills.cast_until.is_some_and(|until| Instant::now() < until) { return false; }
        self.skills.cast_move = self.player.as_ref().map(|p| (p.cid, destination));
        true
    }

    /// Cast reçu : arrêt avant le clip, pour tout sort, buffs sur soi compris.
    fn skills_start_animation(&mut self, caster: i32, skill_tid: i32, target: i32) {
        let assets = self.assets.clone();
        let mine = self.player.as_ref().is_some_and(|p| p.cid == caster);
        let target_position = if target == caster { None } else {
            self.player.as_ref().filter(|p| p.cid == target).map(|p| p.position)
                .or_else(|| self.entities.iter().find(|e| e.cid == target).map(|e| e.position))
        };
        // Facteurs distants non conservés dans Entity ; vitesse nominale dans ce cas.
        // Plafond global et réductions régionales de FUN_1402fb110 encore inconnus.
        let modifier = if mine { self.hud.stats.factors.get(&(factor_part::ATTACK, 3)).copied().unwrap_or(0).max(0) as f32 } else { 0.0 };
        let current = self.anim_equip_of(caster);
        let riding = self.ride.riding(caster);
        let entity = if mine { self.player.as_mut() } else { self.entities.iter_mut().find(|e| e.cid == caster) };
        let Some(entity) = entity else { return };
        entity.destination = entity.position;
        if let Some(turn) = target_position.and_then(|p| crate::scene::turn_towards(entity.position, p)) {
            entity.turn_y = turn;
        }
        let tid = entity.tid;
        let equip = crate::worn::anim_equip_for(&assets, tid, entity.is_player, riding, &entity.equipment, current);
        let sig = crate::animation::signature(&entity.meshes);
        let slot = if mine { self.player_animator.get_or_insert((sig, None)) } else { self.animators.entry(caster).or_insert((sig, None)) };
        if slot.0 != sig || slot.1.as_ref().is_none_or(|a| equip >= 0 && a.equip != equip) {
            *slot = (sig, crate::animation::Animator::new(&assets, tid, &entity.meshes, (caster as u32).wrapping_mul(0x9e37_79b9) | 1, equip, entity.is_player));
        }
        if mine {
            self.skills.cast_until = None;
            self.skills.cast_move = None;
            self.walking = false;
            self.buddy.follow = None;
        }
        let Some(rank) = super::effects_glue::skill_rank(&assets, tid, skill_tid) else { return };
        let Some(file) = crate::animation::skill_file(&assets, tid, rank, equip.max(0) as u32) else { return };
        let played = slot.1.as_mut().and_then(|a| a.play_skill(&assets, &file, modifier));
        if mine {
            self.skills.cast_until = played.map(|seconds| Instant::now() + Duration::from_secs_f32(seconds * (1.0 - modifier * 0.01).max(0.0)));
        }
        log::debug!("sort {skill_tid}, lanceur {caster}, rang {rank}, ensemble {equip} : {file}, duree {played:?}");
    }
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
            // Compétence retirée (AGPMSKILL op 1, oubli des compétences : voir `app_consumables.rs`) ; relance et
            // case de la barre restent, comme dans Archonia qui garde son raccourci grisé.
            SkillEvent::Removed { id } => {
                if let Some((tid, _)) = self.skills.known.remove(&id) {
                    log::info!("compétence {tid} (identifiant {id}) retirée");
                }
            }
            SkillEvent::Level { id, level, .. } => {
                if let Some(tid) = self.skills.known.get_mut(&id).map(|s| {
                    s.1 = level;
                    s.0
                }) {
                    log::info!("compétence {tid} : niveau {level}");
                }
            }
            SkillEvent::Cast { caster, id, tid, target, cast_ms, recast_ms, .. } => {
                log::debug!(target: "archrust_buffs", "moteur_cast lanceur={caster} joueur={me} id={id} tid_recu={tid} tid_connu={:?} cible={target} incantation_ms={cast_ms} relance_ms={recast_ms} annonce_buff=false", self.skills.known.get(&id).filter(|_| caster == me).map(|s| s.0));
                if caster == me {
                    let total = Duration::from_millis(cast_ms as u64 + recast_ms as u64);
                    self.skills.cooldowns.insert(id, (Instant::now() + total, total));
                }
                self.skills_start_animation(caster, if tid == 0 && caster == me { self.skills.known.get(&id).map_or(0, |s| s.0) } else { tid }, target);
            }
            // Refus : message d'Archonia (`Action=4:3`) ; la recharge affichée reste.
            SkillEvent::CastRefused { caster, kind, .. } => {
                log::debug!(target: "archrust_buffs", "moteur_refus lanceur={caster} joueur={me} type={kind}");
                if caster == me {
                    if let Some(key) = cast_refused_event(kind) {
                        let text = super::portal_glue::ui_event_cached(&mut self.skills.events, &assets, key);
                        self.hud.push_system(&text);
                    }
                }
            }
            // Expérience de production : relevée par `product_observe` (voir `app_product.rs`).
            SkillEvent::Exp { .. } => {}
            // Op 6 : la liste des effets du joueur remplace l'ancienne (FUN_1402f5b70, rappel 12).
            SkillEvent::Buffs { owner, buffs } => {
                log::debug!(target: "archrust_buffs", "moteur_liste porteur={owner} joueur={me} accepte_hud={} avant={} buffs_tid_ms_modele={buffs:?}", owner == me, self.hud.buffs.len());
                if owner == me {
                    self.hud.buffs.clear();
                    for (tid, ms, _caster_tid) in buffs {
                        self.hud.add_buff(tid, ms);
                    }
                }
            }
            // Op 10 : rappel 9 sans abonné identifié dans Archonia (rien d'affiché).
            // Genre 7 : ce personnage n'est plus ni attaqué ni ciblé par le joueur (`FUN_140698c30`) ; les effets
            // visibles des autres genres sont joués par `effects_observe` (voir `app_effects.rs`).
            SkillEvent::AdditionalEffect { kind: 7, caster, .. } => {
                if self.player.as_ref().is_some_and(|p| p.cid != caster) {
                    if self.combat_is_attacking(caster) {
                        self.combat_interrupt();
                    }
                    if self.addons.target_cid() == Some(caster) {
                        self.addons.set_target(None);
                    }
                }
            }
            SkillEvent::AdditionalEffect { .. } => {}
            SkillEvent::LevelBonus { owner, bonus } if owner == me || me == 0 => self.skills.level_bonus = bonus.clamp(0, 30),
            SkillEvent::SkillBonus { owner, id, level } if owner == me || me == 0 => {
                self.skills.skill_bonus.insert(id, level);
            }
            // Op 15 (rappel 0x1d → 0x1405d9f20) : la recharge affichée s'efface.
            SkillEvent::CooldownReset { owner, id, tid } if owner == me || me == 0 => {
                let id = if self.skills.known.contains_key(&id) { Some(id) } else { self.skills.id_of(tid) };
                if let Some(id) = id {
                    self.skills.cooldowns.remove(&id);
                }
            }
            SkillEvent::LevelBonus { .. } | SkillEvent::SkillBonus { .. } | SkillEvent::CooldownReset { .. } => {}
            // Part des compétences des caractéristiques secondaires (fenêtre du personnage, `hud_status.rs`).
            SkillEvent::Attach { cid, data } => {
                if cid == me || me == 0 {
                    self.hud.stats.skill_attach = data;
                }
            }
            SkillEvent::MasterOpened { npc } => {
                let teach = self.npc_teach(npc);
                match teach {
                    Some(t) => {
                        self.skills.trainer = Some((npc, t));
                        self.skills.trainer_first = 0;
                        self.skills.trainer_selected = None;
                        self.skills.tree_selected = None;
                        // Instructeur du Dragon Scion : arbre du Scion (`Skill_OpenUpUIScion`, `SKILLMASTERY.txt`
                        // ne donne au Scion aucune liste à acheter).
                        self.skills.trainer_tab = if t.0 == 4 { SkillTab::Scion } else { SkillTab::Combat };
                    }
                    None => log::info!("instructeur {npc} : rien que le client connaisse"),
                }
            }
            // Points héroïques ou charisme manquants : la fenêtre d'Archonia n'affiche rien, le serveur le dit
            // par un message système.
            SkillEvent::Learned { result: learn_result::NOT_ENOUGH_HEROIC_POINTS | learn_result::NOT_ENOUGH_CHARISMA, .. } => {}
            // Boîtes de l'[EventMap] d'Archonia : Skill_LearnSuccess, Skill_ErrorAlreadyLearnSkill,
            // …NotEnoughSkillPoint, …LowLevel, …InvalidClass, …NotEnoughMoney (événements nommés d'après le code).
            SkillEvent::Learned { tid, result } => {
                let event = match result {
                    learn_result::SUCCESS => "Skill_LearnSuccess",
                    learn_result::ALREADY_LEARNED => "Skill_ErrorAlreadyLearnSkill",
                    learn_result::NOT_ENOUGH_POINTS => "Skill_ErrorNotEnoughSkillPoint",
                    learn_result::LOW_LEVEL => "Skill_ErrorLowLevel",
                    learn_result::WRONG_CLASS => "Skill_ErrorInvalidClass",
                    _ => "Skill_ErrorNotEnoughMoney",
                };
                log::info!("apprentissage de la compétence {tid} : résultat {result}");
                self.skills.info = Some(super::portal_glue::ui_event_cached(&mut self.skills.events, &assets, event)).filter(|t| !t.is_empty());
            }
            // Boîtes de l'[EventMap] d'Archonia (FUN_1405dd9c0 : Skill_UpgradeSuccess, Skill_ErrorNotEnoughMoney,
            // …SkillPoint, …HeroicPoint, …CharismaPoint).
            SkillEvent::Upgraded { result, .. } => {
                let event = match result {
                    upgrade_result::SUCCESS => "Skill_UpgradeSuccess",
                    upgrade_result::NOT_ENOUGH_MONEY => "Skill_ErrorNotEnoughMoney",
                    upgrade_result::NOT_ENOUGH_POINTS => "Skill_ErrorNotEnoughSkillPoint",
                    upgrade_result::NOT_ENOUGH_HEROIC_POINTS => "Skill_ErrorNotEnoughHeroicPoint",
                    upgrade_result::NOT_ENOUGH_CHARISMA => "Skill_ErrorNotEnoughCharismaPoint",
                    _ => return,
                };
                self.skills.info = Some(super::portal_glue::ui_event_cached(&mut self.skills.events, &assets, event)).filter(|t| !t.is_empty());
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
        self.skills_cast_at(tid, None);
    }

    /// `FUN_14069a0c0` : `at`, point cliqué d'une compétence au sol (voir `app_groundcast.rs`), le joueur pour cible.
    pub(super) fn skills_cast_at(&mut self, tid: i32, at: Option<[f32; 3]>) {
        if self.skills.cast_until.is_some_and(|until| Instant::now() < until) { return; }
        let assets = self.assets.clone();
        let name = self.skills.name(&assets, tid);
        let Some(id) = self.skills.id_of(tid) else {
            log::debug!("compétence {name} inconnue : rien n'est lancé");
            return;
        };
        // Compétence au sol, première demande : repère sous le curseur, rien n'est vérifié ni envoyé.
        let level = self.skills.known.get(&id).map_or(1, |(_, l)| *l);
        let (ground, area) = self.skills.tables(&assets).by_tid.get(&tid).map_or((false, 0.0), |d| {
            (d.spec.ground, usize::try_from(counted_level(d, level) - 1).ok().and_then(|l| d.levels.get(l).or(d.levels.last())).map_or(0.0, |l| l.area_r))
        });
        if ground {
            if self.ground_cast.pending.is_none() {
                self.groundcast_begin(tid, area);
                return;
            }
            if let Some(p) = at {
                self.ground_cast.last_pos = p;
            }
        }
        self.ground_cast.pending = None;
        if self.skills.cooldown(id) > 0.0 {
            return;
        }
        let Some(me) = self.player.as_ref().map(|p| p.cid) else { return };
        // Compétences d'évolution du Dragon Scion (`target` 100 de SKILL_SPEC) : sur soi, quelle que soit la cible.
        let target = if at.is_some() || self.morph_is_evolution_skill(tid) { me } else { self.addons.target_cid().unwrap_or(me) };
        // Métamorphosé : aucune compétence, sauf pour l'ArchLord (voir `app_morph.rs`).
        if !self.morph_allows_skill(tid) {
            return;
        }
        // Compétence sur un ennemi : pas sur une cible métamorphosée ni attaquable ni ciblable (forme animale).
        if !self.morph_allows_skill_target(tid, target) {
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
        // Après un clic au sol, jamais d'attaque forcée (`FUN_1404e4080`).
        let forced = at.is_none() && self.addon_glue.modifiers.control_key();
        if ground {
            send(self.net.as_ref(), SkillCommand::CastGround { id, target, pos: self.ground_cast.last_pos, forced });
        } else {
            send(self.net.as_ref(), SkillCommand::Cast { id, target, forced });
        }
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
                // Orbe de résurrection, Gem of origin : refus et confirmation du client (voir `app_consumables.rs`).
                if item.is_some_and(|iid| self.consumables_intercepts(iid)) {
                    return;
                }
                match (item, &self.net) {
                    (Some(iid), Some(net)) => net.send(Command::Item(ItemCommand::Use(iid))),
                    (None, _) => log::debug!("raccourci : plus aucun objet {tid} dans l'inventaire"),
                    _ => {}
                }
            }
            None => {}
        }
    }

    /// Statut d'interface reçu (op 0 à l'entrée en jeu, op 1) : appliqué par `skills_frame`, après le
    /// chargement de la barre du personnage.
    pub(super) fn on_ui_status(&mut self, st: archrust_netclient::ui_status::UiStatus) {
        self.skills.server_status.push(st);
    }

    /// Raccourci d'une case du serveur : objet (IID → TID de l'inventaire) ou compétence (identifiant
    /// d'instance → TID) ; `None` s'il n'est pas (encore) connu.
    fn skills_resolve(&self, b: archrust_netclient::ui_status::BeltBase) -> Option<Shortcut> {
        use archrust_netclient::ui_status::base_type;
        match b.kind {
            base_type::SKILL => self.skills.known.get(&b.id).map(|(tid, _)| Shortcut::Skill(*tid)),
            base_type::ITEM => self.items.get(&b.id).and_then(|i| i.template_id).map(Shortcut::Item),
            _ => None,
        }
    }

    /// Base envoyée pour un raccourci : compétence possédée (7, identifiant) ; objet (5, IID du premier
    /// exemplaire du sac, sinon de n'importe où). `None` : rien à envoyer (compétence ou objet absents).
    fn skills_base_of(&self, sc: Shortcut) -> Option<archrust_netclient::ui_status::BeltBase> {
        use archrust_netclient::ui_status::{base_type, BeltBase};
        match sc {
            Shortcut::Skill(tid) => self.skills.id_of(tid).map(|id| BeltBase { kind: base_type::SKILL, id }),
            Shortcut::Item(tid) => {
                let mine = self.items.values().filter(|i| i.template_id == Some(tid));
                let iid = mine.clone().filter(|i| i.status == Some(2)).map(|i| i.id).min().or_else(|| mine.map(|i| i.id).min())?;
                Some(BeltBase { kind: base_type::ITEM, id: iid })
            }
        }
    }

    /// Envoie l'op 1 d'une case locale changée, comme `AgcmUIStatus` : potion posée (champ 6 ou 7, jamais
    /// 0, rien au retrait), case de la barre posée (index + mini) ou vidée (index seul).
    fn skills_sync_slot(&mut self, slot: usize) {
        use archrust_netclient::ui_status::UiStatusUpdate;
        self.skills.unresolved.retain(|(s, _)| *s != slot);
        let content = self.skills.slots.get(slot).copied().flatten();
        let mut u = UiStatusUpdate::default();
        match (belt_index(slot), content) {
            (None, Some(Shortcut::Item(tid))) if tid != 0 => {
                if slot == 0 {
                    u.hp_potion_tid = Some(tid);
                } else {
                    u.mp_potion_tid = Some(tid);
                }
            }
            (None, _) => return,
            (Some(index), None) => u.slot = Some((index, None)),
            (Some(index), Some(sc)) => match self.skills_base_of(sc) {
                Some(b) => u.slot = Some((index, Some(b))),
                // Ni compétence possédée ni objet : Archonia n'envoie rien pour ce qu'il ne sait pas désigner.
                None => return,
            },
        }
        if let Some(net) = &self.net {
            net.send(Command::UiStatus(u));
        }
    }

    /// Paquets 0x14 reçus : op 0 (les 50 cases remplacées si le champ 2 est là, potions telles quelles,
    /// seuils ≥ 0) et op 1 (case posée après avoir retiré la même base de sa rangée de 10, ou vidée ;
    /// potions ≠ 0 ; seuils ≥ 0). Rend `true` si la barre a changé.
    fn skills_apply_status(&mut self) -> bool {
        use archrust_netclient::ui_status::op;
        let mut changed = false;
        for st in std::mem::take(&mut self.skills.server_status) {
            for (k, v) in [st.auto_hp, st.auto_mp].into_iter().enumerate() {
                if v >= 0 {
                    self.skills.auto[k] = v.min(100) as u8;
                    self.skills.auto_sent[k] = v;
                    self.skills.auto_moved[k] = None;
                }
            }
            let potions = [st.hp_potion_tid, st.mp_potion_tid];
            if st.op == op::ADD {
                if let Some(belt) = &st.belt {
                    self.skills.server_belt = true;
                    self.skills.unresolved.clear();
                    for (i, base) in belt.iter().enumerate() {
                        let slot = local_slot(i);
                        let sc = base.and_then(|b| self.skills_resolve(b));
                        if let (Some(b), None) = (base, sc) {
                            self.skills.unresolved.push((slot, *b));
                        }
                        if let Some(cell) = self.skills.slots.get_mut(slot) {
                            *cell = sc;
                        }
                    }
                }
                for (slot, tid) in potions.into_iter().enumerate() {
                    let Some(cell) = self.skills.slots.get_mut(slot) else { continue };
                    if self.skills.server_belt {
                        // Copiées telles quelles, 0 compris.
                        *cell = (tid > 0).then_some(Shortcut::Item(tid));
                    } else if tid > 0 && cell.is_none() {
                        // Serveur sans barre : potions posées sur les cases restées vides du fichier local.
                        *cell = Some(Shortcut::Item(tid));
                    }
                }
                changed = true;
            } else {
                for (slot, tid) in potions.into_iter().enumerate() {
                    if let (true, Some(cell)) = (tid != 0, self.skills.slots.get_mut(slot)) {
                        *cell = Some(Shortcut::Item(tid));
                        changed = true;
                    }
                }
                let Some((index, base)) = st.slot else { continue };
                let slot = local_slot(index);
                self.skills.unresolved.retain(|(s, _)| *s != slot);
                let sc = base.and_then(|b| self.skills_resolve(b));
                if let (Some(b), None) = (base, sc) {
                    self.skills.unresolved.push((slot, b));
                }
                if let Some(sc) = sc {
                    let row = local_slot(index / 10 * 10);
                    for cell in self.skills.slots.iter_mut().skip(row).take(10) {
                        if *cell == Some(sc) {
                            *cell = None;
                        }
                    }
                }
                if let Some(cell) = self.skills.slots.get_mut(slot) {
                    *cell = sc;
                }
                changed = true;
            }
        }
        // Objets et compétences arrivés après la barre.
        if !self.skills.unresolved.is_empty() {
            for (slot, b) in std::mem::take(&mut self.skills.unresolved) {
                match self.skills_resolve(b) {
                    Some(sc) => {
                        if let Some(cell) = self.skills.slots.get_mut(slot) {
                            *cell = Some(sc);
                            changed = true;
                        }
                    }
                    None => self.skills.unresolved.push((slot, b)),
                }
            }
        }
        changed
    }

    /// Seuils déplacés : envoyés une seconde après le dernier mouvement, si la valeur a changé.
    fn skills_send_auto(&mut self) {
        for k in 0..2 {
            let Some(at) = self.skills.auto_moved[k] else { continue };
            if at.elapsed() < AUTO_SEND_DELAY {
                continue;
            }
            self.skills.auto_moved[k] = None;
            let value = self.skills.auto[k].min(100) as i8;
            if value == self.skills.auto_sent[k] {
                continue;
            }
            self.skills.auto_sent[k] = value;
            let mut u = archrust_netclient::ui_status::UiStatusUpdate::default();
            if k == 0 {
                u.auto_hp = Some(value);
            } else {
                u.auto_mp = Some(value);
            }
            if let Some(net) = &self.net {
                net.send(Command::UiStatus(u));
            }
        }
    }

    /// Enregistre la barre du personnage : nombre de barres affichées, et cases seulement si le serveur
    /// ne les garde pas.
    fn skills_save(&self) {
        if let Some(name) = &self.skills.slots_owner {
            let slots: &[Option<Shortcut>] = if self.skills.server_belt { &[] } else { &self.skills.slots };
            save_slots(&self.config.saved_dir, name, slots, self.skills.belts);
        }
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
    /// Plus appelée : les touches passent par la table d'Archonia (`app_hotkey.rs`, `skills_hotkey_*` ci-dessous).
    #[allow(dead_code)]
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

    /// Touches de la table d'Archonia (`app_hotkey.rs`) : `QuickBeltUseHPPotion` (0) / `QuickBeltUseMPPotion` (1).
    pub(super) fn skills_hotkey_potion(&mut self, potion: usize) {
        self.skills_use_slot(potion.min(1));
    }

    /// `QuickBeltUse_<couche>_<case>` (couche 1 à 4, case 1 à 10) : même case que `skills_key` (couche 1 : cases
    /// 2 à 11 de la barre ; couches 2 à 4 : barres en plus).
    pub(super) fn skills_hotkey_belt(&mut self, layer: usize, col: usize) {
        let (layer, col) = (layer.clamp(1, 4), col.clamp(1, 10));
        let slot = if layer == 1 { 2 + (col - 1) } else { 12 + (layer - 2) * 10 + (col - 1) };
        self.skills_use_slot(slot);
    }

    /// `QuickBeltLayerUp` (+1) / `QuickBeltLayerDown` (−1) : barres en plus montrées ou cachées.
    pub(super) fn skills_hotkey_layer(&mut self, delta: i32) {
        self.skills_apply(vec![SkillUi::Belts(delta as _)]);
    }

    /// Demandes des fenêtres et de la barre.
    fn skills_apply(&mut self, requests: Vec<SkillUi>) {
        let mut changed = false;
        // Cases changées, envoyées au serveur (op 1) après le traitement des demandes.
        let mut touched: Vec<usize> = Vec::new();
        for r in requests {
            match r {
                SkillUi::CloseBook => self.skills.show_book = false,
                SkillUi::CloseTrainer => self.skills.trainer = None,
                SkillUi::Cast(tid) => self.skills_cast(tid),
                SkillUi::SelectTrainer(tid) => self.skills.trainer_selected = Some(tid),
                SkillUi::TrainerBuy(tid, learn) => {
                    self.skills.confirm = Some((tid, learn));
                    self.skills.confirm_buy = learn;
                }
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
                    touched.push(slot);
                }
                // FUN_14060b4c0 : objet refusé (statut ou genre) → `QB_Add_Invalid_Item` dans la discussion.
                SkillUi::SlotItem { slot, iid, tid } => {
                    let row = self.consumables.item_row(&self.assets, tid);
                    let status = self.items.get(&iid).and_then(|i| i.status);
                    if !super::consumables_glue::quickbelt_accepts(row, status) {
                        let text = super::portal_glue::ui_events(&self.assets).get("QB_Add_Invalid_Item").cloned().unwrap_or_default();
                        self.hud.push_system(&text);
                        continue;
                    }
                    let Some(cell) = self.skills.slots.get_mut(slot) else { continue };
                    *cell = Some(Shortcut::Item(tid));
                    changed = true;
                    touched.push(slot);
                }
                SkillUi::SlotSwap(a, b) => {
                    if a < self.skills.slots.len() && b < self.skills.slots.len() {
                        self.skills.slots.swap(a, b);
                        changed = true;
                        touched.extend([a, b]);
                    }
                }
                SkillUi::Belts(delta) => {
                    self.skills.belts = (self.skills.belts as i64 + delta as i64).clamp(0, EXTRA_BELTS as i64) as usize;
                    changed = true;
                }
                SkillUi::SlotClear(slot) => {
                    if let Some(cell) = self.skills.slots.get_mut(slot) {
                        *cell = None;
                        touched.push(slot);
                    }
                    changed = true;
                }
                SkillUi::SlotUse(slot) => self.skills_use_slot(slot),
                SkillUi::Tab(tab) => {
                    self.skills.book_tab = Some(tab);
                    self.skills.book_first = 0;
                    self.skills.tree_selected = None;
                    self.skills.book_selected = None;
                }
                SkillUi::BookSelect(tid) => self.skills.book_selected = Some(tid),
                // `Skill_Rollback` (FUN_1405dcd80) : mêmes contrôles que le bouton, puis la confirmation.
                SkillUi::Rollback(tid) => {
                    if self.skills_rollback_active(tid) {
                        self.skills.rollback_confirm = Some(tid);
                    }
                }
                SkillUi::TrainerTab(tab) => {
                    self.skills.trainer_tab = tab;
                    self.skills.trainer_first = 0;
                    self.skills.trainer_selected = None;
                    self.skills.tree_selected = None;
                }
                SkillUi::TreeSelect(tid) => self.skills.tree_selected = Some(tid),
                SkillUi::TreeLearn(tid) => {
                    let learn = self.skills.id_of(tid).is_none();
                    self.skills.confirm = Some((tid, learn));
                    self.skills.confirm_buy = false;
                }
                // Seuil : envoyé une seconde après le dernier mouvement (`skills_send_auto`).
                SkillUi::AutoGauge { hp, value } => {
                    let k = if hp { 0 } else { 1 };
                    self.skills.auto[k] = value.min(100);
                    self.skills.auto_moved[k] = Some(Instant::now());
                }
            }
        }
        touched.sort_unstable();
        touched.dedup();
        for slot in touched {
            self.skills_sync_slot(slot);
        }
        if changed {
            self.skills_save();
        }
    }

    /// À chaque image : demandes des fenêtres, barre du personnage chargée, instructeur fermé en
    /// s'éloignant.
    pub(super) fn skills_frame(&mut self) {
        if self.skills.cast_until.is_some_and(|until| Instant::now() >= until) {
            self.skills.cast_until = None;
            if let Some((cid, destination)) = self.skills.cast_move.take() {
                if let Some(p) = self.player.as_mut().filter(|p| p.cid == cid && p.hp != Some(0)) {
                    let position = p.position;
                    p.destination = destination;
                    self.walking = true;
                    if let Some(net) = &self.net { net.send(Command::MoveTo { position, destination }); }
                    self.combat_interrupt();
                    self.buddy.follow = None;
                }
            }
        }
        let requests = std::mem::take(&mut self.skills.pending);
        if !requests.is_empty() {
            self.skills_apply(requests);
        }
        // OK de la confirmation de la Return Orb (`FUN_1405db3a0`) : contrôles refaits, puis AGPMSKILL op 13.
        if let Some(tid) = self.skills.rollback_ok.take() {
            match self.skills.id_of(tid).filter(|_| self.skills_rollback_active(tid)) {
                Some(id) => {
                    log::info!("Return Orb : remise à zéro de la compétence {tid} (identifiant {id})");
                    if let Some(net) = &self.net {
                        net.send(Command::Consumable(archrust_netclient::consumables::ConsumableCommand::SkillRollback { id }));
                    }
                }
                None => log::info!("Return Orb : compétence {tid} plus remboursable, rien n'est envoyé"),
            }
        }
        // Livre fermé : il rouvre sur son premier onglet.
        if !self.skills.show_book {
            self.skills.book_tab = None;
        }
        if self.skills.trainer.is_none() {
            self.skills.confirm = None;
        }
        let Some((name, me)) = self.player.as_ref().filter(|p| p.cid != 0 && !p.name.is_empty()).map(|p| (p.name.clone(), p.position)) else { return };
        if self.skills.slots_owner.as_deref() != Some(name.as_str()) {
            // Fichier local : barres affichées, et cases en repli tant que le serveur n'envoie pas la sienne.
            (self.skills.slots, self.skills.belts) = load_slots(&self.config.saved_dir, &name);
            self.skills.slots_owner = Some(name);
            self.skills.server_belt = false;
            self.skills.unresolved.clear();
        }
        // Barre gardée par le serveur (AgpmUIStatus 0x14).
        if (!self.skills.server_status.is_empty() || !self.skills.unresolved.is_empty()) && self.skills_apply_status() {
            self.skills_save();
        }
        self.skills_send_auto();
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
        let mut v = SkillViews { points: self.hud.stats.skill_points, heroic_points: self.hud.stats.heroic_points, auto: self.skills.auto, ..Default::default() };
        let known: Vec<(i32, (i32, i32))> = self.skills.known.iter().map(|(id, s)| (*id, *s)).collect();
        let (race, class) = player_race_class(&self.hud).unwrap_or((0, 0));
        let scion = race == 4;
        // Onglets ouvrables : listes toujours, arbres connus pour la classe (Heroic : arbre héroïque de la classe).
        v.allowed = [SkillTab::Combat, SkillTab::Buff, SkillTab::Class, SkillTab::Passive, SkillTab::Al, SkillTab::ScionAl, SkillTab::Combo, SkillTab::Heroic, SkillTab::Scion, SkillTab::Slayer, SkillTab::Orbiter, SkillTab::Summoner, SkillTab::ScionPassive, SkillTab::ScionHeroic]
            .into_iter()
            .filter(|t| !t.is_tree() || self.skills.mastery(&assets).tree(*t, race, class).is_some())
            .collect();
        let book_tab = *self.skills.book_tab.get_or_insert(if scion { SkillTab::Scion } else { SkillTab::Combat });
        v.book_tab = book_tab;
        v.title = {
            let key = self.skills.mastery(&assets).class_key(race, class);
            key.map(|k| crate::login_settings::template_identity(&assets, k).1).filter(|c| !c.is_empty()).map(|c| crate::hud::ui_message(&mut self.hud, &assets, &c)).unwrap_or_default()
        };
        // Noms de l'onglet à liste du livre : catégorie de `SKILLMASTERY.txt`, ou liste « Archlord ».
        let book_names: Option<Vec<String>> = match book_tab {
            _ if !self.skills.show_book => None,
            SkillTab::Al | SkillTab::ScionAl => Some(self.skills.mastery(&assets).categories.archlord.clone()),
            t => t.category().map(|c| self.skills.mastery(&assets).categories.names(race, class, c).to_vec()),
        };
        if self.skills.show_book && book_tab.is_tree() {
            v.book_tree = self.skills_tree(book_tab, false);
        }
        if self.skills.show_book && !book_tab.is_tree() {
            for (id, (tid, level)) in &known {
                if *tid == NORMAL_ATTACK {
                    continue;
                }
                let in_tab = {
                    let tables = self.skills.tables(&assets);
                    let lower = tables.by_tid.get(tid).map(|d| d.name.to_lowercase()).unwrap_or_default();
                    let archlord = tables.by_tid.get(tid).is_some_and(|d| d.kind == 9);
                    match &book_names {
                        _ if matches!(book_tab, SkillTab::Al | SkillTab::ScionAl) && archlord => true,
                        Some(names) => names.contains(&lower),
                        None => true,
                    }
                };
                if !in_tab {
                    continue;
                }
                let name = self.skills.name(&assets, *tid);
                let tip = self.skills.tooltip(&assets, *tid, *level);
                v.book.push(BookEntry { tid: *tid, name, level: *level, cooldown: self.skills.cooldown(*id), left_s: self.skills.left_s(*id), tip });
            }
            v.book.sort_by(|a, b| a.name.cmp(&b.name));
        }
        if let (Some(_), true) = (self.skills.trainer, self.skills.trainer_tab.is_tree()) {
            v.trainer_tree = self.skills_tree(self.skills.trainer_tab, true);
        }
        // « Reset Selected » (`Skill_ActiveRollbackButton`) pour la compétence choisie du livre.
        let book_selected = if book_tab.is_tree() { self.skills.tree_selected } else { self.skills.book_selected };
        v.rollback = self.skills.show_book && book_selected.is_some_and(|t| self.skills_rollback_active(t));
        if let Some(tid) = self.skills.rollback_confirm {
            let name = self.skills.name(&assets, tid);
            // `UI_Skill` et `UI_SkillHighLevel` : `Skill_RollbackConfirm` ({{…SkillSelectedName}}) ; fenêtres héroïques
            // et du Scion : `Skill_Init_Confirm_Message` (« [ %s ] »).
            v.rollback_text = Some(if matches!(book_tab, SkillTab::Combo) || !book_tab.is_tree() {
                super::guild_glue::fill_braces(&super::portal_glue::ui_event_cached(&mut self.skills.events, &assets, "Skill_RollbackConfirm"), &name)
            } else {
                super::chat_glue::printf(&crate::hud::ui_message(&mut self.hud, &assets, "Skill_Init_Confirm_Message"), &[name.as_str()])
            });
        }
        if let Some((_, learn)) = self.skills.confirm {
            // Messages de l'[EventMap] d'Archonia (Skill_ConfirmLearnSkill, Skill_ConfirmUpgradeSkill), sans le nom.
            let key = match (learn, self.skills.confirm_buy) {
                (true, true) => "Skill_ConfirmBuySkill",
                (true, false) => "Skill_ConfirmLearnSkill",
                _ => "Skill_ConfirmUpgradeSkill",
            };
            v.confirm = Some(super::portal_glue::ui_event_cached(&mut self.skills.events, &assets, key));
        }
        let trainer_names: Option<Vec<String>> = self.skills.trainer_tab.category().map(|c| self.skills.mastery(&assets).categories.names(race, class, c).to_vec());
        if let Some((npc, (race, class))) = self.skills.trainer {
            let col = class_column(race, class);
            let (level, money, points) = (self.hud.stats.level.unwrap_or(1), self.hud.stats.money.unwrap_or(0), self.hud.stats.skill_points.unwrap_or(0));
            let mut seen_names = std::collections::HashSet::new();
            // Liste des onglets Combat… Passive seulement : les onglets à arbre (Combo, Heroic, Scion) n'ont pas de liste,
            // et sans catégorie pour filtrer ils refaisaient à chaque image l'infobulle de toutes les compétences de la classe.
            let tree = self.skills.trainer_tab.is_tree();
            let mut tids: Vec<i32> = self
                .skills
                .tables(&assets)
                .by_tid
                .iter()
                // Compétences ordinaires de la classe : niveau requis et prix renseignés (les
                // héroïques et spéciales n'en ont pas) ; les variantes « <ASY>… » sont écartées.
                .filter(|(tid, d)| {
                    !tree
                        && **tid != NORMAL_ATTACK
                        && !d.name.starts_with('<')
                        && d.levels.first().is_some_and(|l| l.require_level > 0 && l.cost > 0 && col.is_some_and(|c| l.classes & (1 << c) != 0))
                })
                .map(|(t, _)| *t)
                .collect();
            tids.sort_unstable();
            let mut entries = Vec::new();
            for tid in tids {
                let owned = known.iter().find(|(_, (t, _))| *t == tid).map(|(_, (_, l))| *l).unwrap_or(0);
                let d = &self.skills.tables(&assets).by_tid[&tid];
                if !seen_names.insert(d.name.clone()) {
                    continue;
                }
                // Onglet Combat… Passive : catégorie de `SKILLMASTERY.txt` (`SkillSetBuyMastery0` à `3`).
                if trainer_names.as_ref().is_some_and(|n| !n.is_empty() && !n.contains(&d.name.to_lowercase())) {
                    continue;
                }
                let next = d.levels.get(owned as usize);
                // Prix de l'instructeur avec la taxe de la région (`FUN_1402b8aa0`…, voir `app_tax.rs`).
                let cost = super::tax_glue::taxed(next.map(|l| if owned == 0 { l.cost } else { l.upgrade_cost }).unwrap_or(0));
                let require_level = next.map(|l| l.require_level).unwrap_or(0);
                let possible = next.is_some_and(|l| level >= l.require_level && points >= l.require_point && money >= cost);
                let (name, max_level) = (d.name.clone(), d.levels.len() as i32);
                let tip = self.skills.tooltip(&assets, tid, owned);
                entries.push(TrainerEntry { tid, name, level: owned, max_level, require_level, cost, possible, tip });
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
                    // Objets à compétence et orbe de résurrection : recharge de l'op 4 / 18 (voir `app_consumables.rs`).
                    let (cooldown, left_s) = self.morph_item_cooldown(*t).or_else(|| self.consumables_item_cooldown(*t)).unwrap_or((0.0, 0));
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

/// Race et classe du joueur dans ses facteurs (`FACTOR_TYPE` 0 et 2) ; celles de la forme courante du Dragon Scion.
fn player_race_class(hud: &HudState) -> Option<(i32, i32)> {
    let f = &hud.stats.factors;
    Some((*f.get(&(factor_part::TYPE, 0))?, *f.get(&(factor_part::TYPE, 2))?))
}

impl App {
    /// Compétence `tid` remboursable par la Return Orb (`FUN_1402bcc70`, `FUN_1402bd5f0`) dans l'onglet courant du
    /// livre : apprise, pas l'attaque normale, pas de production (bit 0x2000) ; onglet à liste : vendue par un
    /// instructeur (prix du niveau 1 non nul, données de maître `+0x10` / `+0x14`) ; arbre : [`tree_refundable`].
    fn skills_refundable(&mut self, tid: i32) -> bool {
        let assets = self.assets.clone();
        let tab = self.skills.book_tab.unwrap_or_default();
        let known: HashMap<i32, i32> = self.skills.known.values().map(|(t, l)| (*t, *l)).collect();
        if tid == NORMAL_ATTACK || known.get(&tid).is_none_or(|l| *l < 1) {
            return false;
        }
        let Some(d) = self.skills.tables(&assets).by_tid.get(&tid).cloned() else { return false };
        if d.spec.product {
            return false;
        }
        if !tab.is_tree() {
            return d.levels.first().is_some_and(|l| l.cost != 0);
        }
        let (race, class) = player_race_class(&self.hud).unwrap_or((0, 0));
        let rows: Vec<MasteryRow> = self.skills.mastery(&assets).tree(tab, race, class).map(<[MasteryRow]>::to_vec).unwrap_or_default();
        let mut without = known.clone();
        without.remove(&tid);
        let total_after = if tab.heroic() { self.skills.heroic_spent(&assets, &without) } else { mastery_levels(&self.skills.tables(&assets).by_tid, &without, None) };
        tree_refundable(&rows, tid, &known, total_after)
    }

    /// `Skill_ActiveRollbackButton` (`FUN_1405da000`) : compétence remboursable et Return Orb (usage 13) dans
    /// l'inventaire ou l'inventaire Cash.
    fn skills_rollback_active(&mut self, tid: i32) -> bool {
        self.consumables_has_return_orb() && self.skills_refundable(tid)
    }

    /// Cases de l'arbre `tab` (livre, ou instructeur avec `trainer`) : niveau connu, prochaine étape possible
    /// et infobulle avec les conditions de l'arbre (FUN_1402b8f40 : compétences exigées, niveaux cumulés, points
    /// héroïques dépensés).
    fn skills_tree(&mut self, tab: SkillTab, trainer: bool) -> Vec<Option<TreeSlot>> {
        let assets = self.assets.clone();
        let (race, class) = player_race_class(&self.hud).unwrap_or((0, 0));
        let rows: Vec<MasteryRow> = self.skills.mastery(&assets).tree(tab, race, class).map(<[MasteryRow]>::to_vec).unwrap_or_default();
        let known: HashMap<i32, i32> = self.skills.known.values().map(|(t, l)| (*t, *l)).collect();
        let heroic = tab.heroic();
        // ConditionD : points héroïques dépensés, ou niveaux cumulés de toutes les compétences apprises.
        let total = if heroic { self.skills.heroic_spent(&assets, &known) } else { mastery_levels(&self.skills.tables(&assets).by_tid, &known, None) };
        let (level, money) = (self.hud.stats.level.unwrap_or(1), self.hud.stats.money.unwrap_or(0));
        let points = if heroic { self.hud.stats.heroic_points } else { self.hud.stats.skill_points }.unwrap_or(0);
        let mut out = Vec::new();
        for row in rows {
            let Some(d) = self.skills.tables(&assets).by_tid.get(&row.tid).cloned() else {
                out.push(None);
                continue;
            };
            let owned = known.get(&row.tid).copied().unwrap_or(0);
            let id = self.skills.id_of(row.tid);
            // Conditions (FUN_1402b8f40) : A et B, niveau compté de la compétence ; C (« 1722;30 »), niveaux cumulés
            // des compétences de la race et de la classe du gabarit ; D, total. Lignes de l'infobulle dans l'ordre des
            // colonnes, textes de FUN_1408756f0 : « %s %s : %s %d%s » (sysstr 678, 14, nom, niveau, 262),
            // `RequireSkillPoint` (nom de la classe : `Slayer`, `Obiter`, `Scion`, `Summerner`), `RequireTotalSkillPoint`
            // ou `RequireTotalHeroicSkillPoint`.
            let mut lines: Vec<(String, bool)> = Vec::new();
            let mut met = true;
            for &(rtid, n) in &row.requires {
                let name = self.skills.name(&assets, rtid);
                let got = self.skills.tables(&assets).by_tid.get(&rtid).map_or(0, |r| counted_level(r, known.get(&rtid).copied().unwrap_or(0)));
                let [required, skill, lv] = [678, 14, 262].map(|id| super::npc_glue::sysstr(&assets, id).unwrap_or_default());
                lines.push((format!("{required} {skill} : {name} {n}{lv}"), got >= n));
                met &= got >= n;
            }
            if let Some((ctid, n)) = row.class_levels {
                let rc = self.skills.mastery(&assets).class_of.get(&ctid).copied().or_else(|| crate::scene::template_race_class(&assets, ctid));
                let got = rc.map(|rc| mastery_levels(&self.skills.tables(&assets).by_tid, &known, Some(rc)));
                let ok = got.is_some_and(|g| g >= n);
                met &= ok;
                if let Some((_, class)) = rc {
                    let key = match class {
                        1 => "Slayer",
                        2 => "Obiter",
                        3 => "Scion",
                        4 => "Summerner",
                        _ => "",
                    };
                    let class_name = if key.is_empty() { String::new() } else { crate::hud::ui_message(&mut self.hud, &assets, key) };
                    let format = crate::hud::ui_message(&mut self.hud, &assets, "RequireSkillPoint");
                    if !format.is_empty() {
                        lines.push((super::chat_glue::printf(&format, &[class_name.as_str(), n.to_string().as_str()]), ok));
                    }
                }
            }
            if row.total > 0 {
                let format = crate::hud::ui_message(&mut self.hud, &assets, if heroic { "RequireTotalHeroicSkillPoint" } else { "RequireTotalSkillPoint" });
                if !format.is_empty() {
                    lines.push((super::chat_glue::printf(&format, &[row.total.to_string().as_str()]), total >= row.total));
                }
                met &= total >= row.total;
            }
            let next = d.levels.get(owned.max(0) as usize).cloned();
            let cost = super::tax_glue::taxed(next.as_ref().map(|l| if owned == 0 { l.cost } else { l.upgrade_cost }).unwrap_or(0));
            let possible = trainer
                && met
                && next.as_ref().is_some_and(|l| level >= l.require_level && points >= if heroic { l.require_heroic } else { l.require_point } && money >= cost);
            let mut tip = self.skills.tooltip(&assets, row.tid, owned);
            // Prix (FUN_1408756f0) : « %s : %d%s » (sysstr 137, prix, `Money_Name`), en rouge sans l'or ; points
            // héroïques et charisme : lignes de `skilltooltip.txt` (voir `tooltip`).
            if let (true, Some(_)) = (trainer, &next) {
                if cost > 0 {
                    let label = super::npc_glue::sysstr(&assets, 137).unwrap_or_default();
                    let money_name = crate::hud::ui_message(&mut self.hud, &assets, "Money_Name");
                    tip.push((format!("{label} : {cost}{money_name}"), if money >= cost { Color32::from_rgb(255, 220, 120) } else { Color32::from_rgb(255, 110, 90) }));
                }
            }
            for (text, ok) in lines {
                tip.push((text, if ok { Color32::from_rgb(160, 230, 160) } else { Color32::from_rgb(255, 110, 90) }));
            }
            let cooldown = id.map(|id| self.skills.cooldown(id)).unwrap_or(0.0);
            out.push(Some(TreeSlot { tid: row.tid, name: d.name.clone(), level: owned, max_level: d.levels.len() as i32, possible, cooldown, tip }));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refus_de_lancement() {
        assert_eq!(cast_refused_event(7), Some("Skill_Cast_InvalidTarget"));
        assert_eq!(cast_refused_event(12), Some("Skill_Cast_NotEnoughArrow"));
        assert_eq!(cast_refused_event(17), Some("Skill_Cast_NotEnoughProduct"));
        assert_eq!((cast_refused_event(13), cast_refused_event(1)), (None, None));
    }

    #[test]
    fn competences_au_sol_selon_range() {
        let spec = parse_spec("Name\tattribute\trange\nA\t1\t701\nB\t1\t702\nC\t1\t7\nD\t1\t304\nE\t1\t\n");
        assert!(spec["A"].ground && spec["B"].ground && spec["C"].ground);
        assert!(!spec["D"].ground && !spec["E"].ground);
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : les six compétences au sol et leur `area_r` × 100.
    #[test]
    fn competences_au_sol_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let t = load_tables(&assets);
        let mut ground: Vec<(&str, f32)> = t.by_tid.values().filter(|d| d.spec.ground).map(|d| (d.name.as_str(), d.levels.first().map_or(0.0, |l| l.area_r))).collect();
        ground.sort_by(|a, b| a.0.cmp(b.0));
        assert_eq!(ground.len(), 6, "{ground:?}");
        assert!(ground.contains(&("Meteoric Shower", 2000.0)) && ground.contains(&("Narcolepsy", 700.0)) && ground.contains(&("Imposed Fatigue", 800.0)), "{ground:?}");
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : coût d'une image d'instructeur, soit les infobulles de toute
    /// une classe (ce que refaisaient les onglets Combo et Heroic) après la première image.
    #[test]
    fn infobulles_gardees_en_memoire() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let dir = std::path::PathBuf::from(dir);
        let server = dir.join("server-data");
        let assets = Assets::new(Some(dir), &[]).with_server_data(server.is_dir().then_some(server.as_path()));
        let mut glue = SkillsGlue::default();
        let col = class_column(3, 1);
        let mut tids: Vec<i32> = glue.tables(&assets).by_tid.iter().filter(|(_, d)| d.levels.first().is_some_and(|l| l.require_level > 0 && l.cost > 0 && col.is_some_and(|c| l.classes & (1 << c) != 0))).map(|(t, _)| *t).collect();
        tids.sort_unstable();
        assert!(!tids.is_empty());
        let first = Instant::now();
        let lines: usize = tids.iter().map(|t| glue.tooltip(&assets, *t, 1).len()).sum();
        let first = first.elapsed();
        let next = Instant::now();
        let again: usize = tids.iter().map(|t| glue.tooltip(&assets, *t, 1).len()).sum();
        let next = next.elapsed();
        eprintln!("{} compétences de la guerrière, {lines} lignes : première image {first:?}, suivantes {next:?}", tids.len());
        assert_eq!(lines, again);
        assert!(next < Duration::from_millis(5), "{next:?}");
    }

    #[test]
    fn return_orb_dans_un_arbre() {
        let row = |tid: i32, requires: Vec<(i32, i32)>, total: i32| MasteryRow { tid, requires, class_levels: None, total };
        let rows = vec![row(10, vec![], 0), row(11, vec![(10, 2)], 0), row(12, vec![], 20), row(13, vec![(10, 0)], 0)];
        let known = |pairs: &[(i32, i32)]| pairs.iter().copied().collect::<HashMap<i32, i32>>();
        // Exigée par une case apprise (ConditionA de niveau 2) : refusée ; case non apprise : sans effet.
        assert!(!tree_refundable(&rows, 10, &known(&[(10, 3), (11, 1)]), 30));
        assert!(tree_refundable(&rows, 10, &known(&[(10, 3)]), 30));
        // Condition de niveau 0 : ne compte pas (`piVar1[4] != 0`).
        assert!(tree_refundable(&rows, 10, &known(&[(10, 3), (13, 1)]), 30));
        // ConditionD d'une autre case apprise : le total restant doit suffire (`FUN_1402be4b0`).
        assert!(!tree_refundable(&rows, 10, &known(&[(10, 3), (12, 1)]), 19));
        assert!(tree_refundable(&rows, 10, &known(&[(10, 3), (12, 1)]), 20));
        // Hors de l'arbre.
        assert!(!tree_refundable(&rows, 99, &known(&[(99, 1)]), 30));
    }

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

    /// Décompte des maîtrises (FUN_1402fdc60, FUN_1402fd210, FUN_1402fe170).
    #[test]
    fn niveaux_des_maitrises() {
        assert_eq!([1, 9, 109, 210, 20010, 29, 18, 1009].map(attribute_rest), [1, 9, 9, 0, 0, 9, 8, 9]);
        let spec = parse_spec("Name\tattribute\ttransform_target\tauto_attack\nSupreme Damage\t109\t\t\nCooking\t8\t\t\nSlayer of Evolution\t10\t5\t\nNormal Attack\t1\t\t1\nDual Slash\t1\t\t\n");
        assert!(spec["Supreme Damage"].passive && spec["Cooking"].product && spec["Slayer of Evolution"].evolution && spec["Normal Attack"].auto_attack);
        assert_eq!(spec["Dual Slash"], Spec::default());
        // Compétences admises au niveau 1 par le Slayer (race 4, classe 1), sauf la 6 (Knight seulement).
        let slayer = 1 << class_column(4, 1).unwrap();
        let skill = |classes: u16, spec: Spec| SkillData { levels: vec![Level { classes, ..Level::default() }], spec, ..SkillData::default() };
        let by_tid: HashMap<i32, SkillData> = [
            (1, skill(slayer, Spec::default())),
            (2, skill(slayer, Spec { passive: true, ..Spec::default() })),
            (3, skill(slayer, Spec { evolution: true, ..Spec::default() })),
            (4, skill(slayer, Spec { auto_attack: true, ..Spec::default() })),
            (5, skill(slayer, Spec { product: true, ..Spec::default() })),
            (6, skill(1 << class_column(1, 1).unwrap(), Spec::default())),
        ]
        .into_iter()
        .collect();
        let known: HashMap<i32, i32> = [(1, 40), (2, 5), (3, 1), (4, 1), (5, 50), (6, 7), (99, 3)].into_iter().collect();
        // ConditionC : niveaux (30 au plus, 100 pour une production), sans passives, évolutions ni attaque normale.
        assert_eq!(mastery_levels(&by_tid, &known, Some((4, 1))), 30 + 50);
        assert_eq!(mastery_levels(&by_tid, &known, Some((4, 2))), 0);
        assert_eq!(mastery_levels(&by_tid, &known, Some((4, 0))), 0);
        // ConditionD (-1, -1) : toutes les compétences connues, sans exclusion.
        assert_eq!(mastery_levels(&by_tid, &known, None), 30 + 5 + 1 + 1 + 50 + 7);
        assert!(admits(&by_tid[&6], 1, 1) && !admits(&by_tid[&6], 1, 3));
        // Textes de l'infobulle (FUN_1408756f0) dans la copie déchiffrée d'ArchCore.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../ArchCore/data/client-ini");
        let Ok(raw) = std::fs::read(dir.join("uimessage.txt")) else { return };
        let text = String::from_utf8_lossy(&raw).into_owned();
        let msg = |k: &str| text.lines().find_map(|l| l.split_once('\t').filter(|(a, _)| a.trim() == k).map(|(_, v)| v.trim().to_string())).unwrap_or_default();
        assert_eq!(super::super::chat_glue::printf(&msg("RequireSkillPoint"), &[msg("Slayer").as_str(), "30"]), "Slayer Skill 30 Points Requirement");
        assert_eq!(super::super::chat_glue::printf(&msg("RequireTotalSkillPoint"), &["20"]), "All Skills 20 Points Requirement");
        assert_eq!(super::super::chat_glue::printf(&msg("RequireTotalHeroicSkillPoint"), &["9"]), "Total Heroic Skill Points Required: 9.");
    }

    #[test]
    fn infobulles_des_competences() {
        let text = "Name\tDescription\tLevel\tRequiredLevel\tCostMP\tCastTime\nDual Slash\tDouble Attack\tV\tV\tV\t\nPower Rush\t\t\t\t\t\n";
        let tips = parse_tips(text);
        let row = &tips["dual slash"];
        assert_eq!(row.description, "Double Attack");
        assert!(row.shown.contains("RequiredLevel") && row.shown.contains("CostMP") && !row.shown.contains("CastTime"));
        assert!(tips["power rush"].shown.is_empty());
        assert_eq!(seconds(2000, true, "Seconde"), "2.0 Seconde");
        assert_eq!(seconds(7450, false, "Seconde"), "7 Seconde");
        assert_eq!(seconds(2500, true, "Seconde"), "2.5 Seconde");
    }
}
