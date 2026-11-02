//! Inventaire Cash (statut 51, `UI_CashInven`) et sac du familier (statut 13, `UI_SubInventory`). Protocole :
//! `archrust_netclient::pet_bag` ; fenêtres : `hud_petbag.rs`.
//!
//! **Inventaire Cash** (AgcmUICashInven d'Archonia) : ouvert ou fermé par `CInven_UIOpen` (`FUN_140600c90`, bascule)
//! — bouton « Chantra Bag » d'`UI_Inventory` (`CONTROL_LoneyButton`), entrée `MenuBtnRoniInventory` du menu
//! `UI_MainMenuChantra`, croix. La touche `0:48` (B, « Chantra Bag ») de l'[EventMap] reste écartée par
//! `app_hotkey.rs`. Objets du statut 51 rangés par ordre d'arrivée (pas de case, `FUN_14030bd50`). Textes d'une ligne
//! (uimessage, `FUN_1406003f0`) :
//! * temps (`FUN_1406027f0`) : familier (`Cash` 6) sysstr 740 « Immortal » ; temps de jeu (`Cash` 2) utilisé ou en
//!   pause : `CashInven_ItemDetailInfoDay` (≥ 1 jour : jours, heures), `…Hour` (≥ 1 h : heures, minutes), `…Minute`,
//!   `…Second` (< 1 min) d'après le temps restant (champ 1 du mini Cash, en ms) ; temps réel (`Cash` 3) utilisé ou
//!   en pause : `CashInven_ItemDetailInfoRealTime` avec la date d'expiration (champ 2) « %y/%m/%d %H:%M » ;
//! * quantité `CashInven_ItemDetailInfo_Count` (`FUN_1406010a0`), moins un pour un parchemin (usage 12) entamé ;
//! * état (`FUN_140601650`) : 1 `CashInven_ItemUsing` ; 0 avec le champ 3 ≥ 1 et `CashItemUseType` 2 :
//!   `CashInven_ItemPause` ;
//! * bouton (`FUN_140601470`, absent si `CashItemUseType` = 0) : 0 `CashInven_ItemUse` (`CashInven_ItemNoUseYet`
//!   si le champ 3 vaut 0), actif ; 1 `CashInven_ItemNoUse` (« Stoppable ») actif si `CanStopUsingItem`, sinon
//!   « Use » inactif ; 2 `CashInven_ItemReUse`, actif si `CanStopUsingItem`. Clic (`FUN_1406009b0`) : AGPMITEM
//!   op 3, le serveur sort ou rappelle le familier et renvoie l'état par l'op 26.
//!
//! **Sac du familier** : disponible (`FUN_14030d1b0`) quand un objet de l'inventaire Cash est utilisable
//! (`ItemType` 1), « en cours d'utilisation » (1) et que sa créature (`Use skill id` → SKILLTEMPLATE.ini →
//! `summons_tid` de SKILL_CONST.txt) a `PetType` 1 dans CharacterDataTable.txt ; comme ArchCore, les familiers
//! 14821 à 15674 (ajouts d'Archonia) n'en donnent pas. Sac indisponible, un objet du statut 13 reçu est ignoré
//! (`FUN_140316a60`). Touche V (`0:47`, `SubInventory_UIUpdate`, `FUN_1407b8550`) : bascule, ouverture seulement
//! vivant et sac disponible. Familier rappelé (rafraîchissement `FUN_140601a80`) : la fenêtre se ferme, les objets
//! restent. Glisser depuis l'inventaire, l'équipement ou la banque, ou dans le sac : op 2 statut 13 (case, ou
//! `[sac, -1, -1]` sur un onglet) ; sortir : grille de l'inventaire ou de la banque (`app_items.rs`, `app_npc.rs`) ;
//! clic droit : utiliser ; corbeille ou lâcher hors des fenêtres : destruction après accord (op 21). « Auto Pick Up »
//! (op 30) : état par personnage dans `Ini/AutoPickUp.xml` (`<World Name><Character Name IsAuto/>`, 1 par défaut,
//! `FUN_1407b8fb0`). Écartés : Ctrl+clic des onglets (ops 37/38), « Sell All », séparation d'une pile du sac.

use crate::assets::Assets;
use crate::hud::{CashInvenView, CashLine, HudState, ItemDrag, ItemUi, PetBagUi, SubInvenView, CASH_ROWS};
use crate::net::NetLink;
use crate::scene::item_name_for_tid;
use archrust_netclient::items::ItemCommand;
use archrust_netclient::pet_bag::{in_use, status, CashState, PetBagCommand, PetBagEvent};
use archrust_netclient::world::ItemRecord;
use archrust_netclient::Command;
use std::collections::HashMap;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

