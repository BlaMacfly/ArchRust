//! Menu d'une invocation ciblée (AgcmUICharacter d'Archonia), d'après `UI_1024X768.ini` :
//!
//! * `UI_TargetSummonsMenu` (93 × 28, `TargetMenuBase.png`) : bouton `CONTROL_1032` (`TargetMenuButton_a…d`) à côté
//!   de la cible quand c'est une invocation du joueur ; son texte est la propension (`SummonsPropensityUD`,
//!   `FUN_1404f8b20` : `SummonsPropensityAttack`, `…Defense`, `…Shadow` d'uimessage) ; clic : `ToggleTargetSummonsMenu`
//!   ouvre ou ferme `UI_TargetSummonsMenu2` ;
//! * `UI_TargetSummonsMenu2` (92 × 84) : boutons `SummonsMenu2_Attack`, `_Defense`, `_Shadow` (`CB_Button_Small*`,
//!   « Attack Type »…) qui ferment le menu et demandent la propension (`SummonsPropensity*Click`).
//!
//! Les demandes reviennent au moteur en [`SummonsUi`] (voir `app_summons.rs`).

use super::npc_ui::{button, window};
use super::portal_ui::static_text;
use super::*;

/// Fenêtres du menu des invocations, chargées avec le HUD (textes fixes d'`ini_fr`).
pub(super) const SUMMONS_WINDOWS: [&str; 2] = ["UI_TargetSummonsMenu", "UI_TargetSummonsMenu2"];

/// Demandes du menu d'une invocation ciblée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummonsUi {
    /// Bouton de `UI_TargetSummonsMenu` (`ToggleTargetSummonsMenu`).
    Toggle,
    /// Bouton de `UI_TargetSummonsMenu2` : propension 0 attaque, 1 défense, 2 ombre.
    Propensity(i8),
    /// Échap : le second menu se ferme.
    Close,
}

/// Bouton de `UI_TargetSummonsMenu` portant `label` (propension) et, si `open`, les trois boutons de
/// `UI_TargetSummonsMenu2`.
pub fn draw_summons_menu(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, label: &str, open: bool, out: &mut Vec<SummonsUi>) {
    let Some(bar) = window(hud, assets, "UI_TargetSummonsMenu") else { return };
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let font = (10.0 * s).max(8.0);
    let origin = layout.window_origin(&bar);
    egui::Area::new(egui::Id::new("archonia-target-summons-menu"))
        .fixed_pos(origin)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(bar.size[0], bar.size[1]) * s, egui::Sense::hover());
            let painter = ui.painter().clone();
            if let Some((_, tex)) = bar.textures.first() {
                control_image(ctx, hud, assets, &painter, rect, bar.size, tex, 1.0);
            }
            for c in &bar.controls {
                let r = layout.rect(rect.min, c.pos, c.size);
                if button(ctx, ui, hud, assets, &painter, r, c, label, font, true, ("target-summons-menu", c.name.as_str())).clicked() {
                    out.push(SummonsUi::Toggle);
                }
            }
        });
    if !open {
        return;
    }
    let Some(w) = window(hud, assets, "UI_TargetSummonsMenu2") else { return };
    let at = layout.window_origin(&w);
    egui::Area::new(egui::Id::new("archonia-target-summons-menu2"))
        .fixed_pos(at)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w.size[0], w.size[1]) * s, egui::Sense::hover());
            let painter = ui.painter().clone();
            for (name, value) in [("SummonsMenu2_Attack", 0), ("SummonsMenu2_Defense", 1), ("SummonsMenu2_Shadow", 2)] {
                let Some(c) = w.control(name) else { continue };
                let r = layout.rect(rect.min, c.pos, c.size);
                if button(ctx, ui, hud, assets, &painter, r, c, static_text(c), font, true, ("target-summons-menu2", name)).clicked() {
                    out.push(SummonsUi::Propensity(value));
                }
            }
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        out.push(SummonsUi::Close);
    }
}
