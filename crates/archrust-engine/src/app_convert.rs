//! Conversion des objets côté moteur : état de chaque objet (renforcement, sockets, TID sertis),
//! forgeron (fonction 22 d'un PNJ), pierre d'esprit glissée sur un objet du sac, infobulle des objets.
//! Protocole : `archrust_netclient::convert` ; fenêtres d'Archonia : `hud_convert.rs`.
//!
//! * Forgeron : bouton « Ameliorer l'objet » (`sysstr` 327) de son dialogue → AGPMEVENTITEMCONVERT
//!   op 0 ; l'accord ouvre l'inventaire et `UI_ConvertPhysics` (`Convert_MainUI_Open`). Onglets :
//!   renforcement (objet + potion de renforcement → op 1), anneau de contrat (objet → op 2), rune
//!   (« Prayer Stone » : objet + rune → op 3 ; une pierre d'esprit posée là part en op 4). Chaque
//!   demande passe par `UI_ConvertConfirm` (`Convert_Physical_Confirm`, `Convert_Socket_Confirm`,
//!   `Convert_Rune_Confirm`).
//! * Résultats (Archonia FUN_140882700, FUN_140881a80, FUN_140883580) dans la zone de texte de
//!   l'onglet : `Convert_Physical_*` (op 5), `Convert_Socket_*` (op 6), `Convert_Rune_*` (op 8) ;
//!   pierres (op 7, 0 = réussite) : messages `Convert_SS_Result_*` de l'`[EventMap]`, en message système.
//! * Pierre d'esprit (op 4) ou rune (op 3) lâchée sur un objet du sac : envoyée directement (le serveur
//!   n'exige pas de forgeron).
//! * Options (AGPMITEM champ 20) : `Item_AdditionalOption_Title` puis la `Description` de chaque option
//!   visible d'itemoptiontable.txt ; runes serties : les options de leur `OptionTID`.
//! * Infobulle (FUN_1405471d0, FUN_14054b060, FUN_1405440f0) : « Nom + niveau [remplis/sockets] »
//!   (bleu `ITEM_TOOLTIP_COLOR_SPIRIT` s'il porte une pierre), rang (lettre de la ligne `PhysicalRank`
//!   d'`ItemConvertTable.txt`, `sysstr` 369 et 769), `Item_Convertable_Physical_Rank`,
//!   `Item_Convertable_SpiritStone_Rank`, puis `Item_Convert_Socket_Status` et une ligne par pierre
//!   (« Nom + valeur » sur une arme, « Nom + valeur% » sur une armure, couleur de l'élément).
//!
//! Sockets d'un objet sans état reçu : max(1, `MinSocket`) pour une arme (hors lance), une armure ou
//! un « autre » (genres 4, 8, 16, 64), comme le client et ArchCore.

use super::App;
use crate::assets::Assets;
use crate::hud::{ConvertTab, ConvertUi, ConvertView, HudState, ItemUi};
use crate::net::NetLink;
use crate::scene::item_name_for_tid;
use crate::ui::UiState;
use archrust_netclient::convert::{op, result, rune_result, stone_result, ConvertCommand, ConvertEvent, ConvertState};
use archrust_netclient::world::ItemRecord;
use archrust_netclient::Command;
use egui::Color32;
use glam::Vec3;
use std::collections::HashMap;

/// Au-delà, la fenêtre du forgeron se ferme (comme celles des autres PNJ).
const CLOSE_RANGE: f32 = 2200.0;
/// Statut AgpmItem d'un objet du sac, d'un objet porté.
const STATUS_INVENTORY: i8 = 2;
const STATUS_EQUIP: i8 = 3;
/// Genres (`Kind` d'itemdatatable.txt) : armure, arme ; type d'arme lance (`Type` 17).
const KIND_ARMOR: i64 = 1;
const KIND_WEAPON: i64 = 2;
const WEAPON_LANCE: i64 = 17;
/// Consommables (`ItemType` 1) : `Type` 2 pierre d'esprit, 8 potion de renforcement.
const USABLE_SPIRIT_STONE: i64 = 2;
const USABLE_REINFORCEMENT: i64 = 8;
const USABLE_RUNE: i64 = 9;
/// Couleur de chaque élément de pierre (1 magie, 2 eau, 3 feu, 4 terre, 5 air), FUN_1405440f0.
const ELEMENT_COLORS: [u32; 5] = [0xb136ff, 0x0078ff, 0xff3710, 0xe0c867, 0x7cff38];

/// État de conversion connu d'un objet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemConvert {
    pub level: u8,
    pub sockets: u8,
    pub filled: Vec<i32>,
}

/// `ItemTooltipDesc.txt` (`TID`, `Name`, `Description`) : descriptions non vides, tronquées à 640 octets.
fn parse_tooltip_descs(text: &str) -> HashMap<i32, String> {
    let mut out = HashMap::new();
    for line in text.lines().skip(1) {
        let c: Vec<&str> = line.split('\t').collect();
        let (Some(tid), Some(desc)) = (c.first().and_then(|v| v.trim().parse::<i32>().ok()), c.get(2).map(|v| v.trim()).filter(|v| !v.is_empty())) else { continue };
        let mut end = desc.len().min(640);
        while !desc.is_char_boundary(end) {
            end -= 1;
        }
        out.insert(tid, desc[..end].to_string());
    }
    out
}

#[cfg(test)]
mod desc_tests {
    use super::*;

    #[test]
    fn descriptions_d_objets() {
        let long = "x".repeat(700);
        let t = parse_tooltip_descs(&format!("TID\tName\tDescription\n84\tBell\tUne\\nDeux\n85\tVide\t\n86\tLong\t{long}\n"));
        assert_eq!(t[&84], "Une\\nDeux");
        assert!(!t.contains_key(&85));
        assert_eq!(t[&86].len(), 640);
    }
}

/// Colonnes utiles d'itemdatatable.txt.
#[derive(Debug, Clone, Default)]
struct ItemInfo {
    kind: i64,
    part: i64,
    item_type: i64,
    kind_type: i64,
    subtype: i64,
    spirit: i64,
    rank: i64,
    min_socket: i64,
    physical_rank: i64,
    /// Options du modèle (`OptionTID`, liste séparée par des `;`) : celles d'une rune sertie.
    options: Vec<i32>,
    /// Groupes d'options liées du modèle (`Link_id`, `;`, 10 au plus ; `AgpdItemTemplate + 0x30c`).
    links: Vec<i32>,
}

/// Colonnes d'itemoptiontable.txt que somme la fenêtre du personnage (chargeur FUN_1401d8c30), rangées par
/// indice `k` de FUN_14051a150, et bornes de niveau d'une option liée.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct OptionValues {
    /// 2 taux critique (`skill_rate` si `skill_type` = 1), 3 dégâts critiques (`Critical`, quel que soit
    /// `skill_type`), 4 `Ignore_Attribute_Defense_Rate`, 5 `Ignore_Physical_Defense_Rate`,
    /// 6 `Critical_Defense_Rate`, 7 `StunResistance`, 8 « renvoi » : probabilité (`skill_rate` si `skill_type`
    /// = 3, coup reçu changé en PV), 9 « renvoi » : quantité (`damage_convert_hp`). Valeurs brutes.
    k: [i32; 10],
    /// `Level_min`, `Level_max` (options liées).
    level: (i32, i32),
}

impl OptionValues {
    /// Valeurs d'une ligne, `num` donnant la colonne nommée (0 si vide ou absente).
    fn of(num: impl Fn(&str) -> i32) -> Self {
        let (skill_type, skill_rate) = (num("skill_type"), num("skill_rate"));
        let mut k = [0; 10];
        k[2] = if skill_type == 1 { skill_rate } else { 0 };
        k[3] = num("Critical");
        k[4] = num("Ignore_Attribute_Defense_Rate");
        k[5] = num("Ignore_Physical_Defense_Rate");
        k[6] = num("Critical_Defense_Rate");
        k[7] = num("StunResistance");
        k[8] = if skill_type == 3 { skill_rate } else { 0 };
        k[9] = num("damage_convert_hp");
        OptionValues { k, level: (num("Level_min"), num("Level_max")) }
    }
}

/// Somme élément par élément (`int` d'Archonia : dépassement en boucle).
fn add_into(acc: &mut [i32; 10], v: &[i32; 10]) {
    for (a, b) in acc.iter_mut().zip(v) {
        *a = a.wrapping_add(*b);
    }
}

impl ItemInfo {
    fn is_weapon(&self) -> bool {
        self.kind == KIND_WEAPON && self.kind_type != WEAPON_LANCE
    }