/// Types Cash (colonne `Cash` d'itemdatatable.txt, `+0x254`).
const CASH_PLAY_TIME: i64 = 2;
const CASH_REAL_TIME: i64 = 3;
const CASH_PET: i64 = 6;
/// `PetType` d'une créature qui donne un sac.
const PET_TYPE_BAG: i64 = 1;
/// Familiers ajoutés par Archonia, sans sac (comme ArchCore).
const ARCHONIA_PETS: RangeInclusive<i32> = 14821..=15674;
/// Places de l'inventaire Cash affichées (« Chantra Item %d / 500 », `FUN_1406023b0`).
const CASH_MAX: usize = 500;
/// « Immortal » : temps d'un familier.
const SYSSTR_PET_TIME: u32 = 740;
/// Usage « parchemin » (`Type` 12).
const USE_SKILL_SCROLL: i64 = 12;

/// Colonnes utiles d'itemdatatable.txt et créature du familier.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CashRow {
    pub item_type: i64,
    pub use_type: i64,
    /// `Cash` (valeur − 100 si ≥ 100).
    pub cash: i64,
    pub can_stop: bool,
    pub use_kind: i64,
    pub skill_tid: i32,
    pub skill_level: i32,
    /// `PetType` de la créature invoquée par la compétence de l'objet.
    pub pet_type: Option<i64>,
}

/// Colonnes d'un fichier tabulé à en-tête.
fn columns(header: &str) -> Vec<String> {
    header.split('\t').map(|c| c.trim().to_ascii_lowercase()).collect()
}

fn num(cells: &[&str], col: Option<usize>) -> i64 {
    col.and_then(|i| cells.get(i)).and_then(|v| v.trim().split(';').next()?.trim().parse::<f64>().ok()).unwrap_or(0.0) as i64
}

/// Objets d'itemdatatable.txt.
pub fn parse_items(text: &str) -> HashMap<i32, CashRow> {
    let mut lines = text.lines();
    let cols = columns(lines.next().unwrap_or(""));
    let col = |n: &str| cols.iter().position(|c| c == n);
    let (tid, it, ty, cash, stop, kind, skill, level) = (col("tid"), col("itemtype"), col("type"), col("cash"), col("canstopusingitem"), col("cashitemusetype"), col("use skill id"), col("use skill level"));
    let mut out = HashMap::new();
    for line in lines {
        let c: Vec<&str> = line.split('\t').collect();
        let t = num(&c, tid) as i32;
        if t <= 0 {
            continue;
        }
        let raw = num(&c, cash);
        out.insert(
            t,
            CashRow {
                item_type: num(&c, it),
                use_type: num(&c, ty),
                cash: if raw >= 100 { raw - 100 } else { raw },
                can_stop: num(&c, stop) != 0,
                use_kind: num(&c, kind),
                skill_tid: num(&c, skill) as i32,
                skill_level: num(&c, level).max(1) as i32,
                pet_type: None,
            },
        );
    }
    out
}

/// `summons_tid` par (nom de compétence en minuscules, niveau) : SKILL_CONST.txt, nom sur la première ligne du groupe.
pub fn parse_summons(text: &str) -> HashMap<(String, i32), i32> {
    let mut lines = text.lines();
    let cols = columns(lines.next().unwrap_or(""));
    let (level, summons) = (cols.iter().position(|c| c == "skill_level"), cols.iter().position(|c| c == "summons_tid"));
    let mut out = HashMap::new();
    let mut current = String::new();
    for line in lines {
        let c: Vec<&str> = line.split('\t').collect();
        let name = c.first().map(|s| s.trim()).unwrap_or("");
        if !name.is_empty() {
            current = name.to_lowercase();
        }
        let tid = num(&c, summons) as i32;
        if !current.is_empty() && tid > 0 {
            out.insert((current.clone(), num(&c, level) as i32), tid);
        }
    }
    out
}

/// `PetType` par TID de créature (CharacterDataTable.txt).
pub fn parse_pet_types(text: &str) -> HashMap<i32, i64> {
    let mut lines = text.lines();
    let cols = columns(lines.next().unwrap_or(""));
    let (tid, pet) = (cols.iter().position(|c| c == "tid"), cols.iter().position(|c| c == "pettype"));
    lines
        .filter_map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            let p = num(&c, pet);
            (p != 0).then_some((num(&c, tid) as i32, p))
        })
        .collect()
}

