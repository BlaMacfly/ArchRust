//! Inventaire Cash et sac du familier d'Archonia, d'après `UI_1024X768.ini` :
//!
//! * `UI_CashInven` (342 × 496, `CASH_INVEN_BASE.png`, titre « Chantra Bag », module AgcmUICashInven) : onglet
//!   `CINVEN_TAB_ALL` « All » (le seul), liste `CINVEL_ITEM_LIST` de 7 lignes `CINVEN_ITEM` (299 × 56) avec
//!   défilement (`CINVEN_SCROLL`, boutons `CONTROL_3282` / `CONTROL_30834`, molette). Chaque ligne : icône
//!   `CINVEN_ITEM_GRID` (clic droit : `InventoryDoubleRightClick`), nom `CONTROL_6363` (`CInven_ItemInfoName`),
//!   quantité `CONTROL_30097` (`CInven_ItemInfoCount`), temps `CINVEN_ITEM_INFO` (`CInven_ItemInfo`), état
//!   `CONTROL_23039` (`CInven_UsingItem`), bouton `CINVEN_USE_ITEM` (`CInven_UseItemButton`, `CInven_ClickUseItem`).
//!   Bas : corbeille `CashInven_Waste`, compte `Text_CashItemCount` (« Chantra Item %d / 500 », `FUN_1406023b0`).
//!   Écartés : `CINVEN_BUY_ITEM` « Purchase » (`CashMallOpenUI`, boutique Chantra), le filtre
//!   `CashInven_Edit_Filter`, la jauge d'endurance `CONTROL_HUNGRY` et le cadenas `CONTROL_11277` (données que le
//!   serveur n'envoie pas).
//! * `UI_SubInventory` (230 × 215, `Pet_Inven_Base.png`, « Pet Inventory ») : onglets `CONTROL_SubInv_Layer1..4_Btn`
//!   (`INV_TAB` / `INV_TAB_SEL`, objet lâché dessus : `SubInventory_DragDropItem`), grille `CONTROL_SubInv_Grid`
//!   2 × 4 cases de 50 px, corbeille `CONTROL_WasteBasket`, bouton `CONTROL_26077` « Auto Pick Up ON/OFF »
//!   (`SubInventory_ToggleAutoGetDropItem`), croix `CONTROL_SubInven_X_Btn`. Écarté : `CONTROL_SubInv_AllItemSell`
//!   « Sell All » (ajout d'Archonia).
//!
//! Les demandes reviennent au moteur en [`PetBagUi`] (voir `app_petbag.rs`).

use super::portal_ui::{control_text, state_button};
use super::npc_ui::{button, frame, window};
use super::*;

/// Fenêtres chargées avec le HUD.
pub(super) const PETBAG_WINDOWS: [&str; 2] = ["UI_CashInven", "UI_SubInventory"];

/// Lignes visibles de la liste (`CINVEL_ITEM_LIST` 392 px, `List=7`).
pub const CASH_ROWS: usize = 7;
/// Hauteur d'une ligne (`CINVEN_ITEM`).
const ROW_H: f32 = 56.0;
/// Contrôles d'une ligne (`CtrlParent=CINVEN_ITEM`), placés pour la première.
const ROW_CONTROLS: [&str; 6] = ["CINVEN_ITEM_GRID", "CONTROL_6363", "CONTROL_30097", "CINVEN_ITEM_INFO", "CONTROL_23039", "CINVEN_USE_ITEM"];

/// Ligne de l'inventaire Cash, textes préparés par le moteur.
#[derive(Debug, Clone)]
pub struct CashLine {
    pub item: ItemRecord,
    pub name: String,
    pub count: String,
    pub info: String,
    pub using: String,
    /// Bouton d'usage : libellé et état actif ; `None` : pas de bouton (`CashItemUseType` 0).
    pub button: Option<(String, bool)>,
}

pub struct CashInvenView<'a> {
    pub lines: &'a [CashLine],
    /// Première ligne affichée.
    pub first: usize,
    pub count_text: String,
}

pub struct SubInvenView<'a> {
    /// Objets du sac (statut 13) et leur nom.
    pub items: &'a [(ItemRecord, String)],
    pub layer: i16,
    pub auto_label: String,
}

