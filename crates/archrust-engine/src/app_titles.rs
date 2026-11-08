//! Titres de l'épisode 8 (AgpmTitle 0x60, AgcmTitle d'Archonia ; paquets dans `archrust_netclient::titles`).
//!
//! * **Données** : `ini/titledatatable.txt` (90 colonnes, lecture `FUN_1402452c0`, lues par leur en-tête),
//!   `<langue>/titlestringtable.txt` (`FUN_140246170` : catégorie, ordre, nom, conditions, bonus),
//!   `ini/titlesetting.xml` (`FUN_1405f31c0` : couleur par état 1 Ready … 7 Failed) et
//!   `ini/charismatitletable.txt` (rangs de charisme). Cible : catégories de la table de Webzen (Collector,
//!   Expert Collector, Natural Enemy, Addict, Killer, Advanced, Storyteller, Researcher : 2 768 titres, exportés
//!   par le serveur dans `server-data/`) ; Defender of Chantra (32) et Schola 1-3 (33-35, 38-40, 43-45) seulement si le serveur les
//!   active (TID ≠ 0 dans l'op 10, réglage `titles.schola` d'ArchCore) ou en envoie un ; écartés (ajouts
//!   d'Archonia) : Christmas Event (11000-11005), Schola 4 / 5, `TitleRelatedSkills.txt`,
//!   `UI_TitleSelectSkillRank` et l'op 11 (choix d'une compétence).
//! * **États** (+0x844, `FUN_1405f1de0`) : 1 Ready, 2 Running (objectif), 3 Complete (compteurs atteints),
//!   4 Enable (possédé), 6 Activate (équipé, un seul : this+0x58).
//! * **Fenêtres** : H (`HotkeyDesc=Open Title Window`, table des touches) ouvre ou ferme `UI_TitleCurrent` ; l'op 10
//!   (PNJ « <Title Manager> ») ouvre `UI_TitleAll` et `UI_TitleMini`, fermées au-delà de 149 unités du point
//!   d'ouverture (`FUN_1405ee4e0`). Boutons (`FUN_1405f9e40`) : Ready → « Set up goal. » ; Complete →
//!   « Complete » ; Enable → « Delete Title » et « Equip » ; Activate → « Unequip » seul.
//! * **Contrôles avant l'envoi** (`FUN_1405f6ea0`, boîte OK `TitleErrorMsg_*`) : ArchLord, maître de guilde,
//!   classe (`FUN_1405f63f0`), niveau, CON, STR, INT, DEX, WIS, CHA, or, monnaie payante, objets, titres
//!   préalables ; puis bornes (`FUN_1405f4570`) : 999 objectifs (`CannotRunMoreTitle`), 500 titres et objectifs
//!   (`CannotTryMoreTitle`) ; confirmation `ConfirmSetTargetTitle` → op 6.0. « Complete » : chaque compteur ≥ sa
//!   valeur, sinon `TitleErrorMsg_NotCompletedYet` → op 6.4. Équiper / retirer : `ConfirmActivateTitle` /
//!   `ConfirmDeActivateTitle` → op 2 ; refus op 3 code 3 : `TitleErrorMsg_CannotExChangeTitleIn2Hour` (mois, jour,
//!   heure, minute locaux du dernier changement, retenu des ops 5 et 3). Supprimer `ConfirmDeleteTitle`,
//!   abandonner (« Give Up ») `ConfirmSurrenderTitle` → op 7.
//! * **Au-dessus des têtes** : op 9 (et op 3 réussie pour soi) pose le nom du titre sur la plaque (`board+0x501`,
//!   couleur de l'état 6, `FUN_1405f51f0`), voir `app_boards.rs`. Rang de charisme : première ligne de
//!   `charismatitletable.txt` dont le CHA est sous le maximum (`FUN_14017fec0` / `FUN_14017fdd0`), rangé dans la
//!   plaque à sa création (`board+0x104` = rang − 1, `+0x108` = nom) ; le dessin d'Archonia (`FUN_1408270a0`) ne
//!   lit pas ces champs : ArchRust les tient sans les dessiner.

use super::App;
use crate::assets::Assets;
use crate::hud::{HudState, TitleAllView, TitleDetail, TitleListView, TitleRow, TitleUi, TitleWindow, TITLE_PAGE_ROWS};
use crate::net::NetLink;
use archrust_netclient::titles::{TitleBody, TitleCommand, TitlePacket, USE_CODE_TOO_SOON};
use archrust_netclient::world::{factor_part, WorldEvent};
use archrust_netclient::Command;
use egui::Color32;
use std::collections::{BTreeMap, HashMap};

/// Bornes du client (`FUN_1405f4570`).
pub const MAX_RUNNING: usize = 999;
pub const MAX_TITLES: usize = 500;
/// Distance de fermeture d'`UI_TitleAll` (`FUN_1405ee4e0` : `0x95 < (int) distance`).
pub const CLOSE_DISTANCE: i32 = 149;
/// Statut d'un objet du sac.
const STATUS_INVENTORY: i8 = 2;

/// États d'un titre côté client.
pub mod state {
    pub const READY: i32 = 1;
    pub const RUNNING: i32 = 2;
    pub const COMPLETE: i32 = 3;
    pub const ENABLE: i32 = 4;
    pub const ACTIVATE: i32 = 6;
}

/// Condition d'obtention (`Title_Check_n`, `_Set`, `_Value`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TitleCheck {
    pub kind: i32,
    pub set: i32,
    pub value: i32,
}

/// Ligne de `titledatatable.txt`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TitleTemplate {
    pub tid: i32,
    pub class: String,
    pub name: String,
    pub level: i32,
    pub class_type: i32,
    pub guild_master: bool,
    pub archlord: bool,
    /// CON, STR, INT, DEX, WIS, CHA (ordre des contrôles d'Archonia).
    pub stats: [i32; 6],
    pub gold: i64,
    pub cash: i64,
    pub items: Vec<(i32, i32)>,
    pub first_titles: Vec<i32>,
    /// Conditions non vides, dans l'ordre (les compteurs des ops 6.3 / 6.7 les suivent).
    pub checks: Vec<TitleCheck>,
    pub skills: Vec<i32>,
    pub effects: Vec<[i32; 4]>,
}

/// Ligne de `titlestringtable.txt`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TitleText {
    pub category: i32,
    pub class: String,
    pub order: i32,
    pub name: String,
    pub condition: String,
    pub effect: String,
}

/// Place d'un titre dans la cible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Ep8,
    /// Defender of Chantra, Schola 1-3 : sur activation du serveur.
    Optional,
    Excluded,
}

/// Catégories de l'épisode 8 (`Title_Class` de la table anglaise de Webzen ; « Natural Enemy » suivi de la tranche de
/// niveaux, « Natural Enemy (1-10) »…).
pub const EP8_CLASSES: [&str; 8] = ["Collector", "Expert Collector", "Natural Enemy", "Addict", "Killer", "Advanced", "Storyteller", "Researcher"];