fn load_rows(assets: &Assets) -> HashMap<i32, CashRow> {
    let text = |f: &str| super::npc_glue::client_text_in(assets, "ini", f);
    let mut rows = parse_items(&text("itemdatatable.txt"));
    let summons = parse_summons(&text("SKILL_CONST.txt"));
    let pets = parse_pet_types(&text("CharacterDataTable.txt"));
    let mut names: HashMap<i32, String> = HashMap::new();
    if let Ok(t) = assets.table("ini", "SKILLTEMPLATE.ini") {
        let name_col = t.section("").and_then(|h| h.entries.iter().find(|(_, v)| v.eq_ignore_ascii_case("Name")).map(|(k, _)| k.clone()));
        for s in &t.sections {
            if let (Ok(tid), Some(n)) = (s.name.parse::<i32>(), name_col.as_deref().and_then(|c| s.get(c))) {
                names.insert(tid, n.trim().to_lowercase());
            }
        }
    }
    for r in rows.values_mut().filter(|r| r.cash == CASH_PET && r.skill_tid > 0) {
        let creature = names.get(&r.skill_tid).and_then(|n| summons.get(&(n.clone(), r.skill_level)).or_else(|| summons.get(&(n.clone(), 1))));
        r.pet_type = creature.map(|c| pets.get(c).copied().unwrap_or(0));
    }
    log::info!("inventaire Cash : {} objet(s), {} familier(s) à sac", rows.len(), rows.values().filter(|r| r.pet_type == Some(PET_TYPE_BAG)).count());
    rows
}

/// Objet de l'inventaire Cash qui rend le sac du familier disponible (`FUN_14030d1b0`), s'il y en a un.
pub fn bag_item(items: &HashMap<i32, ItemRecord>, cash: &HashMap<i32, CashState>, rows: &HashMap<i32, CashRow>) -> Option<i32> {
    let mut found: Vec<i32> = items
        .values()
        .filter(|i| i.status == Some(status::CASH_INVENTORY))
        .filter(|i| cash.get(&i.id).and_then(|c| c.in_use) == Some(in_use::YES))
        .filter(|i| i.template_id.is_some_and(|t| !ARCHONIA_PETS.contains(&t) && rows.get(&t).is_some_and(|r| r.item_type == 1 && r.pet_type == Some(PET_TYPE_BAG))))
        .map(|i| i.id)
        .collect();
    found.sort();
    found.first().copied()
}

/// Clé du bouton d'usage et son état (`FUN_140601470`) ; `None` : pas de bouton.
pub fn use_button(row: &CashRow, st: &CashState) -> Option<(&'static str, bool)> {
    if row.use_kind == 0 {
        return None;
    }
    Some(match st.in_use.unwrap_or(in_use::NO) {
        // `+0x258` du modèle (non relevé) pris à 0 : « Stoppable » plutôt que « Can Toggle Off ».
        in_use::YES if row.can_stop => ("CashInven_ItemNoUse", true),
        in_use::YES => ("CashInven_ItemUse", false),
        in_use::PAUSED => ("CashInven_ItemReUse", row.can_stop),
        _ if st.field3.unwrap_or(0) == 0 => ("CashInven_ItemNoUseYet", true),
        _ => ("CashInven_ItemUse", true),
    })
}

/// Clé de l'état d'une ligne (`FUN_140601650`).
pub fn using_label(row: &CashRow, st: &CashState) -> Option<&'static str> {
    match st.in_use.unwrap_or(in_use::NO) {
        in_use::YES => Some("CashInven_ItemUsing"),
        in_use::NO if st.field3.unwrap_or(0) >= 1 && row.use_kind == 2 => Some("CashInven_ItemPause"),
        _ => None,
    }
}

/// Remplace les `%d` successifs, puis le premier `%s`.
fn printf(format: &str, ints: &[i64], s: Option<&str>) -> String {
    let mut out = String::new();
    let mut rest = format;
    let mut ints = ints.iter();
    let mut s = s;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 1..];
        match tail.chars().next() {
            Some('d') => out.push_str(&ints.next().map(|v| v.to_string()).unwrap_or_default()),
            Some('s') => out.push_str(s.take().unwrap_or_default()),
            Some('%') => out.push('%'),
            _ => {
                out.push('%');
                rest = tail;
                continue;
            }
        }
        rest = &tail[1..];
    }
    out.push_str(rest);
    out
}

