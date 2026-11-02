//! Usure des objets et réparation chez les forgerons côté moteur. Protocole :
//! `archrust_netclient::repair` ; fenêtres d'Archonia : `hud_repair.rs`.
//!
//! Comportement relevé dans Archonia (sha256 e9c7609b…) :
//! * durabilité : actuelle = facteur objet, sous-valeur 0 (AGPMITEM champ 10, mise à jour par l'op 2
//!   sans toucher au reste) ; maximum = colonne `DUR` d'itemdatatable.txt (sous-valeur 6, `@` en tête :
//!   objet inusable, 50000) ; sans facteur reçu, l'objet garde la copie du modèle (plein) ;
//!   pourcentage `FUN_1401be960` (voir `repair::durability_percent`) ;
//! * `UI_EquipStatus` (`FUN_14053f970`, `FUN_14053ff90`) : corps 2, tête 3, mains 5, jambes 6, pieds 7,
//!   bouclier 9, arme (main droite 10, sinon arme de la main gauche) allumés sous 21 % ; fenêtre
//!   affichée si une partie l'est ; case d'équipement marquée à 0 %, case du sac à 20 % et moins
//!   (`FUN_1407579b0`) ;
//! * infobulle : « Repair Cost : %I64d Gold » (`sysstr` 416, chaîne `"%s : %I64d %s"`) pour les armes,
//!   armures, boucliers, montures et armes montées (`itemtooltip.txt`), sauf objet inusable ; prix
//!   `FUN_1402b43a0` avec la taxe de la région du joueur (voir `app_tax.rs`) ;
//! * PNJ de fonction 17 : bouton « Repair » (`sysstr` 325, FUN_1406ca6a0) → op 3 ; l'accord (op 4,
//!   `FUN_1406ac0a0`) ouvre `UI_Repair` et l'inventaire (`ItemRepair_UIOpen_Event`, grille et coût à
//!   zéro, `FUN_1406519b0`) et retient la position ; la fenêtre se ferme si le joueur s'en éloigne de
//!   plus de 99 unités (`FUN_140651e50`) ;
//! * dépôt (`FUN_140652660`) : objet du sac (statut 2, 13 ou 15), grille pleine → `sysstr` 321,
//!   maximum ≤ 0 → `ItemRepair_DontRepair_Msg`, déjà plein → `ItemRepair_AlreadyMax_Msg`, un objet déjà
//!   posé se déplace sans être recompté ; retrait : son coût est retiré (`FUN_140651b10`) ;
//! * « Repair » (`FUN_140651670`) : grille vide → `sysstr` 322 ; sinon op 0 avec le coût total et
//!   verrou jusqu'au résultat ; résultat (`FUN_140651540`) : 1 `sysstr` 317 et fermeture de la fenêtre
//!   et de l'inventaire, 2 `ItemRepair_InsufficientMoney_Msg`, 3 `ItemRepair_AlreadyMax_Msg`,
//!   4 `sysstr` 318, 5 `sysstr` 319.

use super::App;
use crate::assets::Assets;
use crate::hud::{HudState, RepairUi, RepairView};
use crate::net::NetLink;
use crate::ui::UiState;
use archrust_netclient::repair::{durability_percent, repair_cost, result, RepairCommand, RepairEvent, GRID_CELLS};
use archrust_netclient::world::ItemRecord;
use archrust_netclient::Command;
use egui::Color32;
use glam::Vec3;
use std::collections::HashMap;

/// Déplacement au-delà duquel la fenêtre se ferme (FUN_140651e50 : `99 < distance`).
const CLOSE_MOVE: f32 = 99.0;
/// Statuts acceptés dans la grille (sac, et deux statuts de sac secondaires d'Archonia).
const BAG_STATUSES: [i8; 3] = [2, 13, 15];
const STATUS_EQUIP: i8 = 3;
/// Genres (`Kind`) qui affichent le coût de réparation : armure, arme, bouclier, monture.
const REPAIR_COST_KINDS: [i64; 4] = [1, 2, 4, 32];
/// Durabilité des objets inusables (`@` dans la colonne `DUR`).
const UNBREAKABLE: i32 = 50_000;

