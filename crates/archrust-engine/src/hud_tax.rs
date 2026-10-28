//! Fenêtres des taxes d'Archonia (AgcmUITax, `UI_1024X768.ini`) et annonces d'AgcmUIArchlord :
//!
//! * `UI_TaxSteward` (`NPC_Dialog_Base.png`) : titre (`CTL_ST_CAPTION`, nom de l'intendant), texte `TAX_INTRO`
//!   (`CTL_ED_INTRO`), boutons « Control tax rate » (`CTL_BTN_MODIFY`) et « Tax Ledger » (`CTL_BTN_VIEW`) ;
//! * `UI_TaxModify` (`Tax_Modify_Ratio_Base.png`) : titre `TAX_MODIFY_CAPTION`, noms des régions
//!   (`CTL_ST_REG1…4`), taux saisis (`CTL_ED_TAX1…4`, `Tax_Ratio_Bar.png`), « Confirm Tax Rate », « Reset »,
//!   « Go Back » ;
//! * `UI_TaxView` (`Tax_View_Income_Base.png`) : titre `TAX_VIEW_CAPTION`, 5 lignes région / taux / recette (la 5ᵉ :
//!   tribut à l'ArchLord), recettes des régions, recette totale, date du dernier versement ;
//! * annonce du centre de l'écran (`FUN_1405e7a20` : `FUN_140854c80` à 19 % de la hauteur, fond `RegionInfo.png`).

use super::npc_ui::{button, window};
use super::portal_ui::{control_text, state_button, static_text};
use super::*;

pub(super) const TAX_WINDOWS: [&str; 3] = ["UI_TaxSteward", "UI_TaxModify", "UI_TaxView"];

/// Contenu variable d'une fenêtre de taxe.
#[derive(Default)]
pub struct TaxView<'a> {
    /// Textes des contrôles (`CtrlName` → texte).
    pub texts: Vec<(&'a str, String)>,
    /// Champs de saisie des taux (`CtrlName` → texte), chiffres seulement.
    pub edits: Vec<(&'a str, &'a mut String)>,
}

/// Texte multiligne d'un `Edit` (`<\n>` et `\n` en retour à la ligne), coupé à la largeur du contrôle.
fn edit_text(painter: &egui::Painter, r: Rect, text: &str, font: f32) {
    let text = text.replace("<\\n>", "\n");
    let galley = painter.layout(text, FontId::proportional(font), Color32::WHITE, r.width().max(10.0));
    painter.with_clip_rect(r).galley(r.min, galley, Color32::WHITE);
}

/// Fenêtre de taxe `name` : fond, textes fixes et variables, champs de saisie, boutons. Rend les contrôles cliqués.
pub fn draw_tax_window(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, name: &str, mut view: TaxView, out: &mut Vec<String>) {
    let Some(w) = window(hud, assets, name) else { return };
    super::npc_ui::frame(ctx, hud, assets, &w, &format!("archonia-tax-{name}"), |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            if let Some((_, text)) = view.edits.iter_mut().find(|(n, _)| *n == c.name) {
                if let Some((_, tex)) = c.textures.first() {
                    control_image(ctx, hud, assets, painter, r, c.size, tex, 1.0);
                }
                let edit = egui::TextEdit::singleline(&mut **text)
                    .id(egui::Id::new(("archonia-tax-edit", name, &c.name)))
                    .frame(false)
                    .char_limit(3)
                    .horizontal_align(egui::Align::Center)
                    .font(FontId::proportional(font))
                    .text_color(Color32::WHITE);
                ui.put(r, edit);
                text.retain(|ch| ch.is_ascii_digit() || ch == '-');
                continue;
            }
            if let Some((_, t)) = view.texts.iter().find(|(n, _)| *n == c.name) {
                if c.kind == 2 {
                    edit_text(painter, r.shrink(2.0 * s), t, font);
                } else {
                    control_text(painter, r, c, t, font, None);
                }
                continue;
            }
            match c.kind {
                3 => {
                    let label = static_text(c);
                    let resp = if c.button_states.is_some_and(|b| b[0] != 1) {
                        state_button(ctx, ui, hud, assets, painter, r, c, label, font, true, ("tax-btn", name, &c.name))
                    } else {
                        button(ctx, ui, hud, assets, painter, r, c, label, font * 0.95, true, ("tax-btn", name, &c.name))
                    };
                    if resp.clicked() {
                        out.push(c.name.clone());
                    }
                }
                1 | 2 => {
                    if let Some((_, tex)) = c.textures.first() {
                        control_image(ctx, hud, assets, painter, r, c.size, tex, 1.0);
                    }
                    let t = static_text(c);
                    if !t.is_empty() {
                        control_text(painter, r, c, t, font, None);
                    }
                }
                _ => {}
            }
        }
    });
}

/// Annonce du centre de l'écran (`FUN_1405e7a20`) : texte sur `RegionInfo.png`, à 19 % de la hauteur.
pub fn draw_announce(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, text: &str, alpha: f32) {
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let screen = ctx.screen_rect();
    let center = Pos2::new(screen.center().x, screen.min.y + screen.height() * 0.19);
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("archonia-archlord-announce")));
    let a = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
    if let Some((tex, orig)) = hud.image(ctx, assets, "RegionInfo.png") {
        let r = Rect::from_center_size(center, Vec2::new(orig[0], orig[1]) * s);
        painter.image(tex, r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::from_white_alpha(a));
    }
    shadow_text(&painter, center, Align2::CENTER_CENTER, text, (16.0 * s).max(11.0), Color32::from_rgba_unmultiplied(255, 235, 170, a));
}
