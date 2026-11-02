//! Maîtrises des compétences : onglets du livre (`UI_Skill`) et de l'instructeur (`UI_SkillBuy`), arbres
//! supérieurs (`UI_SkillHighLevel`, `UI_SkillHighLevelUp`), héroïques (`UI_HeroicSkill`,
//! `UI_HeroicSkill_Upgrade`) et fenêtres du Dragon Scion (`UI_SkillDragonScion_*`, `UI_SkillUpDragonScion_*`,
//! `UI_ScionArchlordSkill`). Fenêtres : `hud_mastery.rs` ; colle : `app_skills.rs`.
//!
//! * `SKILLMASTERY.txt` : une section par race et classe (« Human Knight »… « DragonScion Monk »), puis ses
//!   catégories `Battle`, `Enchant`, `Special`, `Passive`, `Produce` (onglets Combat, Buff, Class, Passive :
//!   `SkillSetMastery0` à `3` d'Archonia, FUN_1405dc1a0…) ; section `Archlord` (onglet AL, `SkillSetMastery4`).
//! * `SkillMastery_High.txt` et `SkillMastery_Heroic.txt` : une ligne par classe (TID du gabarit en colonne 0),
//!   puis ses compétences dans l'ordre des cases (TID, nom, ConditionA et B « TID;niveau » d'une compétence,
//!   ConditionC « TID;N » d'un gabarit de personnage, ConditionD « total; ») ; le chargeur héroïque (FUN_14022a510)
//!   ne lit que A et D, le supérieur (FUN_140229e10) range C à part de A et B. Contrôle (FUN_1402b8f40) : A et B,
//!   niveau de la compétence apprise ; C, niveaux cumulés des compétences de la race et de la classe du gabarit
//!   (« 1722;30 » : 30 niveaux de compétences du Slayer) ; D, niveaux cumulés de toutes les compétences apprises
//!   (supérieur, FUN_1402fdc60(-1, -1)) ou points héroïques dépensés (héroïque, FUN_1402b9200). Archonia remplit les 11 cases supérieures dans l'ordre du fichier (FUN_1405dfc60, liste du
//!   gabarit du joueur, 0x5c octets par entrée) ; le Berserker (TID 4) a deux arbres héroïques (lignes « 0 » et
//!   « 1 », compétences décalées d'une colonne), le second rangé sous 10004 (FUN_14022a510). La ligne
//!   « 10004;4 » de `SkillMastery_High.txt` porte les 15 passives du Dragon Scion (`UI_SkillDragonScion_Pas`,
//!   15 cases) : 10004 = 10000 + race 4, l'arbre de la race que FUN_1402b8f40 consulte quand la compétence n'est
//!   pas dans l'arbre du gabarit (race 0 : aucune condition) ; le « ;4 » n'est lu par aucun contrôle.

use crate::assets::Assets;
use std::collections::HashMap;

pub use crate::hud::SkillTab;

/// Clé de l'arbre des passives du Dragon Scion dans `SkillMastery_High.txt` (ligne « 10004;4 » : 10000 + race 4).
pub const SCION_PASSIVE_TREE: i32 = 10004;
/// Décalage de la clé du second arbre héroïque d'une classe (Berserker : 4 → 10004, FUN_14022a510).
pub const SECOND_TREE: i32 = 10000;

/// Une case d'un arbre de maîtrise.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MasteryRow {
    pub tid: i32,
    /// Conditions « TID;niveau » d'une compétence (ConditionA et B).
    pub requires: Vec<(i32, i32)>,
    /// ConditionC « TID;N » (arbre supérieur seulement) : N niveaux cumulés des compétences apprises admises par
    /// la race et la classe du gabarit de personnage TID (FUN_1402fdc60).
    pub class_levels: Option<(i32, i32)>,
    /// ConditionD, 0 sinon : niveaux cumulés de toutes les compétences apprises (arbre supérieur) ou points
    /// héroïques dépensés (arbre héroïque).
    pub total: i32,
}

/// Condition « TID;niveau » d'une case.
fn condition(cell: &str) -> Option<(i32, i32)> {
    let mut p = cell.split(';');
    Some((p.next()?.trim().parse().ok()?, p.next()?.trim().parse().ok()?))
}