/// Clé et valeurs du temps restant d'un objet à temps de jeu (ms).
pub fn remaining_parts(ms: i64) -> (&'static str, Vec<i64>) {
    if ms >= 86_400_000 {
        ("CashInven_ItemDetailInfoDay", vec![ms / 86_400_000, ms % 86_400_000 / 3_600_000])
    } else if ms >= 3_600_000 {
        ("CashInven_ItemDetailInfoHour", vec![ms / 3_600_000, ms % 3_600_000 / 60_000])
    } else if ms < 60_000 {
        ("CashInven_ItemDetailInfoSecond", vec![ms.max(0) / 1000])
    } else {
        ("CashInven_ItemDetailInfoMinute", vec![ms / 60_000])
    }
}

/// Date « %y/%m/%d %H:%M » d'un horodatage Unix (UTC : le fuseau du poste n'est pas lu).
pub fn short_date(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let (h, m) = (secs % 86_400 / 3600, secs % 3600 / 60);
    // Jours depuis 1970 → date civile (algorithme de H. Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(mo <= 2);
    format!("{:02}/{:02}/{:02} {:02}:{:02}", y % 100, mo, d, h, m)
}

/// Textes du client utilisés ici.
#[derive(Default)]
struct Texts {
    pet_time: String,
}

#[derive(Default)]
pub struct PetBagGlue {
    rows: Option<HashMap<i32, CashRow>>,
    texts: Option<Texts>,
    events: Option<HashMap<String, String>>,
    /// Mini Cash de chaque objet (IID).
    pub cash: HashMap<i32, CashState>,
    pub show_cash: bool,
    pub show_bag: bool,
    first: usize,
    layer: i16,
    /// Ramassage automatique (lu dans `AutoPickUp.xml` au premier besoin).
    auto_pick: Option<bool>,
    /// Objet dont la destruction attend l'accord du joueur.
    destroy: Option<i32>,
}

impl PetBagGlue {
    fn rows(&mut self, assets: &Assets) -> &HashMap<i32, CashRow> {
        self.rows.get_or_insert_with(|| load_rows(assets))
    }

    /// Objet qui rend le sac disponible.
    pub fn bag_item(&mut self, assets: &Assets, items: &HashMap<i32, ItemRecord>) -> Option<i32> {
        self.rows(assets);
        bag_item(items, &self.cash, self.rows.as_ref().unwrap())
    }

    /// Bascule de l'inventaire Cash (`CInven_UIOpen`).
    pub fn toggle_cash(&mut self) {
        self.show_cash = !self.show_cash;
    }
}

/// Personnage du joueur, relevé par le moteur.
pub struct Me<'a> {
    pub world: Option<&'a str>,
    pub name: &'a str,
}

/// Fichier `Ini/AutoPickUp.xml` du jeu.
fn auto_pick_path(game_dir: &Path) -> PathBuf {
    let dir = std::fs::read_dir(game_dir)
        .ok()
        .and_then(|rd| rd.flatten().map(|e| e.path()).find(|p| p.is_dir() && p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("ini"))))
        .unwrap_or_else(|| game_dir.join("Ini"));
    std::fs::read_dir(&dir)
        .ok()
        .and_then(|rd| rd.flatten().map(|e| e.path()).find(|p| p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("AutoPickUp.xml"))))
        .unwrap_or_else(|| dir.join("AutoPickUp.xml"))
}

fn attr<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let start = line.find(&key)? + key.len();
    let len = line[start..].find('"')?;
    Some(&line[start..start + len])
}

/// État d'`AutoPickUp.xml` pour ce monde et ce personnage ; `None` : absent (1 par défaut).
pub fn auto_pick_read(text: &str, world: &str, name: &str) -> Option<bool> {
    let mut in_world = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with("<World") {
            in_world = attr(line, "Name") == Some(world);
        } else if line.starts_with("</World") {
            in_world = false;
        } else if in_world && line.starts_with("<Character") && attr(line, "Name") == Some(name) {
            return Some(attr(line, "IsAuto").map(|v| v.trim() != "0").unwrap_or(true));
        }
    }
    None
}