    fn usable(&self, t: i64) -> bool {
        self.item_type == 1 && self.kind_type == t
    }

    /// Reçoit des sockets : arme (hors lance), armure, « autre » (bouclier, anneau, collier…).
    fn takes_sockets(&self) -> bool {
        self.is_weapon() || matches!(self.kind, KIND_ARMOR | 4 | 8 | 16 | 64)
    }

    /// Renforçable avec une potion d'après le client (FUN_1403415b0) : arme hors lance, genre 4,
    /// armure de corps, tête, mains, jambes ou pieds.
    fn physical_convertible(&self) -> bool {
        self.is_weapon() || self.kind == 4 || (self.kind == KIND_ARMOR && matches!(self.part, 2 | 3 | 5 | 6 | 7))
    }
}

/// Personnage qui regarde l'infobulle : niveau, race et classe (`CharacterTemplatePublic.ini`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(super) struct Wearer {
    pub level: i64,
    pub race: i64,
    pub class: i64,
}

impl Wearer {
    pub(super) fn of(assets: &Assets, tid: i32, level: i64) -> Wearer {
        let read = || -> Option<(i64, i64)> {
            let table = assets.table("ini", "CharacterTemplatePublic.ini").ok()?;
            let header = table.section("")?;
            let column = |n: &str| header.entries.iter().find(|(_, v)| v.eq_ignore_ascii_case(n)).map(|(k, _)| k.clone());
            let section = table.section(&tid.to_string())?;
            let num = |c: Option<String>| c.and_then(|c| section.get(&c)?.trim().parse::<i64>().ok()).unwrap_or(0);
            Some((num(column("Race")), num(column("Class"))))
        };
        let (race, class) = read().unwrap_or_default();
        Wearer { level, race, class }
    }
}

/// Conditions d'équipement d'un objet (bloc `RestrictFactor` d'ItemTemplateAll.ini : la seconde
/// colonne `Level`, puis `Race` et `Class`, masques de bits, 0 pour tous), `Type` et `Kind`.
#[derive(Debug, Clone, Copy, Default)]
struct Restrict {
    item_type: i64,
    kind: i64,
    level: i64,
    race: i64,
    class: i64,
}

fn restrict_of(assets: &Assets, tid: i32) -> Option<Restrict> {
    let table = assets.table("ini", "ItemTemplateAll.ini").ok()?;
    let header = table.section("")?;
    let column = |n: &str| header.entries.iter().find(|(_, v)| v.eq_ignore_ascii_case(n)).map(|(k, _)| k.clone());
    let item = table.section(&tid.to_string())?;
    let num = |v: Option<&str>| v.and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0);
    let first = |n: &str| num(column(n).and_then(|c| item.get(&c)));
    let level = column("Level").map(|c| num(item.get_all(&c).nth(1))).unwrap_or(0);
    Some(Restrict { item_type: first("Type"), kind: first("Kind"), level, race: first("Race"), class: first("Class") })
}

/// Bit d'une race ou d'une classe (1 à 4) dans un masque de conditions ; un masque nul admet tout.
fn admits(mask: i64, value: i64) -> bool {
    mask == 0 || (1..=4).contains(&value) && mask & (1 << (value - 1)) != 0
}

/// Ligne de la première section d'ItemConvertTable.txt.
#[derive(Debug, Clone, Default)]
struct Level {
    rank: String,
    /// Rangs de pierre admis (indice 1 à 5).
    stones: [bool; 6],
}

/// Tables du client utiles à la conversion.
#[derive(Default)]
struct ConvertData {
    items: HashMap<i32, ItemInfo>,
    /// Indice = niveau 1 à 16.
    levels: Vec<Level>,
    /// Rang de pierre (indice 1 à 5) : valeur sur une arme, sur une armure (%).
    stones: [(i32, i32); 6],
    colors: HashMap<String, Color32>,
    messages: HashMap<String, String>,
    events: HashMap<String, String>,
    rank_title: String,
    rank_suffix: String,
    /// itemoptiontable.txt : TID → (`Description`, colonne `visible` remplie).
    options: HashMap<i32, (String, bool)>,
    /// TID des runes (itemruneattributetable.txt).
    runes: std::collections::HashSet<i32>,
    /// itemoptiontable.txt : TID → colonnes sommées par la fenêtre du personnage.
    option_values: HashMap<i32, OptionValues>,
    /// Options de chaque groupe lié (`Link_id` > 0), dans l'ordre du fichier (carte `AgpmItem + 0x18d8`).
    link_groups: HashMap<i32, Vec<i32>>,
}

impl ConvertData {
    fn msg(&self, key: &str) -> String {
        self.messages.get(key).cloned().unwrap_or_default()
    }

    fn color(&self, key: &str) -> Color32 {
        self.colors.get(key).copied().unwrap_or(Color32::WHITE)
    }

    fn info(&self, tid: Option<i32>) -> ItemInfo {
        tid.and_then(|t| self.items.get(&t).cloned()).unwrap_or_default()
    }

    /// Rune : dans itemruneattributetable.txt, ou consommable de `Type` 9.
    fn is_rune(&self, tid: Option<i32>) -> bool {
        tid.is_some_and(|t| self.runes.contains(&t)) || self.info(tid).usable(USABLE_RUNE)
    }

    /// Option liée du groupe `link` pour un niveau (FUN_1401c7f00) : la première du groupe, dans l'ordre du
    /// fichier, avec `Level_min` ≤ niveau ≤ `Level_max`.
    fn linked(&self, link: i32, level: i32) -> Option<&OptionValues> {
        self.link_groups.get(&link)?.iter().filter_map(|t| self.option_values.get(t)).find(|o| o.level.0 <= level && level <= o.level.1)
    }

    /// Somme des `OptionTID` (5 au plus, `AgpdItemTemplate + 0x280`) du modèle `tid`.
    fn template_options(&self, tid: i32, acc: &mut [i32; 10]) {
        let Some(info) = self.items.get(&tid) else { return };
        for o in info.options.iter().take(5).filter_map(|u| self.option_values.get(u)) {
            add_into(acc, &o.k);
        }
    }

    /// Liste interne de 50 `u16` d'un objet (`AgpdItem + 0x2c6`), remplie par le client lui-même : à la
    /// réception de l'état de conversion (FUN_14028e150), pour chaque serti (1 à 8) dont le modèle est une rune
    /// (`ItemType` 1, `Type` 9), chacune de ses 5 `OptionTID` passe par FUN_14032d620, qui range le TID de
    /// l'option (s'il existe dans la table) à la première place libre, tant qu'il en reste. Archonia n'efface
    /// pas la liste à une nouvelle réception (accumulation éventuelle non reproduite : la liste est tirée de
    /// l'état connu).
    fn rune_option_list(&self, filled: &[i32]) -> Vec<i32> {
        let mut list = Vec::new();
        if !(1..=8).contains(&filled.len()) {
            return list;
        }
        for tid in filled {
            let Some(info) = self.items.get(tid).filter(|i| i.usable(USABLE_RUNE)) else { continue };
            for u in info.options.iter().take(5) {
                if list.len() < 50 && self.option_values.contains_key(u) {
                    list.push(*u);
                }
            }
        }
        list
    }

    /// Part d'un objet porté dans les caractéristiques secondaires, pour chaque indice `k` (FUN_14051a150) :
    /// (a) options liées du modèle au niveau du personnage, (b) 5 options de l'objet (AGPMITEM champ 20),
    /// (c) liste interne des runes, (d) `OptionTID` des modèles sertis ; (e) prestige (ajout d'Archonia, hors
    /// EP8) : 0.
    fn item_properties(&self, tid: i32, options: &[u16], filled: &[i32], level: i32) -> [i32; 10] {
        let mut acc = [0; 10];
        // (a) `Link_id` > 0 du modèle (10 au plus).
        for link in self.items.get(&tid).map(|i| i.links.as_slice()).unwrap_or_default().iter().take(10).filter(|l| **l > 0) {
            if let Some(o) = self.linked(*link, level) {
                add_into(&mut acc, &o.k);
            }
        }
        // (b) options tirées de l'objet.
        for o in options.iter().take(5).filter_map(|t| self.option_values.get(&(*t as i32))) {
            add_into(&mut acc, &o.k);
        }
        // (c) Incohérence d'Archonia, reproduite : la liste contient des TID d'*option*, que FUN_14051a150 relit
        // comme des TID de *modèle d'objet* (FUN_1401af480) avant d'en sommer les `OptionTID`. La plupart ne
        // tombent sur aucun modèle à options (la rune n'est donc pas comptée deux fois), mais quelques-uns
        // coïncident avec un modèle qui en a (option 5012 → modèle 5012 « Logo Ring » : 5 options en plus).
        for t in self.rune_option_list(filled) {
            self.template_options(t, &mut acc);
        }
        // (d) modèles sertis.
        for t in filled {
            self.template_options(*t, &mut acc);
        }
        acc
    }
}