/// Arbres d'un fichier `SkillMastery_*.txt`, par clé de classe ; `heroic` : ConditionB et C ne sont pas lues
/// (FUN_14022a510).
pub fn parse_trees(text: &str, heroic: bool) -> HashMap<i32, Vec<MasteryRow>> {
    let mut out: HashMap<i32, Vec<MasteryRow>> = HashMap::new();
    let mut class: Option<i32> = None;
    let mut key: Option<i32> = None;
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split('\t').map(str::trim).collect();
        let cell = |i: usize| f.get(i).copied().unwrap_or("");
        if !cell(0).is_empty() {
            class = cell(0).split(';').next().and_then(|c| c.trim().parse::<i32>().ok());
            key = class;
            continue;
        }
        // Ligne d'arbre (numéro seul en colonne 1) : second arbre de la classe sous clé + 10000.
        if !cell(1).is_empty() && cell(2).is_empty() && (3..=7).all(|i| cell(i).is_empty()) {
            if let (Some(c), Ok(n)) = (class, cell(1).parse::<i32>()) {
                key = Some(if n == 0 { c } else { c + SECOND_TREE * n });
            }
            continue;
        }
        let off = usize::from(cell(1).is_empty());
        let (Some(k), Ok(tid)) = (key, cell(1 + off).parse::<i32>()) else { continue };
        let requires = (if heroic { 3..=3 } else { 3..=4 }).filter_map(|i| condition(cell(i + off))).collect();
        let class_levels = if heroic { None } else { condition(cell(5 + off)) };
        let total = cell(6 + off).split_once(';').and_then(|(t, _)| t.trim().parse().ok()).unwrap_or(0);
        out.entry(k).or_default().push(MasteryRow { tid, requires, class_levels, total });
    }
    out
}

/// Sections de `SKILLMASTERY.txt` dans l'ordre du fichier, avec (race, classe) : la dixième est le Dragon
/// Scion, toutes formes.
const SECTIONS: [(&str, i32, i32); 10] = [
    ("Human Knight", 1, 1),
    ("Human Archer", 1, 2),
    ("Human Mage", 1, 4),
    ("Orc Berserker", 2, 1),
    ("Orc Hunter", 2, 2),
    ("Orc Sorcerer", 2, 4),
    ("MoonElf Warrior", 3, 1),
    ("MoonElf Ranger", 3, 2),
    ("MoonElf Elementalist", 3, 4),
    ("DragonScion Monk", 4, 0),
];

/// Catégories de `SKILLMASTERY.txt`, dans l'ordre des onglets.
const CATEGORIES: [&str; 5] = ["Battle", "Enchant", "Special", "Passive", "Produce"];

/// `SKILLMASTERY.txt` : noms de compétences par section (en minuscules) et par catégorie, et la liste
/// « Archlord ».
#[derive(Debug, Clone, Default)]
pub struct Categories {
    pub sections: HashMap<String, [Vec<String>; 5]>,
    pub archlord: Vec<String>,
}

pub fn parse_categories(text: &str) -> Categories {
    let mut out = Categories::default();
    let mut section = String::new();
    let mut category: Option<usize> = None;
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').map(str::trim).collect();
        let cell = |i: usize| f.get(i).copied().unwrap_or("");
        if !cell(0).is_empty() {
            section = cell(0).to_lowercase();
            category = None;
            continue;
        }
        if !cell(1).is_empty() {
            category = CATEGORIES.iter().position(|c| c.eq_ignore_ascii_case(cell(1)));
        }
        let name = cell(2).to_lowercase();
        if name.is_empty() {
            continue;
        }
        if section == "archlord" {
            out.archlord.push(name);
        } else if let Some(c) = category {
            out.sections.entry(section.clone()).or_default()[c].push(name);
        }
    }
    out
}

