//! Réparation et usure d'Archonia : fenêtres `UI_Repair` (grille de 5 × 4 cases, coût, or du sac,
//! boutons Réparer / Fermer) et `UI_EquipStatus` (silhouette, parties allumées sous 21 % de
//! durabilité), d'AgcmUIEventItemRepair et d'AgcmUIItem.
//!
//! Tout vient d'`UI_1024X768.ini` (fond `Repair_Base.png`, textures `EquipStatus_*.png`, libellés
//! d'`ini_fr`) ; les demandes reviennent au moteur en [`RepairUi`] (voir `app_repair.rs`).

use super::convert_ui::edit_box;
use super::items_ui::{icon, ItemDrag};
use super::npc_ui::{button, window};
use super::*;

/// Fenêtres chargées avec le HUD.
pub(super) const REPAIR_WINDOWS: [&str; 2] = ["UI_Repair", "UI_EquipStatus"];
/// Cases de la grille (`Grid=0:0:2:2:50:50` dans 206 × 258 : 4 colonnes, 5 lignes).
const COLS: usize = 4;
const CELLS: usize = 20;
/// Texture posée sur une case d'objet à durabilité nulle (drapeau +0x40 de l'élément de grille,
/// `durability_zero` du gestionnaire d'interface, FUN_1404e75e0).
pub(super) const DURABILITY_ZERO: &str = "DurabilityZero.png";

/// Ce que montre `UI_Repair`.
pub struct RepairView {
    /// Objets posés : TID et quantité.
    pub grid: [Option<(i32, i32)>; CELLS],
    /// Coût total (`ItemRepair_Cost_Display`, `%I64d`).
    pub cost: i64,
    /// Or du sac (`ItemRepair_InventoryMoney_Display`).
    pub money: i64,
    /// Demande en cours (+0x173c) : grille et bouton bloqués.
    pub busy: bool,
}

/// Demandes d'`UI_Repair`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairUi {
    Close,
    Repair,
    /// Objet du sac lâché sur une case (ligne × 4 + colonne), `ItemRepair_MoveItem`.
    Drop { cell: usize, drag: ItemDrag },
    /// Clic droit sur une case : l'objet en sort (FUN_140651b10).
    Remove(usize),
}

fn fixed(c: &UiControl) -> &str {
    c.display.as_deref().and_then(|d| d.strip_prefix("1:")).unwrap_or("")
}

