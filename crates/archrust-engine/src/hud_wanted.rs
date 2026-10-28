//! Avis de recherche d'Archonia (AgcmUIWantedCriminal) : tableau `UI_WantedCriminal` (« Wanted Bulletin »,
//! 10 lignes `CTL_LISTITEM` : rang, criminel, prime cumulée, niveau, classe, en ligne `WantedCriminal_On/Off`,
//! pages 1 à 10, flèches, « Refresh », « Close »), mise à prix `UI_CriminalArrange1..3`, et icône
//! `WantedAlarm` (`WantedCriminalAlarm.png`) de la grille système `UI_SystemMessageGrid`.
//!
//! Tout vient d'`UI_1024X768.ini` ; les demandes reviennent au moteur en [`WantedUi`] (voir `app_wanted.rs`).

use super::convert_ui::edit_box;
use super::dialogs_ui::{draw_dialog, number_edit};
use super::npc_ui::{button, frame, window};
use super::portal_ui::{argb, control_text, static_text};
use super::*;

/// Fenêtres chargées avec le HUD.
pub(super) const WANTED_WINDOWS: [&str; 5] = ["UI_WantedCriminal", "UI_CriminalArrange1", "UI_CriminalArrange2", "UI_CriminalArrange3", "UI_SystemMessageGrid"];

/// Ligne du tableau (textes déjà mis en forme).
#[derive(Debug, Clone, Default)]
pub struct WantedRow {
    pub rank: String,
    pub name: String,
    pub bounty: String,
    pub level: String,
    pub class: String,
    pub online: bool,
}

pub struct WantedBoardView<'a> {
    pub rows: &'a [WantedRow],
    /// Page affichée (0 à 9).
    pub page: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WantedUi {
    Page(i32),
    Left,
    Right,
    Refresh,
    Close,
}

/// Tableau des avis.
pub fn draw_wanted_board(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &WantedBoardView, out: &mut Vec<WantedUi>) {
    let Some(w) = window(hud, assets, "UI_WantedCriminal") else { return };
    frame(ctx, hud, assets, &w, "archonia-wanted-board", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let item = w.control("CTL_LISTITEM").map_or(20.0, |c| c.size[1]);
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            let cell = |i: usize| r.translate(Vec2::new(0.0, item * s * i as f32));
            match c.name.as_str() {
                "CTL_ST_CAPTION" | "CTL_ST_HRANK" | "CTL_ST_HNAME" | "CTL_ST_HMONEY" | "CTL_ST_HLV" | "CTL_ST_HCLASS" | "CTL_ST_HONOFF" => {
                    control_text(painter, r, c, static_text(c), font, None)
                }
                "CTL_ST_RANK" | "CTL_ST_CHAR" | "CTL_ST_MONEY" | "CTL_ST_LV" | "CTL_ST_CLASS" => {
                    for (i, row) in view.rows.iter().enumerate().take(10) {
                        let text = match c.name.as_str() {
                            "CTL_ST_RANK" => &row.rank,
                            "CTL_ST_CHAR" => &row.name,
                            "CTL_ST_MONEY" => &row.bounty,
                            "CTL_ST_LV" => &row.level,
                            _ => &row.class,
                        };
                        control_text(painter, cell(i), c, text, font, None);
                    }
                }
                // `Status2` 0 « ON » (`WantedCriminal_On.png`), 1 « OFF » (`WantedCriminal_Off.png`).
                "CTL_ST_ONOFF" => {
                    for (i, row) in view.rows.iter().enumerate().take(10) {
                        let want = if row.online { "on.png" } else { "off.png" };
                        if let Some((_, t)) = c.textures.iter().find(|(_, n)| n.to_ascii_lowercase().ends_with(want)) {
                            control_image(ctx, hud, assets, painter, cell(i), c.size, t, 1.0);
                        }
                    }
                }
                n if n.starts_with("CTL_BTN_PAGE") => {
                    let page = n.trim_start_matches("CTL_BTN_PAGE").parse::<i32>().unwrap_or(1) - 1;
                    // Pas de texture (`Button=-1`) : le libellé, blanc pour la page affichée, gris sinon.
                    let color = c.status_colors.get(if page == view.page { 0 } else { 1 }).map_or(Color32::WHITE, |v| argb(*v));
                    let resp = ui.interact(r, egui::Id::new(("wanted-page", page)), egui::Sense::click());
                    shadow_text(painter, r.center(), Align2::CENTER_CENTER, static_text(c), font, if resp.hovered() { Color32::WHITE } else { color });
                    if resp.clicked() {
                        out.push(WantedUi::Page(page));
                    }
                }
                "CTL_BTN_LEFT" | "CTL_BTN_RIGHT" | "CTL_BTN_REFRESH" | "CTL_BTN_CLOSE" | "CTL_BTN_SYSCLOSE" => {
                    let label = if matches!(c.name.as_str(), "CTL_BTN_REFRESH" | "CTL_BTN_CLOSE") { static_text(c) } else { "" };
                    if button(ctx, ui, hud, assets, painter, r, c, label, font, true, ("wanted-btn", c.name.as_str())).clicked() {
                        out.push(match c.name.as_str() {
                            "CTL_BTN_LEFT" => WantedUi::Left,
                            "CTL_BTN_RIGHT" => WantedUi::Right,
                            "CTL_BTN_REFRESH" => WantedUi::Refresh,
                            _ => WantedUi::Close,
                        });
                    }
                }
                _ => {}
            }
        }
    });
}

