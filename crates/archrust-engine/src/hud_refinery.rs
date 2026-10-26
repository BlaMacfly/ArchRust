//! Sphère d'alchimie d'Archonia : fenêtres `UI_Refinery` (grille de 4 × 5 cases, objet produit,
//! boutons Raffiner / Répéter / Réinitialiser / Recette / Fermer), `UI_Refinery_Recipe` (liste
//! filtrable, détail des ingrédients, ingrédients en icônes) et `UI_Cooldown` (barre d'une seconde
//! d'AgcmUICooldown, titre `sysstr` 450 « En cours de raffinage »).
//!
//! Tout vient d'`UI_1024X768.ini` (fonds, boutons, grilles, libellés d'`ini_fr`) ; les demandes
//! reviennent au moteur en [`RefineryUi`] et [`RecipeUi`] (voir `app_refinery.rs`).

use super::convert_ui::{edit_box, edit_box_aligned};
use super::items_ui::{icon, ItemDrag};
use super::npc_ui::{button, window};
use super::*;

/// Fenêtres de la sphère d'alchimie, chargées avec le HUD (libellés d'`ini_fr`).
pub(super) const REFINERY_WINDOWS: [&str; 3] = ["UI_Refinery", "UI_Refinery_Recipe", "UI_Cooldown"];
/// Lignes de la liste des recettes (`List=22`, rangées de 20 px sur 381).
const RECIPE_ROWS: usize = 19;

/// Ce que montre `UI_Refinery`.
pub struct RefineryView<'a> {
    /// Texte de gauche (`sysstr` 449).
    pub main: &'a str,
    /// Sous-titre (`REFINE_SUB_TITLE_ITEM`) et message (`REFINE_SUB_NOTIFY_ITEM`, `REFINE_*_ITEM`).
    pub subtitle: &'a str,
    pub notify: &'a str,
    /// Cases de la grille : TID et quantité de l'objet posé.
    pub grid: [Option<(i32, i32)>; 20],
    /// Objet produit (`CTL_GRID_RESULT`).
    pub result: Option<i32>,
}

/// Demandes d'`UI_Refinery`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefineryUi {
    Close,
    Refine,
    Reset,
    Repeat,
    Recipe,
    /// Objet du sac lâché sur une case (ligne × 5 + colonne).
    Drop { cell: usize, drag: ItemDrag },
    /// Clic droit sur une case : la vider.
    Clear(usize),
}

/// Ce que montre `UI_Refinery_Recipe`.
pub struct RecipeView<'a> {
    /// Recettes affichées : nom de l'objet produit et couleur (gris si elle n'est pas réalisable).
    pub rows: &'a [(String, Color32)],
    pub selected: Option<usize>,
    pub first_row: usize,
    /// Détail de la recette choisie (codes `<H…>`).
    pub detail: &'a str,
    /// Ingrédients de la recette choisie : TID et quantité.
    pub ingredients: &'a [(i32, i32)],
    /// Texte de recherche (`CTL_EDIT_BASE`).
    pub search: &'a mut String,
}

/// Demandes d'`UI_Refinery_Recipe`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipeUi {
    Close,
    Select(usize),
    Refine,
    /// Défilement de la liste (lignes, négatif vers le haut).
    Scroll(i32),
}

fn fixed(c: &UiControl) -> &str {
    c.display.as_deref().and_then(|d| d.strip_prefix("1:")).unwrap_or("")
}

