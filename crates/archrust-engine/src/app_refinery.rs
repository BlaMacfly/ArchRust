//! Sphère d'alchimie côté moteur. Protocole : `archrust_netclient::refinery` ; fenêtres
//! d'Archonia : `hud_refinery.rs`.
//!
//! Comportement relevé dans Archonia (AgcmUIRefinery, sha256 e9c7609b…) :
//! * clic sur l'objet du décor qui porte la fonction 26 (`Function=26` dans `obj*.ini`) :
//!   AGPMEVENTREFINERY op 0 ; l'accord (op 1) ouvre `UI_Refinery` et l'inventaire
//!   (`REFINE_EVENT_OPEN`) ; la fenêtre se ferme si le joueur s'éloigne de plus de 149 unités du point
//!   d'ouverture (FUN_14071f900) ;
//! * textes (FUN_140720270) : à gauche `sysstr` 449, sous-titre `REFINE_SUB_TITLE_ITEM`, message
//!   `REFINE_SUB_NOTIFY_ITEM` puis, après un refus, `REFINE_INSITEM_ITEM` (1, 3), `REFINE_DIFITEM_ITEM`
//!   (2), `REFINE_INSMONEY_ITEM` (4), `REFINE_FULLINVEN_ITEM` (5) (FUN_140720f80) ;
//! * grille : objets du sac glissés dans les 20 cases (ligne × 5 + colonne) ; « Raffiner »
//!   (FUN_14071e190) demande le prix (op 5), puis `REFINE_EVENT_MONEYMSGBOX`, la barre d'une seconde
//!   d'`UI_Cooldown` (`sysstr` 450, FUN_14071f3a0) et l'op 2 ; après un résultat, le même bouton remet
//!   la fenêtre à zéro ; « Répéter » refait le dernier raffinage de grille
//!   (`REFINE_EVENT_REPEAT_MONEYMSGBOX`), sinon `sysstr` 883 ;
//! * recettes (`UI_Refinery_Recipe`, `RefineryItem2.txt`) : nom de l'objet produit, gris (AEA59C)
//!   si les ingrédients manquent (FUN_14071e570) ; détail « [nom] - Matériaux nécessaires - »,
//!   ingrédients en rouge s'il en manque, « Prix: X Gold » (FUN_14071e6a0) ; « Raffiner »
//!   (FUN_14071f1f0) : `REFINE_EVENT_RECIPE_REFINE`, barre d'une seconde, op 6 ;
//! * réussite : grille vidée, objet produit dans `CTL_GRID_RESULT` (`REFINE_EVENT_SUCCESS`).
//!
//! Les 100 recettes au plus, réalisables en tête, viennent du relevé de la session ArchCore ; la
//! recherche garde celles dont le nom contient le texte saisi.

use super::App;
use crate::assets::Assets;
use crate::hud::{HudState, ItemUi, RecipeUi, RecipeView, RefineryUi, RefineryView};
use crate::net::NetLink;
use crate::objects::{tile_function_objects, Placement};
use crate::scene::item_name_for_tid;
use crate::ui::UiState;
use crate::world::{sector_of, tile_of};
use archrust_netclient::refinery::{result, RefineryCommand, RefineryEvent, FUNCTION_REFINERY, GRID_CELLS};
use archrust_netclient::world::ItemRecord;
use archrust_netclient::Command;
use egui::Color32;
use glam::Vec3;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Déplacement au-delà duquel la fenêtre se ferme (FUN_14071f900 : `0x95 < distance`).
const CLOSE_MOVE: f32 = 149.0;
/// Durée de la barre d'attente avant l'envoi (`FUN_140694d50(…, 1000, sysstr 450)`).
const PROGRESS: Duration = Duration::from_millis(1000);
/// Recettes affichées au plus.
const MAX_RECIPES: usize = 100;
/// Statut AgpmItem d'un objet du sac.
const STATUS_INVENTORY: i8 = 2;
/// Couleur d'une recette dont les ingrédients manquent (0xFFAEA59C).
const UNAVAILABLE: Color32 = Color32::from_rgb(0xae, 0xa5, 0x9c);