/// itemoptiontable.txt (colonnes lues par nom, en-têtes anglais) : `Description` et `visible` (infobulles),
/// colonnes de la fenêtre du personnage ([`OptionValues`]) et groupes liés (`Link_id`).
fn load_option_table(d: &mut ConvertData, text: &str) {
    let mut lines = text.lines();
    let Some(header) = lines.next() else { return };
    let cols: Vec<&str> = header.split('\t').map(str::trim).collect();
    let col = |name: &str| cols.iter().position(|c| c.eq_ignore_ascii_case(name));
    let (tid, desc, visible, link) = (col("tid"), col("Description"), col("visible"), col("Link_id"));
    let names = ["skill_type", "skill_rate", "Critical", "damage_convert_hp", "Ignore_Physical_Defense_Rate", "Ignore_Attribute_Defense_Rate", "Critical_Defense_Rate", "StunResistance", "Level_min", "Level_max"];
    let numeric: Vec<(&str, Option<usize>)> = names.iter().map(|n| (*n, col(n))).collect();
    for line in lines {
        let cells: Vec<&str> = line.split('\t').collect();
        let get = |c: Option<usize>| c.and_then(|i| cells.get(i).copied()).unwrap_or("");
        let int = |c: Option<usize>| get(c).trim().parse::<f64>().map_or(0, |v| v as i32);
        let Ok(t) = get(tid).trim().parse::<i32>() else { continue };
        d.options.insert(t, (get(desc).trim().trim_matches('"').to_string(), !get(visible).trim().is_empty()));
        d.option_values.insert(t, OptionValues::of(|name| numeric.iter().find(|(n, _)| *n == name).map_or(0, |(_, c)| int(*c))));
        let l = int(link);
        if l > 0 {
            d.link_groups.entry(l).or_default().push(t);
        }
    }
}

fn load_data(assets: &Assets) -> ConvertData {
    let mut d = ConvertData { levels: vec![Level::default(); 17], ..Default::default() };
    let num = |s: Option<&str>| s.and_then(|v| v.trim().parse::<f64>().ok()).map(|v| v as i64).unwrap_or(0);
    let text = super::npc_glue::client_text_in(assets, "ini", "itemdatatable.txt");
    let mut lines = text.lines();
    if let Some(header) = lines.next() {
        let cols: Vec<&str> = header.split('\t').map(str::trim).collect();
        let col = |name: &str| cols.iter().position(|c| c.eq_ignore_ascii_case(name));
        let (tid, kind, part, item_type, kind_type, spirit, rank, min_socket, physical) =
            (col("TID"), col("Kind"), col("Part"), col("ItemType"), col("Type"), col("Spirit Type"), col("Rank"), col("MinSocket"), col("PhysicalRank"));
        let subtype = col("SubType");
        let (option_tid, link_id) = (col("OptionTID"), col("Link_id"));
        let ids = |v: Option<&str>| v.map(|v| v.split(|c: char| !c.is_ascii_digit()).filter_map(|n| n.parse::<i32>().ok()).filter(|n| *n > 0).collect()).unwrap_or_default();
        for line in lines {
            let cells: Vec<&str> = line.split('\t').collect();
            let get = |c: Option<usize>| c.and_then(|i| cells.get(i).copied());
            let Some(t) = get(tid).and_then(|v| v.trim().parse::<i32>().ok()) else { continue };
            d.items.insert(
                t,
                ItemInfo {
                    kind: num(get(kind)),
                    part: num(get(part)),
                    item_type: num(get(item_type)),
                    kind_type: num(get(kind_type)),
                    subtype: num(get(subtype)),
                    spirit: num(get(spirit)),
                    rank: num(get(rank)),
                    min_socket: num(get(min_socket)),
                    physical_rank: num(get(physical)),
                    // « 5594;13207 », parfois entre crochets : « [20;135;40;1021] ».
                    options: ids(get(option_tid)),
                    // « 9;16;29;22;30 ».
                    links: ids(get(link_id)),
                },
            );
        }
    }
    // itemoptiontable.txt (`ini_fr` d'abord : descriptions traduites) : tid, Description, …, visible.
    load_option_table(&mut d, &super::npc_glue::client_text(assets, "itemoptiontable.txt"));
    // itemruneattributetable.txt : Item_Name, TID, …
    let text = super::npc_glue::client_text_in(assets, "ini", "itemruneattributetable.txt");
    let mut lines = text.lines();
    if let Some(tid) = lines.next().and_then(|h| h.split('\t').position(|c| c.trim().eq_ignore_ascii_case("TID"))) {
        d.runes = lines.filter_map(|l| l.split('\t').nth(tid).and_then(|v| v.trim().parse().ok())).collect();
    }
    // ItemConvertTable.txt : sections séparées par une ligne vide ; la première (niveaux : bonus,
    // rang, pierres admises) et la sixième (rangs de pierre : valeur arme, chance, valeur armure, chance).
    let table = super::npc_glue::client_text_in(assets, "ini", "ItemConvertTable.txt");
    let mut sections: Vec<Vec<Vec<&str>>> = vec![Vec::new()];
    for line in table.lines() {
        let cells: Vec<&str> = line.trim_end_matches('\r').split('\t').collect();
        if cells.iter().all(|c| c.trim().is_empty()) {
            if !sections.last().is_some_and(Vec::is_empty) {
                sections.push(Vec::new());
            }
            continue;
        }
        sections.last_mut().unwrap().push(cells);
    }
    for row in sections.first().into_iter().flatten().skip(1) {
        let lv = num(row.first().copied()) as usize;
        if (1..=16).contains(&lv) {
            let mut stones = [false; 6];
            for (k, ok) in stones.iter_mut().enumerate().skip(1) {
                *ok = row.get(3 + k).is_some_and(|c| !c.trim().is_empty());
            }
            d.levels[lv] = Level { rank: row.get(3).map(|c| c.trim().to_string()).unwrap_or_default(), stones };
        }
    }
    for row in sections.get(5).into_iter().flatten().skip(1) {
        let r = num(row.first().copied()) as usize;
        if (1..=5).contains(&r) {
            d.stones[r] = (num(row.get(1).copied()) as i32, num(row.get(3).copied()) as i32);
        }
    }
    if let Ok(t) = assets.table("ini", "color.ini") {
        if let Some(s) = t.section("ItemTooltip") {
            for (k, v) in &s.entries {
                if let Ok(rgb) = u32::from_str_radix(v.trim(), 16) {
                    d.colors.insert(k.clone(), Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8));
                }
            }
        }
    }
    d.messages = super::npc_glue::client_text(assets, "uimessage.txt").lines().filter_map(|l| l.split_once('\t')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect();
    d.events = super::portal_glue::ui_events(assets);
    d.rank_title = super::npc_glue::sysstr(assets, 369).unwrap_or_default();
    d.rank_suffix = super::npc_glue::sysstr(assets, 769).unwrap_or_default();
    log::info!(
        "conversion : {} objet(s) décrits, {} niveau(x) de renforcement, {} option(s), {} rune(s)",
        d.items.len(),
        d.levels.iter().filter(|l| l.stones[1]).count(),
        d.options.len(),
        d.runes.len()
    );
    d
}

/// Catalyseur EP8 : usage 8, sous-type 0. Garde la sélection compatible ; sinon priorité à une
/// potion liée au propriétaire (bit 8), puis dernière potion du parcours de la grille.
fn physical_catalyst(data: &ConvertData, items: &HashMap<i32, ItemRecord>, flags: &HashMap<i32, u32>, current: Option<i32>) -> Option<i32> {
    let eligible = |i: &&ItemRecord| {
        let info = data.info(i.template_id);
        i.status == Some(STATUS_INVENTORY) && i.count.unwrap_or(1) > 0 && info.usable(USABLE_REINFORCEMENT) && info.subtype == 0
    };
    if let Some(i) = current.and_then(|iid| items.get(&iid)).filter(|i| eligible(i)) { return Some(i.id) }
    let mut candidates: Vec<_> = items.values().filter(eligible).collect();
    candidates.sort_by_key(|i| (i.grid.unwrap_or([i16::MAX; 3]), i.id));
    candidates.iter().find(|i| flags.get(&i.id).copied().unwrap_or(0) & 8 != 0).or_else(|| candidates.last()).map(|i| i.id)
}