/// `AutoPickUp.xml` réécrit avec l'état de ce personnage (mise en page d'Archonia, lignes CRLF).
pub fn auto_pick_write(text: &str, world: &str, name: &str, on: bool) -> String {
    // (monde, personnages) dans l'ordre du fichier.
    let mut worlds: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with("<World") {
            worlds.push((attr(line, "Name").unwrap_or("").to_string(), Vec::new()));
        } else if line.starts_with("<Character") {
            if let (Some(w), Some(n)) = (worlds.last_mut(), attr(line, "Name")) {
                w.1.push((n.to_string(), attr(line, "IsAuto").unwrap_or("1").to_string()));
            }
        }
    }
    let flag = if on { "1" } else { "0" }.to_string();
    match worlds.iter_mut().find(|(w, _)| w == world) {
        Some((_, chars)) => match chars.iter_mut().find(|(n, _)| n == name) {
            Some(c) => c.1 = flag,
            None => chars.push((name.to_string(), flag)),
        },
        None => worlds.push((world.to_string(), vec![(name.to_string(), flag)])),
    }
    let mut out = String::from("<AutoPickUp>\r\n");
    for (w, chars) in &worlds {
        if chars.is_empty() {
            out.push_str(&format!("    <World Name=\"{w}\" />\r\n"));
            continue;
        }
        out.push_str(&format!("    <World Name=\"{w}\">\r\n"));
        for (n, v) in chars {
            out.push_str(&format!("        <Character Name=\"{n}\" IsAuto=\"{v}\" />\r\n"));
        }
        out.push_str("    </World>\r\n");
    }
    out.push_str("</AutoPickUp>\r\n");
    out
}

fn send(net: Option<&NetLink>, cmd: Command) {
    if let Some(net) = net {
        net.send(cmd);
    }
}

fn label(assets: &Assets, item: &ItemRecord) -> String {
    item.template_id.and_then(|t| item_name_for_tid(assets, t)).unwrap_or_default()
}

/// Événement réseau de ce module.
pub fn on_event(glue: &mut PetBagGlue, ev: PetBagEvent) {
    match ev {
        PetBagEvent::Cash { iid, state, refresh } => {
            glue.cash.entry(iid).or_default().merge(&state);
            if refresh {
                log::debug!("inventaire Cash : objet {iid} « en cours d'utilisation » = {:?}", state.in_use);
            }
        }
        PetBagEvent::AutoPick { on, .. } => glue.auto_pick = Some(on),
    }
}