/// Demandes des deux fenêtres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetBagUi {
    CloseCash,
    /// Défilement de la liste (lignes, négatif vers le haut).
    Scroll(i32),
    /// Bouton d'usage d'une ligne (`CInven_ClickUseItem`).
    UseCash(i32),
    /// Clic droit sur une icône (`InventoryDoubleRightClick`) ou un objet du sac (`Inventory_UseItem`).
    Activate(i32),
    /// Objet lâché sur une corbeille (`InventoryWaste_DragDropItem`).
    Waste(ItemDrag),
    CloseBag,
    Layer(i16),
    /// Objet lâché sur une case du sac (`SubInventory_MoveItem`).
    DropOnBag { drag: ItemDrag, grid: [i16; 3] },
    /// Objet lâché sur l'onglet d'un sac (`SubInventory_DragDropItem`).
    DropOnTab { drag: ItemDrag, bag: i16 },
    AutoPick,
}

fn dragged(ctx: &egui::Context) -> Option<ItemDrag> {
    egui::DragAndDrop::payload::<ItemDrag>(ctx).map(|p| *p)
}

/// Icône d'objet saisissable (icône, quantité, infobulle, clic droit), commune aux deux fenêtres.
#[allow(clippy::too_many_arguments)]
fn item_cell(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, resp: &egui::Response, cell: Rect, item: &ItemRecord, name: &str, font: f32, s: f32, out: &mut Vec<PetBagUi>) {
    let lifted = dragged(ctx).is_some_and(|d| d.iid == item.id);
    items_ui::icon(ctx, hud, assets, painter, cell, item.template_id, if lifted { Color32::from_white_alpha(70) } else { Color32::WHITE });
    let count = item.count.unwrap_or(1);
    if count > 1 && !lifted {
        shadow_text(painter, cell.right_bottom() - Vec2::new(3.0, 2.0) * s, Align2::RIGHT_BOTTOM, &count.to_string(), font, Color32::WHITE);
    }
    if resp.dragged() {
        resp.dnd_set_drag_payload(ItemDrag { iid: item.id, tid: item.template_id.unwrap_or(0), worn: false, bank: false });
    }
    if resp.secondary_clicked() {
        out.push(PetBagUi::Activate(item.id));
    }
    if dragged(ctx).is_none() {
        match hud.item_tips.get(&item.id) {
            Some(tip) if resp.hovered() => super::item_tooltip(ctx, tip, ""),
            Some(_) => {}
            None => {
                resp.clone().on_hover_text(name.to_string());
            }
        }
    }
}

/// Cadre de la case visée ou survolée.
fn cell_stroke(painter: &egui::Painter, cell: Rect, resp: &egui::Response, dragging: bool) {
    let target = dragging && resp.contains_pointer();
    if resp.hovered() || target {
        let color = if target { Color32::from_rgb(255, 210, 110) } else { Color32::from_rgb(120, 200, 255) };
        painter.rect_stroke(cell, 2.0, egui::Stroke::new(1.0_f32, color), egui::StrokeKind::Inside);
    }
}

/// Corbeille : objet lâché dessus.
#[allow(clippy::too_many_arguments)]
fn waste(ctx: &egui::Context, ui: &mut egui::Ui, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, r: Rect, c: &UiControl, id: &str, out: &mut Vec<PetBagUi>) {
    let resp = ui.interact(r, egui::Id::new(id), egui::Sense::hover());
    let hot = dragged(ctx).is_some() && resp.contains_pointer();
    // Texture du repos ou du survol d'après `Button=` (les deux corbeilles n'ont pas le même ordre).
    let idx = c.button_states.map(|b| b[usize::from(hot)]).unwrap_or(if hot { 2 } else { 1 });
    let tex = c.textures.iter().find(|(i, _)| *i == idx).or(c.textures.first()).map(|(_, n)| n.clone());
    if let Some(t) = tex {
        control_image(ctx, hud, assets, painter, r, c.size, &t, 1.0);
    }
    if let Some(drag) = resp.dnd_release_payload::<ItemDrag>() {
        if !drag.worn && !drag.bank {
            out.push(PetBagUi::Waste(*drag));
        }
    }
}