pub fn scope(tid: i32, class: &str) -> Scope {
    let ep8 = EP8_CLASSES.iter().any(|c| class == *c || class.strip_prefix(c).is_some_and(|r| r.starts_with(" (")));
    if ep8 && !(32..=47).contains(&tid) {
        Scope::Ep8
    } else if tid == 32 || ((33..=47).contains(&tid) && (tid - 33) % 5 < 3) {
        Scope::Optional
    } else {
        Scope::Excluded
    }
}

/// Colonnes d'un fichier tabulé, par nom d'en-tête.
fn rows(text: &str) -> (HashMap<String, usize>, Vec<Vec<&str>>) {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let head: HashMap<String, usize> = lines.next().unwrap_or("").split('\t').enumerate().map(|(i, h)| (h.trim().to_string(), i)).collect();
    (head, lines.map(|l| l.trim_end_matches('\r').split('\t').collect()).collect())
}

/// Lit `titledatatable.txt` (titres hors cible écartés).
pub fn parse_data_table(text: &str) -> BTreeMap<i32, TitleTemplate> {
    let (head, lines) = rows(text);
    let mut out = BTreeMap::new();
    for cols in lines {
        let get = |name: &str| head.get(name).and_then(|&i| cols.get(i)).map_or("", |v| v.trim());
        let num = |name: &str| get(name).parse::<i64>().unwrap_or(0);
        let Ok(tid) = get("TID").parse::<i32>() else { continue };
        let class = get("Title_Class").to_string();
        if scope(tid, &class) == Scope::Excluded {
            continue;
        }
        let n = |name: String| num(&name) as i32;
        let t = TitleTemplate {
            tid,
            name: get("Title_Name").to_string(),
            level: num("Require_level") as i32,
            class_type: num("Require_class_Type") as i32,
            guild_master: num("Require_class_GuildMaster") != 0,
            archlord: num("Require_class_ArchLord") != 0,
            stats: ["Require_Con", "Require_Str", "Require_Int", "Require_Dex", "Require_Wis", "Require_Cha"].map(|k| num(k) as i32),
            gold: num("Cost_Geld"),
            cash: num("Cost_Cash"),
            items: (1..=5).map(|i| (n(format!("Cost_Item_{i}")), n(format!("Cost_Item_{i}_Count")))).filter(|&(t, c)| t > 0 && c > 0).collect(),
            first_titles: (1..=5).map(|i| n(format!("First_Title {i}"))).filter(|&t| t > 0).collect(),
            checks: (1..=5)
                .map(|i| TitleCheck { kind: n(format!("Title_Check_{i}")), set: n(format!("Title_Check_{i}_Set")), value: n(format!("Title_Check_{i}_Value")) })
                .filter(|c| c.kind != 0)
                .collect(),
            skills: (1..=10).map(|i| n(format!("Skill_Tid_{i}"))).filter(|&s| s > 0).collect(),
            effects: (1..=5)
                .map(|i| [n(format!("Title_Effect_{i}")), n(format!("Title_Effect_{i}_Set")), n(format!("Title_Effect_{i}_Value_1")), n(format!("Title_Effect_{i}_Value_2"))])
                .filter(|e| e[0] != 0)
                .collect(),
            class,
        };
        out.insert(tid, t);
    }
    out
}

/// Lit `titlestringtable.txt`.
pub fn parse_string_table(text: &str) -> HashMap<i32, TitleText> {
    let (head, lines) = rows(text);
    let mut out = HashMap::new();
    for cols in lines {
        let get = |name: &str| head.get(name).and_then(|&i| cols.get(i)).map_or("", |v| v.trim());
        let Ok(tid) = get("TID").parse::<i32>() else { continue };
        out.insert(
            tid,
            TitleText {
                category: get("FirstCategory").parse().unwrap_or(0),
                class: get("Title_Class").to_string(),
                order: get("SecondCategory").parse().unwrap_or(0),
                name: get("Title_Name").to_string(),
                condition: get("Necessary_Condition_Desc").to_string(),
                effect: get("Title_Effect_Desc").to_string(),
            },
        );
    }
    out
}

/// Couleurs `FontColor` de `titlesetting.xml` (état → RVB).
pub fn parse_setting(xml: &str) -> HashMap<i32, Color32> {
    let mut out = HashMap::new();
    for tag in xml.split("<FontColor").skip(1) {
        let tag = tag.split("/>").next().unwrap_or("");
        let attr = |k: &str| {
            let key = format!("{k}=\"");
            tag.find(&key).and_then(|p| tag[p + key.len()..].split('"').next()).and_then(|v| v.trim().parse::<i32>().ok())
        };
        if let (Some(i), Some(r), Some(g), Some(b)) = (attr("StateIndex"), attr("Red"), attr("Green"), attr("Blue")) {
            out.insert(i, Color32::from_rgb(r.clamp(0, 255) as u8, g.clamp(0, 255) as u8, b.clamp(0, 255) as u8));
        }
    }
    out
}

/// Rangs de `charismatitletable.txt` : (maximum, titre).
pub fn parse_charisma(text: &str) -> Vec<(i32, String)> {
    text.lines()
        .skip(1)
        .filter_map(|l| {
            let mut c = l.trim_end_matches('\r').split('\t');
            let _min = c.next()?;
            let max = c.next()?.trim().parse().ok()?;
            Some((max, c.next().unwrap_or("").trim().to_string()))
        })
        .collect()
}

/// Rang de charisme (`FUN_14017fec0` : première ligne dont `cha < Max`) et nom (`FUN_14017fdd0`).
pub fn charisma_rank(table: &[(i32, String)], cha: i32) -> Option<(usize, String)> {
    table.iter().position(|(max, _)| cha < *max).map(|i| (i, table[i].1.clone()))
}

/// Classe exigée (`FUN_1405f63f0`) : 0 tous ; 101/201/301 chevalier, archer, mage humains ; 401/501/601
/// berserker, chasseur, sorcier orcs ; 701/801/901 swashbuckler, ranger, élémentaliste elfes de lune ;
/// 1001/1101/1201/1301 Dragon Scion (race seule).
pub fn class_allowed(class_type: i32, race: i32, class: i32) -> bool {
    match class_type {
        0 => true,
        101 => (race, class) == (1, 1),
        201 => (race, class) == (1, 2),
        301 => (race, class) == (1, 4),
        401 => (race, class) == (2, 1),
        501 => (race, class) == (2, 2),
        601 => (race, class) == (2, 4),
        701 => (race, class) == (3, 1),
        801 => (race, class) == (3, 2),
        901 => (race, class) == (3, 4),
        1001 | 1101 | 1201 | 1301 => race == 4,
        _ => false,
    }
}