/// Colonnes utiles d'itemdatatable.txt.
#[derive(Debug, Clone, Copy, Default)]
struct ItemDur {
    /// Maximum (`DUR`), 0 sans durabilité.
    max: i32,
    /// `NPC Price`.
    price: i64,
    unbreakable: bool,
    kind: i64,
    part: i64,
}

#[derive(Default)]
struct RepairData {
    items: HashMap<i32, ItemDur>,
    sysstr: HashMap<u32, String>,
    events: HashMap<String, String>,
}

impl RepairData {
    fn sys(&self, id: u32) -> String {
        self.sysstr.get(&id).cloned().unwrap_or_default()
    }

    fn event(&self, name: &str) -> String {
        self.events.get(name).cloned().unwrap_or_default()
    }

    fn item(&self, tid: Option<i32>) -> ItemDur {
        tid.and_then(|t| self.items.get(&t).copied()).unwrap_or_default()
    }
}

fn load_data(assets: &Assets) -> RepairData {
    let mut d = RepairData::default();
    let text = super::npc_glue::client_text_in(assets, "ini", "itemdatatable.txt");
    let mut lines = text.lines();
    if let Some(header) = lines.next() {
        let cols: Vec<&str> = header.split('\t').map(str::trim).collect();
        let col = |name: &str| cols.iter().position(|c| c.eq_ignore_ascii_case(name));
        let (tid, dur, price, kind, part) = (col("TID"), col("DUR"), col("NPC Price"), col("Kind"), col("Part"));
        for line in lines {
            let cells: Vec<&str> = line.split('\t').collect();
            let get = |c: Option<usize>| c.and_then(|i| cells.get(i)).map(|v| v.trim()).unwrap_or("");
            let num = |c: Option<usize>| get(c).parse::<f64>().map(|v| v as i64).unwrap_or(0);
            let Ok(t) = get(tid).parse::<i32>() else { continue };
            let raw = get(dur);
            let unbreakable = raw.starts_with('@');
            let max = if unbreakable { UNBREAKABLE } else { raw.parse::<f64>().map(|v| v as i32).unwrap_or(0) };
            d.items.insert(t, ItemDur { max, price: num(price), unbreakable, kind: num(kind), part: num(part) });
        }
    }
    for id in [10, 317, 318, 319, 321, 322, 416] {
        d.sysstr.insert(id, super::npc_glue::sysstr(assets, id).unwrap_or_default());
    }
    d.events = super::portal_glue::ui_events(assets);
    log::info!("réparation : {} objet(s) avec durabilité", d.items.values().filter(|i| i.max > 0).count());
    d
}

/// Fenêtre ouverte : PNJ et position du joueur à l'accord.
struct Open {
    npc: i32,
    at: Vec3,
}

#[derive(Default)]
pub struct RepairGlue {
    data: Option<RepairData>,
    /// Durabilité actuelle reçue par IID (absente : pleine, copie du modèle).
    pub durability: HashMap<i32, i32>,
    open: Option<Open>,
    /// IID posé dans chaque case.
    grid: [Option<i32>; GRID_CELLS],
    /// Demande envoyée, résultat attendu (+0x173c).
    busy: bool,
    /// Boîte d'information (`ItemRepair_*_Msg`).
    info: Option<String>,
}

impl RepairGlue {
    fn data(&mut self, assets: &Assets) -> &RepairData {
        self.data.get_or_insert_with(|| load_data(assets))
    }

    /// Durabilité actuelle et maximum d'un objet.
    fn current_max(&mut self, assets: &Assets, item: &ItemRecord) -> (i32, i32) {
        let max = self.data(assets).item(item.template_id).max;
        (self.durability.get(&item.id).copied().unwrap_or(max), max)
    }

    /// Coût de réparation d'un objet (FUN_1402b43a0).
    fn cost_of(&mut self, assets: &Assets, item: &ItemRecord) -> i64 {
        let (cur, max) = self.current_max(assets, item);
        let price = self.data(assets).item(item.template_id).price;
        repair_cost(cur, max, price, super::tax_glue::rate() as i64)
    }