/// Fenêtres, demandes de la souris et accord de destruction, avant `app_items.rs` (le bouton « Chantra Bag » de
/// l'inventaire est consommé ici ; un objet Cash ne se pose ni dans l'inventaire ni sur l'équipement).
#[allow(clippy::too_many_arguments)]
pub fn panel(ctx: &egui::Context, glue: &mut PetBagGlue, item_requests: &mut Vec<ItemUi>, items: &HashMap<i32, ItemRecord>, me: &Me, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>) {
    item_requests.retain(|r| match *r {
        ItemUi::CashInventory => {
            glue.toggle_cash();
            false
        }
        ItemUi::DropOnBag { drag, .. } | ItemUi::DropOnEquip(drag) => items.get(&drag.iid).is_none_or(|i| i.status != Some(status::CASH_INVENTORY)),
        _ => true,
    });
    let pet = glue.bag_item(assets, items);
    // Familier rappelé : sac fermé, objets gardés (`FUN_140601a80`).
    if pet.is_none() {
        glue.show_bag = false;
    }
    let mut out = Vec::new();
    if glue.show_cash {
        let texts = glue.texts.get_or_insert_with(|| Texts { pet_time: super::npc_glue::sysstr(assets, SYSSTR_PET_TIME).unwrap_or_default() });
        let pet_time = texts.pet_time.clone();
        let rows = glue.rows.clone().unwrap_or_default();
        let mut list: Vec<&ItemRecord> = items.values().filter(|i| i.status == Some(status::CASH_INVENTORY)).collect();
        list.sort_by_key(|i| i.id);
        let lines: Vec<CashLine> = list
            .into_iter()
            .map(|i| {
                let row = i.template_id.and_then(|t| rows.get(&t)).copied().unwrap_or_default();
                let st = glue.cash.get(&i.id).copied().unwrap_or_default();
                let msg = |hud: &mut HudState, k: &str| crate::hud::ui_message(hud, assets, k);
                let used = matches!(st.in_use, Some(in_use::YES | in_use::PAUSED));
                let info = if row.cash == CASH_PET {
                    pet_time.clone()
                } else if row.cash == CASH_PLAY_TIME && used {
                    let (key, v) = remaining_parts(st.remaining_ms.unwrap_or(0));
                    printf(&msg(hud, key), &v, None)
                } else if row.cash == CASH_REAL_TIME && used && st.expire.is_some_and(|e| e != 0 && e != u64::MAX) {
                    let date = st.expire.map(short_date).unwrap_or_default();
                    printf(&msg(hud, "CashInven_ItemDetailInfoRealTime"), &[], Some(&date))
                } else {
                    String::new()
                };
                let scroll = used && row.item_type == 1 && row.use_type == USE_SKILL_SCROLL && (st.remaining_ms.unwrap_or(0) > 0 || st.expire.unwrap_or(0) != 0);
                let count = i64::from(i.count.unwrap_or(1)) - i64::from(scroll);
                CashLine {
                    item: i.clone(),
                    name: label(assets, i),
                    count: printf(&msg(hud, "CashInven_ItemDetailInfo_Count"), &[count], None),
                    info,
                    using: using_label(&row, &st).map(|k| msg(hud, k)).unwrap_or_default(),
                    button: use_button(&row, &st).map(|(k, on)| (msg(hud, k), on)),
                }
            })
            .collect();
        glue.first = glue.first.min(lines.len().saturating_sub(CASH_ROWS));
        let count_text = printf(&crate::hud::ui_message(hud, assets, "TextFormat_CashItemCount"), &[lines.len() as i64, CASH_MAX as i64], None);
        crate::hud::draw_cash_inventory(ctx, hud, assets, &CashInvenView { lines: &lines, first: glue.first, count_text }, &mut out);
    }
    if glue.show_bag {
        if glue.auto_pick.is_none() {
            let text = assets.game_dir.as_deref().map(auto_pick_path).and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
            glue.auto_pick = Some(me.world.and_then(|w| auto_pick_read(&text, w, me.name)).unwrap_or(true));
        }
        let mut bag: Vec<(ItemRecord, String)> = items.values().filter(|i| i.status == Some(status::SUB_INVENTORY)).map(|i| (i.clone(), label(assets, i))).collect();
        bag.sort_by_key(|(i, _)| i.id);
        let key = if glue.auto_pick != Some(false) { "AutoPickGroundItemON" } else { "AutoPickGroundItemOFF" };
        let auto_label = crate::hud::ui_message(hud, assets, key);
        crate::hud::draw_sub_inventory(ctx, hud, assets, &SubInvenView { items: &bag, layer: glue.layer, auto_label }, &mut out);
    }
    for r in out {
        match r {
            PetBagUi::CloseCash => glue.show_cash = false,
            PetBagUi::Scroll(d) => glue.first = (glue.first as i64 + i64::from(d)).max(0) as usize,
            PetBagUi::UseCash(iid) => send(net, Command::Item(ItemCommand::Use(iid))),
            // Clic droit : utiliser ou porter (`app_items.rs`).
            PetBagUi::Activate(iid) => item_requests.push(ItemUi::Activate { iid, worn: false }),
            PetBagUi::Waste(drag) => {
                if items.get(&drag.iid).is_some_and(|i| matches!(i.status, Some(2 | 13 | 15 | 51))) {
                    glue.destroy = Some(drag.iid);
                }
            }
            PetBagUi::CloseBag => glue.show_bag = false,
            PetBagUi::Layer(l) => glue.layer = l.clamp(0, 3),
            PetBagUi::DropOnBag { drag, grid } => {
                if let Some(cmd) = store_request(items, pet, &drag, Some(grid)) {
                    send(net, Command::PetBag(cmd));
                }
            }
            PetBagUi::DropOnTab { drag, bag } => {
                if let Some(cmd) = store_request(items, pet, &drag, Some([bag, -1, -1])) {
                    send(net, Command::PetBag(cmd));
                }
            }
            PetBagUi::AutoPick => {
                let on = glue.auto_pick == Some(false);
                if let Some(iid) = pet {
                    send(net, Command::PetBag(PetBagCommand::AutoPick { iid, on }));
                }
                glue.auto_pick = Some(on);
                if let (Some(path), Some(world)) = (assets.game_dir.as_deref().map(auto_pick_path), me.world) {
                    let text = std::fs::read_to_string(&path).unwrap_or_default();
                    if let Err(e) = std::fs::write(&path, auto_pick_write(&text, world, me.name, on)) {
                        log::warn!("{} : {e}", path.display());
                    }
                }
            }
        }
    }
    if let Some(iid) = glue.destroy {
        let item = items.get(&iid);
        let text = match item {
            Some(i) if i.status == Some(status::CASH_INVENTORY) => crate::hud::ui_message(hud, assets, "DestoryCashItem"),
            Some(i) => format!("{}___[{}]", super::portal_glue::ui_event_cached(&mut glue.events, assets, "Item_ConfirmDestroyItem"), label(assets, i)),
            None => String::new(),
        };
        match (item, crate::hud::confirm_box(ctx, hud, assets, ("petbag-destroy", iid), &text)) {
            (Some(_), Some(true)) => {
                send(net, Command::Item(ItemCommand::Destroy(iid)));
                glue.destroy = None;
            }
            (None, _) | (_, Some(false)) => glue.destroy = None,
            _ => {}
        }
    }
}