/// Ce que le joueur est et possède, pour les contrôles de `FUN_1405f6ea0`.
#[derive(Debug, Clone, Default)]
pub struct TitlesMe {
    pub name: String,
    pub level: i32,
    pub race: i32,
    pub class: i32,
    /// CON, STR, INT, DEX, WIS, CHA.
    pub stats: [i32; 6],
    pub gold: i64,
    pub cash: i64,
    pub items: HashMap<i32, i32>,
    pub guild_master: bool,
    pub archlord: bool,
    /// Position (x, z).
    pub pos: (f32, f32),
}

/// Titre possédé.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Owned;

/// Objectif en cours.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Goal {
    pub accepted: i64,
    pub counters: [i32; 5],
}

#[derive(Debug, Clone)]
pub struct TitleData {
    pub templates: BTreeMap<i32, TitleTemplate>,
    pub texts: HashMap<i32, TitleText>,
    pub colors: HashMap<i32, Color32>,
    pub charisma: Vec<(i32, String)>,
}

/// Question ouverte (boîte Confirmer / Annuler).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Ask {
    SetTarget(i32),
    Delete(i32),
    Surrender(i32),
    Use(i32, bool),
}

#[derive(Default)]
pub struct TitlesGlue {
    data: Option<TitleData>,
    pub owned: BTreeMap<i32, Owned>,
    pub goals: BTreeMap<i32, Goal>,
    pub active: Option<i32>,
    /// Schola 1-3 et Defender of Chantra activés par le serveur.
    pub optional: bool,
    pub show_all: bool,
    pub show_current: bool,
    pub show_mini: bool,
    /// Position du joueur à l'ouverture d'`UI_TitleAll` (this+0x64 / +0x6c).
    opened_at: (f32, f32),
    category: Option<usize>,
    /// Page d'`UI_TitleAll`, à partir de 1 (this+0x70).
    page: usize,
    selected_all: Option<i32>,
    selected_current: Option<i32>,
    selected_mini: Option<i32>,
    ask: Option<(Ask, String)>,
    alert: Option<String>,
    /// Refus « moins de 2 h » à afficher : heure du dernier changement.
    too_soon: Option<i64>,
    /// Heure du dernier changement de titre du joueur (ops 5 et 3).
    last_change: i64,
    /// Titre affiché au-dessus de chaque personnage (nom en minuscules → TID), op 9.
    pub over_head: HashMap<String, i32>,
    /// Joueur de l'image (voir `App::titles_frame`).
    pub me: TitlesMe,
}

impl TitlesGlue {
    fn data(&mut self, assets: &Assets) -> &TitleData {
        self.data.get_or_insert_with(|| TitleData {
            templates: parse_data_table(&super::npc_glue::client_text_in(assets, "ini", "titledatatable.txt")),
            texts: parse_string_table(&super::npc_glue::client_text(assets, "titlestringtable.txt")),
            colors: parse_setting(&super::npc_glue::client_text_in(assets, "ini", "titlesetting.xml")),
            charisma: parse_charisma(&super::npc_glue::client_text_in(assets, "ini", "charismatitletable.txt")),
        })
    }

    /// Le titre est-il repris (cible de l'épisode 8, ou Schola / Defender activés) ?
    fn usable(&self, tid: i32) -> bool {
        let Some(t) = self.data.as_ref().and_then(|d| d.templates.get(&tid)) else { return false };
        scope(tid, &t.class) == Scope::Ep8 || self.optional
    }

    /// État d'un titre (`FUN_1405f1de0`).
    pub fn state(&self, tid: i32) -> i32 {
        if self.active == Some(tid) {
            state::ACTIVATE
        } else if self.owned.contains_key(&tid) {
            state::ENABLE
        } else if let Some(g) = self.goals.get(&tid) {
            if self.completed(tid, g) {
                state::COMPLETE
            } else {
                state::RUNNING
            }
        } else {
            state::READY
        }
    }

    /// Chaque compteur atteint sa valeur (`FUN_1403075e0`) ; sans condition : terminable dès l'acceptation.
    fn completed(&self, tid: i32, g: &Goal) -> bool {
        let Some(t) = self.data.as_ref().and_then(|d| d.templates.get(&tid)) else { return false };
        t.checks.iter().zip(g.counters.iter()).all(|(c, v)| *v >= c.value)
    }

    fn name_of(&self, tid: i32) -> String {
        let d = self.data.as_ref();
        d.and_then(|d| d.texts.get(&tid)).map(|t| t.name.clone()).filter(|n| !n.is_empty()).or_else(|| d.and_then(|d| d.templates.get(&tid)).map(|t| t.name.clone())).unwrap_or_default()
    }

    fn color(&self, state: i32) -> Color32 {
        self.data.as_ref().and_then(|d| d.colors.get(&state)).copied().unwrap_or(Color32::WHITE)
    }

    /// Catégories d'`UI_TitleAll` (FirstCategory croissant) : (id, nom).
    fn categories(&self) -> Vec<(i32, String)> {
        let Some(d) = self.data.as_ref() else { return Vec::new() };
        let mut cats: BTreeMap<i32, String> = BTreeMap::new();
        for tid in d.templates.keys().filter(|&&t| self.usable(t)) {
            if let Some(t) = d.texts.get(tid) {
                cats.entry(t.category).or_insert_with(|| t.class.clone());
            }
        }
        cats.into_iter().collect()
    }

    /// Titres d'une catégorie, dans l'ordre `SecondCategory`.
    fn titles_in(&self, category: i32) -> Vec<i32> {
        let Some(d) = self.data.as_ref() else { return Vec::new() };
        let mut v: Vec<(i32, i32)> = d.templates.keys().filter(|&&t| self.usable(t)).filter_map(|t| d.texts.get(t).filter(|x| x.category == category).map(|x| (x.order, *t))).collect();
        v.sort();
        v.into_iter().map(|(_, t)| t).collect()
    }

    /// Nom, conditions (avec « (n/m) » pour un objectif, `FUN_1405faa20`) et bonus.
    fn detail(&self, tid: i32) -> TitleDetail {
        let text = self.data.as_ref().and_then(|d| d.texts.get(&tid)).cloned().unwrap_or_default();
        let mut info = text.condition.clone();
        let st = self.state(tid);
        if let (true, Some(g), Some(c)) = (matches!(st, state::RUNNING | state::COMPLETE), self.goals.get(&tid), self.data.as_ref().and_then(|d| d.templates.get(&tid)).and_then(|t| t.checks.first())) {
            if c.value > 0 {
                info = format!("{info}</n>({}/{})", g.counters[0].min(c.value), c.value);
            }
        }
        TitleDetail { name: self.name_of(tid), info, effect: text.effect }
    }

