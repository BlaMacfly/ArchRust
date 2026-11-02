//! Fenêtres d'Archonia des actions sociales et de l'achat chez un PNJ (sha256 e9c7609b…, `UI_1024X768.ini`) :
//!
//! * `UI_SocialAction` « Emotes » (235 × 325, `Social_Base.png`, AgcmUISocialAction FUN_1406d27c0) : croix
//!   `CONTROL_2183`, titre `CONTROL_18952`, grille `CONTROL_14844` (`SocialActionGrid`, cases de 50 px, 4 par
//!   rangée) remplie par `Ini\SocialAction.txt` (icône de la colonne `filename`, objet de grille de type 0x40
//!   d'identifiant serial − 1) ; double-clic ou clic droit (`CBSocialActionDLClick` / `RClick`, FUN_1406d2610) :
//!   l'action.
//! * `UI_SHRINEITEM_BUY_CONFIRM` (409 × 241, `Shrine_BuyConfirm.png`, AgcmUIShrineShop FUN_14067ea80), qu'Archonia
//!   ouvre pour tout achat chez un PNJ : nom (`CONTROL_ITEM_NAME`), icône (`CONTROL_ITEM_IMAGE`), quantité
//!   (`CONTROL_6387`), colonnes « Prix » / « Reste » des lignes charisme, Shrine Coins et or
//!   (`Shrine_Item*Price_Display`, `Shrine_Item*Rest_Display`) ; un reste négatif est écrit en rouge (+0x214) ;
//!   « Acheter » (`CONTROL_BUY`, `Shrine_ItemBuyClick`), « Annuler » (`CONTROL_CANCLE`) et la croix
//!   (`CONTROL_15530`).

use super::dialogs_ui::draw_dialog;
use super::npc_ui::{frame, window};
use super::portal_ui::{argb, control_text, state_button};
use super::*;

/// Fenêtres chargées avec le reste de l'interface.
pub(super) const SOCIAL_WINDOWS: [&str; 1] = ["UI_SocialAction"];

/// Action de la grille : icône (`SocialAction.txt`, colonne `filename`) et infobulle.
pub struct SocialView<'a> {
    pub actions: &'a [(String, String)],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocialUi {
    Close,
    /// Action choisie : type 0-16 (rang dans la grille).
    Play(u8),
}

/// Grille des actions sociales.
pub fn draw_social_action(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &SocialView, out: &mut Vec<SocialUi>) {
    let Some(w) = window(hud, assets, "UI_SocialAction") else { return };
    frame(ctx, hud, assets, &w, "archonia-social-action", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match c.name.as_str() {
                "CONTROL_2183" => {
                    if state_button(ctx, ui, hud, assets, painter, r, c, "", font, true, "social-x").clicked() {
                        out.push(SocialUi::Close);
                    }
                }
                "CONTROL_18952" => control_text(painter, r, c, c.fixed_label(), font, None),
                "CONTROL_14844" => {
                    let [gx, gy, cw, ch] = c.grid.unwrap_or([2.0, 2.0, 50.0, 50.0]);
                    let cols = ((c.size[0] + gx) / (cw + gx)).floor().max(1.0) as usize;
                    for (i, (icon, tip)) in view.actions.iter().enumerate() {
                        let (row, col) = (i / cols, i % cols);
                        let cell = layout.rect(origin, [c.pos[0] + col as f32 * (cw + gx), c.pos[1] + row as f32 * (ch + gy)], [cw, ch]);
                        if !cell.intersects(r.expand(1.0)) {
                            break;
                        }
                        if !icon.is_empty() {
                            control_image(ctx, hud, assets, painter, cell, [cw, ch], icon, 1.0);
                        }
                        let resp = ui.interact(cell, egui::Id::new(("social-cell", i)), egui::Sense::click());
                        if resp.double_clicked() || resp.secondary_clicked() {
                            out.push(SocialUi::Play(i as u8));
                        }
                        if resp.hovered() {
                            painter.rect_stroke(cell, 2.0, egui::Stroke::new(1.0_f32, Color32::from_rgb(120, 200, 255)), egui::StrokeKind::Inside);
                            if !tip.is_empty() {
                                super::item_tooltip(ctx, &[(tip.clone(), Color32::WHITE)], "");
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    });
}

/// Contenu d'`UI_SHRINEITEM_BUY_CONFIRM` : prix déjà multipliés par la quantité, restes (porté − prix).
pub struct ShrineBuyView {
    pub name: String,
    pub tid: i32,
    pub count: i64,
    pub charisma_price: i64,
    pub charisma_rest: i64,
    pub coin_price: i64,
    pub coin_rest: i64,
    pub gold_price: i64,
    pub gold_rest: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShrineBuyUi {
    Buy,
    Cancel,
}

/// Confirmation d'un achat chez un PNJ.
pub fn draw_shrine_buy_confirm(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &ShrineBuyView, out: &mut Vec<ShrineBuyUi>) {
    let red = argb(0xFF_FF_00_00);
    let mut icon_at = None;
    let mut count_at = None;
    let answer = draw_dialog(ctx, hud, assets, "UI_SHRINEITEM_BUY_CONFIRM", |_, _, painter, c, r, font| {
        let value = |v: i64| v.to_string();
        let (text, color) = match c.name.as_str() {
            "CONTROL_ITEM_NAME" => (view.name.clone(), None),
            "CONTROL_ITEM_IMAGE" => {
                icon_at = Some(r);
                return true;
            }
            // Quantité sur le coin de l'icône (rien pour un exemplaire), écrite après elle.
            "CONTROL_6387" => {
                count_at = Some((r, c.clone(), font));
                return true;
            }
            "CONTROL_CHARISMA_PRICE" => (value(view.charisma_price), None),
            "CONTROL_SHRINECOIN_PRICE" => (value(view.coin_price), None),
            "CONTROL_GHELD_PRICE" => (value(view.gold_price), None),
            "CONTROL_CHARISMA_REST" => (value(view.charisma_rest), (view.charisma_rest < 0).then_some(red)),
            "CONTROL_SHRINECOIN_REST" => (value(view.coin_rest), (view.coin_rest < 0).then_some(red)),
            "CONTROL_GHELD_REST" => (value(view.gold_rest), (view.gold_rest < 0).then_some(red)),
            // Textes fixes (« Prix », « Reste », lignes, question) : alignement de leur `CtrlFont`.
            _ if c.kind == 1 => (c.fixed_label().to_string(), None),
            _ => return false,
        };
        if !text.is_empty() {
            control_text(painter, r, c, &text, font * 0.95, color);
        }
        true
    });
    // Icône dessinée après coup : l'aide d'icône demande le contexte, pris par la fenêtre.
    if let Some(r) = icon_at {
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new(("archonia-dialog", "UI_SHRINEITEM_BUY_CONFIRM"))));
        super::items_ui::icon(ctx, hud, assets, &painter, r, Some(view.tid), Color32::WHITE);
        if let (true, Some((cr, c, font))) = (view.count > 1, &count_at) {
            control_text(&painter, *cr, c, &view.count.to_string(), font * 0.95, None);
        }
    }
    let Some(answer) = answer else { return };
    match answer.clicked.as_deref() {
        Some("CONTROL_BUY") => out.push(ShrineBuyUi::Buy),
        Some("CONTROL_CANCLE" | "CONTROL_15530") => out.push(ShrineBuyUi::Cancel),
        _ if answer.escape => out.push(ShrineBuyUi::Cancel),
        _ => {}
    }
}