/// Une recette de `RefineryItem2.txt`.
#[derive(Debug, Clone, Default)]
struct Recipe {
    id: i32,
    product: i32,
    price: i64,
    /// Ingrédients : TID et quantité.
    resources: Vec<(i32, i32)>,
}

#[derive(Default)]
struct RefineData {
    recipes: Vec<Recipe>,
    messages: HashMap<String, String>,
    events: HashMap<String, String>,
    sysstr: HashMap<u32, String>,
}

impl RefineData {
    fn msg(&self, key: &str) -> String {
        self.messages.get(key).cloned().unwrap_or_default()
    }

    fn sys(&self, id: u32) -> String {
        self.sysstr.get(&id).cloned().unwrap_or_default()
    }

    /// Phrase d'un événement de l'`[EventMap]`, prix à la place de `{{REFINE_UD_PRICE.REFINE_DISP_PRICE}}`.
    fn price_box(&self, event: &str, price: i64) -> String {
        self.events.get(event).cloned().unwrap_or_default().replace("{{REFINE_UD_PRICE.REFINE_DISP_PRICE}}", &price.to_string())
    }
}

fn load_data(assets: &Assets) -> RefineData {
    let mut d = RefineData::default();
    // Recipe ID, MakeItem Total, Keep… (5), MakeItem TID (7), …, Combine Use Price (17), timer,
    // Resource TotalCount (19), puis TID / nom / quantité par ingrédient. Les lignes sans identifiant
    // (ou qui répètent son identifiant) sont les autres issues de la recette au-dessus.
    let text = super::npc_glue::client_text_in(assets, "ini", "RefineryItem2.txt");
    let mut seen = HashSet::new();
    for line in text.lines().skip(1) {
        let cells: Vec<&str> = line.split('\t').collect();
        let num = |i: usize| cells.get(i).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0);
        let Some(id) = cells.first().and_then(|v| v.trim().parse::<i32>().ok()) else { continue };
        if !seen.insert(id) {
            continue;
        }
        let total = num(19).max(0) as usize;
        let resources = (0..total).map(|k| (num(20 + 3 * k) as i32, num(22 + 3 * k) as i32)).filter(|(t, n)| *t > 0 && *n > 0).collect();
        d.recipes.push(Recipe { id, product: num(7) as i32, price: num(17), resources });
    }
    d.messages = super::npc_glue::client_text(assets, "uimessage.txt").lines().filter_map(|l| l.split_once('\t')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect();
    d.events = super::portal_glue::ui_events(assets);
    for id in [3, 11, 38, 449, 450, 883] {
        d.sysstr.insert(id, super::npc_glue::sysstr(assets, id).unwrap_or_default());
    }
    log::info!("raffinage : {} recette(s)", d.recipes.len());
    d
}

/// Sphère ouverte : position du joueur à l'ouverture.
struct Open {
    at: Vec3,
}

#[derive(Default)]
pub struct RefineryGlue {
    data: Option<RefineData>,
    /// Tuiles déjà lues et sphères qu'elles portent.
    tiles: HashSet<u32>,
    spheres: Vec<(u32, Placement)>,
    open: Option<Open>,
    /// IID posé dans chaque case.
    grid: [Option<i32>; GRID_CELLS],
    /// Objet produit au dernier raffinage.
    result: Option<i32>,
    /// Résultat du dernier raffinage (Archonia `0x6750`, -1 : aucun).
    last_result: Option<i32>,
    notify: String,
    /// Grille dont le prix a été demandé (op 5).
    quoted: Option<[i32; GRID_CELLS]>,
    /// Dernière grille raffinée et son prix (« Répéter »).
    last_grid: Option<([i32; GRID_CELLS], i64)>,
    /// Boîte de prix ouverte : phrase, demande et prix.
    ask: Option<(String, RefineryCommand, i64)>,
    /// Barre d'attente : début et demande à envoyer.
    progress: Option<(Instant, RefineryCommand)>,
    recipes_open: bool,
    search: String,
    selected: Option<i32>,
    first_row: usize,
}