impl Categories {
    /// Noms des compétences de la catégorie `cat` pour la race et la classe données.
    pub fn names(&self, race: i32, class: i32, cat: usize) -> &[String] {
        let Some((name, ..)) = SECTIONS.iter().find(|(_, r, c)| *r == race && (*c == class || race == 4)) else { return &[] };
        self.sections.get(&name.to_lowercase()).map(|s| s[cat.min(4)].as_slice()).unwrap_or(&[])
    }
}

/// Tables des maîtrises.
#[derive(Debug, Default)]
pub struct MasteryTables {
    pub high: HashMap<i32, Vec<MasteryRow>>,
    pub heroic: HashMap<i32, Vec<MasteryRow>>,
    pub categories: Categories,
    /// (race, classe) du gabarit de chaque clé de classe des deux fichiers (`CharacterTemplatePublic.ini`).
    pub class_of: HashMap<i32, (i32, i32)>,
}

impl MasteryTables {
    pub fn load(assets: &Assets) -> Self {
        let text = |f: &str| super::npc_glue::client_text_in(assets, "ini", f);
        let high = parse_trees(&text("SkillMastery_High.txt"), false);
        let heroic = parse_trees(&text("SkillMastery_Heroic.txt"), true);
        let categories = parse_categories(&text("SKILLMASTERY.txt"));
        let class_of = high.keys().chain(heroic.keys()).filter(|k| **k < SECOND_TREE).filter_map(|k| Some((*k, crate::scene::template_race_class(assets, *k)?))).collect();
        log::info!("maîtrises : {} arbre(s) supérieur(s), {} héroïque(s), {} section(s)", high.len(), heroic.len(), categories.sections.len());
        MasteryTables { high, heroic, categories, class_of }
    }

    /// Clé de classe du gabarit de race et classe données.
    pub fn class_key(&self, race: i32, class: i32) -> Option<i32> {
        let mut keys: Vec<i32> = self.class_of.iter().filter(|(_, rc)| **rc == (race, class)).map(|(k, _)| *k).collect();
        keys.sort_unstable();
        keys.first().copied()
    }

