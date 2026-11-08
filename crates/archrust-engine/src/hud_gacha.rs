//! Gacha d'Archonia (« Fortune Gate ») : fenêtre `UI_Gacha` d'`UI_1024X768.ini` (fond `Gacha_background.png`, bande
//! `GachaWindow` d'AcUIGacha, nom de l'objet au centre, texte du coût, boutons « Start » / « Close » et croix).
//!
//! Bande (rendu d'AcUIGacha, `FUN_140880df0`) : six icônes de 50 × 50 au plus, la `k`-ième en `k × 50 − (p mod 50)`
//! du contrôle, dessinées si elles tiennent entre 0 et 320 dans la fenêtre ; puis `Gacha_cover.png` (texture 1 du
//! contrôle) en (0, 68) de la fenêtre, 316 × 95 ; tirage fini : `Gacha_zoom.png` (texture 2) en (107, 66) à sa taille
//! et l'icône tirée en (120, 79) à la sienne. Les demandes reviennent au moteur en [`GachaUi`] (`app_gacha.rs`).

use super::convert_ui::edit_box;
use super::items_ui::icon;
use super::npc_ui::{button, window};
use super::*;

/// Fenêtre du gacha, chargée avec le HUD (libellés d'`ini_fr`).
pub(super) const GACHA_WINDOWS: [&str; 1] = ["UI_Gacha"];
/// Largeur d'une case de la bande (`0x32`).
pub const CELL: f32 = 50.0;

/// Ce que montre `UI_Gacha`.
pub struct GachaView<'a> {
    /// TID de la bande.
    pub band: &'a [i32],
    /// Position de la bande, en pixels.
    pub offset: f32,
    /// Nom de l'objet au centre (`Gacha_ItemName`).
    pub item_name: &'a str,
    /// Texte du coût (`Gacha_TextData`).
    pub notice: &'a str,
    /// Objet tiré, une fois la bande arrêtée.
    pub won: Option<i32>,
    /// Bande en train de tourner : « Start » et « Close » sans effet.
    pub rolling: bool,
}

/// Demandes d'`UI_Gacha`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GachaUi {
    Roll,
    Close,
}

fn fixed(c: &UiControl) -> &str {
    c.display.as_deref().and_then(|d| d.strip_prefix("1:")).unwrap_or("")
}

/// Rang dans la bande des icônes visibles et leur abscisse dans le contrôle (`FUN_140880df0`).
pub fn visible_cells(len: usize, offset: f32, control_x: f32, window_w: f32) -> Vec<(usize, f32)> {
    if len == 0 {
        return Vec::new();
    }
    let p = offset.max(0.0);
    let first = (p / CELL).floor() as usize % len;
    let shift = p % CELL;
    (0..6)
        .map(|k| ((first + k) % len, k as f32 * CELL - shift))
        .filter(|(_, x)| control_x + x >= 0.0 && control_x + x + CELL < window_w)
        .collect()
}

/// Fenêtre du gacha.
pub fn draw_gacha(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: GachaView, out: &mut Vec<GachaUi>) {
    let Some(w) = window(hud, assets, "UI_Gacha") else { return };
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let size = Vec2::new(w.size[0], w.size[1]) * s;
    egui::Area::new(egui::Id::new("archonia-gacha"))
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
                    (16, _) => {
                        let clip = painter.with_clip_rect(layout.rect(origin, [0.0, 0.0], w.size));
                        for (k, x) in visible_cells(view.band.len(), view.offset, c.pos[0], w.size[0]) {
                            let cell = layout.rect(origin, [c.pos[0] + x, c.pos[1]], [CELL, CELL]);
                            icon(ctx, hud, assets, &clip, cell, Some(view.band[k]), Color32::WHITE);
                        }
                        let tex = |i: u32| c.textures.iter().find(|(n, _)| *n == i).map(|(_, t)| t.clone());
                        if let Some(cover) = tex(1) {
                            control_image(ctx, hud, assets, &painter, layout.rect(origin, [0.0, 68.0], [316.0, 95.0]), [316.0, 95.0], &cover, 1.0);
                        }
                        if let (Some(tid), Some(zoom)) = (view.won, tex(2)) {
                            if let Some((_, orig)) = hud.image(ctx, assets, &zoom) {
                                control_image(ctx, hud, assets, &painter, layout.rect(origin, [107.0, 66.0], orig), orig, &zoom, 1.0);
                            }
                            let native = icon_size(hud, tid).unwrap_or([64.0, 64.0]);
                            icon(ctx, hud, assets, &painter, layout.rect(origin, [120.0, 79.0], native), Some(tid), Color32::WHITE);
                        }
                    }
                    (2, "GACHA_ITEM_DISPLAY") => shadow_text(&painter, r.center(), Align2::CENTER_CENTER, view.item_name, font, Color32::WHITE),
                    (2, "GACHA_INFO_TEXT") => edit_box(&painter, r, view.notice, font),
                    (1, _) => shadow_text(&painter, r.center(), Align2::CENTER_CENTER, fixed(c), font, Color32::WHITE),
                    (3, name) => {
                        let action = match name {
                            "CONTROL_GACHA" => Some(GachaUi::Roll),
                            "CONTROL_CLOSEGACHA" | "GACHA_COMMON_CLOSE_BUTTON" => Some(GachaUi::Close),
                            _ => None,
                        };
                        if button(ctx, ui, hud, assets, &painter, r, c, fixed(c), font, !view.rolling, ("gacha-btn", name)).clicked() {
                            out.extend(action);
                        }
                    }
                    _ => {}
                }
            }
        });
}

/// Taille d'origine de l'icône d'un objet, une fois chargée.
fn icon_size(hud: &HudState, tid: i32) -> Option<[f32; 2]> {
    let name = hud.item_icons.as_ref()?.get(&tid)?;
    hud.images.get(&format!("icon:{name}"))?.as_ref().map(|i| i.orig)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cases_visibles() {
        // Bande de 10 objets arrêtée sur le rang 4 : `p = (7 × 10 − 2 + 4) × 50`, le rang au centre (k = 2) est le 4.
        let p = (7 * 10 - 2 + 4) as f32 * CELL;
        let cells = visible_cells(10, p, 33.0, 320.0);
        assert_eq!(cells.first(), Some(&(2, 0.0)));
        assert_eq!(cells.get(2), Some(&(4, 100.0)));
        // 33 + x + 50 < 320 : cinq icônes arrêtée, six à mi-case (la première déborde à gauche du contrôle).
        assert_eq!(cells.len(), 5);
        let half = visible_cells(10, 25.0, 33.0, 320.0);
        assert_eq!((half[0], half.len()), ((0, -25.0), 6));
        assert!(visible_cells(0, 0.0, 33.0, 320.0).is_empty());
    }
}