impl RefineryGlue {
    fn data(&mut self, assets: &Assets) -> &RefineData {
        self.data.get_or_insert_with(|| load_data(assets))
    }

    /// Grille, résultat et message remis à zéro (FUN_140721b70).
    fn reset(&mut self, assets: &Assets) {
        self.grid = [None; GRID_CELLS];
        self.result = None;
        self.last_result = None;
        self.quoted = None;
        self.notify = self.data(assets).msg("REFINE_SUB_NOTIFY_ITEM");
    }

    fn grid_iids(&self) -> [i32; GRID_CELLS] {
        self.grid.map(|g| g.unwrap_or(0))
    }

    /// Bouton « Raffiner » : prix demandé, ou remise à zéro après un résultat.
    fn refine_button(&mut self, assets: &Assets, net: Option<&NetLink>) {
        if self.last_result.is_some() {
            self.reset(assets);
            return;
        }
        if self.grid.iter().all(Option::is_none) {
            return;
        }
        let grid = self.grid_iids();
        self.quoted = Some(grid);
        send(net, RefineryCommand::Quote(grid));
    }

    /// Bouton « Répéter ».
    fn repeat_button(&mut self, assets: &Assets, hud: &mut HudState, ui: &mut UiState) {
        match self.last_grid {
            Some((grid, price)) => {
                let text = self.data(assets).price_box("REFINE_EVENT_REPEAT_MONEYMSGBOX", price);
                self.ask = Some((text, RefineryCommand::Refine(grid), price));
            }
            None => {
                let text = self.data(assets).sys(883);
                hud.push_system(&text);
                ui.push_chat(format!("[raffinage] {text}"));
            }
        }
    }

    /// Recettes affichées : nom contenant la recherche, réalisables en tête, 100 au plus.
    fn visible_recipes(&mut self, assets: &Assets, have: &HashMap<i32, i32>) -> Vec<(i32, String, bool)> {
        let search = self.search.trim().to_lowercase();
        let d = self.data(assets);
        let mut rows: Vec<(i32, String, bool)> = d
            .recipes
            .iter()
            .filter_map(|r| {
                let name = item_name_for_tid(assets, r.product)?;
                (search.is_empty() || name.to_lowercase().contains(&search)).then(|| (r.id, name, makeable(r, have)))
            })
            .collect();
        rows.sort_by_key(|(_, _, ok)| !*ok);
        rows.truncate(MAX_RECIPES);
        rows
    }

    /// Détail d'une recette (FUN_14071e6a0).
    fn detail(&mut self, assets: &Assets, id: i32, have: &HashMap<i32, i32>, money: i64) -> (String, Vec<(i32, i32)>) {
        let d = self.data(assets);
        let Some(r) = d.recipes.iter().find(|r| r.id == id) else { return (String::new(), Vec::new()) };
        let name = item_name_for_tid(assets, r.product).unwrap_or_default();
        let mut text = format!("______[{name}]______- {} -___", d.sys(38));
        let mut merged: Vec<(i32, i32)> = Vec::new();
        for (tid, n) in &r.resources {
            match merged.iter_mut().find(|(t, _)| t == tid) {
                Some(m) => m.1 += n,
                None => merged.push((*tid, *n)),
            }
        }
        for (tid, n) in &merged {
            let rname = item_name_for_tid(assets, *tid).unwrap_or_default();
            let line = format!("{rname} {n}{}", d.sys(11));
            text.push_str(&if have.get(tid).copied().unwrap_or(0) < *n { format!("<HFF0000>{line}<HFFFFFF>___") } else { format!("{line}___") });
        }
        let price = format!("{}: {} Gold", d.sys(3), r.price);
        text.push_str(&if money < r.price { format!("<HFF0000>{price}<HFFFFFF>___") } else { format!("{price}___") });
        (text, merged)
    }

