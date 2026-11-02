//! Coiffeur d'Archonia (AgcmUICustomize) : fenêtres `UI_Customizing` (listes `CustomizeHairList` /
//! `CustomizeFaceList` de six lignes sur `CustomizeListBar.png`, textes de la coiffure, du visage et du total,
//! aperçu `CustomizePreview` (raster hors écran du personnage cloné, voir `app_customize.rs`) avec rotation et zoom, boutons « Purchase », « Reset », « Cancel ») et
//! `UI_CustomizeBuyConfirm` (« Cost : %d Gold »).
//!
//! Tout vient d'`UI_1024X768.ini` (fond `Customizing_Base.png`, libellés d'`ini_fr`) ; les demandes reviennent
//! au moteur en [`CustomizeUi`] (voir `app_customize.rs`).

use super::convert_ui::edit_box_aligned;
use super::dialogs_ui::draw_dialog;
use super::npc_ui::{button, frame, window};
use super::portal_ui::{control_text, static_text};
use super::*;

/// Fenêtres chargées avec le HUD.
pub(super) const CUSTOMIZE_WINDOWS: [&str; 2] = ["UI_Customizing", "UI_CustomizeBuyConfirm"];
/// Lignes visibles d'une liste (`List=6`).
pub const LIST_ROWS: usize = 6;

/// Ce que montre `UI_Customizing`.
pub struct CustomizeView<'a> {
    pub hairs: &'a [String],
    pub faces: &'a [String],
    pub hair: Option<usize>,
    pub face: Option<usize>,
    pub first_hair: usize,
    pub first_face: usize,
    /// Textes des contrôles d'édition (codes `<C…>` d'Archonia) : coiffure, visage, total.
    pub hair_text: &'a str,
    pub face_text: &'a str,
    pub total_text: &'a str,
    /// « Purchase » actif (`CustomizeIsActiveBuy`).
    pub can_buy: bool,
    /// Raster de l'aperçu hors écran (`AgcmCustomizeRender`, voir `app_customize.rs`), dessiné dans le
    /// contrôle de type 16 « CustomizePreview » (`CONTROL_4835`, 209 × 370).
    pub preview: Option<egui::TextureId>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CustomizeUi {
    SelectHair(usize),
    SelectFace(usize),
    /// Défilement d'une liste (coiffure si `true`) d'une ligne vers le haut (-1) ou le bas (+1).
    Scroll { hair: bool, delta: i32 },
    Buy,
    Reset,
    Close,
    /// Bouton de rotation ou de zoom tenu : sens (-1 gauche / arrière, +1 droite / avant).
    Rotate(f32),
    Zoom(f32),
}

/// Liste de six lignes : nom, barre `CustomizeListBar.png` sur la ligne choisie ou survolée.
#[allow(clippy::too_many_arguments)]
fn list(ctx: &egui::Context, ui: &mut egui::Ui, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, bar: &UiControl, r: Rect, names: &[String], first: usize, selected: Option<usize>, font: f32, hair: bool, out: &mut Vec<CustomizeUi>) {
    let row_h = r.height();
    for i in 0..LIST_ROWS {
        let index = first + i;
        let Some(name) = names.get(index) else { break };
        let rr = r.translate(Vec2::new(0.0, row_h * i as f32));
        let resp = ui.interact(rr, egui::Id::new(("customize-row", hair, i)), egui::Sense::click());
        if selected == Some(index) || resp.hovered() {
            if let Some((_, t)) = bar.textures.first() {
                control_image(ctx, hud, assets, painter, rr, bar.size, t, 1.0);
            }
        }
        let color = if selected == Some(index) { Color32::WHITE } else { Color32::from_rgb(220, 220, 220) };
        shadow_text(painter, rr.left_center() + Vec2::new(6.0, 0.0), Align2::LEFT_CENTER, name, font, color);
        if resp.clicked() {
            out.push(if hair { CustomizeUi::SelectHair(index) } else { CustomizeUi::SelectFace(index) });
        }
        if resp.hovered() {
            let dy = ui.input(|i| i.raw_scroll_delta.y);
            if dy != 0.0 {
                out.push(CustomizeUi::Scroll { hair, delta: if dy > 0.0 { -1 } else { 1 } });
            }
        }
    }
}