    fn total_cost(&mut self, assets: &Assets, items: &HashMap<i32, ItemRecord>) -> i64 {
        let posed: Vec<ItemRecord> = self.grid.iter().flatten().filter_map(|i| items.get(i)).cloned().collect();
        posed.iter().map(|i| self.cost_of(assets, i)).sum()
    }

    /// Fenêtre remise à zéro (`ItemRepair_UIOpen`, FUN_1406519b0).
    fn reset(&mut self) {
        self.grid = [None; GRID_CELLS];
        self.busy = false;
    }

    fn close(&mut self) {
        self.open = None;
        self.reset();
    }

    /// Objet posé dans la case `cell` (FUN_140652660) ; message d'Archonia en cas de refus.
    fn drop_item(&mut self, assets: &Assets, item: &ItemRecord, cell: usize, hud: &mut HudState, _ui: &mut UiState) {
        if !item.status.is_some_and(|s| BAG_STATUSES.contains(&s)) {
            return;
        }
        // Déjà posé : déplacé sans être recompté.
        if let Some(from) = self.grid.iter().position(|c| *c == Some(item.id)) {
            if self.grid[cell].is_none() {
                self.grid[from] = None;
                self.grid[cell] = Some(item.id);
            }
            return;
        }
        let (cur, max) = self.current_max(assets, item);
        if max <= 0 {
            self.info = Some(self.data(assets).event("ItemRepair_DontRepair_Msg"));
            return;
        }
        if cur == max {
            self.info = Some(self.data(assets).event("ItemRepair_AlreadyMax_Msg"));
            return;
        }
        let target = if self.grid[cell].is_none() { Some(cell) } else { self.grid.iter().position(Option::is_none) };
        match target {
            Some(c) => self.grid[c] = Some(item.id),
            None => {
                let text = self.data(assets).sys(321);
                hud.push_system(&text);
            }
        }
    }

    /// Bouton « Repair » (FUN_140651670).
    fn repair_button(&mut self, assets: &Assets, items: &HashMap<i32, ItemRecord>, net: Option<&NetLink>, hud: &mut HudState, _ui: &mut UiState) {
        let Some(npc) = self.open.as_ref().map(|o| o.npc) else { return };
        if self.busy {
            return;
        }
        if self.grid.iter().all(Option::is_none) {
            let text = self.data(assets).sys(322);
            hud.push_system(&text);
            return;
        }
        let cost = self.total_cost(assets, items);
        let grid = self.grid.map(|g| g.unwrap_or(0));
        self.busy = true;
        log::info!("réparation : {} objet(s) pour {cost} or auprès du PNJ {npc}", grid.iter().filter(|i| **i != 0).count());
        send(net, RepairCommand::Repair { npc, grid, cost });
    }

    /// Ligne « Repair Cost » de l'infobulle d'un objet, s'il en a une.
    fn tooltip_line(&mut self, assets: &Assets, item: &ItemRecord) -> Option<String> {
        let info = self.data(assets).item(item.template_id);
        if info.unbreakable || info.max <= 0 || !REPAIR_COST_KINDS.contains(&info.kind) {
            return None;
        }
        let cost = self.cost_of(assets, item);
        let d = self.data(assets);
        Some(format!("{} : {cost} {}", d.sys(416), d.sys(10)))
    }
}

fn send(net: Option<&NetLink>, cmd: RepairCommand) {
    if let Some(net) = net {
        net.send(Command::Repair(cmd));
    }
}