    /// Bouton « Raffiner » des recettes (FUN_14071f1f0) : recette réalisable, puis boîte de prix.
    fn recipe_refine(&mut self, assets: &Assets, have: &HashMap<i32, i32>) {
        let Some(id) = self.selected else { return };
        let d = self.data(assets);
        let Some(r) = d.recipes.iter().find(|r| r.id == id).cloned() else { return };
        if makeable(&r, have) {
            let text = d.price_box("REFINE_EVENT_RECIPE_REFINE", r.price);
            self.ask = Some((text, RefineryCommand::RefineRecipe(id), r.price));
        }
    }
}

/// Les ingrédients d'une recette sont dans le sac (FUN_1401e6d00).
fn makeable(r: &Recipe, have: &HashMap<i32, i32>) -> bool {
    let mut need: HashMap<i32, i32> = HashMap::new();
    for (t, n) in &r.resources {
        *need.entry(*t).or_default() += n;
    }
    !need.is_empty() && need.iter().all(|(t, n)| have.get(t).copied().unwrap_or(0) >= *n)
}

/// Quantité de chaque TID dans le sac.
fn bag_counts(items: &HashMap<i32, ItemRecord>) -> HashMap<i32, i32> {
    let mut have = HashMap::new();
    for i in items.values().filter(|i| i.status == Some(STATUS_INVENTORY)) {
        if let Some(t) = i.template_id {
            *have.entry(t).or_default() += i.count.unwrap_or(1).max(1);
        }
    }
    have
}

fn send(net: Option<&NetLink>, cmd: RefineryCommand) {
    if let Some(net) = net {
        net.send(Command::Refinery(cmd));
    }
}