/// `UI_CashInven` : liste des objets Cash.
pub fn draw_cash_inventory(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &CashInvenView, out: &mut Vec<PetBagUi>) {
    let Some(w) = window(hud, assets, "UI_CashInven") else { return };
    frame(ctx, hud, assets, &w, "archonia-cash-inven", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let dragging = dragged(ctx).is_some();
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match c.name.as_str() {
                "CONTROL_14574" => control_text(painter, r, c, c.fixed_label(), font * 1.05, None),
                "CINVEN_TAB_ALL" => {
                    // Onglet unique, toujours choisi.
                    let tex = c.textures.iter().find(|(i, _)| *i == 2).or(c.textures.first()).map(|(_, n)| n.clone());
                    if let Some(t) = tex {
                        control_image(ctx, hud, assets, painter, r, c.size, &t, 1.0);
                    }
                    shadow_text(painter, r.center(), Align2::CENTER_CENTER, c.fixed_label(), font, Color32::from_rgb(255, 230, 160));
                }
                "CINVEN_CLOSE" => {
                    if button(ctx, ui, hud, assets, painter, r, c, "", font, true, "cash-inven-x").clicked() {
                        out.push(PetBagUi::CloseCash);
                    }
                }
                "CashInven_Waste" => waste(ctx, ui, hud, assets, painter, r, c, "cash-inven-waste", out),
                "Text_CashItemCount" => shadow_text(painter, r.left_center(), Align2::LEFT_CENTER, &view.count_text, font, Color32::WHITE),
                "CONTROL_3282" | "CONTROL_30834" => {
                    let up = c.name == "CONTROL_3282";
                    if state_button(ctx, ui, hud, assets, painter, r, c, "", font, true, ("cash-inven-scroll", up)).clicked() {
                        out.push(PetBagUi::Scroll(if up { -1 } else { 1 }));
                    }
                }
                "CINVEN_SCROLL_BUTTON" => {
                    // Curseur placé entre les deux boutons selon la première ligne affichée.
                    let pages = view.lines.len().saturating_sub(CASH_ROWS);
                    let track = w.control("CINVEN_SCROLL").map(|t| t.size[1]).unwrap_or(390.0) - 32.0 - c.size[1];
                    let dy = if pages > 0 { track * view.first.min(pages) as f32 / pages as f32 } else { 0.0 };
                    if let Some((_, t)) = c.textures.first() {
                        let rr = layout.rect(origin, [c.pos[0], c.pos[1] + dy], c.size);
                        control_image(ctx, hud, assets, painter, rr, c.size, t, 1.0);
                    }
                }
                "CINVEL_ITEM_LIST" => {
                    let resp = ui.interact(r, egui::Id::new("cash-inven-list"), egui::Sense::hover());
                    if resp.hovered() {
                        let wheel = ui.input(|i| i.raw_scroll_delta.y);
                        if wheel.abs() > 0.5 {
                            out.push(PetBagUi::Scroll(if wheel > 0.0 { -1 } else { 1 }));
                        }
                    }
                }
                _ => {}
            }
        }
        // Lignes : les contrôles de `CINVEN_ITEM`, décalés de 56 px par ligne.
        for (row, line) in view.lines.iter().skip(view.first).take(CASH_ROWS).enumerate() {
            let dy = row as f32 * ROW_H;
            for name in ROW_CONTROLS {
                let Some(c) = w.control(name) else { continue };
                let r = layout.rect(origin, [c.pos[0], c.pos[1] + dy], c.size);
                match name {
                    "CINVEN_ITEM_GRID" => {
                        if let Some((_, t)) = c.textures.first() {
                            control_image(ctx, hud, assets, painter, r, c.size, t, 1.0);
                        }
                        let resp = ui.interact(r, egui::Id::new(("cash-inven-cell", line.item.id)), egui::Sense::click_and_drag());
                        item_cell(ctx, hud, assets, painter, &resp, r, &line.item, &line.name, font, s, out);
                        cell_stroke(painter, r, &resp, dragging);
                    }
                    "CONTROL_6363" => control_text(painter, r, c, &line.name, font, Some(Color32::WHITE)),
                    "CONTROL_30097" => control_text(painter, r, c, &line.count, font, Some(Color32::from_rgb(220, 220, 220))),
                    "CINVEN_ITEM_INFO" => control_text(painter, r, c, &line.info, font, Some(Color32::from_rgb(220, 220, 220))),
                    "CONTROL_23039" => control_text(painter, r, c, &line.using, font, None),
                    "CINVEN_USE_ITEM" => {
                        if let Some((label, enabled)) = &line.button {
                            if button(ctx, ui, hud, assets, painter, r, c, label, font * 0.9, *enabled, ("cash-inven-use", line.item.id)).clicked() {
                                out.push(PetBagUi::UseCash(line.item.id));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    });
}

/// `UI_SubInventory` : sac du familier.
pub fn draw_sub_inventory(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &SubInvenView, out: &mut Vec<PetBagUi>) {
    let Some(w) = window(hud, assets, "UI_SubInventory") else { return };
    frame(ctx, hud, assets, &w, "archonia-sub-inven", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let dragging = dragged(ctx).is_some();
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            if let Some(bag) = c.name.strip_prefix("CONTROL_SubInv_Layer").and_then(|n| n.strip_suffix("_Btn")).and_then(|n| n.parse::<i16>().ok()).map(|n| n - 1) {
                let selected = bag == view.layer;
                let tex = c.textures.iter().find(|(i, _)| *i == if selected { 2 } else { 1 }).or(c.textures.first()).map(|(_, n)| n.clone());
                if let Some(t) = tex {
                    control_image(ctx, hud, assets, painter, r, c.size, &t, 1.0);
                }
                let color = if selected { Color32::from_rgb(255, 230, 160) } else { Color32::from_rgb(190, 190, 190) };
                shadow_text(painter, r.center(), Align2::CENTER_CENTER, c.fixed_label(), font, color);
                let resp = ui.interact(r, egui::Id::new(("sub-inven-tab", bag)), egui::Sense::click());
                if resp.clicked() {
                    out.push(PetBagUi::Layer(bag));
                }
                if let Some(drag) = resp.dnd_release_payload::<ItemDrag>() {
                    out.push(PetBagUi::DropOnTab { drag: *drag, bag });
                }
                cell_stroke(painter, r, &resp, dragging);
                continue;
            }
            match c.name.as_str() {
                "CONTROL_26490" => control_text(painter, r, c, c.fixed_label(), font * 1.05, Some(Color32::from_rgb(235, 225, 200))),
                "CONTROL_SubInven_X_Btn" => {
                    if button(ctx, ui, hud, assets, painter, r, c, "", font, true, "sub-inven-x").clicked() {
                        out.push(PetBagUi::CloseBag);
                    }
                }
                "CONTROL_WasteBasket" => waste(ctx, ui, hud, assets, painter, r, c, "sub-inven-waste", out),
                "CONTROL_26077" => {
                    if state_button(ctx, ui, hud, assets, painter, r, c, &view.auto_label, font * 0.85, true, "sub-inven-autopick").clicked() {
                        out.push(PetBagUi::AutoPick);
                    }
                }
                "CONTROL_SubInv_Grid" => {
                    let [gx, gy, cw, ch] = c.grid.unwrap_or([2.0, 2.0, 50.0, 50.0]);
                    let cols = ((c.size[0] + gx) / (cw + gx)).floor().max(1.0) as i16;
                    let rows = ((c.size[1] + gy) / (ch + gy)).floor().max(1.0) as i16;
                    for row in 0..rows {
                        for col in 0..cols {
                            let cell = layout.rect(origin, [c.pos[0] + col as f32 * (cw + gx), c.pos[1] + row as f32 * (ch + gy)], [cw, ch]);
                            let grid = [view.layer, row, col];
                            let resp = ui.interact(cell, egui::Id::new(("sub-inven-cell", grid)), egui::Sense::click_and_drag());
                            if let Some((item, name)) = view.items.iter().find(|(i, _)| i.grid == Some(grid)) {
                                item_cell(ctx, hud, assets, painter, &resp, cell, item, name, font, s, out);
                            }
                            if let Some(drag) = resp.dnd_release_payload::<ItemDrag>() {
                                out.push(PetBagUi::DropOnBag { drag: *drag, grid });
                            }
                            cell_stroke(painter, cell, &resp, dragging);
                        }
                    }
                }
                _ => {}
            }
        }
    });
}