/// Contrôles de `UI_EquipStatus` allumés (durabilité < 21 % sur une partie portée).
fn lit_parts(glue: &mut RepairGlue, assets: &Assets, items: &HashMap<i32, ItemRecord>) -> Vec<&'static str> {
    let worn: Vec<ItemRecord> = items.values().filter(|i| i.status == Some(STATUS_EQUIP)).cloned().collect();
    let mut lit = Vec::new();
    let mut low = |part_ok: &dyn Fn(&ItemDur) -> bool, name: &'static str, glue: &mut RepairGlue| {
        let hit = worn.iter().find(|i| part_ok(&glue.data(assets).item(i.template_id))).is_some_and(|i| {
            let (cur, max) = glue.current_max(assets, i);
            (0..21).contains(&durability_percent(cur, max))
        });
        if hit {
            lit.push(name);
        }
    };
    low(&|d| d.part == 2, "CONTROL_BODY", glue);
    low(&|d| d.part == 3, "CONTROL_HEAD", glue);
    low(&|d| d.part == 5, "CONTROL_HAND", glue);
    low(&|d| d.part == 6, "CONTROL_LEGS", glue);
    low(&|d| d.part == 7, "CONTROL_FOOT", glue);
    low(&|d| d.part == 9 && d.kind != 2, "CONTROL_SHIELD", glue);
    // Arme en main : la main droite, sinon une arme de la main gauche (arc).
    let right = worn.iter().any(|i| glue.data(assets).item(i.template_id).part == 10);
    if right {
        low(&|d| d.part == 10, "CONTROL_WEAPON", glue);
    } else {
        low(&|d| d.part == 9 && d.kind == 2, "CONTROL_WEAPON", glue);
    }
    lit
}

/// Fenêtres de la réparation et silhouette d'usure. À appeler après l'inventaire, avant
/// `app_items::panel`.
#[allow(clippy::too_many_arguments)]
pub fn panel(
    ctx: &egui::Context,
    glue: &mut RepairGlue,
    items: &HashMap<i32, ItemRecord>,
    hud: &mut HudState,
    assets: &Assets,
    net: Option<&NetLink>,
    ui: &mut UiState,
) {
    let lit = lit_parts(glue, assets, items);
    crate::hud::draw_equip_status(ctx, hud, assets, &lit);

    if glue.open.is_some() {
        // Objets disparus ou sortis du sac : leur case se vide (et leur coût avec).
        for cell in glue.grid.iter_mut() {
            if cell.is_some_and(|i| !items.get(&i).is_some_and(|it| it.status.is_some_and(|s| BAG_STATUSES.contains(&s)))) {
                *cell = None;
            }
        }
        let grid = glue.grid.map(|g| g.and_then(|i| items.get(&i)).map(|i| (i.template_id.unwrap_or(0), i.count.unwrap_or(1))));
        let cost = glue.total_cost(assets, items);
        let view = RepairView { grid, cost, money: hud.stats.money.unwrap_or(0), busy: glue.busy };
        let mut out = Vec::new();
        crate::hud::draw_repair(ctx, hud, assets, view, &mut out);
        for r in out {
            match r {
                // `ItemRepair_UIClose_Event` (l'inventaire reste ouvert).
                RepairUi::Close => {
                    glue.close();
                    break;
                }
                RepairUi::Repair => glue.repair_button(assets, items, net, hud, ui),
                RepairUi::Drop { cell, drag } => {
                    if let Some(item) = items.get(&drag.iid).cloned() {
                        glue.drop_item(assets, &item, cell, hud, ui);
                    }
                }
                RepairUi::Remove(cell) => glue.grid[cell] = None,
            }
        }
    }
    if let Some(text) = glue.info.clone() {
        if text.is_empty() || crate::hud::info_box(ctx, hud, assets, &text) {
            glue.info = None;
        }
    }
}

impl App {
    /// Réponses du serveur et durabilité des objets.
    pub(super) fn on_repair_event(&mut self, ev: RepairEvent) {
        let assets = self.assets.clone();
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        let glue = &mut self.repair;
        match ev {
            RepairEvent::Durability { iid, current } => {
                glue.durability.insert(iid, current);
            }
            RepairEvent::Granted { npc, cid } if cid == me || me == 0 => {
                // `ItemRepair_UIOpen_Event` et `ItemRepair_InventoryUIOpen_Event`.
                let at = self.player.as_ref().map(|p| p.position).unwrap_or(Vec3::ZERO);
                glue.open = Some(Open { npc, at });
                glue.reset();
                self.ui.show_inventory = true;
                log::info!("réparation : fenêtre ouverte (PNJ {npc})");
            }
            RepairEvent::Granted { .. } => {}
            RepairEvent::Result { cid, .. } if cid != me && me != 0 => {}
            RepairEvent::Result { code, .. } => {
                glue.busy = false;
                log::info!("réparation : résultat {code}");
                let d = glue.data(&assets);
                let text = match code {
                    result::SUCCESS => d.sys(317),
                    result::NOT_ENOUGH_MONEY => d.event("ItemRepair_InsufficientMoney_Msg"),
                    result::ALREADY_MAX => d.event("ItemRepair_AlreadyMax_Msg"),
                    result::FAILED => d.sys(318),
                    result::CANNOT_REPAIR => d.sys(319),
                    _ => return,
                };
                match code {
                    result::SUCCESS => {
                        // Fermeture de `UI_Repair` et de l'inventaire.
                        glue.close();
                        self.ui.show_inventory = false;
                        self.hud.push_system(&text);
                    }
                    result::NOT_ENOUGH_MONEY | result::ALREADY_MAX => glue.info = Some(text.clone()),
                    _ => self.hud.push_system(&text),
                }
            }
        }
    }