/// Fenêtres de la sphère, boîte de prix et barre d'attente. À appeler après l'inventaire, avant
/// `app_items::panel` (un clic droit sur un objet du sac le pose dans la grille).
#[allow(clippy::too_many_arguments)]
pub fn panel(
    ctx: &egui::Context,
    glue: &mut RefineryGlue,
    item_requests: &mut Vec<ItemUi>,
    items: &HashMap<i32, ItemRecord>,
    hud: &mut HudState,
    assets: &Assets,
    net: Option<&NetLink>,
    ui: &mut UiState,
) {
    if glue.open.is_none() {
        return;
    }
    // Objets consommés ou sortis du sac : leur case se vide.
    for cell in glue.grid.iter_mut() {
        if cell.is_some_and(|i| !items.get(&i).is_some_and(|it| it.status == Some(STATUS_INVENTORY))) {
            *cell = None;
        }
    }
    // Clic droit sur un objet du sac : première case libre.
    item_requests.retain(|r| {
        let ItemUi::Activate { iid, worn: false } = *r else { return true };
        if !glue.grid.contains(&Some(iid)) {
            if glue.last_result.is_some() {
                glue.reset(assets);
            }
            if let Some(cell) = glue.grid.iter_mut().find(|c| c.is_none()) {
                *cell = Some(iid);
            }
        }
        false
    });

    let main = glue.data(assets).sys(449);
    let subtitle = glue.data(assets).msg("REFINE_SUB_TITLE_ITEM");
    let notify = glue.notify.clone();
    let grid = glue.grid.map(|g| g.and_then(|i| items.get(&i)).map(|i| (i.template_id.unwrap_or(0), i.count.unwrap_or(1))));
    let mut out = Vec::new();
    crate::hud::draw_refinery(ctx, hud, assets, RefineryView { main: &main, subtitle: &subtitle, notify: &notify, grid, result: glue.result }, &mut out);
    for r in out {
        match r {
            // `REFINE_CLOSE` : la fenêtre et celle des recettes.
            RefineryUi::Close => {
                glue.open = None;
                glue.recipes_open = false;
                glue.ask = None;
                glue.reset(assets);
                return;
            }
            RefineryUi::Refine => glue.refine_button(assets, net),
            RefineryUi::Reset => glue.reset(assets),
            RefineryUi::Repeat => glue.repeat_button(assets, hud, ui),
            RefineryUi::Recipe => glue.recipes_open = true,
            RefineryUi::Drop { cell, drag } => {
                if items.get(&drag.iid).is_some_and(|i| i.status == Some(STATUS_INVENTORY)) {
                    if glue.last_result.is_some() {
                        glue.reset(assets);
                    }
                    for c in glue.grid.iter_mut().filter(|c| **c == Some(drag.iid)) {
                        *c = None;
                    }
                    glue.grid[cell] = Some(drag.iid);
                }
            }
            RefineryUi::Clear(cell) => glue.grid[cell] = None,
        }
    }

    if glue.recipes_open {
        let have = bag_counts(items);
        let money = hud.stats.money.unwrap_or(0);
        let rows = glue.visible_recipes(assets, &have);
        let shown: Vec<(String, Color32)> = rows.iter().map(|(_, n, ok)| (n.clone(), if *ok { Color32::WHITE } else { UNAVAILABLE })).collect();
        let selected = glue.selected.and_then(|id| rows.iter().position(|(r, _, _)| *r == id));
        let (detail, ingredients) = match glue.selected {
            Some(id) => glue.detail(assets, id, &have, money),
            None => (String::new(), Vec::new()),
        };
        glue.first_row = glue.first_row.min(rows.len().saturating_sub(1));
        let mut search = std::mem::take(&mut glue.search);
        let mut out = Vec::new();
        crate::hud::draw_recipes(ctx, hud, assets, RecipeView { rows: &shown, selected, first_row: glue.first_row, detail: &detail, ingredients: &ingredients, search: &mut search }, &mut out);
        if search != glue.search {
            glue.first_row = 0;
        }
        glue.search = search;
        for r in out {
            match r {
                RecipeUi::Close => glue.recipes_open = false,
                RecipeUi::Select(k) => glue.selected = rows.get(k).map(|(id, _, _)| *id),
                RecipeUi::Refine => glue.recipe_refine(assets, &have),
                RecipeUi::Scroll(n) => glue.first_row = (glue.first_row as i32 + n).clamp(0, rows.len().saturating_sub(1) as i32) as usize,
            }
        }
    }

    if let Some((text, cmd, price)) = glue.ask.clone() {
        match crate::hud::confirm_box(ctx, hud, assets, "refinery-price", &text) {
            Some(true) => {
                glue.ask = None;
                if let RefineryCommand::Refine(grid) = &cmd {
                    glue.last_grid = Some((*grid, price));
                }
                glue.progress = Some((Instant::now(), cmd));
            }
            Some(false) => glue.ask = None,
            None => {}
        }
    }
    if let Some((since, cmd)) = glue.progress.clone() {
        let f = since.elapsed().as_secs_f32() / PROGRESS.as_secs_f32();
        let title = glue.data(assets).sys(450);
        crate::hud::draw_cooldown(ctx, hud, assets, f.min(1.0), &title);
        if since.elapsed() >= PROGRESS {
            glue.progress = None;
            send(net, cmd);
        } else {
            ctx.request_repaint();
        }
    }
}