/// Fenêtre de réparation.
pub fn draw_repair(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: RepairView, out: &mut Vec<RepairUi>) {
    let Some(w) = window(hud, assets, "UI_Repair") else { return };
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let size = Vec2::new(w.size[0], w.size[1]) * s;
    let dragging = egui::DragAndDrop::payload::<ItemDrag>(ctx).map(|p| *p);
    egui::Area::new(egui::Id::new("archonia-repair"))
        .default_pos(layout.window_origin(&w))
        .movable(true)
        .constrain(true)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let origin = rect.min;
            let painter = ui.painter().clone();
            if let Some((_, tex)) = w.textures.first() {
                control_image(ctx, hud, assets, &painter, rect, w.size, tex, 1.0);
            }
            let font = (11.0 * s).max(9.0);
            for c in &w.controls {
                let r = layout.rect(origin, c.pos, c.size);
                let display = c.display.as_deref().unwrap_or("");
                match c.kind {
                    1 if display.contains("ItemRepair_Cost_Data") => {
                        shadow_text(&painter, r.center(), Align2::CENTER_CENTER, &view.cost.to_string(), font, Color32::WHITE);
                    }
                    1 if display.contains("ItemRepair_InventoryMoney_Data") => {
                        shadow_text(&painter, r.center(), Align2::CENTER_CENTER, &view.money.to_string(), font, Color32::WHITE);
                    }
                    1 => shadow_text(&painter, r.center(), Align2::CENTER_CENTER, fixed(c), font, Color32::WHITE),
                    2 => edit_box(&painter, r, fixed(c), font),
                    8 => {
                        let [gx, gy, cw, ch] = c.grid.unwrap_or([2.0, 2.0, 50.0, 50.0]);
                        for cell in 0..CELLS {
                            let (row, col) = ((cell / COLS) as f32, (cell % COLS) as f32);
                            let rr = layout.rect(origin, [c.pos[0] + col * (cw + gx), c.pos[1] + row * (ch + gy)], [cw, ch]);
                            let resp = ui.interact(rr, egui::Id::new(("repair-cell", cell)), egui::Sense::click());
                            if let Some((tid, count)) = view.grid[cell] {
                                icon(ctx, hud, assets, &painter, rr, Some(tid), Color32::WHITE);
                                if count > 1 {
                                    shadow_text(&painter, rr.right_bottom() - Vec2::new(3.0, 2.0) * s, Align2::RIGHT_BOTTOM, &count.to_string(), font, Color32::WHITE);
                                }
                                if resp.secondary_clicked() && !view.busy {
                                    out.push(RepairUi::Remove(cell));
                                }
                            }
                            if let Some(drag) = resp.dnd_release_payload::<ItemDrag>() {
                                if !drag.bank && !view.busy {
                                    out.push(RepairUi::Drop { cell, drag: *drag });
                                }
                            }
                            let target = dragging.is_some() && resp.contains_pointer();
                            if resp.hovered() || target {
                                let color = if target { Color32::from_rgb(255, 210, 110) } else { Color32::from_rgb(120, 200, 255) };
                                painter.rect_stroke(rr, 2.0, egui::Stroke::new(1.0_f32, color), egui::StrokeKind::Inside);
                            }
                        }
                    }
                    3 => {
                        // `Action=3:ItemRepair_RepairReq` ; les autres boutons ferment la fenêtre
                        // (`Action=1:2:0:1:UI_Repair`).
                        let repair = c.action.as_deref() == Some("ItemRepair_RepairReq");
                        let action = if repair { RepairUi::Repair } else { RepairUi::Close };
                        if button(ctx, ui, hud, assets, &painter, r, c, fixed(c), font, !(repair && view.busy), ("repair-btn", c.name.as_str())).clicked() {
                            out.push(action);
                        }
                    }
                    _ => {}
                }
            }
        });
}

/// Silhouette `UI_EquipStatus` : fond, puis les contrôles « On » (`CONTROL_HEAD`, `_BODY`…) des
/// parties nommées dans `lit`. Rien si aucune partie n'est allumée (`EquipStatusClose`).
pub fn draw_equip_status(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, lit: &[&str]) {
    if lit.is_empty() {
        return;
    }
    let Some(w) = window(hud, assets, "UI_EquipStatus") else { return };
    let layout = Layout::new(ctx.screen_rect());
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("archonia-equip-status")));
    let origin = layout.window_origin(&w);
    let rect = Rect::from_min_size(origin, Vec2::new(w.size[0], w.size[1]) * layout.scale);
    if let Some((_, tex)) = w.textures.first() {
        control_image(ctx, hud, assets, &painter, rect, w.size, tex, 1.0);
    }
    for c in w.controls.iter().filter(|c| lit.contains(&c.name.as_str())) {
        let r = layout.rect(origin, c.pos, c.size);
        if let Some((_, tex)) = c.textures.first() {
            control_image(ctx, hud, assets, &painter, r, c.size, tex, 1.0);
        }
    }
}

/// Marque « durabilité nulle » sur une case de grille.
pub(super) fn durability_mark(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, cell: Rect) {
    match hud.image(ctx, assets, DURABILITY_ZERO) {
        Some((tex, _)) => {
            painter.image(tex, cell.shrink(1.0), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::from_white_alpha(150));
        }
        None => {
            painter.rect_filled(cell.shrink(1.0), 2.0, Color32::from_rgba_unmultiplied(255, 90, 90, 90));
        }
    }
}