    /// Contrôles de `FUN_1405f6ea0` puis bornes de `FUN_1405f4570` : clé du message de refus, `None` si permis.
    pub fn set_target_refusal(&self, tid: i32) -> Option<&'static str> {
        let t = self.data.as_ref()?.templates.get(&tid)?;
        let me = &self.me;
        if t.archlord && !me.archlord {
            return Some("TitleErrorMsg_RequireArchlord");
        }
        if t.guild_master && !me.guild_master {
            return Some("TitleErrorMsg_RequireGuildMaster");
        }
        if !class_allowed(t.class_type, me.race, me.class) {
            return Some("TitleErrorMsg_RequireClassType");
        }
        if me.level < t.level {
            return Some("TitleErrorMsg_RequireLevel");
        }
        const STATS: [&str; 6] = ["TitleErrorMsg_RequireCon", "TitleErrorMsg_RequireStr", "TitleErrorMsg_RequireInt", "TitleErrorMsg_RequireDex", "TitleErrorMsg_RequireWis", "TitleErrorMsg_RequireCharisma"];
        if let Some(i) = (0..6).find(|&i| me.stats[i] < t.stats[i]) {
            return Some(STATS[i]);
        }
        if me.gold < t.gold {
            return Some("TitleErrorMsg_RequireGheld");
        }
        if me.cash < t.cash {
            return Some("TitleErrorMsg_RequireCash");
        }
        if t.items.iter().any(|(item, n)| me.items.get(item).copied().unwrap_or(0) < *n) {
            return Some("TitleErrorMsg_RequireItem");
        }
        if t.first_titles.iter().any(|f| !self.owned.contains_key(f)) {
            return Some("TitleErrorMsg_RequireTitle");
        }
        if self.goals.len() >= MAX_RUNNING {
            return Some("CannotRunMoreTitle");
        }
        if self.goals.len() + self.owned.len() >= MAX_TITLES {
            return Some("CannotTryMoreTitle");
        }
        None
    }

    /// Libellés et activité des trois boutons d'`UI_TitleAll` (`FUN_1405f9e40`).
    fn buttons(&self, hud: &mut HudState, assets: &Assets) -> [(String, bool); 3] {
        let msg = |hud: &mut HudState, k: &str| crate::hud::ui_message(hud, assets, k);
        let mut b = [(msg(hud, "TitleBtnText_SetTarget"), false), (msg(hud, "TitleBtnText_ActivateTitle"), false), (String::new(), false)];
        let Some(tid) = self.selected_all else { return b };
        match self.state(tid) {
            state::READY => b[0].1 = true,
            state::COMPLETE => b[2].1 = true,
            state::ENABLE => {
                b[0] = (msg(hud, "TitleBtnText_DeleteTitle"), true);
                b[1].1 = true;
            }
            state::ACTIVATE => {
                b[0].0 = msg(hud, "TitleBtnText_DeleteTitle");
                b[1] = (msg(hud, "TitleBtnText_DeActivateTitle"), true);
            }
            _ => {}
        }
        b
    }

    /// Titre au-dessus du personnage `name` : texte et couleur de l'état 6 (`FUN_1405f51f0`).
    pub fn over_head_title(&self, name: &str) -> Option<(String, Color32)> {
        let tid = *self.over_head.get(&name.to_lowercase())?;
        let text = self.name_of(tid);
        (!text.is_empty()).then(|| (text, self.color(state::ACTIVATE)))
    }

    /// Rang de charisme du joueur (rang − 1 comme `board+0x104`, nom comme `+0x108`).
    pub fn my_charisma_rank(&self) -> Option<(i32, String)> {
        let table = &self.data.as_ref()?.charisma;
        charisma_rank(table, self.me.stats[5]).map(|(i, n)| (i as i32 - 1, n))
    }

    /// Ferme `UI_TitleAll` et `UI_TitleMini` au-delà de 149 unités du point d'ouverture.
    fn check_distance(&mut self) {
        if !self.show_all {
            return;
        }
        let (dx, dz) = (self.me.pos.0 - self.opened_at.0, self.me.pos.1 - self.opened_at.1);
        if (dx * dx + dz * dz).sqrt() as i32 > CLOSE_DISTANCE {
            self.show_all = false;
            self.show_mini = false;
        }
    }

    fn open_npc_windows(&mut self) {
        self.show_all = true;
        self.show_mini = true;
        self.opened_at = self.me.pos;
        self.category = None;
        self.page = 1;
        self.selected_all = None;
        self.selected_mini = None;
    }

    /// Paquet 0x60 (`FUN_1403058b0` et rappels d'AgcmTitle).
    pub fn on_packet(&mut self, assets: &Assets, p: TitlePacket) -> Option<String> {
        self.data(assets);
        let mine = p.name.is_empty() || p.name.eq_ignore_ascii_case(&self.me.name);
        let tid = p.tid;
        match p.body {
            TitleBody::UseNear { on } => {
                if on {
                    self.over_head.insert(p.name.to_lowercase(), tid);
                } else {
                    self.over_head.remove(&p.name.to_lowercase());
                }
                if mine {
                    self.active = if on { Some(tid) } else { self.active.filter(|a| *a != tid) };
                }
            }
            TitleBody::UiOpen => {
                if tid != 0 {
                    self.optional = true;
                }
                self.open_npc_windows();
            }
            _ if !mine => {}
            TitleBody::AddResult { result: 1, .. } => {
                self.note_optional(tid);
                self.goals.remove(&tid);
                self.owned.entry(tid).or_default();
                let name = self.name_of(tid);
                return (!name.is_empty()).then_some(name);
            }
            TitleBody::UseResult { on, result, time, .. } if result == 1 => {
                // Heure du changement retenue pour le personnage (`FUN_1405f5480`).
                self.last_change = time;
                if on {
                    self.active = Some(tid);
                    self.over_head.insert(self.me.name.to_lowercase(), tid);
                } else if self.active == Some(tid) {
                    self.active = None;
                    self.over_head.remove(&self.me.name.to_lowercase());
                }
            }
            TitleBody::UseResult { code, .. } if code == USE_CODE_TOO_SOON => {
                // Heure du dernier changement retenue pour le joueur (`FUN_1405f83d0`), pas celle du paquet.
                self.too_soon = Some(self.last_change);
            }
            TitleBody::ListEntry { changed_at, valid: true, skip: 0, .. } => {
                self.note_optional(tid);
                self.owned.insert(tid, Owned);
                self.last_change = self.last_change.max(changed_at);
            }
            TitleBody::QuestAddResult { accepted, result } if result != 0 => {
                self.note_optional(tid);
                self.goals.insert(tid, Goal { accepted, counters: [0; 5] });
            }
            TitleBody::QuestCheck { values, .. } => {
                if let Some(g) = self.goals.get_mut(&tid) {
                    g.counters = values;
                }
            }
            TitleBody::QuestCompleteResult { result, .. } if result != 0 => {
                self.goals.remove(&tid);
            }
            TitleBody::QuestEntry { valid: true, done, accepted, counters, .. } => {
                self.note_optional(tid);
                if !done {
                    self.goals.insert(tid, Goal { accepted, counters });
                }
            }
            TitleBody::DeleteResult { result } if result != 0 => {
                self.owned.remove(&tid);
                self.goals.remove(&tid);
                if self.active == Some(tid) {
                    self.active = None;
                    self.over_head.remove(&self.me.name.to_lowercase());
                }
            }
            _ => {}
        }
        None
    }

    /// Un titre Schola / Defender envoyé par le serveur les active.
    fn note_optional(&mut self, tid: i32) {
        if self.data.as_ref().and_then(|d| d.templates.get(&tid)).is_some_and(|t| scope(tid, &t.class) == Scope::Optional) {
            self.optional = true;
        }
    }

    /// Remise à zéro (nouvelle connexion) : données gardées.
    fn reset(&mut self) {
        let data = self.data.take();
        *self = TitlesGlue { data, ..Default::default() };
    }
}