/// Fenêtre de raffinage.
pub fn draw_refinery(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: RefineryView, out: &mut Vec<RefineryUi>) {
    let Some(w) = window(hud, assets, "UI_Refinery") else { return };
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let size = Vec2::new(w.size[0], w.size[1]) * s;
    let dragging = egui::DragAndDrop::payload::<ItemDrag>(ctx).map(|p| *p);
    let busy = view.grid.iter().any(Option::is_some);
    egui::Area::new(egui::Id::new("archonia-refinery"))
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
                match (c.kind, c.name.as_str()) {
                    (2, "CTL_EDIT_MAIN") => edit_box(&painter, r, view.main, font),
                    (2, "CTL_EDIT_NOTIFY") => edit_box(&painter, r, view.notify, font),
                    (1, "CTL_ST_SUBTITLE") => shadow_text(&painter, r.center(), Align2::CENTER_CENTER, view.subtitle, font, Color32::WHITE),
                    (1, _) => shadow_text(&painter, r.center(), Align2::CENTER_CENTER, fixed(c), font, Color32::WHITE),
                    (8, "CTL_GRID_SOURCE") => {
                        let [gx, gy, cw, ch] = c.grid.unwrap_or([2.0, 2.0, 50.0, 50.0]);
                        for cell in 0..20 {
                            let (row, col) = ((cell / 5) as f32, (cell % 5) as f32);
                            let rr = layout.rect(origin, [c.pos[0] + col * (cw + gx), c.pos[1] + row * (ch + gy)], [cw, ch]);
                            let resp = ui.interact(rr, egui::Id::new(("refinery-cell", cell)), egui::Sense::click());
                            if let Some((tid, count)) = view.grid[cell] {
                                icon(ctx, hud, assets, &painter, rr, Some(tid), Color32::WHITE);
                                if count > 1 {
                                    shadow_text(&painter, rr.right_bottom() - Vec2::new(3.0, 2.0) * s, Align2::RIGHT_BOTTOM, &count.to_string(), font, Color32::WHITE);
                                }
                                if resp.secondary_clicked() {
                                    out.push(RefineryUi::Clear(cell));
                                }
                            }
                            if let Some(drag) = resp.dnd_release_payload::<ItemDrag>() {
                                if !drag.worn && !drag.bank {
                                    out.push(RefineryUi::Drop { cell, drag: *drag });
                                }
                            }
                            let target = dragging.is_some() && resp.contains_pointer();
                            if resp.hovered() || target {
                                let color = if target { Color32::from_rgb(255, 210, 110) } else { Color32::from_rgb(120, 200, 255) };
                                painter.rect_stroke(rr, 2.0, egui::Stroke::new(1.0_f32, color), egui::StrokeKind::Inside);
                            }
                        }
                    }
                    (8, "CTL_GRID_RESULT") => {
                        if view.result.is_some() {
                            icon(ctx, hud, assets, &painter, r, view.result, Color32::WHITE);
                        }
                    }
                    (3, name) => {
                        let (action, enabled) = match name {
                            "CTL_BTN_CLOSE" | "CTL_BTN_SYSCLOSE" => (Some(RefineryUi::Close), true),
                            "CTL_BTN_REFINE" => (Some(RefineryUi::Refine), busy),
                            "CTL_BTN_RESET" => (Some(RefineryUi::Reset), true),
                            "CTL_BTN_REPEAT" => (Some(RefineryUi::Repeat), true),
                            "CTL_BTN_RECIPE" => (Some(RefineryUi::Recipe), true),
                            // Flèches de la barre de défilement du texte (il tient sans défiler).
                            _ => (None, true),
                        };
                        if button(ctx, ui, hud, assets, &painter, r, c, fixed(c), font, enabled, ("refinery-btn", name)).clicked() {
                            out.extend(action);
                        }
                    }
                    (15, _) => {
                        if let Some((_, tex)) = c.textures.first() {
                            control_image(ctx, hud, assets, &painter, r, c.size, tex, 1.0);
                        }
                    }
                    _ => {}
                }
            }
        });
}