    /// Cases de l'arbre d'un onglet pour un personnage de race et classe données (`None` : pas d'arbre).
    pub fn tree(&self, tab: SkillTab, race: i32, class: i32) -> Option<&[MasteryRow]> {
        let rows = match tab {
            SkillTab::Combo => self.high.get(&self.class_key(race, class)?),
            SkillTab::Heroic | SkillTab::ScionHeroic => self.heroic.get(&self.class_key(race, class)?),
            SkillTab::ScionPassive => self.high.get(&SCION_PASSIVE_TREE),
            t => {
                let (r, c) = t.scion_form()?;
                self.high.get(&self.class_key(r, c)?)
            }
        };
        rows.map(Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arbres_et_categories() {
        let high = "Class TID\tSkill TID\tSkill Name\tConditionA\tConditionB\tConditionC\tConditionD\n96\t\t\t\t\t\t\n\t526\tx\t\t\t\t\n\t525\ty\t526;4\t\t\t\n\t558\tz\t525;2\t527;2\t\t\n\t\t\t\t\t\t\n1722\t\t\t\t\t\t\n\t1155\tw\t1153;1\t\t1722;30\t\n10004;4\t\t\t\t\t\t\n\t351\tp\t\t\t\t\n\t1242\tq\t351;1\t\t\t20;\n";
        let t = parse_trees(high, false);
        assert_eq!(t[&96].iter().map(|r| r.tid).collect::<Vec<_>>(), vec![526, 525, 558]);
        assert_eq!(t[&96][2].requires, vec![(525, 2), (527, 2)]);
        // ConditionC rangée à part de A et B (FUN_140229e10).
        assert_eq!((t[&1722][0].requires.clone(), t[&1722][0].class_levels), (vec![(1153, 1)], Some((1722, 30))));
        assert_eq!(t[&96][2].class_levels, None);
        assert_eq!((t[&SCION_PASSIVE_TREE][1].requires.clone(), t[&SCION_PASSIVE_TREE][1].total), (vec![(351, 1)], 20));
        // Berserker héroïque : deux arbres, compétences décalées d'une colonne, le second sous 10004.
        let heroic = "Class TID\tSkill TID\tSkill Name\tConditionA\tConditionB\tConditionC\tConditionD\t\n4\t\t\t\t\t\t\t\n\t0\t\t\t\t\t\t\n\t\t1472\tx\t\t\t\t\t\n\t\t1467\ty\t\t\t\t9;\t\n\t1\t\t\t\t\t\t\n\t\t4427\tz\t\t\t\t\t\n8\t\t\t\t\t\t\t\n\t1482\tw\t\t\t\t\t\n";
        let h = parse_trees(heroic, true);
        assert_eq!(h[&4].iter().map(|r| (r.tid, r.total)).collect::<Vec<_>>(), vec![(1472, 0), (1467, 9)]);
        assert_eq!(h[&10004][0].tid, 4427);
        assert_eq!(h[&8][0].tid, 1482);
        // Chargeur héroïque : ConditionB et C ignorées.
        let h = parse_trees("Class TID	Skill TID	Skill Name	ConditionA	ConditionB	ConditionC	ConditionD
8						
	1482	w	1481;3	1480;2	1722;30	19;
", true);
        assert_eq!((h[&8][0].requires.clone(), h[&8][0].class_levels, h[&8][0].total), (vec![(1481, 3)], None, 19));

        let cats = parse_categories("Human Knight\t\t\t\n\tBattle\tDual Slash\t\n\t\tLinear Blow\t\n\tPassive\tSupreme Damage\t\nDragonScion Monk\t\t\t\n\tProduce\tCooking\t\nArchlord\t\t\t\n\t\tDay and Night\t\n");
        assert_eq!(cats.names(1, 1, 0), ["dual slash", "linear blow"]);
        assert_eq!(cats.names(1, 1, 3), ["supreme damage"]);
        assert!(cats.names(1, 2, 0).is_empty());
        assert_eq!(cats.names(4, 1, 4), ["cooking"]);
        assert_eq!(cats.archlord, ["day and night"]);
    }

    #[test]
    fn onglets() {
        assert_eq!(SkillTab::from_label("Passive", true), Some(SkillTab::ScionPassive));
        assert_eq!(SkillTab::from_label("Heroic", false), Some(SkillTab::Heroic));
        assert_eq!(SkillTab::Combo.trainer_window(), Some("UI_SkillHighLevelUp"));
        assert_eq!(SkillTab::ScionHeroic.book_window(), "UI_SkillDragonScion_Heroic");
        assert!(SkillTab::Slayer.is_tree() && !SkillTab::Al.is_tree() && SkillTab::ScionHeroic.heroic());
    }

    /// Fichiers du client (copies déchiffrées d'ArchCore) : nombre de cases par arbre égal au nombre de grilles
    /// des fenêtres (11 supérieures, 10 héroïques, Scion 18, Slayer 11, Orbiter et Summoner 12, passives 15).
    #[test]
    fn arbres_du_client() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../ArchCore/data/client-ini");
        let read = |f: &str| std::fs::read(dir.join(f)).ok().map(|b| String::from_utf8_lossy(&b).into_owned());
        let (Some(high), Some(heroic)) = (read("SkillMastery_High.txt"), read("SkillMastery_Heroic.txt")) else { return };
        let (h, e) = (parse_trees(&high, false), parse_trees(&heroic, true));
        for k in [96, 1, 6, 4, 8, 3, 460, 9, 377] {
            assert_eq!(h[&k].len(), 11, "arbre supérieur {k}");
            assert_eq!(e[&k].len(), 10, "arbre héroïque {k}");
        }
        assert_eq!([1724, 1722, 1723, 1732, SCION_PASSIVE_TREE].map(|k| h[&k].len()), [18, 11, 12, 12, 15]);
        assert_eq!(e[&10004].len(), 10);
        // ConditionC des arbres du Scion : Slayer 1155, Orbiter 1165, Summoner 1178.
        let c = |k: i32, tid: i32| h[&k].iter().find(|r| r.tid == tid).and_then(|r| r.class_levels);
        assert_eq!([c(1722, 1155), c(1723, 1165), c(1732, 1178)], [Some((1722, 30)), Some((1723, 30)), Some((1732, 30))]);
    }
}