    /// À chaque image : pourcentages pour les cases, ligne « Repair Cost » des infobulles, fenêtre
    /// fermée en s'éloignant. Après `convert_frame` (qui prépare les infobulles).
    pub(super) fn repair_frame(&mut self) {
        let assets = self.assets.clone();
        if let Some(me) = self.player.as_ref().map(|p| p.position) {
            if self.repair.open.as_ref().is_some_and(|o| Vec3::new(me.x - o.at.x, 0.0, me.z - o.at.z).length() > CLOSE_MOVE) {
                self.repair.close();
            }
        }
        let glue = &mut self.repair;
        glue.durability.retain(|iid, _| self.items.contains_key(iid));
        let mut marks = HashMap::new();
        for item in self.items.values() {
            let (cur, max) = glue.current_max(&assets, item);
            if max > 0 {
                marks.insert(item.id, durability_percent(cur, max));
            }
            let line = glue.tooltip_line(&assets, item);
            let label = glue.data(&assets).sys(416);
            if let Some(tip) = self.hud.item_tips.get_mut(&item.id) {
                let at = tip.iter().position(|(t, _)| !label.is_empty() && t.starts_with(&format!("{label} : ")));
                match (at, line) {
                    (Some(i), Some(l)) if tip[i].0 != l => tip[i].0 = l,
                    (Some(_), Some(_)) => {}
                    (Some(i), None) => {
                        tip.remove(i);
                    }
                    (None, Some(l)) => {
                        let color = tip.first().map(|(_, c)| *c).unwrap_or(Color32::WHITE);
                        tip.push((l, color));
                    }
                    (None, None) => {}
                }
            }
        }
        self.hud.durability = marks;
    }