/// Message `TitleErrorMsg_CannotExChangeTitleIn2Hour` : mois, jour, heure et minute locaux de `t`.
pub fn too_soon_text(template: &str, t: i64) -> String {
    let (_, mo, d, h, mi, _) = super::boss_glue::civil(t + super::mail_glue::local_offset(t));
    super::pvp_glue::fill(&template.replace("%02d", "%s"), &[mo, d, h, mi].map(|v| format!("{v:02}")))
}

fn send(net: Option<&NetLink>, c: TitleCommand) {
    if let Some(net) = net {
        net.send(Command::Title(c));
    }
}

/// Fenêtres, boîtes et clics (passe egui).
pub fn panel(ctx: &egui::Context, glue: &mut TitlesGlue, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>) {
    glue.data(assets);
    let mut out = Vec::new();
    if glue.show_all {
        let cats = glue.categories();
        let names: Vec<String> = cats.iter().map(|c| c.1.clone()).collect();
        let list = glue.category.and_then(|i| cats.get(i)).map(|c| glue.titles_in(c.0)).unwrap_or_default();
        let pages = list.len().div_ceil(TITLE_PAGE_ROWS);
        glue.page = glue.page.clamp(1, pages.max(1));
        let page: Vec<i32> = list.iter().copied().skip((glue.page - 1) * TITLE_PAGE_ROWS).take(TITLE_PAGE_ROWS).collect();
        let rows: Vec<TitleRow> = page.iter().map(|&t| TitleRow { text: glue.name_of(t), color: glue.color(glue.state(t)) }).collect();
        let detail = glue.selected_all.map(|t| glue.detail(t));
        let buttons = glue.buttons(hud, assets);
        let view = TitleAllView {
            categories: &names,
            category: glue.category,
            rows: &rows,
            selected: glue.selected_all.and_then(|t| page.iter().position(|p| *p == t)),
            page: if glue.category.is_some() { glue.page } else { 0 },
            pages: if glue.category.is_some() { pages } else { 0 },
            detail: detail.as_ref(),
            buttons,
        };
        crate::hud::draw_title_all(ctx, hud, assets, &view, &mut out);
        for a in &mut out {
            if let TitleUi::Row(TitleWindow::All, i) = a {
                glue.selected_all = page.get(*i).copied();
            }
        }
        for a in &out {
            match *a {
                TitleUi::Category(i) if i < cats.len() => {
                    glue.category = Some(i);
                    glue.page = 1;
                    glue.selected_all = None;
                }
                TitleUi::Page(p) => glue.page = p.clamp(1, pages.max(1)),
                TitleUi::PrevPage => glue.page = glue.page.saturating_sub(1).max(1),
                TitleUi::NextPage => glue.page = (glue.page + 1).min(pages.max(1)),
                _ => {}
            }
        }
    }
    let owned: Vec<i32> = glue.owned.keys().copied().collect();
    if glue.show_current {
        let rows: Vec<TitleRow> = owned.iter().map(|&t| TitleRow { text: glue.name_of(t), color: glue.color(glue.state(t)) }).collect();
        let detail = glue.selected_current.map(|t| glue.detail(t));
        let view = TitleListView { rows: &rows, selected: glue.selected_current.and_then(|t| owned.iter().position(|o| *o == t)), detail: detail.as_ref(), surrender: false };
        let n = out.len();
        crate::hud::draw_title_current(ctx, hud, assets, &view, &mut out);
        for a in &out[n..] {
            if let TitleUi::Row(TitleWindow::Current, i) = a {
                glue.selected_current = owned.get(*i).copied();
            }
        }
    }
    let goals: Vec<i32> = glue.goals.keys().copied().collect();
    if glue.show_mini {
        let rows: Vec<TitleRow> = goals.iter().map(|&t| TitleRow { text: glue.name_of(t), color: glue.color(glue.state(t)) }).collect();
        glue.selected_mini = glue.selected_mini.filter(|t| goals.contains(t));
        let view = TitleListView { rows: &rows, selected: glue.selected_mini.and_then(|t| goals.iter().position(|g| *g == t)), detail: None, surrender: glue.selected_mini.is_some() };
        let n = out.len();
        crate::hud::draw_title_mini(ctx, hud, assets, &view, &mut out);
        for a in &out[n..] {
            if let TitleUi::Row(TitleWindow::Mini, i) = a {
                glue.selected_mini = goals.get(*i).copied();
            }
        }
    }
    let msg = |hud: &mut HudState, k: &str| crate::hud::ui_message(hud, assets, k);
    for a in out {
        match a {
            TitleUi::Close(TitleWindow::All) => glue.show_all = false,
            TitleUi::Close(TitleWindow::Current) => glue.show_current = false,
            TitleUi::Close(TitleWindow::Mini) => glue.show_mini = false,
            TitleUi::SetTarget => {
                let Some(tid) = glue.selected_all else { continue };
                match glue.state(tid) {
                    state::READY => match glue.set_target_refusal(tid) {
                        Some(key) => glue.alert = Some(msg(hud, key)),
                        None => glue.ask = Some((Ask::SetTarget(tid), msg(hud, "ConfirmSetTargetTitle"))),
                    },
                    state::ENABLE => glue.ask = Some((Ask::Delete(tid), msg(hud, "ConfirmDeleteTitle"))),
                    _ => {}
                }
            }
            TitleUi::Activate => {
                let Some(tid) = glue.selected_all else { continue };
                match glue.state(tid) {
                    state::ENABLE => glue.ask = Some((Ask::Use(tid, true), msg(hud, "ConfirmActivateTitle"))),
                    state::ACTIVATE => glue.ask = Some((Ask::Use(tid, false), msg(hud, "ConfirmDeActivateTitle"))),
                    _ => {}
                }
            }
            TitleUi::Complete => {
                let Some(tid) = glue.selected_all else { continue };
                if glue.state(tid) == state::COMPLETE {
                    send(net, TitleCommand::QuestComplete(tid));
                } else if glue.state(tid) == state::RUNNING {
                    glue.alert = Some(msg(hud, "TitleErrorMsg_NotCompletedYet"));
                }
            }
            TitleUi::Surrender => {
                if let Some(tid) = glue.selected_mini {
                    glue.ask = Some((Ask::Surrender(tid), msg(hud, "ConfirmSurrenderTitle")));
                }
            }
            _ => {}
        }
    }
    if let Some((ask, text)) = glue.ask.clone() {
        if let Some(yes) = crate::hud::confirm_box(ctx, hud, assets, "title-ask", &text) {
            glue.ask = None;
            if yes {
                match ask {
                    Ask::SetTarget(tid) => send(net, TitleCommand::QuestAdd(tid)),
                    Ask::Delete(tid) | Ask::Surrender(tid) => send(net, TitleCommand::Delete(tid)),
                    Ask::Use(tid, on) => send(net, TitleCommand::Use { tid, on }),
                }
            }
        }
    }
    if let Some(text) = glue.alert.clone() {
        if crate::hud::info_box(ctx, hud, assets, &text) {
            glue.alert = None;
        }
    }
}