/// Étape de la mise à prix.
pub struct ArrangeView<'a> {
    /// 1, 2 ou 3 (`UI_CriminalArrange%d`).
    pub step: u8,
    /// Phrase de l'étape (`WC_ARRANGE`, `WC_ARRANGE2`, `WC_ARRANGE3`).
    pub text: &'a str,
    /// Or porté + banque (étape 2, `WC_ARRANGE_TGHELD`).
    pub held: i64,
    /// Prime saisie (étape 2, modifiable) ou retenue (étape 3).
    pub bounty: &'a mut i64,
}

/// Fenêtre de l'étape : `Some(true)` « Confirm » (ou Entrée), `Some(false)` « Cancel ».
pub fn draw_arrange(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: ArrangeView) -> Option<bool> {
    let name = match view.step {
        1 => "UI_CriminalArrange1",
        2 => "UI_CriminalArrange2",
        _ => "UI_CriminalArrange3",
    };
    let ArrangeView { step, text, held, bounty } = view;
    let res = draw_dialog(ctx, hud, assets, name, |ui, _, painter, c, r, font| match c.name.as_str() {
        "CTL_ED_DESC" | "CTL_ED_DESC2" | "CTL_ED_DESC3" => {
            edit_box(painter, r, text, font);
            true
        }
        "CTL_ST_MONEY" => {
            control_text(painter, r, c, &held.to_string(), font, None);
            true
        }
        "CTL_ED_BOUNTY" if step == 2 => {
            number_edit(ui, r, "wanted-bounty", bounty, i64::MAX / 2, font);
            true
        }
        "CTL_ED_BOUNTY" => {
            control_text(painter, r, c, &bounty.to_string(), font, None);
            true
        }
        _ => false,
    })?;
    match res.clicked.as_deref() {
        Some("CTL_BTN_OK") => Some(true),
        Some("CTL_BTN_CANCEL") => Some(false),
        _ if res.enter => Some(true),
        _ if res.escape => Some(false),
        _ => None,
    }
}

/// Icône `WantedAlarm` dans la première case de `UI_SystemMessageGrid`, infobulle au survol.
pub fn draw_wanted_alarm(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, tooltip: &[(String, Color32)]) {
    let Some(w) = window(hud, assets, "UI_SystemMessageGrid") else { return };
    let Some(grid) = w.control("CONTROL_32129") else { return };
    let layout = Layout::new(ctx.screen_rect());
    let origin = layout.window_origin(&w);
    let [_, _, cw, ch] = grid.grid.unwrap_or([3.0, 0.0, 32.0, 32.0]);
    let cell = layout.rect(origin, grid.pos, [cw, ch]);
    egui::Area::new(egui::Id::new("archonia-wanted-alarm")).fixed_pos(cell.min).order(egui::Order::Middle).show(ctx, |ui| {
        let (r, resp) = ui.allocate_exact_size(cell.size(), egui::Sense::hover());
        let painter = ui.painter().clone();
        match hud.image(ctx, assets, "WantedCriminalAlarm.png") {
            Some(_) => control_image(ctx, hud, assets, &painter, r, [cw, ch], "WantedCriminalAlarm.png", 1.0),
            None => {
                painter.rect_filled(r, 3.0, Color32::from_rgb(160, 30, 30));
            }
        }
        if resp.hovered() {
            if let Some(p) = resp.hover_pos() {
                crate::hud::item_tooltip_at(ctx, p, tooltip, "");
            }
        }
    });
}
