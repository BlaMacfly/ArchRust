//! Fenêtres d'Archonia de la résurrection par objet et de l'oubli des compétences (`UI_1024X768.ini`) :
//!
//! * bouton « Use revival item » de `UI_Resurrection` (`CONTROL_23965`, `Item_UseReverseOrb`, actif selon
//!   `IsActiveReverseOrb`), placé comme la fenêtre de `draw_resurrection_with` (centrée) ;
//! * `UI_ResurrectTownConfirm` (« Do you want to revive in the town? », Confirm `CONTROL_20697` →
//!   `CharResurrectionToTown`, Cancel `CONTROL_27090`, croix `CONTROL_25749`) : les quatre lignes de perte d'XP
//!   (`CONTROL_6185`, `CONTROL_32279`, `CONTROL_28072`, `CONTROL_8508`) ont `CtrlShow=1:IsExpPenalty`, et
//!   `IsExpPenalty` d'Archonia rend toujours 0 (`FUN_1400ef910`) : elles ne sont jamais dessinées ;
//! * `UI_SkillInitializeConfirm` : texte `Skill_ConfirmInitialize` dans `CONTROL_12049`
//!   (`Skill_SetConfirmInitializeText`), OK `CONTROL_24520` → `Skill_InitializeOK`, Cancel `CONTROL_30135`, croix
//!   `CONTROL_12593`.
//!
//! La boîte `SkillInitConfirm` de la Gem of origin est la confirmation modale ordinaire (`confirm_box`).

use super::dialogs_ui::draw_dialog;
use super::npc_ui::button;
use super::*;

/// Fenêtres chargées avec le HUD.
pub(super) const CONSUMABLE_WINDOWS: [&str; 2] = ["UI_ResurrectTownConfirm", "UI_SkillInitializeConfirm"];

/// Lignes de perte d'XP d'`UI_ResurrectTownConfirm` (`CtrlShow=1:IsExpPenalty ExpPenalty`), masquées.
const EXP_PENALTY_LINES: [&str; 4] = ["CONTROL_6185", "CONTROL_32279", "CONTROL_28072", "CONTROL_8508"];

/// Bouton « Use revival item » (`CONTROL_23965`) de `UI_Resurrection`, grisé si `active` est faux. Rend `true` au
/// clic.
pub fn draw_resurrection_orb(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, active: bool) -> bool {
    let Some(w) = hud.windows(assets).iter().find(|w| w.name == "UI_Resurrection").cloned() else { return false };
    let Some(c) = w.control("CONTROL_23965").cloned() else { return false };
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale * 1.25;
    let origin = layout.screen.center() - Vec2::new(w.size[0], w.size[1]) * s / 2.0 - Vec2::new(0.0, 80.0 * layout.scale);
    let r = Rect::from_min_size(origin + Vec2::new(c.pos[0], c.pos[1]) * s, Vec2::new(c.size[0], c.size[1]) * s);
    let label = c.display.as_deref().and_then(|d| d.strip_prefix("1:")).unwrap_or("").to_string();
    let mut clicked = false;
    egui::Area::new(egui::Id::new("archrust-resurrect-orb")).fixed_pos(r.min).order(egui::Order::Foreground).show(ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(r.size(), egui::Sense::hover());
        let painter = ui.painter().clone();
        clicked = button(ctx, ui, hud, assets, &painter, rect, &c, &label, (11.0 * s).max(9.0), active, "archrust-resurrect-orb-btn").clicked();
    });
    active && clicked
}

/// `UI_ResurrectTownConfirm` : `Some(true)` « Confirm », `Some(false)` « Cancel », croix ou Échap.
pub fn draw_resurrect_town_confirm(ctx: &egui::Context, hud: &mut HudState, assets: &Assets) -> Option<bool> {
    let res = draw_dialog(ctx, hud, assets, "UI_ResurrectTownConfirm", |_, _, _, c, _, _| EXP_PENALTY_LINES.contains(&c.name.as_str()))?;
    match res.clicked.as_deref() {
        Some("CONTROL_20697") => Some(true),
        Some("CONTROL_27090" | "CONTROL_25749") => Some(false),
        _ if res.enter => Some(true),
        _ if res.escape => Some(false),
        _ => None,
    }
}

/// `UI_SkillInitializeConfirm` avec le texte `text` (`___` : retour à la ligne) : `Some(true)` « OK »,
/// `Some(false)` « Cancel », croix ou Échap.
pub fn draw_skill_init_confirm(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, text: &str) -> Option<bool> {
    let res = draw_dialog(ctx, hud, assets, "UI_SkillInitializeConfirm", |_, _, painter, c, r, font| {
        if c.name != "CONTROL_12049" {
            return false;
        }
        let galley = painter.layout(text.replace("___", "\n"), FontId::proportional(font), Color32::WHITE, r.width());
        let at = Pos2::new(r.center().x - galley.size().x / 2.0, r.center().y - galley.size().y / 2.0);
        painter.galley(at, galley, Color32::WHITE);
        true
    })?;
    match res.clicked.as_deref() {
        Some("CONTROL_24520") => Some(true),
        Some("CONTROL_30135" | "CONTROL_12593") => Some(false),
        _ if res.escape => Some(false),
        _ => None,
    }
}