/// Demande de rangement dans le sac (`SubInventory_MoveItem`, `SubInventory_DragDropItem`) : sac disponible, objet
/// connu hors de l'inventaire Cash, pas déjà à cette case.
pub fn store_request(items: &HashMap<i32, ItemRecord>, pet: Option<i32>, drag: &ItemDrag, grid: Option<[i16; 3]>) -> Option<PetBagCommand> {
    pet?;
    let item = items.get(&drag.iid)?;
    if item.status == Some(status::CASH_INVENTORY) || (item.status == Some(status::SUB_INVENTORY) && item.grid == grid) {
        return None;
    }
    Some(PetBagCommand::Store { iid: drag.iid, grid })
}

impl super::App {
    /// Touche V (`SubInventory_UIUpdate`) : bascule du sac, ouvert seulement vivant et sac disponible.
    pub(super) fn petbag_toggle_bag(&mut self) {
        if self.petbag.show_bag {
            self.petbag.show_bag = false;
            return;
        }
        let alive = !self.combat.player_dead && self.player.is_some();
        if alive && self.petbag.bag_item(&self.assets, &self.items).is_some() {
            self.petbag.show_bag = true;
        }
    }

    /// Objet du sac reçu sac indisponible : ignoré, comme Archonia (`FUN_140316a60`).
    pub(super) fn petbag_ignores(&mut self, item: &ItemRecord) -> bool {
        item.operation != 1 && item.status == Some(status::SUB_INVENTORY) && self.petbag.bag_item(&self.assets, &self.items).is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: i32, tid: i32, st: i8) -> ItemRecord {
        ItemRecord { operation: 0, id, template_id: Some(tid), owner_cid: Some(7), count: Some(1), status: Some(st), grid: None }
    }

    #[test]
    fn tables_et_regle_du_sac() {
        let items = parse_items("TID\tName\tItemType\tType\tUse skill id\tUse skill level\tCash\tCanStopUsingItem\tCashItemUseType\n7564\tDoogie\t1\t12\t970\t1\t6\t1\t2\n1830\tTullan\t1\t0\t\t\t101\t\t\n");
        assert_eq!(items[&7564], CashRow { item_type: 1, use_type: 12, cash: 6, can_stop: true, use_kind: 2, skill_tid: 970, skill_level: 1, pet_type: None });
        assert_eq!(items[&1830].cash, 1, "valeur − 100");
        let summons = parse_summons("name\tskill_level\tsummons_tid\nPet Doogie\t1\t1362\n\t2\t1363\n");
        assert_eq!((summons.get(&("pet doogie".into(), 1)), summons.get(&("pet doogie".into(), 2))), (Some(&1362), Some(&1363)));
        assert_eq!(parse_pet_types("TID\tName\tPetType\n1362\tDoogie\t1\n195\tTest\t\n").get(&1362), Some(&1));

        let mut rows = items;
        rows.get_mut(&7564).unwrap().pet_type = Some(1);
        rows.insert(14821, CashRow { pet_type: Some(1), ..rows[&7564] });
        let all: HashMap<i32, ItemRecord> = [rec(5, 7564, 51), rec(6, 14821, 51), rec(9, 11, 2)].into_iter().map(|i| (i.id, i)).collect();
        let mut cash = HashMap::new();
        assert_eq!(bag_item(&all, &cash, &rows), None, "familier pas sorti");
        cash.insert(6, CashState { in_use: Some(1), ..Default::default() });
        assert_eq!(bag_item(&all, &cash, &rows), None, "Yabbit : ajout d'Archonia");
        cash.insert(5, CashState { in_use: Some(1), ..Default::default() });
        assert_eq!(bag_item(&all, &cash, &rows), Some(5));
        // Rangement : sac disponible, pas un objet Cash.
        let drag = |iid| ItemDrag { iid, tid: 0, worn: false, bank: false };
        assert_eq!(store_request(&all, Some(5), &drag(9), Some([1, -1, -1])), Some(PetBagCommand::Store { iid: 9, grid: Some([1, -1, -1]) }));
        assert_eq!(store_request(&all, None, &drag(9), Some([0, 0, 0])), None);
        assert_eq!(store_request(&all, Some(5), &drag(5), Some([0, 0, 0])), None);
    }