/// Fenêtre des recettes.
pub fn draw_recipes(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: RecipeView, out: &mut Vec<RecipeUi>) {
    let Some(w) = window(hud, assets, "UI_Refinery_Recipe") else { return };
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let size = Vec2::new(w.size[0], w.size[1]) * s;
    egui::Area::new(egui::Id::new("archonia-refinery-recipe"))
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
            let bar = w.control("CTL_BTN_SEL").and_then(|c| c.textures.first()).map(|(_, t)| t.clone());
            for c in &w.controls {
                let r = layout.rect(origin, c.pos, c.size);
                match (c.kind, c.name.as_str()) {
                    (2, "CTL_EDIT_BASE") => {
                        let edit = egui::TextEdit::singleline(view.search)
                            .id(egui::Id::new("refinery-recipe-search"))
                            .frame(false)
                            .font(FontId::proportional(font))
                            .text_color(Color32::WHITE);
                        ui.put(r, edit);
                    }
                    (2, "CTL_EDIT_SUB") => edit_box_aligned(&painter, r, view.detail, font, egui::Align::Min),
                    (1, _) => shadow_text(&painter, r.center(), Align2::CENTER_CENTER, fixed(c), font, Color32::WHITE),
                    (8, "CTL_GRID_SRC") => {
                        let [gx, gy, cw, ch] = c.grid.unwrap_or([2.0, 2.0, 50.0, 50.0]);
                        let cols = ((c.size[0] + gx) / (cw + gx)).floor().max(1.0) as usize;
                        for (i, (tid, count)) in view.ingredients.iter().enumerate().take(cols * 3) {
                            let (row, col) = ((i / cols) as f32, (i % cols) as f32);
                            let rr = layout.rect(origin, [c.pos[0] + col * (cw + gx), c.pos[1] + row * (ch + gy)], [cw, ch]);
                            icon(ctx, hud, assets, &painter, rr, Some(*tid), Color32::WHITE);
                            shadow_text(&painter, rr.right_bottom() - Vec2::new(3.0, 2.0) * s, Align2::RIGHT_BOTTOM, &count.to_string(), font, Color32::WHITE);
                        }
                    }
                    (10, "CTL_LIST_SEL") => {
                        // Lignes de 20 px ; la colonne de droite (15 px) est la barre de défilement.
                        let list = Rect::from_min_size(r.min, Vec2::new(r.width() - 16.0 * s, r.height()));
                        let resp = ui.interact(list, egui::Id::new("refinery-recipe-list"), egui::Sense::hover());
                        let wheel = ui.input(|i| i.raw_scroll_delta.y);
                        if resp.hovered() && wheel.abs() > 0.5 {
                            out.push(RecipeUi::Scroll(if wheel > 0.0 { -3 } else { 3 }));
                        }
                        for (k, (name, color)) in view.rows.iter().enumerate().skip(view.first_row).take(RECIPE_ROWS) {
                            let row = (k - view.first_row) as f32;
                            let rr = Rect::from_min_size(list.min + Vec2::new(0.0, row * 20.0 * s), Vec2::new(list.width(), 20.0 * s));
                            let line = ui.interact(rr, egui::Id::new(("refinery-recipe-row", k)), egui::Sense::click());
                            if view.selected == Some(k) || line.hovered() {
                                if let Some(t) = &bar {
                                    control_image(ctx, hud, assets, &painter, rr, [207.0, 20.0], t, 1.0);
                                }
                            }
                            shadow_text(&painter, rr.left_center() + Vec2::new(4.0 * s, 0.0), Align2::LEFT_CENTER, name, font, *color);
                            if line.clicked() {
                                out.push(RecipeUi::Select(k));
                            }
                        }
                    }
                    (3, "CTL_BTN_SEL") => {}
                    (3, name) => {
                        let action = match name {
                            "CTL_BTN_OK" => Some(RecipeUi::Refine),
                            "CTL_BTN_CLOSE" | "CTL_BTN_SYSCLOSE" => Some(RecipeUi::Close),
                            "CTL_PGUP" => Some(RecipeUi::Scroll(-(RECIPE_ROWS as i32))),
                            "CTL_PGDN" => Some(RecipeUi::Scroll(RECIPE_ROWS as i32)),
                            _ => None,
                        };
                        if button(ctx, ui, hud, assets, &painter, r, c, fixed(c), font, true, ("refinery-recipe-btn", name)).clicked() {
                            out.extend(action);
                        }
                    }
                    (15, _) => {
                        // Curseur de défilement placé selon la première ligne affichée.
                        let span = (view.rows.len().saturating_sub(RECIPE_ROWS)).max(1) as f32;
                        let track = 380.0 - 2.0 * 16.0 - c.size[1];
                        let y = (view.first_row as f32 / span).clamp(0.0, 1.0) * track;
                        let rr = Rect::from_min_size(r.min + Vec2::new(0.0, (y - (c.pos[1] - 58.0 - 16.0).max(0.0)) * s), r.size());
                        if let Some((_, tex)) = c.textures.first() {
                            control_image(ctx, hud, assets, &painter, rr, c.size, tex, 1.0);
                        }
                    }
                    _ => {}
                }
            }
        });
}

/// Barre d'attente d'AgcmUICooldown (`UI_Cooldown`) : `fraction` de 0 à 1, titre centré.
pub fn draw_cooldown(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, fraction: f32, title: &str) {
    let Some(w) = window(hud, assets, "UI_Cooldown") else { return };
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let origin = layout.window_origin(&w);
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("archonia-cooldown")));
    let rect = Rect::from_min_size(origin, Vec2::new(w.size[0], w.size[1]) * s);
    if let Some((_, tex)) = w.textures.first() {
        control_image(ctx, hud, assets, &painter, rect, w.size, tex, 1.0);
    }
    let font = (10.0 * s).max(8.0);
    for c in &w.controls {
        let r = layout.rect(origin, c.pos, c.size);
        match c.kind {
            9 => {
                if let Some((_, tex)) = c.textures.first() {
                    control_image(ctx, hud, assets, &painter, r, c.size, tex, fraction);
                }
            }
            1 => shadow_text(&painter, r.center(), Align2::CENTER_CENTER, title, font, Color32::WHITE),
            _ => {}
        }
    }
}