impl App {
    /// Sphères des tuiles autour du joueur ; fenêtre fermée en s'éloignant du point d'ouverture.
    pub(super) fn refinery_frame(&mut self) {
        let Some(me) = self.player.as_ref().map(|p| p.position) else { return };
        let (sx, sy) = sector_of(me.x, me.z);
        for dy in [-16, 0, 16] {
            for dx in [-16, 0, 16] {
                let tile = tile_of(sx + dx, sy + dy);
                if self.refinery.tiles.insert(tile) {
                    let found = tile_function_objects(&self.assets, tile, FUNCTION_REFINERY);
                    if !found.is_empty() {
                        log::info!("raffinage : {} sphère(s) d'alchimie dans obj{tile:05}.ini", found.len());
                    }
                    self.refinery.spheres.extend(found);
                }
            }
        }
        if self.refinery.open.as_ref().is_some_and(|o| Vec3::new(me.x - o.at.x, 0.0, me.z - o.at.z).length() > CLOSE_MOVE) {
            self.refinery.open = None;
            self.refinery.recipes_open = false;
            self.refinery.ask = None;
        }
    }

    /// Clic gauche sur une sphère d'alchimie : demande au serveur (op 0). Rend `true` si une sphère
    /// est visée.
    pub(super) fn refinery_left_click(&mut self) -> bool {
        let (Some(gpu), Some((cx, cy))) = (&self.gpu, self.last_cursor) else { return false };
        let (w, h) = (gpu.renderer.config.width as f32, gpu.renderer.config.height as f32);
        let ndc = Vec3::new((cx as f32 / w) * 2.0 - 1.0, 1.0 - (cy as f32 / h) * 2.0, 0.0);
        let inv = self.camera.view_proj(gpu.renderer.aspect()).inverse();
        let near = inv.project_point3(ndc);
        let dir = (inv.project_point3(Vec3::new(ndc.x, ndc.y, 1.0)) - near).normalize_or_zero();
        // Sphère englobante du modèle, sinon 150 unités autour du point posé.
        let hit = self
            .refinery
            .spheres
            .iter()
            .filter_map(|(id, p)| {
                let (center, radius) = self.objects.placed_bounds(p).unwrap_or((p.position + Vec3::new(0.0, 100.0, 0.0), 150.0));
                let t = (center - near).dot(dir);
                (t > 0.0 && (near + dir * t).distance(center) <= radius).then_some((*id, t))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id);
        let Some(object) = hit else { return false };
        log::info!("raffinage : demande à la sphère {object}");
        send(self.net.as_ref(), RefineryCommand::Open(object));
        true
    }

    /// Réponses du serveur.
    pub(super) fn on_refinery_event(&mut self, ev: RefineryEvent) {
        let assets = self.assets.clone();
        let glue = &mut self.refinery;
        match ev {
            // `REFINE_EVENT_OPEN` : la sphère et l'inventaire.
            RefineryEvent::Opened => {
                let at = self.player.as_ref().map(|p| p.position).unwrap_or(Vec3::ZERO);
                glue.open = Some(Open { at });
                glue.reset(&assets);
                self.ui.show_inventory = true;
            }
            RefineryEvent::Quote { price } => {
                if let Some(grid) = glue.quoted.take() {
                    let text = glue.data(&assets).price_box("REFINE_EVENT_MONEYMSGBOX", price);
                    glue.ask = Some((text, RefineryCommand::Refine(grid), price));
                }
            }
            RefineryEvent::Result { code, tid } => {
                glue.last_result = Some(code);
                let key = match code {
                    result::SUCCESS => {
                        // `REFINE_EVENT_SUCCESS` : grille vidée, objet produit affiché.
                        glue.grid = [None; GRID_CELLS];
                        glue.result = tid;
                        let name = tid.and_then(|t| item_name_for_tid(&assets, t)).unwrap_or_default();
                        log::info!("raffinage : réussi, {name} ({tid:?})");
                        self.ui.push_chat(format!("[raffinage] {name}"));
                        return;
                    }
                    result::MISSING_ITEMS | result::MISSING_ITEMS_2 => "REFINE_INSITEM_ITEM",
                    result::NO_RECIPE => "REFINE_DIFITEM_ITEM",
                    result::NO_MONEY => "REFINE_INSMONEY_ITEM",
                    result::FULL_INVENTORY => "REFINE_FULLINVEN_ITEM",
                    _ => "REFINE_DIFITEM_ITEM",
                };
                let text = glue.data(&assets).msg(key);
                log::info!("raffinage : résultat {code} ({key})");
                glue.notify = text.clone();
                self.hud.push_system(&text);
                self.ui.push_chat(format!("[raffinage] {text}"));
            }
        }
    }

    /// Commandes (essais sans souris, `--exec`) : `/sphere` (sphère la plus proche, comme un clic),
    /// `/grille nom|TID` (objet du sac dans la première case libre), `/raffiner` (bouton
    /// « Raffiner » ; `/raffiner oui` accepte la boîte de prix), `/repeter`, `/recette [id]` (fenêtre
    /// des recettes, recette choisie ; `/recette ok` : son bouton « Raffiner »).
    pub(super) fn refinery_slash(&mut self, cmd: &str, arg: &str) -> bool {
        let assets = self.assets.clone();
        let arg = arg.trim();
        match cmd {
            "/sphere" | "/sphère" => {
                let me = self.player.as_ref().map_or(Vec3::ZERO, |p| p.position);
                match self.refinery.spheres.iter().map(|(id, p)| (*id, p.position, p.position.distance(me))).min_by(|a, b| a.2.total_cmp(&b.2)) {
                    Some((object, at, d)) => {
                        self.ui.push_chat(format!("[raffinage] sphère {object} en {:.0}, {:.0}, à {d:.0} unités : demande envoyée.", at.x, at.z));
                        log::info!("raffinage : sphère {object} en {at:?}, à {d:.0} unités");
                        send(self.net.as_ref(), RefineryCommand::Open(object));
                    }
                    None => self.ui.push_chat("[raffinage] Aucune sphère d'alchimie dans les tuiles proches."),
                }
            }
            "/grille" => {
                let wanted = arg.to_lowercase();
                let mut found: Vec<&ItemRecord> = self
                    .items
                    .values()
                    .filter(|i| i.status == Some(STATUS_INVENTORY) && !wanted.is_empty())
                    .filter(|i| i.template_id.map(|t| t.to_string()) == Some(wanted.clone()) || i.template_id.and_then(|t| item_name_for_tid(&assets, t)).is_some_and(|n| n.to_lowercase().contains(&wanted)))
                    .collect();
                found.sort_by_key(|i| i.id);
                match found.first().map(|i| i.id) {
                    Some(iid) => {
                        if self.refinery.last_result.is_some() {
                            self.refinery.reset(&assets);
                        }
                        if let Some(cell) = self.refinery.grid.iter_mut().find(|c| c.is_none()) {
                            *cell = Some(iid);
                        }
                    }
                    None => self.ui.push_chat(format!("[raffinage] Aucun objet « {arg} » dans le sac.")),
                }
            }
            "/raffiner" if arg == "oui" => {
                if let Some((_, cmd, price)) = self.refinery.ask.take() {
                    if let RefineryCommand::Refine(grid) = &cmd {
                        self.refinery.last_grid = Some((*grid, price));
                    }
                    self.refinery.progress = Some((Instant::now(), cmd));
                }
            }
            "/raffiner" => self.refinery.refine_button(&assets, self.net.as_ref()),
            "/repeter" | "/répéter" => self.refinery.repeat_button(&assets, &mut self.hud, &mut self.ui),
            "/recette" if arg == "ok" => {
                let have = bag_counts(&self.items);
                self.refinery.recipe_refine(&assets, &have);
            }
            "/recette" => {
                self.refinery.recipes_open = true;
                if let Ok(id) = arg.parse::<i32>() {
                    self.refinery.selected = Some(id);
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
    fn recette_realisable() {
        let r = Recipe { id: 4013, product: 245, price: 5000, resources: vec![(244, 100)] };
        let mut have = HashMap::new();
        have.insert(244, 99);
        assert!(!makeable(&r, &have));
        have.insert(244, 100);
        assert!(makeable(&r, &have));
    }
}