    #[test]
    fn textes_d_une_ligne() {
        let pet = CashRow { item_type: 1, cash: 6, can_stop: true, use_kind: 2, ..Default::default() };
        let st = |u: i8, f3: i32| CashState { in_use: Some(u), field3: Some(f3), ..Default::default() };
        assert_eq!(use_button(&pet, &st(0, 0)), Some(("CashInven_ItemNoUseYet", true)));
        assert_eq!(use_button(&pet, &st(0, 1)), Some(("CashInven_ItemUse", true)));
        assert_eq!(use_button(&pet, &st(1, 0)), Some(("CashInven_ItemNoUse", true)));
        assert_eq!(use_button(&CashRow { can_stop: false, ..pet }, &st(1, 0)), Some(("CashInven_ItemUse", false)));
        assert_eq!(use_button(&pet, &st(2, 0)), Some(("CashInven_ItemReUse", true)));
        assert_eq!(use_button(&CashRow { use_kind: 0, ..pet }, &st(0, 0)), None);
        assert_eq!(using_label(&pet, &st(1, 0)), Some("CashInven_ItemUsing"));
        assert_eq!(using_label(&pet, &st(0, 1)), Some("CashInven_ItemPause"));
        assert_eq!(using_label(&pet, &st(0, 0)), None);
        assert_eq!(remaining_parts(90_000_000), ("CashInven_ItemDetailInfoDay", vec![1, 1]));
        assert_eq!(remaining_parts(3_660_000), ("CashInven_ItemDetailInfoHour", vec![1, 1]));
        assert_eq!(remaining_parts(120_000), ("CashInven_ItemDetailInfoMinute", vec![2]));
        assert_eq!(remaining_parts(59_000), ("CashInven_ItemDetailInfoSecond", vec![59]));
        assert_eq!(printf("Expiry: %d Day %d Hr", &[1, 1], None), "Expiry: 1 Day 1 Hr");
        assert_eq!(printf("Time: %s", &[], Some("26/11/01 12:30")), "Time: 26/11/01 12:30");
        assert_eq!(printf("Chantra Item %d / %d", &[3, 500], None), "Chantra Item 3 / 500");
        assert_eq!(short_date(1_793_536_200), "26/11/01 12:30");
    }

    /// Données du jeu : Doogie <Rank 1> (7564) donne un sac, Test Ramda (7497) non.
    #[test]
    fn familiers_du_jeu() {
        let Ok(dir) = std::env::var("ARCHRUST_GAME_DIR") else { return };
        let rows = load_rows(&Assets::new(Some(PathBuf::from(dir)), &[]));
        assert_eq!(rows.get(&7564).map(|r| (r.cash, r.can_stop, r.use_kind, r.pet_type)), Some((6, true, 2, Some(1))));
        assert_ne!(rows.get(&7497).and_then(|r| r.pet_type), Some(1));
        assert!(rows.values().filter(|r| r.pet_type == Some(PET_TYPE_BAG)).count() >= 160);
    }

    #[test]
    fn fichier_auto_pick_up() {
        let text = "<AutoPickUp>\r\n    <World Name=\"ArchLord\">\r\n        <Character Name=\"Furkan\" IsAuto=\"1\" />\r\n    </World>\r\n    <World Name=\"Event\" />\r\n</AutoPickUp>\r\n";
        assert_eq!(auto_pick_read(text, "ArchLord", "Furkan"), Some(true));
        assert_eq!(auto_pick_read(text, "Event", "Furkan"), None);
        let out = auto_pick_write(text, "ArchLord", "Furkan", false);
        assert_eq!(auto_pick_read(&out, "ArchLord", "Furkan"), Some(false));
        let out = auto_pick_write(&out, "Event", "Gm", true);
        assert_eq!(out, "<AutoPickUp>\r\n    <World Name=\"ArchLord\">\r\n        <Character Name=\"Furkan\" IsAuto=\"0\" />\r\n    </World>\r\n    <World Name=\"Event\">\r\n        <Character Name=\"Gm\" IsAuto=\"1\" />\r\n    </World>\r\n</AutoPickUp>\r\n");
        assert_eq!(auto_pick_write(text, "ArchLord", "Furkan", true), text, "réécriture identique");
    }
}