impl App {
    /// Joueur de l'image pour les contrôles, fermeture d'`UI_TitleAll` en s'éloignant.
    pub(super) fn titles_frame(&mut self) {
        let Some(p) = self.player.as_ref() else { return };
        let factors = &self.hud.stats.factors;
        let factor = |part: u8, i: u8| factors.get(&(part, i)).copied();
        let tid = p.tid;
        let template = |k: &str| self.assets.table("ini", "charactertemplate.ini").ok().and_then(|t| t.section(&tid.to_string())?.get(k)?.trim().parse::<i32>().ok());
        let name = p.name.clone();
        let me = TitlesMe {
            level: self.hud.stats.level.unwrap_or(0),
            race: factor(factor_part::TYPE, 0).filter(|r| *r > 0).or_else(|| template("Race")).unwrap_or(0),
            class: factor(factor_part::TYPE, 2).filter(|c| *c > 0).or_else(|| template("Class")).unwrap_or(0),
            stats: [0u8, 1, 2, 3, 6, 4].map(|i| factor(factor_part::STATUS, i).unwrap_or(0)),
            gold: self.hud.stats.money.unwrap_or(0),
            cash: self.hud.stats.cash.unwrap_or(0),
            items: {
                let mut have = HashMap::new();
                for i in self.items.values().filter(|i| i.status == Some(STATUS_INVENTORY)) {
                    if let Some(t) = i.template_id {
                        *have.entry(t).or_default() += i.count.unwrap_or(1).max(1);
                    }
                }
                have
            },
            guild_master: self.guild.state.member(&name).is_some_and(|m| m.rank == archrust_netclient::guild::rank::MASTER),
            archlord: self.archlord_is_me(),
            pos: (p.position.x, p.position.z),
            name,
        };
        self.titles.me = me;
        self.titles.check_distance();
    }

    /// Touche « Open Title Window » : `UI_TitleCurrent` ouverte ou fermée.
    pub(super) fn titles_toggle_current(&mut self) {
        self.titles.show_current = !self.titles.show_current;
    }

    /// Paquet AgpmTitle.
    pub(super) fn on_title_event(&mut self, p: TitlePacket) {
        let assets = self.assets.clone();
        let got = self.titles.on_packet(&assets, p);
        if let Some(last) = self.titles.too_soon.take() {
            let template = crate::hud::ui_message(&mut self.hud, &assets, "TitleErrorMsg_CannotExChangeTitleIn2Hour");
            if !template.is_empty() {
                self.titles.alert = Some(too_soon_text(&template, last));
            }
        }
        if let Some(name) = got {
            // `NotifyGetNewTitle` « You received the title of %s. » (`FUN_1405efe90`).
            let text = super::pvp_glue::fill(&crate::hud::ui_message(&mut self.hud, &assets, "NotifyGetNewTitle"), &[name]);
            if !text.trim().is_empty() {
                self.hud.push_system(&text);
            }
        }
    }