    /// Commandes (essais sans souris, `--exec`) : `/reparation` (PNJ le plus proche qui répare, comme
    /// son bouton), `/aréparer nom|TID` (objet du sac dans la grille), `/reparer` (bouton « Repair »),
    /// `/durabilite` (durabilité des objets portés dans la discussion).
    pub(super) fn repair_slash(&mut self, cmd: &str, arg: &str) -> bool {
        let assets = self.assets.clone();
        match cmd {
            "/reparation" | "/réparation" => {
                let me = self.player.as_ref().map(|p| p.position).unwrap_or(Vec3::ZERO);
                let near = self
                    .entities
                    .iter()
                    .filter(|e| self.npc_functions(e.cid).iter().any(|f| f.function == archrust_netclient::repair::FUNCTION))
                    .map(|e| (e.cid, Vec3::new(e.position.x - me.x, 0.0, e.position.z - me.z).length()))
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                match near {
                    Some((npc, d)) => {
                        self.ui.push_chat(format!("[réparation] PNJ {npc} à {d:.0} unités : demande envoyée."));
                        send(self.net.as_ref(), RepairCommand::Open(npc));
                    }
                    None => self.ui.push_chat("[réparation] Aucun forgeron en vue."),
                }
            }
            "/areparer" | "/aréparer" => {
                let wanted = arg.trim().to_lowercase();
                let mut found: Vec<ItemRecord> = self
                    .items
                    .values()
                    .filter(|i| i.status.is_some_and(|s| BAG_STATUSES.contains(&s)) && !wanted.is_empty())
                    .filter(|i| {
                        i.template_id.map(|t| t.to_string()) == Some(wanted.clone())
                            || i.template_id.and_then(|t| crate::scene::item_name_for_tid(&assets, t)).is_some_and(|n| n.to_lowercase().contains(&wanted))
                    })
                    .cloned()
                    .collect();
                found.sort_by_key(|i| i.id);
                match found.first() {
                    Some(item) => {
                        let cell = self.repair.grid.iter().position(Option::is_none).unwrap_or(0);
                        self.repair.drop_item(&assets, item, cell, &mut self.hud, &mut self.ui);
                    }
                    None => self.ui.push_chat(format!("[réparation] Aucun objet « {} » dans le sac.", arg.trim())),
                }
            }
            "/reparer" | "/réparer" => {
                let items = self.items.clone();
                self.repair.repair_button(&assets, &items, self.net.as_ref(), &mut self.hud, &mut self.ui);
            }
            "/durabilite" | "/durabilité" => {
                let items: Vec<ItemRecord> = self.items.values().filter(|i| matches!(i.status, Some(2 | 3))).cloned().collect();
                for item in items {
                    let (cur, max) = self.repair.current_max(&assets, &item);
                    if max > 0 {
                        let name = item.template_id.and_then(|t| crate::scene::item_name_for_tid(&assets, t)).unwrap_or_default();
                        let cost = self.repair.cost_of(&assets, &item);
                        self.ui.push_chat(format!("[réparation] {name} : {cur}/{max} ({} %), {cost} or", durability_percent(cur, max)));
                    }
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

    fn item(id: i32, tid: i32, status: i8) -> ItemRecord {
        ItemRecord { operation: 0, id, template_id: Some(tid), owner_cid: Some(7), count: Some(1), status: Some(status), grid: None }
    }

    fn glue() -> RepairGlue {
        let mut d = RepairData::default();
        d.items.insert(2, ItemDur { max: 69, price: 21_000, unbreakable: false, kind: 2, part: 10 });
        d.items.insert(9, ItemDur { max: 0, price: 10, unbreakable: false, kind: 64, part: 0 });
        d.items.insert(172, ItemDur { max: 100, price: 1_000, unbreakable: false, kind: 1, part: 2 });
        RepairGlue { data: Some(d), ..Default::default() }
    }

    #[test]
    fn depot_dans_la_grille() {
        let assets = Assets::new(None, &[]);
        let (mut hud, mut ui) = (HudState::default(), UiState::default());
        let mut g = glue();
        // Plein : refusé (déjà au maximum) ; sans durabilité : refusé ; porté : ignoré.
        g.drop_item(&assets, &item(1, 2, 2), 0, &mut hud, &mut ui);
        assert!(g.grid.iter().all(Option::is_none) && g.info.is_some());
        g.info = None;
        g.drop_item(&assets, &item(2, 9, 2), 0, &mut hud, &mut ui);
        assert!(g.grid.iter().all(Option::is_none) && g.info.is_some());
        g.durability.insert(3, 30);
        g.drop_item(&assets, &item(3, 2, STATUS_EQUIP), 0, &mut hud, &mut ui);
        assert!(g.grid.iter().all(Option::is_none));
        // Usé, dans le sac : posé ; reposé ailleurs : déplacé, pas compté deux fois.
        g.drop_item(&assets, &item(3, 2, 2), 5, &mut hud, &mut ui);
        g.drop_item(&assets, &item(3, 2, 2), 7, &mut hud, &mut ui);
        assert_eq!((g.grid[5], g.grid[7]), (None, Some(3)));
        let mut items = HashMap::new();
        items.insert(3, item(3, 2, 2));
        assert_eq!(g.total_cost(&assets, &items), 5934);
    }

    #[test]
    fn silhouette_d_usure() {
        let assets = Assets::new(None, &[]);
        let mut g = glue();
        let mut items = HashMap::new();
        items.insert(1, item(1, 2, STATUS_EQUIP));
        items.insert(2, item(2, 172, STATUS_EQUIP));
        assert!(lit_parts(&mut g, &assets, &items).is_empty(), "pleins : rien d'allumé");
        g.durability.insert(1, 14); // 20 %
        g.durability.insert(2, 21); // 21 %
        assert_eq!(lit_parts(&mut g, &assets, &items), vec!["CONTROL_WEAPON"]);
    }
}