/// Fenêtre du coiffeur.
pub fn draw_customizing(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &CustomizeView, out: &mut Vec<CustomizeUi>) {
    let Some(w) = window(hud, assets, "UI_Customizing") else { return };
    frame(ctx, hud, assets, &w, "archonia-customizing", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match c.name.as_str() {
                "CONTROL_29107" | "CONTROL_3910" | "CONTROL_16177" | "CONTROL_17277" => control_text(painter, r, c, static_text(c), font, None),
                // Rangées : la barre-bouton de chaque liste donne la place d'une ligne.
                "CONTROL_17928" => list(ctx, ui, hud, assets, painter, c, r, view.hairs, view.first_hair, view.hair, font, true, out),
                "CONTROL_24801" => list(ctx, ui, hud, assets, painter, c, r, view.faces, view.first_face, view.face, font, false, out),
                "CONTROL_25735" => edit_box_aligned(painter, r, view.hair_text, font, egui::Align::LEFT),
                "CONTROL_7163" => edit_box_aligned(painter, r, view.face_text, font, egui::Align::LEFT),
                "CONTROL_9217" => edit_box_aligned(painter, r, view.total_text, font, egui::Align::LEFT),
                // Aperçu : raster du personnage cloné (fond transparent), sous les boutons et l'étiquette.
                "CONTROL_4835" => {
                    if let Some(tex) = view.preview {
                        painter.image(tex, r, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                    }
                }
                // Étiquette `CustomizePreview.png` posée sur l'aperçu.
                "CONTROL_31341" => {
                    if let Some((_, t)) = c.textures.first() {
                        control_image(ctx, hud, assets, painter, r, c.size, t, 1.0);
                    }
                }
                "CONTROL_13122" | "CONTROL_32728" | "CONTROL_8223" | "CONTROL_17527" => {
                    let hair = matches!(c.name.as_str(), "CONTROL_13122" | "CONTROL_32728");
                    let delta = if matches!(c.name.as_str(), "CONTROL_13122" | "CONTROL_8223") { -1 } else { 1 };
                    if button(ctx, ui, hud, assets, painter, r, c, "", font, true, ("customize-scroll", c.name.as_str())).clicked() {
                        out.push(CustomizeUi::Scroll { hair, delta });
                    }
                }
                // Rotation et zoom : actifs tant que le bouton est tenu (`CustomizeRotateEnd` / `CustomizeZoomEnd` au
                // relâchement).
                "CONTROL_31228" | "CONTROL_6406" | "CONTROL_22477" | "CONTROL_27175" => {
                    let resp = button(ctx, ui, hud, assets, painter, r, c, "", font, true, ("customize-view", c.name.as_str()));
                    if resp.is_pointer_button_down_on() {
                        out.push(match c.name.as_str() {
                            "CONTROL_31228" => CustomizeUi::Rotate(-1.0),
                            "CONTROL_6406" => CustomizeUi::Rotate(1.0),
                            "CONTROL_27175" => CustomizeUi::Zoom(1.0),
                            _ => CustomizeUi::Zoom(-1.0),
                        });
                    }
                }
                "CONTROL_5228" => {
                    if button(ctx, ui, hud, assets, painter, r, c, static_text(c), font, view.can_buy, ("customize-btn", "buy")).clicked() {
                        out.push(CustomizeUi::Buy);
                    }
                }
                "CONTROL_13022" => {
                    if button(ctx, ui, hud, assets, painter, r, c, static_text(c), font, true, ("customize-btn", "reset")).clicked() {
                        out.push(CustomizeUi::Reset);
                    }
                }
                "CONTROL_30832" | "CONTROL_26382" => {
                    let label = if c.name == "CONTROL_26382" { "" } else { static_text(c) };
                    if button(ctx, ui, hud, assets, painter, r, c, label, font, true, ("customize-btn", c.name.as_str())).clicked() {
                        out.push(CustomizeUi::Close);
                    }
                }
                _ => {}
            }
        }
    });
}

/// `UI_CustomizeBuyConfirm` : `Some(true)` « Confirm », `Some(false)` « Cancel » ou croix.
pub fn draw_customize_confirm(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, price_text: &str) -> Option<bool> {
    let res = draw_dialog(ctx, hud, assets, "UI_CustomizeBuyConfirm", |_, _, painter, c, r, font| {
        if c.name == "CONTROL_24097" {
            shadow_text(painter, r.center(), Align2::CENTER_CENTER, price_text, font, Color32::WHITE);
            return true;
        }
        false
    })?;
    match res.clicked.as_deref() {
        Some("CONTROL_8150") => Some(true),
        Some("CONTROL_10107" | "CONTROL_2856") => Some(false),
        _ if res.escape => Some(false),
        _ => None,
    }
}