/// Remplace les `%d` / `%s` d'une phrase d'Archonia dans l'ordre.
fn fill(template: &str, args: &[String]) -> String {
    let mut out = String::new();
    let mut rest = template;
    let mut args = args.iter();
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 1..];
        match tail.chars().next() {
            Some('d' | 's') => {
                out.push_str(args.next().map(String::as_str).unwrap_or(""));
                rest = &tail[1..];
            }
            Some('%') => {
                out.push('%');
                rest = &tail[1..];
            }
            _ => {
                out.push('%');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

#[derive(Default)]
pub struct ConvertGlue {
    /// État de chaque objet converti, par IID.
    pub states: HashMap<i32, ItemConvert>,
    /// Options de chaque objet (AGPMITEM champ 20), par IID.
    pub options: HashMap<i32, Vec<u16>>,
    data: Option<ConvertData>,
    /// Forgeron dont la fenêtre est ouverte.
    npc: Option<i32>,
    tab: ConvertTab,
    /// Objet à améliorer et potion ou pierre posés dans la fenêtre (IID).
    item: Option<i32>,
    material: Option<i32>,
    /// Texte de la zone de résultat de l'onglet.
    edit: String,
    /// Demande en attente de confirmation (`UI_ConvertConfirm`) et sa phrase.
    confirm: Option<(ConvertCommand, String)>,
    /// `ItemTooltipDesc.txt` : descriptions par TID.
    tooltip_descs: Option<HashMap<i32, String>>,
    /// Boîte d'information (`Convert_Err_*`).
    info: Option<String>,
    /// Signature des objets dont l'infobulle a été préparée.
    tips_for: u64,
    /// Version des états (à chaque paquet de conversion).
    version: u64,
    /// Infobulle épinglée par `/infobulle` (essais, captures).
    pinned_tip: Option<i32>,
    /// Dernier calcul de la part des objets des caractéristiques secondaires.
    properties_at: Option<std::time::Instant>,
}

/// Contenu d'une socket pour les effets des pierres d'esprit (`app_stones.rs`) : rune, ou pierre
/// d'esprit avec son élément (`Spirit Type`, `SPIRIT_STONE_TYPE` du client) et son rang.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SocketFill {
    pub rune: bool,
    pub spirit: i64,
    pub rank: i64,
}

impl ConvertGlue {
    fn data(&mut self, assets: &Assets) -> &ConvertData {
        self.data.get_or_insert_with(|| load_data(assets))
    }

    /// État de conversion connu de l'objet `iid` pour les effets des pierres d'esprit : niveau de
    /// renforcement, nombre de sockets et contenu de chaque socket, dans l'ordre.
    /// Colonne `Rank` d'`itemdatatable.txt` du modèle `tid` (facteur 10, 2 de l'objet, `FUN_140572e20`).
    pub(super) fn item_rank(&mut self, assets: &Assets, tid: i32) -> i64 {
        self.data(assets).info(Some(tid)).rank
    }

    pub(super) fn socket_fills(&mut self, assets: &Assets, iid: i32) -> Option<(u8, u8, Vec<SocketFill>)> {
        let st = self.states.get(&iid)?.clone();
        let d = self.data(assets);
        let fills = st.filled.iter().map(|t| {
            let info = d.info(Some(*t));
            SocketFill { rune: d.is_rune(Some(*t)), spirit: info.spirit, rank: info.rank }
        });
        Some((st.level, st.sockets, fills.collect()))
    }

    fn apply(&mut self, s: &ConvertState) {
        let e = self.states.entry(s.iid).or_default();
        if let Some(l) = s.level {
            e.level = l;
        }
        if let Some(n) = s.sockets {
            e.sockets = n;
        }
        match (&s.filled, s.filled_count) {
            (Some(f), _) => e.filled = f.clone(),
            (None, Some(0)) => e.filled.clear(),
            _ => {}
        }
        self.version += 1;
    }

    /// État d'un objet, sockets comptés comme le client pour un objet sans état reçu.
    fn state_of(&mut self, assets: &Assets, item: &ItemRecord) -> ItemConvert {
        let mut st = self.states.get(&item.id).cloned().unwrap_or_default();
        let info = self.data(assets).info(item.template_id);
        if st.sockets == 0 && info.takes_sockets() {
            st.sockets = info.min_socket.clamp(1, 8) as u8;
        }
        st
    }

    /// « Nom + niveau [remplis/sockets] » (FUN_1405471d0 ; pas de sockets affichés sur une lance ni
    /// sur le genre 64).
    fn name_line(&mut self, assets: &Assets, item: &ItemRecord) -> String {
        let st = self.state_of(assets, item);
        let info = self.data(assets).info(item.template_id);
        let mut name = item.template_id.and_then(|t| item_name_for_tid(assets, t)).unwrap_or_default();
        if st.level > 0 {
            name.push_str(&format!(" + {}", st.level));
        }
        let hide = (info.kind == KIND_WEAPON && info.kind_type == WEAPON_LANCE) || info.kind == 64;
        if st.sockets > 0 && !hide {
            name.push_str(&format!(" [{}/{}]", st.filled.len(), st.sockets));
        }
        name
    }

    /// Lignes de l'infobulle d'un objet.
    fn tooltip(&mut self, assets: &Assets, item: &ItemRecord, who: Wearer, labels: &[String; 4]) -> Vec<(String, Color32)> {
        let st = self.state_of(assets, item);
        let name = self.name_line(assets, item);
        let item_options = self.options.get(&item.id).cloned().unwrap_or_default();
        let restrict = item.template_id.and_then(|t| restrict_of(assets, t));
        let d = self.data(assets);
        let info = d.info(item.template_id);
        let normal = d.color("ITEM_TOOLTIP_COLOR_NORMAL");
        let spirit = d.color("ITEM_TOOLTIP_COLOR_SPIRIT");
        let mut lines = vec![(name, if st.filled.is_empty() { normal } else { spirit })];
        // Conditions d'équipement, en LACK_ABILITY quand le personnage ne les remplit pas :
        // « Required Level : %3d Level » (FUN_14054bb20), races (FUN_14054bd60), classes selon les
        // races admises (FUN_14054b370) ; rien pour un objet de genre 0x40.
        if let Some(r) = restrict.filter(|r| r.item_type == 0) {
            let lack = d.color("ITEM_TOOLTIP_COLOR_LACK_ABILITY");
            if r.level >= 1 {
                let text = format!("{} : {:3}{}", labels[0], r.level, labels[1]);
                lines.push((text, if who.level >= r.level { normal } else { lack }));
            }
            if r.kind != 0x40 {
                let race_ok = admits(r.race, who.race);
                let has = |bit: i64| r.race == 0 || r.race & bit != 0;
                let mut races = labels[2].clone();
                if r.race == 0 {
                    races += &format!(" {}", d.msg("Item_Restrict_RaceAll"));
                } else {
                    let mut first = true;
                    for (bit, key) in [(1, "Item_Restrict_RaceHuman"), (2, "Item_Restrict_RaceOrc"), (4, "Item_Restrict_RaceMoonElf")] {
                        if has(bit) {
                            races += &format!("{}{}", if first { " " } else { " / " }, d.msg(key));
                            first = false;
                        }
                    }
                    if has(8) {
                        races += &format!(" {}", d.msg("DragonScion"));
                    }
                }
                lines.push((races, if race_ok { normal } else { lack }));
                let mut classes = labels[3].clone();
                if r.class == 0 {
                    classes += &format!(" {}", d.msg("Item_Restrict_ClassAll"));
                } else {
                    let groups: [(i64, [&str; 4]); 4] = [
                        (1, ["Knight", "Berserker", "SwashBuckler", "Slayer"]),
                        (2, ["Archer", "Hunter", "Ranger", "Obiter"]),
                        (8, ["Mage", "Sorcerer", "Elementalist", "Summerner"]),
                        (4, ["", "", "", "Scion"]),
                    ];
                    for (bit, names) in groups {
                        if r.class & bit == 0 {
                            continue;
                        }
                        let mut group = String::new();
                        for (_, key) in names.iter().enumerate().filter(|(i, k)| !k.is_empty() && has(1 << i)) {
                            group += &format!("{}{}", if group.is_empty() { " " } else { " / " }, d.msg(key));
                        }
                        classes += &format!(" {group}");
                    }
                }
                lines.push((classes, if race_ok && admits(r.class, who.class) { normal } else { lack }));
            }
        }
        let in_bag = item.status == Some(STATUS_INVENTORY);
        // « Rank » : rang de renforcement possible et ce que l'objet accepte.
        if (1..=16).contains(&info.physical_rank) {
            let row = &d.levels[info.physical_rank as usize];
            lines.push((String::new(), normal));
            lines.push((format!("{} : {} {}", d.rank_title, row.rank, d.rank_suffix).replace("  ", " "), normal));
            if in_bag && info.physical_convertible() {
                lines.push((fill(&d.msg("Item_Convertable_Physical_Rank"), &[info.physical_rank.to_string()]), normal));
            }
            let stone_ok = (info.is_weapon() || info.kind == KIND_ARMOR) && st.sockets > 0;
            if in_bag && stone_ok {
                let ranks = (1..=5).take_while(|k| row.stones[*k]).count();
                lines.push((fill(&d.msg("Item_Convertable_SpiritStone_Rank"), &[ranks.to_string()]), normal));
            }
        }
        // « Convert » (FUN_1405440f0) : options tirées (titre `Item_AdditionalOption_Title` une fois,
        // puis la `Description` de chaque option visible), options du modèle, sockets, pierres, runes.
        let option_color = d.color("ITEM_TOOLTIP_COLOR_OPTION");
        let visible = |tid: i32| d.options.get(&tid).filter(|(desc, vis)| *vis && !desc.is_empty()).map(|(desc, _)| desc.clone());
        let rolled: Vec<String> = item_options.iter().filter_map(|t| visible(*t as i32)).collect();
        let fixed: Vec<String> = info.options.iter().filter_map(|t| visible(*t)).collect();
        if !rolled.is_empty() || !fixed.is_empty() {
            lines.push((String::new(), normal));
        }
        if !rolled.is_empty() {
            lines.push((d.msg("Item_AdditionalOption_Title"), normal));
            lines.extend(rolled.into_iter().map(|t| (t, option_color)));
        }
        lines.extend(fixed.into_iter().map(|t| (t, option_color)));
        if !st.filled.is_empty() {
            lines.push((String::new(), normal));
            lines.push((fill(&d.msg("Item_Convert_Socket_Status"), &[st.sockets.to_string(), st.filled.len().to_string()]), normal));
            for tid in st.filled.iter().filter(|t| !d.is_rune(Some(**t))) {
                let stone = d.info(Some(*tid));
                let stone_name = item_name_for_tid(assets, *tid).unwrap_or_default();
                let (weapon_value, armor_value) = d.stones.get(stone.rank.clamp(0, 5) as usize).copied().unwrap_or_default();
                let text = if info.kind == KIND_WEAPON { format!("{stone_name} + {weapon_value}") } else { format!("{stone_name} + {armor_value}%") };
                let color = match stone.spirit {
                    e @ 1..=5 => {
                        let v = ELEMENT_COLORS[e as usize - 1];
                        Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
                    }
                    _ => spirit,
                };
                lines.push((text, color));
            }
            // Runes : la `Description` de chaque option de leur `OptionTID`, couleur RUNE.
            let rune_color = d.color("ITEM_TOOLTIP_COLOR_RUNE");
            for tid in st.filled.iter().filter(|t| d.is_rune(Some(**t))) {
                for opt in &d.info(Some(*tid)).options {
                    if let Some((desc, _)) = d.options.get(opt).filter(|(desc, _)| !desc.is_empty()) {
                        lines.push((desc.clone(), rune_color));
                    }
                }
            }
        }
        // `ItemTooltipDesc.txt` (`FUN_140549ec0` : colonnes 0 TID, 2 description, 640 caractères au plus ;
        // `\n` écrit en toutes lettres) : en dernier (place dans l'infobulle déduite).
        let descs = self.tooltip_descs.get_or_insert_with(|| parse_tooltip_descs(&super::npc_glue::client_text(assets, "ItemTooltipDesc.txt")));
        if let Some(text) = item.template_id.and_then(|t| descs.get(&t)) {
            lines.extend(text.split("\\n").filter(|l| !l.trim().is_empty()).map(|l| (l.to_string(), normal)));
        }
        lines
    }

    fn info_event(&mut self, assets: &Assets, event: &str) {
        let text = self.data(assets).events.get(event).cloned().unwrap_or_default();
        if !text.is_empty() {
            self.info = Some(text);
        }
    }

    /// Texte de départ de la zone de résultat : renforcement « titre \n coût » (FUN_140882700),
    /// rune « Convert_Init_Title » sans objet, sinon « Convert_Rune_None », puis le coût (FUN_140883440).
    fn reset_edit(&mut self, assets: &Assets) {
        let (item, tab) = (self.item.is_some(), self.tab);
        let d = self.data(assets);
        self.edit = match (item, tab) {
            (false, ConvertTab::Socket) => d.msg("Convert_Init_Title"),
            (true, ConvertTab::Socket) => String::new(),
            (false, ConvertTab::Physical) => format!("{}\n<C16773320>{}", d.msg("Convert_Init_Title"), d.msg("Convert_Physical_Cost_Catalyst")),
            (true, ConvertTab::Physical) => format!("<C16773320>{}", d.msg("Convert_Physical_Cost_Catalyst")),
            (false, ConvertTab::Rune) => format!("{}\n<C16773320>{}", d.msg("Convert_Init_Title"), d.msg("Convert_Rune_Cost")),
            (true, ConvertTab::Rune) => format!("{}\n<C16773320>{}", d.msg("Convert_Rune_None"), d.msg("Convert_Rune_Cost")),
        };
    }

    /// Bouton d'amélioration : vérifications du client, puis confirmation.
    fn start(&mut self, assets: &Assets, items: &HashMap<i32, ItemRecord>) {
        let Some(item) = self.item.and_then(|i| items.get(&i)).cloned() else {
            self.info_event(assets, "Convert_Err_NotExistItem");
            return;
        };
        let name = self.name_line(assets, &item);
        let st = self.state_of(assets, &item);
        let material = self.material.and_then(|i| items.get(&i)).cloned();
        let tab = self.tab;
        let d = self.data(assets);
        let request = match tab {
            ConvertTab::Physical => match &material {
                Some(m) => {
                    let base = item.template_id.and_then(|t| item_name_for_tid(assets, t)).unwrap_or_default();
                    let next = format!("{base} +{} [{}/{}]", st.level + 1, st.filled.len(), st.sockets);
                    Ok((ConvertCommand::Physical { iid: item.id, catalyst: m.id }, fill(&d.msg("Convert_Physical_Confirm"), &[name, next])))
                }
                None => Err("Convert_Err_InvalidCatalyst"),
            },
            ConvertTab::Socket => Ok((ConvertCommand::Socket { iid: item.id }, fill(&d.msg("Convert_Socket_Confirm"), &[name]))),
            // Rune (op 3) ; une pierre d'esprit posée ici part en op 4.
            ConvertTab::Rune => match &material {
                Some(m) => {
                    let rune = m.template_id.and_then(|t| item_name_for_tid(assets, t)).unwrap_or_default();
                    let cmd = if d.is_rune(m.template_id) { ConvertCommand::Rune { iid: item.id, rune: m.id } } else { ConvertCommand::SpiritStone { iid: item.id, stone: m.id } };
                    Ok((cmd, fill(&d.msg("Convert_Rune_Confirm"), &[name, rune])))
                }
                None => Err("Convert_Err_InvalidRune"),
            },
        };
        match request {
            Ok(r) => self.confirm = Some(r),
            Err(e) => self.info_event(assets, e),
        }
    }

    /// Texte du résultat d'un renforcement (FUN_140882700) ; le client français met la phrase avant
    /// le niveau.
    fn physical_text(&mut self, assets: &Assets, code: i32, level: u8, before: u8) -> String {
        let d = self.data(assets);
        let cost = d.msg("Convert_Physical_Cost_Catalyst");
        let red = |t: String| format!("<C16711680>{t}\n<C16773320>{cost}");
        match code {
            result::SUCCESS => format!("<C10092288>{} {level}\n<C16773320>{cost}", d.msg("Convert_Physical_Success")),
            result::FULL => format!("{}\n<C16773320>{cost}", d.msg("Convert_Physical_Already_Full")),
            result::IMPROPER_ITEM | 4 => format!("{}\n<C16773320>{cost}", d.msg("Convert_Physical_Improper_Item")),
            result::KEEP => red(d.msg("Convert_Physical_KeepCurrent")),
            result::INIT_SAME => red(fill(&d.msg("Convert_Physical_Initialize_Same"), &[before.to_string()])),
            result::INIT => red(d.msg("Convert_Physical_Initialize")),
            result::DESTROY => red(d.msg("Convert_Physical_Destroy")),
            result::IMPROPER => red(d.msg("Convert_Socket_Improper_Item")),
            _ => format!("{}\n<C16773320>{cost}", d.msg("Convert_Init_Title")),
        }
    }

    /// Texte du résultat d'un ajout de socket (FUN_140881a80).
    fn socket_text(&mut self, assets: &Assets, code: i32, sockets: u8) -> String {
        let d = self.data(assets);
        let red = |key: &str| format!("<C16711680>{}\n<C16773320>", d.msg(key));
        match code {
            result::SUCCESS => format!("<C10092288>{}\n<C16773320>", fill(&d.msg("Convert_Socket_Success"), &[sockets.to_string()])),
            result::FULL => d.msg("Convert_Socket_Already_Full"),
            4 => red("Convert_Socket_NotEnoughMoney"),
            result::KEEP => red("Convert_Socket_KeepCurrent"),
            result::INIT_SAME => red("Convert_Socket_Initialize_Same"),
            result::INIT => red("Convert_Socket_Initialize"),
            result::DESTROY => red("Convert_Socket_Destroy"),
            result::IMPROPER => format!("<C16711680>{}", d.msg("Convert_Socket_Improper_Item")),
            _ => d.msg("Convert_Init_Title"),
        }
    }

    /// Texte du résultat d'un sertissage de rune (FUN_140883580).
    fn rune_text(&mut self, assets: &Assets, code: i32, rune: &str) -> String {
        let has_item = self.item.is_some();
        let d = self.data(assets);
        let cost = d.msg("Convert_Rune_Cost");
        let red = |key: &str| format!("<C16711680>{}\n<C16773320>{cost}", d.msg(key));
        match code {
            rune_result::SUCCESS => format!("<C10092288>{rune} {}\n<C16773320>{cost}", d.msg("Convert_Rune_Success")),
            rune_result::INVALID_RUNE => red("Convert_Rune_Invalid_Rune"),
            rune_result::FULL => red("Convert_Rune_Already_Full"),
            rune_result::LOW_ITEM_LEVEL => red("Convert_Rune_Low_Item_Level"),
            rune_result::IMPROPER_ITEM => red("Convert_Rune_Improper_Item"),
            rune_result::ALREADY_ANTI_CONVERT => red("Convert_Rune_Already_Anti_Conv"),
            rune_result::KEEP => red("Convert_Rune_KeepCurrent"),
            rune_result::INIT_SAME => red("Convert_Rune_Initialize_Same"),
            rune_result::INIT => red("Convert_Rune_Initialize"),
            rune_result::DESTROY => red("Convert_Rune_Destroy"),
            rune_result::INVALID_ITEM => red("Convert_Socket_Improper_Item"),
            _ => format!("{}\n<C16773320>{cost}", d.msg(if has_item { "Convert_Rune_None" } else { "Convert_Init_Title" })),
        }
    }
}

fn send(net: Option<&NetLink>, cmd: ConvertCommand) {
    if let Some(net) = net {
        net.send(Command::Convert(cmd));
    }
}

/// Fenêtres du forgeron et pierre d'esprit lâchée sur un objet du sac. À appeler après
/// l'inventaire, avant `app_items::panel` (qui traiterait la pierre comme un déplacement).
#[allow(clippy::too_many_arguments)]
pub fn panel(
    ctx: &egui::Context,
    glue: &mut ConvertGlue,
    item_requests: &mut Vec<ItemUi>,
    items: &HashMap<i32, ItemRecord>,
    item_flags: &HashMap<i32, u32>,
    hud: &mut HudState,
    assets: &Assets,
    net: Option<&NetLink>,
    ui: &mut UiState,
) {
    // Pierre d'esprit (op 4) ou rune (op 3) lâchée sur un autre objet du sac : sertissage.
    item_requests.retain(|r| {
        let ItemUi::DropOnBag { drag, grid } = *r else { return true };
        if drag.worn || drag.bank {
            return true;
        }
        let d = glue.data(assets);
        let (stone, rune) = (d.info(Some(drag.tid)).usable(USABLE_SPIRIT_STONE), d.is_rune(Some(drag.tid)));
        if !stone && !rune {
            return true;
        }
        let Some(target) = items.values().find(|i| i.status == Some(STATUS_INVENTORY) && i.grid == Some(grid) && i.id != drag.iid) else { return true };
        send(net, if rune { ConvertCommand::Rune { iid: target.id, rune: drag.iid } } else { ConvertCommand::SpiritStone { iid: target.id, stone: drag.iid } });
        false
    });

    // Objets posés disparus (détruit, potion ou pierre consommée).
    if glue.item.is_some_and(|i| !items.contains_key(&i)) {
        glue.item = None;
    }
    if glue.material.is_some_and(|i| !items.contains_key(&i)) {
        glue.material = None;
        if glue.npc.is_some() && glue.tab == ConvertTab::Physical {
            glue.material = physical_catalyst(glue.data(assets), items, item_flags, None);
        }
    }

    if glue.npc.is_some() {
        let item = glue.item.and_then(|i| items.get(&i)).cloned();
        let view_item = item.map(|i| (i.template_id.unwrap_or(0), glue.name_line(assets, &i)));
        let material = glue.material.and_then(|i| items.get(&i)).and_then(|i| i.template_id);
        let edit = glue.edit.clone();
        let view = ConvertView { tab: glue.tab, item: view_item, material, edit: &edit, cost: 0, money: hud.stats.money.unwrap_or(0) };
        let mut out = Vec::new();
        crate::hud::draw_convert(ctx, hud, assets, view, &mut out);
        for r in out {
            match r {
                // `Convert_UI_Close` : la fenêtre et l'inventaire se ferment.
                ConvertUi::Close => {
                    glue.npc = None;
                    glue.confirm = None;
                    ui.show_inventory = false;
                }
                ConvertUi::Tab(t) => {
                    glue.tab = t;
                    glue.material = if t == ConvertTab::Physical { physical_catalyst(glue.data(assets), items, item_flags, None) } else { None };
                    glue.reset_edit(assets);
                }
                ConvertUi::DropItem(drag) => {
                    if items.get(&drag.iid).is_some_and(|i| i.status == Some(STATUS_INVENTORY)) {
                        glue.item = Some(drag.iid);
                        if glue.tab == ConvertTab::Physical {
                            let current = glue.material;
                            glue.material = physical_catalyst(glue.data(assets), items, item_flags, current);
                        }
                        glue.reset_edit(assets);
                    } else {
                        glue.info_event(assets, "Convert_Err_TryEquipItem");
                    }
                }
                ConvertUi::DropMaterial(drag) => {
                    let info = glue.data(assets).info(Some(drag.tid));
                    let rune = glue.data(assets).is_rune(Some(drag.tid));
                    match glue.tab {
                        ConvertTab::Physical if info.usable(USABLE_REINFORCEMENT) => glue.material = Some(drag.iid),
                        ConvertTab::Physical => glue.info_event(assets, "Convert_Err_InvalidCatalyst"),
                        ConvertTab::Rune if info.usable(USABLE_SPIRIT_STONE) || rune => glue.material = Some(drag.iid),
                        ConvertTab::Rune => glue.info_event(assets, "Convert_Err_InvalidRune"),
                        ConvertTab::Socket => {}
                    }
                }
                ConvertUi::ClearItem => {
                    glue.item = None;
                    glue.reset_edit(assets);
                }
                ConvertUi::ClearMaterial => glue.material = None,
                ConvertUi::Start => glue.start(assets, items),
            }
        }
    }

    if let Some((cmd, text)) = glue.confirm.clone() {
        match crate::hud::draw_convert_confirm(ctx, hud, assets, &text) {
            Some(true) => {
                send(net, cmd);
                glue.confirm = None;
            }
            Some(false) => glue.confirm = None,
            None => {}
        }
    }
    if let Some(text) = glue.info.clone() {
        if crate::hud::info_box(ctx, hud, assets, &text) {
            glue.info = None;
        }
    }
    if let Some(tip) = glue.pinned_tip.and_then(|i| hud.item_tips.get(&i)) {
        let at = ctx.pointer_latest_pos().unwrap_or(ctx.screen_rect().center() - egui::Vec2::new(40.0, 160.0));
        crate::hud::item_tooltip_at(ctx, at, tip, "");
    }
}

impl App {
    pub(super) fn on_convert_event(&mut self, ev: ConvertEvent) {
        let assets = self.assets.clone();
        match ev {
            ConvertEvent::State(s) => self.convert.apply(&s),
            ConvertEvent::Options { iid, options } => {
                self.convert.options.insert(iid, options);
                self.convert.version += 1;
            }
            ConvertEvent::WindowGranted { npc } => {
                // `Convert_MainUI_Open` : inventaire et renforcement.
                let glue = &mut self.convert;
                glue.npc = Some(npc);
                glue.tab = ConvertTab::Physical;
                glue.item = None;
                glue.material = physical_catalyst(glue.data(&assets), &self.items, &self.auction.item_flags, None);
                glue.reset_edit(&assets);
                self.ui.show_inventory = true;
            }
            ConvertEvent::Result { op: o, result: code, state, added_tid } => {
                let before = self.convert.states.get(&state.iid).map(|s| s.level).unwrap_or(0);
                self.convert.apply(&state);
                let after = self.convert.states.get(&state.iid).cloned().unwrap_or_default();
                log::info!("conversion : op {o}, résultat {code}, objet {} (+{}, {} socket(s), {:?}), TID ajouté {added_tid:?}", state.iid, after.level, after.sockets, after.filled);
                let glue = &mut self.convert;
                match o {
                    op::RESPONSE_PHYSICAL => {
                        let text = glue.physical_text(&assets, code, after.level, before);
                        if glue.npc.is_some() && glue.tab == ConvertTab::Physical {
                            glue.edit = text.clone();
                        }
                    }
                    op::RESPONSE_SOCKET => {
                        let text = glue.socket_text(&assets, code, after.sockets);
                        if glue.npc.is_some() && glue.tab == ConvertTab::Socket {
                            glue.edit = text.clone();
                        }
                    }
                    op::RESPONSE_RUNE => {
                        let rune = added_tid.and_then(|t| item_name_for_tid(&assets, t)).unwrap_or_default();
                        let text = glue.rune_text(&assets, code, &rune);
                        if glue.npc.is_some() && glue.tab == ConvertTab::Rune {
                            glue.edit = text.clone();
                        }
                    }
                    _ => {
                        let event = match code {
                            stone_result::SUCCESS => "Convert_SS_Result_Success",
                            stone_result::FAILED => "Convert_SS_Result_Fail",
                            stone_result::FULL => "Convert_SS_Result_AlreadyFull",
                            stone_result::INVALID_STONE | stone_result::IMPROPER_ITEM => "Convert_SS_Result_ImproperItem",
                            _ => "Convert_SS_Result_Fail",
                        };
                        let text = glue.data(&assets).events.get(event).cloned().unwrap_or_default();
                        if glue.npc.is_some() && glue.tab == ConvertTab::Rune {
                            let color = if code == stone_result::SUCCESS { "10092288" } else { "16711680" };
                            glue.edit = format!("<C{color}>{text}");
                        }
                        self.hud.push_system(&text);
                    }
                }
            }
        }
    }

    /// À chaque image : infobulles des objets (à leur changement), fenêtre fermée en s'éloignant du
    /// forgeron.
    pub(super) fn convert_frame(&mut self) {
        let assets = self.assets.clone();
        let mut sig = self.convert.version.wrapping_mul(0x9e37_79b9_7f4a_7c15);
        for i in self.items.values() {
            sig = sig.wrapping_add((i.id as u64).wrapping_mul(31) ^ ((i.template_id.unwrap_or(0) as u64) << 20) ^ ((i.status.unwrap_or(0) as u64) << 52));
        }
        sig ^= self.items.len() as u64;
        let who = Wearer::of(&assets, self.player.as_ref().map(|p| p.tid).unwrap_or(0), self.hud.stats.level.unwrap_or(1) as i64);
        sig = sig.wrapping_add(((who.level as u64) << 8 | (who.race as u64) << 4 | who.class as u64).wrapping_mul(0x1000_0000_01b3));
        if sig != self.convert.tips_for {
            self.convert.tips_for = sig;
            let items: Vec<ItemRecord> = self.items.values().cloned().collect();
            // Libellés de sysstr.txt : « Required Level », « Level », « Race », « Class ».
            let labels = [378, 262, 379, 380].map(|id| super::npc_glue::sysstr(&assets, id).unwrap_or_default());
            self.hud.item_tips = items.iter().map(|i| (i.id, self.convert.tooltip(&assets, i, who, &labels))).collect();
        }
        self.worn_properties_frame(&assets);
        if let Some(npc) = self.convert.npc {
            let me = self.player.as_ref().map(|p| p.position);
            let far = match (me, self.entities.iter().find(|e| e.cid == npc).map(|e| e.position)) {
                (Some(a), Some(b)) => Vec3::new(a.x - b.x, 0.0, a.z - b.z).length() > CLOSE_RANGE,
                _ => true,
            };
            if far {
                self.convert.npc = None;
                self.convert.confirm = None;
            }
        }
    }
}

impl App {
    /// Part des objets portés dans les caractéristiques secondaires de la fenêtre du personnage
    /// (`CharacterStats::item_properties`, lue par `hud_status.rs`). Comme l'OnIdle d'AgcmUICharacter
    /// (FUN_1404fd3e0) : au plus une fois par seconde, pour le joueur seul ; somme sur toutes les cases
    /// d'équipement (FUN_14051aa90, sans contrôle de durabilité) au niveau du personnage.
    fn worn_properties_frame(&mut self, assets: &Assets) {
        if self.convert.properties_at.is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(1000)) {
            return;
        }
        self.convert.properties_at = Some(std::time::Instant::now());
        self.convert.data(assets);
        let glue = &self.convert;
        let Some(d) = glue.data.as_ref() else { return };
        let level = self.hud.stats.level.unwrap_or(0);
        let mut acc = [0; 10];
        for i in self.items.values().filter(|i| i.status == Some(STATUS_EQUIP)) {
            let Some(tid) = i.template_id else { continue };
            let options = glue.options.get(&i.id).map(Vec::as_slice).unwrap_or_default();
            let filled = glue.states.get(&i.id).map(|s| s.filled.as_slice()).unwrap_or_default();
            add_into(&mut acc, &d.item_properties(tid, options, filled, level));
        }
        self.hud.stats.item_properties = acc;
    }

    /// Objet du sac par nom ou TID (le premier par IID).
    fn convert_find(&self, wanted: &str) -> Option<ItemRecord> {
        let wanted = wanted.trim().to_lowercase();
        if wanted.is_empty() {
            return None;
        }
        let mut found: Vec<&ItemRecord> = self
            .items
            .values()
            .filter(|i| i.status == Some(STATUS_INVENTORY))
            .filter(|i| i.template_id.map(|t| t.to_string()) == Some(wanted.clone()) || i.template_id.and_then(|t| item_name_for_tid(&self.assets, t)).is_some_and(|n| n.to_lowercase().contains(&wanted)))
            .collect();
        found.sort_by_key(|i| i.id);
        found.first().map(|i| (*i).clone())
    }

    /// Commandes du forgeron (essais sans souris, `--exec`) : `/ameliorer nom` pose l'objet dans la
    /// fenêtre, `/catalyseur nom` la potion ou la pierre, `/onglet physique|anneau|pierre`,
    /// `/convertir` (bouton d'amélioration ; `oui` : sans confirmation), `/sertir pierre > objet`
    /// (comme une pierre lâchée sur l'objet), `/infobulle nom` épingle l'infobulle d'un objet.
    pub(super) fn convert_slash(&mut self, cmd: &str, arg: &str) -> bool {
        let assets = self.assets.clone();
        match cmd {
            "/ameliorer" | "/améliorer" | "/catalyseur" => match self.convert_find(arg) {
                Some(i) if cmd == "/catalyseur" => self.convert.material = Some(i.id),
                Some(i) => {
                    self.convert.item = Some(i.id);
                    self.convert.reset_edit(&assets);
                }
                None => self.ui.push_chat(format!("[forgeron] Aucun objet « {} » dans le sac.", arg.trim())),
            },
            "/onglet" => {
                self.convert.tab = match arg.trim() {
                    "anneau" | "socket" => ConvertTab::Socket,
                    "pierre" | "rune" => ConvertTab::Rune,
                    _ => ConvertTab::Physical,
                };
                self.convert.material = None;
                self.convert.reset_edit(&assets);
            }
            "/convertir" => {
                let items = self.items.clone();
                self.convert.start(&assets, &items);
                if arg.trim() == "oui" {
                    if let Some((c, _)) = self.convert.confirm.take() {
                        send(self.net.as_ref(), c);
                    }
                }
            }
            "/sertir" => {
                let (a, b) = arg.split_once('>').unwrap_or((arg, ""));
                match (self.convert_find(a), self.convert_find(b)) {
                    (Some(stone), Some(item)) => {
                        let rune = self.convert.data(&assets).is_rune(stone.template_id);
                        send(self.net.as_ref(), if rune { ConvertCommand::Rune { iid: item.id, rune: stone.id } } else { ConvertCommand::SpiritStone { iid: item.id, stone: stone.id } });
                    }
                    _ => self.ui.push_chat("[forgeron] usage : /sertir pierre > objet"),
                }
            }
            "/infobulle" => {
                // Sans nom : l'objet posé chez le forgeron ; `/infobulle non` la retire.
                self.convert.pinned_tip = match arg.trim() {
                    "" => self.convert.item,
                    "non" | "off" => None,
                    a => self.convert_find(a).map(|i| i.id),
                };
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
    fn potion_choisie_a_l_ouverture() {
        let mut data = ConvertData::default();
        data.items.insert(486, ItemInfo { item_type: 1, kind_type: USABLE_REINFORCEMENT, ..Default::default() });
        data.items.insert(999, ItemInfo { item_type: 1, kind_type: USABLE_REINFORCEMENT, subtype: 1, ..Default::default() });
        let item = |id, tid, status, grid| ItemRecord { operation: 0, id, template_id: Some(tid), owner_cid: None, count: Some(1), status: Some(status), grid: Some(grid) };
        let mut items: HashMap<_, _> = [item(7, 486, 2, [0, 0, 0]), item(8, 486, 2, [4, 4, 4]), item(9, 999, 2, [4, 4, 4]), item(10, 486, 51, [0, 0, 0])].into_iter().map(|i| (i.id, i)).collect();
        let flags = HashMap::new();
        assert_eq!(physical_catalyst(&data, &items, &flags, None), Some(8));
        assert_eq!(physical_catalyst(&data, &items, &flags, Some(7)), Some(7), "conserver la potion posée");
        let flags = HashMap::from([(7, 8)]);
        assert_eq!(physical_catalyst(&data, &items, &flags, None), Some(7), "liée au propriétaire d'abord");
        items.remove(&7);
        assert_eq!(physical_catalyst(&data, &items, &flags, Some(7)), Some(8), "pile consommée : suivante");
        items.remove(&8);
        assert_eq!(physical_catalyst(&data, &items, &flags, None), None, "ni variante ajoutée ni potion Cash");
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : la Fighter Cuirass (172) est réservée aux
    /// orcs chevaliers de niveau 2, le Lunatic Rune Kilt (158) aux orcs mages de niveau 7 ; le
    /// chevalier humain (TID 96) ne remplit ni la race ni, pour le kilt, la classe.
    #[test]
    fn conditions_d_equipement() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let cuirass = restrict_of(&assets, 172).unwrap();
        assert_eq!((cuirass.item_type, cuirass.level, cuirass.race, cuirass.class), (0, 2, 2, 1));
        let kilt = restrict_of(&assets, 158).unwrap();
        assert_eq!((kilt.level, kilt.race, kilt.class), (7, 2, 8));
        let knight = Wearer::of(&assets, 96, 2);
        assert_eq!((knight.race, knight.class), (1, 1));
        assert!(!admits(cuirass.race, knight.race) && admits(cuirass.class, knight.class));
        assert!(!admits(kilt.class, knight.class));
        assert!(admits(0, 3), "masque nul : tout le monde");
    }

    /// Lignes réelles d'itemoptiontable.txt (colonnes utiles seulement) : critique lié au niveau (groupe 22),
    /// rune de critique, option « Critical » sans `skill_type`, conversion en PV, défense ignorée.
    const OPTIONS: &str = "tid\tDescription\tskill_type\tskill_rate\tCritical\tdamage_convert_hp\tvisible\tLevel_min\tLevel_max\tLink_id\tIgnore_Physical_Defense_Rate\tStunResistance\n\
        883\t15% chance on hit to strike your target for 250% damage.\t1\t15\t250\t\t1\t\t\t\t\t\n\
        2151\t1% chance on hit to strike your target for 110% damage.\t1\t1\t110\t\t1\t1\t10\t22\t\t\n\
        2152\t2% chance on hit to strike your target for 120% damage.\t1\t2\t120\t\t1\t11\t20\t22\t\t\n\
        2153\t3% chance on hit to strike your target for 130% damage.\t1\t3\t130\t\t1\t21\t30\t22\t\t\n\
        3191\tCritical Hit Rate +10%\t1\t10\t\t\t1\t\t\t\t\t\n\
        3242\t5% chance when hit to convert 10% of damage taken into restored health.\t3\t5\t\t10\t1\t\t\t\t\t\n\
        8061\tIgnores Defense with 1% chance during Attack\t\t\t\t\t1\t1\t10\t156\t1\t\n\
        13822\tCritical Hit Rate +1%\t\t\t1\t\t1\t\t\t\t\t\n";

    fn option_data() -> ConvertData {
        let mut d = ConvertData::default();
        load_option_table(&mut d, OPTIONS);
        let model = |options: Vec<i32>, links: Vec<i32>, rune: bool| ItemInfo { item_type: if rune { 1 } else { 0 }, kind_type: if rune { USABLE_RUNE } else { 0 }, options, links, ..Default::default() };
        // « Arm pad of Balakas » (Link_id 9;16;29;22;30), « Critical Hit Prayer Stone<Lv1> » (OptionTID 3191),
        // « Conversion Prayer Stone Lv 1 » (OptionTID 3242).
        d.items.insert(3919, model(vec![], vec![9, 16, 29, 22, 30], false));
        d.items.insert(1890, model(vec![3191], vec![], true));
        d.items.insert(609, model(vec![3242], vec![], true));
        d
    }

    #[test]
    fn somme_des_options_portees() {
        let d = option_data();
        // Règles du chargeur : `skill_rate` → taux critique si `skill_type` 1, « renvoi » si 3 ; `Critical`
        // toujours (13822 « Critical Hit Rate +1% » donne +1 de dégâts critiques, pas de taux).
        assert_eq!(d.option_values[&883].k[2..4], [15, 250]);
        assert_eq!(d.option_values[&13822].k[2..4], [0, 1]);
        assert_eq!(d.option_values[&3242].k[8..10], [5, 10]);
        assert_eq!((d.option_values[&8061].k[5], d.option_values[&8061].level), (1, (1, 10)));
        // Groupe 22 au niveau 15 : 2152 ; hors bornes : rien.
        assert_eq!(d.linked(22, 15), d.option_values.get(&2152));
        assert_eq!(d.linked(22, 31), None);
        // Exemple de la spec : niveau 15, brassard (2152 : taux 2, dégâts 120) + rune 1890 sertie (3191 : taux 10).
        let p = d.item_properties(3919, &[], &[1890], 15);
        assert_eq!(p, [0, 0, 12, 120, 0, 0, 0, 0, 0, 0]);
        // Options tirées (champ 20, 5 au plus) et rune de conversion sertie.
        let p = d.item_properties(3919, &[13822, 0, 0, 0, 0, 883], &[1890, 609], 5);
        assert_eq!(p, [0, 0, 1 + 10, 110 + 1, 0, 0, 0, 0, 5, 10]);
        // Liste interne des runes : TID d'option (3191, 3242), relus comme TID de modèle. Un modèle 3191 qui
        // porterait l'option 883 ajoute donc 15 / 250 (incohérence d'Archonia reproduite).
        assert_eq!(d.rune_option_list(&[1890, 609]), vec![3191, 3242]);
        let mut d = d;
        d.items.insert(3191, ItemInfo { options: vec![883], ..Default::default() });
        assert_eq!(d.item_properties(1, &[], &[1890], 15)[2..4], [10 + 15, 250]);
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : la table réelle donne l'exemple de la spec.
    #[test]
    fn somme_des_options_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let d = load_data(&assets);
        assert_eq!(d.option_values[&883].k[2..4], [15, 250]);
        assert_eq!(d.option_values[&13822].k[2..4], [0, 1]);
        assert_eq!(d.items[&3919].links, vec![9, 16, 29, 22, 30]);
        assert_eq!(d.linked(22, 15), d.option_values.get(&2152));
        assert_eq!(d.item_properties(3919, &[], &[1890], 15)[2..4], [12, 120]);
    }

    #[test]
    fn phrases_d_archonia() {
        assert_eq!(fill("Etat : %d au total / %d utilises", &["2".into(), "1".into()]), "Etat : 2 au total / 1 utilises");
        assert_eq!(fill("%s + %d%%", &["Pierre".into(), "7".into()]), "Pierre + 7%");
    }
}