    /// Remises à zéro et sorties de vue.
    pub(super) fn titles_observe(&mut self, ev: &WorldEvent) {
        if matches!(ev, WorldEvent::Disconnected(_) | WorldEvent::Connected) {
            self.titles.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "TID\tTitle_Class\tTitle_Name\tString TID\tRequire_level\tRequire_class_Type\tRequire_class_GuildMaster\tRequire_class_ArchLord\tRequire_Con\tRequire_Str\tRequire_Int\tRequire_Dex\tRequire_Wis\tRequire_Cha\tCost_Geld\tCost_Cash\tCost_Item_1\tCost_Item_1_Count\tCost_Item_1_Type\tCost_Item_2\tCost_Item_2_Count\tCost_Item_2_Type\tCost_Item_3\tCost_Item_3_Count\tCost_Item_3_Type\tCost_Item_4\tCost_Item_4_Count\tCost_Item_4_Type\tCost_Item_5\tCost_Item_5_Count\tCost_Item_5_Type\tFirst_Title 1\tFirst_Title 2\tFirst_Title 3\tFirst_Title 4\tFirst_Title 5\tTitle_Check_1\tTitle_Check_1_Set\tTitle_Check_1_Value\tTitle_Check_2\tTitle_Check_2_Set\tTitle_Check_2_Value\tTitle_Check_3\tTitle_Check_3_Set\tTitle_Check_3_Value\tTitle_Check_4\tTitle_Check_4_Set\tTitle_Check_4_Value\tTitle_Check_5\tTitle_Check_5_Set\tTitle_Check_5_Value\tTrade_Possibility\tDuration\tReward_Money\tReward_Item_1\tReward_Item_2\tReward_Item_3\tReward_Item_4\tReward_Item_5\tSkill_Tid_1\tSkill_Tid_2\tSkill_Tid_3\tSkill_Tid_4\tSkill_Tid_5\tSkill_Tid_6\tSkill_Tid_7\tSkill_Tid_8\tSkill_Tid_9\tSkill_Tid_10\tTitle_Effect_1\tTitle_Effect_1_Set\tTitle_Effect_1_Value_1\tTitle_Effect_1_Value_2\tTitle_Effect_2\tTitle_Effect_2_Set\tTitle_Effect_2_Value_1\tTitle_Effect_2_Value_2\tTitle_Effect_3\tTitle_Effect_3_Set\tTitle_Effect_3_Value_1\tTitle_Effect_3_Value_2\tTitle_Effect_4\tTitle_Effect_4_Set\tTitle_Effect_4_Value_1\tTitle_Effect_4_Value_2\tTitle_Effect_5\tTitle_Effect_5_Set\tTitle_Effect_5_Value_1\tTitle_Effect_5_Value_2\tTitle_Comment";

    /// Ligne de 90 colonnes : `set` remplace les colonnes données.
    fn line(set: &[(usize, &str)]) -> String {
        let mut cols = vec![""; 90];
        for (i, v) in set {
            cols[*i] = v;
        }
        cols.join("\t")
    }

    fn table() -> String {
        [
            HEAD.to_string(),
            line(&[(0, "10001"), (1, "Expert Collector"), (2, "Expert Collector of Spriggan Chargers"), (3, "10001"), (4, "1"), (5, "0"), (36, "2"), (37, "5"), (38, "50"), (69, "7"), (70, "0"), (71, "5"), (72, "2")]),
            line(&[(0, "40001"), (1, "Addict"), (2, "Small Life Potion Addict"), (4, "1"), (36, "5"), (37, "190"), (38, "150"), (69, "9"), (71, "190"), (72, "25")]),
            line(&[(0, "140001"), (1, "Advanced"), (2, "Novice Human Knight"), (4, "15"), (5, "101"), (36, "15"), (38, "15"), (59, "1555")]),
            line(&[(0, "140002"), (1, "Advanced"), (2, "Apprentice Human Knight"), (4, "30"), (5, "101"), (31, "140001"), (36, "15"), (38, "30")]),
            line(&[(0, "200001"), (1, "Researcher"), (2, "Alchemy Expert"), (36, "25"), (38, "10"), (69, "3"), (71, "10")]),
            line(&[(0, "33"), (1, "Gracia's Schola"), (2, "Gracia's Recruit"), (16, "14531"), (17, "1")]),
            line(&[(0, "36"), (1, "Gracia's Schola"), (2, "Crusader of Gracia")]),
            line(&[(0, "11000"), (1, "Christmas Event"), (2, "Santa")]),
        ]
        .join("\r\n")
    }

    const STRINGS: &str = "TID\tFirstCategory\tTitle_Class\tSecondCategory\tTitle_Name\tNecessary_Condition_Desc\tTitle_Effect_Desc\n\
        10001\t200\tExpert Collector\t1\tExpert Collector of Spriggan Chargers\tCollect at least <HFFFF33>50<HFFFFFFFF> Spriggan Chargers.\t<H00FF33>Double items.<HFFFFFFFF>\n\
        40001\t500\tAddict\t1\tSmall Life Potion Addict\tUse 100 Small Life Potions.\t+25\n\
        140001\t1500\tAdvanced\t1\tNovice Human Knight\tLevel 15\tStrength +2</n>Stamina +2\n\
        140002\t1500\tAdvanced\t2\tApprentice Human Knight\tLevel 30\tStrength +5\n\
        200001\t2100\tResearcher\t1\tAlchemy Expert\tAlchemy 10\t+10%\n\
        33\t2\tGracia's Schola\t1\tGracia's Recruit\t-\t-\n";

    fn glue() -> TitlesGlue {
        let mut g = TitlesGlue::default();
        g.data = Some(TitleData { templates: parse_data_table(&table()), texts: parse_string_table(STRINGS), colors: parse_setting(SETTING), charisma: parse_charisma(CHARISMA) });
        g.me = TitlesMe { name: "Sim".into(), level: 20, race: 1, class: 1, ..Default::default() };
        g
    }

    const SETTING: &str = "<TitleSetting><TitleFontColor>\
        <FontColor StateIndex=\"1\"   StateName=\"Ready\"     Red=\"128\"   Green=\"128\"   Blue=\"128\" />\
        <FontColor StateIndex=\"6\"   StateName=\"Activate\"  Red=\"215\"   Green=\"203\"   Blue=\"173\" />\
        </TitleFontColor></TitleSetting>";

    const CHARISMA: &str = "Charisma Point Min\tCharisma Point Max\tTitle\n0\t99\t\n100\t199\tNovice\n200\t399\tTough\n70000\t1000000\tFates\n";

    #[test]
    fn table_des_titres_filtree() {
        let t = parse_data_table(&table());
        // Christmas Event et Schola rang 4 écartés ; Schola 1 gardé (activable).
        assert_eq!(t.keys().copied().collect::<Vec<_>>(), vec![33, 10001, 40001, 140001, 140002, 200001]);
        let c = &t[&10001];
        assert_eq!(c.checks, vec![TitleCheck { kind: 2, set: 5, value: 50 }]);
        assert_eq!(c.effects, vec![[7, 0, 5, 2]]);
        let k = &t[&140002];
        assert_eq!((k.level, k.class_type, k.first_titles.clone()), (30, 101, vec![140001]));
        assert_eq!(t[&140001].skills, vec![1555]);
        assert_eq!(t[&33].items, vec![(14531, 1)]);
        assert_eq!([scope(32, "Defender of Chantra"), scope(35, "x"), scope(37, "x"), scope(38, "x"), scope(11003, "Christmas Event")], [Scope::Optional, Scope::Optional, Scope::Excluded, Scope::Optional, Scope::Excluded]);
        // Catégories de Webzen absentes de la table d'Archonia.
        assert_eq!([scope(1, "Collector"), scope(30001, "Natural Enemy (1-10)"), scope(90001, "Killer"), scope(170001, "Storyteller"), scope(9, "Natural Enemyx")], [Scope::Ep8, Scope::Ep8, Scope::Ep8, Scope::Ep8, Scope::Excluded]);
    }

    #[test]
    fn categories_et_schola() {
        let mut g = glue();
        let names: Vec<String> = g.categories().into_iter().map(|c| c.1).collect();
        assert_eq!(names, ["Expert Collector", "Addict", "Advanced", "Researcher"]);
        assert_eq!(g.titles_in(1500), vec![140001, 140002]);
        // Op 10 avec TID ≠ 0 : Schola / Defender activés.
        g.on_packet(&Assets::new(None, &[]), TitlePacket::new("Sim", 1, TitleBody::UiOpen));
        assert!(g.show_all && g.show_mini && g.optional);
        assert_eq!(g.categories()[0], (2, "Gracia's Schola".to_string()));
    }

    #[test]
    fn controles_avant_l_objectif() {
        let mut g = glue();
        assert_eq!(g.set_target_refusal(140001), None);
        g.me.class = 2;
        assert_eq!(g.set_target_refusal(140001), Some("TitleErrorMsg_RequireClassType"));
        g.me.class = 1;
        g.me.level = 10;
        assert_eq!(g.set_target_refusal(140001), Some("TitleErrorMsg_RequireLevel"));
        g.me.level = 40;
        assert_eq!(g.set_target_refusal(140002), Some("TitleErrorMsg_RequireTitle"));
        g.owned.insert(140001, Owned);
        assert_eq!(g.set_target_refusal(140002), None);
        g.optional = true;
        assert_eq!(g.set_target_refusal(33), Some("TitleErrorMsg_RequireItem"));
        g.me.items.insert(14531, 1);
        assert_eq!(g.set_target_refusal(33), None);
        for t in 0..MAX_TITLES as i32 {
            g.goals.insert(1_000_000 + t, Goal::default());
        }
        assert_eq!(g.set_target_refusal(10001), Some("CannotTryMoreTitle"));
        assert!(class_allowed(1201, 4, 2) && !class_allowed(601, 2, 1) && class_allowed(601, 2, 4));
    }

    #[test]
    fn cycle_d_un_titre() {
        let assets = Assets::new(None, &[]);
        let mut g = glue();
        assert_eq!(g.state(10001), state::READY);
        g.on_packet(&assets, TitlePacket::new("Sim", 10001, TitleBody::QuestAddResult { accepted: 100, result: 1 }));
        assert_eq!(g.state(10001), state::RUNNING);
        assert_eq!(g.detail(10001).info, "Collect at least <HFFFF33>50<HFFFFFFFF> Spriggan Chargers.</n>(0/50)");
        g.on_packet(&assets, TitlePacket::new("Sim", 10001, TitleBody::QuestCheck { values: [50, 0, 0, 0, 0], progress: true }));
        assert_eq!(g.state(10001), state::COMPLETE);
        g.on_packet(&assets, TitlePacket::new("Sim", 10001, TitleBody::QuestCompleteResult { completed: 200, result: 1 }));
        assert_eq!(g.on_packet(&assets, TitlePacket::new("Sim", 10001, TitleBody::AddResult { result: 1, skill: 0 })), Some("Expert Collector of Spriggan Chargers".into()));
        assert_eq!(g.state(10001), state::ENABLE);
        g.on_packet(&assets, TitlePacket::new("Sim", 10001, TitleBody::UseResult { on: true, result: 1, time: 1000, code: 0 }));
        assert_eq!(g.state(10001), state::ACTIVATE);
        assert_eq!(g.over_head_title("sim"), Some(("Expert Collector of Spriggan Chargers".into(), Color32::from_rgb(215, 203, 173))));
        // Refus « moins de 2 h » : le moteur reçoit l'heure du dernier changement.
        g.on_packet(&assets, TitlePacket::new("Sim", 10001, TitleBody::UseResult { on: false, result: 0, time: 0, code: 3 }));
        assert_eq!(g.too_soon.take(), Some(1000));
        g.on_packet(&assets, TitlePacket::new("Sim", 10001, TitleBody::DeleteResult { result: 1 }));
        assert_eq!((g.state(10001), g.over_head_title("Sim")), (state::READY, None));
        // Titre d'un autre joueur (op 9) ; paquets des listes d'un autre nom ignorés.
        g.on_packet(&assets, TitlePacket::new("Bob", 140001, TitleBody::UseNear { on: true }));
        assert_eq!(g.over_head_title("bob").map(|t| t.0), Some("Novice Human Knight".into()));
        g.on_packet(&assets, TitlePacket::new("Bob", 140001, TitleBody::ListEntry { changed_at: 0, valid: true, skip: 0, skill: 0 }));
        assert!(g.owned.is_empty());
        g.on_packet(&assets, TitlePacket::new("Bob", 140001, TitleBody::UseNear { on: false }));
        assert_eq!(g.over_head_title("bob"), None);
    }

    #[test]
    fn boutons_selon_l_etat() {
        let mut hud = HudState::default();
        let assets = Assets::new(None, &[]);
        let mut g = glue();
        g.selected_all = Some(140001);
        assert_eq!(g.buttons(&mut hud, &assets).map(|b| b.1), [true, false, false]);
        g.owned.insert(140001, Owned);
        assert_eq!(g.buttons(&mut hud, &assets).map(|b| b.1), [true, true, false]);
        g.active = Some(140001);
        assert_eq!(g.buttons(&mut hud, &assets).map(|b| b.1), [false, true, false]);
        g.selected_all = Some(200001);
        g.goals.insert(200001, Goal { accepted: 0, counters: [10, 0, 0, 0, 0] });
        assert_eq!(g.buttons(&mut hud, &assets).map(|b| b.1), [false, false, true]);
    }

    #[test]
    fn distance_et_charisme() {
        let mut g = glue();
        g.open_npc_windows();
        g.me.pos = (149.0, 0.0);
        g.check_distance();
        assert!(g.show_all);
        g.me.pos = (150.0, 0.0);
        g.check_distance();
        assert!(!g.show_all && !g.show_mini);
        let t = parse_charisma(CHARISMA);
        assert_eq!(charisma_rank(&t, 50), Some((0, String::new())));
        assert_eq!(charisma_rank(&t, 150), Some((1, "Novice".into())));
        assert_eq!(charisma_rank(&t, 2_000_000), None);
        g.me.stats[5] = 250;
        assert_eq!(g.my_charisma_rank(), Some((1, "Tough".into())));
        assert_eq!(too_soon_text("%02d (Month) %02d (Day) %02d (Hour) %02d (Minute)", 0).len(), "01 (Month) 01 (Day) 00 (Hour) 00 (Minute)".len());
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : tables, fenêtres et contrôles d'Archonia.
    #[test]
    fn donnees_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let mut g = TitlesGlue::default();
        let d = g.data(&assets).clone();
        // 674 titres de l'épisode 8 et 16 Schola / Defender (rangs 1-3).
        let ep8 = d.templates.values().filter(|t| scope(t.tid, &t.class) == Scope::Ep8).count();
        assert_eq!((ep8, d.templates.len()), (674, 684));
        assert_eq!(d.templates[&40001].checks[0], TitleCheck { kind: 5, set: 190, value: 150 });
        assert_eq!(d.colors.get(&6), Some(&Color32::from_rgb(215, 203, 173)));
        assert_eq!(d.charisma.len(), 14);
        let ui = assets.table("ini", "UI_1024X768.ini").unwrap();
        let all = crate::hud::UiWindow::parse(&ui, None, "UI_TitleAll").unwrap();
        for c in ["List_TitleCategory", "ListItem_TitleCategory", "List_TitleList", "ListItem_TItle", "Button_ListPage5", "Edit_TitlePage", "Edit_TitleInfo", "Edit_TitleEffect", "Button_SetTargetTitle", "Button_ActivateTitle", "Button_CompleteTitle", "Button_CloseDialog"] {
            assert!(all.control(c).is_some(), "{c}");
        }
        let mini = crate::hud::UiWindow::parse(&ui, None, "UI_TitleMini").unwrap();
        assert!(mini.control("Button_SurrenderTitle").is_some() && mini.control("StaticText_TitleMiniCaption").is_some());
        assert!(crate::hud::UiWindow::parse(&ui, None, "UI_TitleCurrent").unwrap().control("Button_Close").is_some());
    }
}
