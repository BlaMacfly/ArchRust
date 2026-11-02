//! Fenêtres d'Archonia des touches personnalisables (`UI_Hotkey`) et de sortie (`UI_EXIT`) d'`UI_1024X768.ini`
//! (logique : `app_hotkey.rs`).
//!
//! * `UI_Hotkey` (`Hotkey_Base.png`, 334 × 500) : titre `CONTROL_32053` ; liste `CONTROL_26981` (15,40, 298 × 322)
//!   de lignes `CONTROL_10154` (278 × 32) : libellé `CONTROL_15305` (`HotKey_DispItemDesc`, centré) et case
//!   `CONTROL_29492` (`Hotkey_InputBox.png`, `HotKey_DispItemCode` ; clic → `HotKey_FuncInputCode`) ; défilement
//!   `CONTROL_HOTKEY_SCROLL` (`scroll_btn_up`, `scroll_btn_down`, curseur `scroll_bar`) ; « Confirm »
//!   `CONTROL_22920`, « Cancel » `CONTROL_10341`, « Reset » `CONTROL_20945`, croix `CONTROL_HOTKEY_EXIT`. La case
//!   qui attend sa touche montre un curseur de saisie après le nom de la touche.
//! * `UI_EXIT` (`UI_EXITMAIN.png`, 236 × 197) : `CONTROL_8415` (`LoginGameServerName`), `CT_EXIT_BUTTON0`
//!   « Return to the game », `CT_EXIT_BUTTON2` « Select Character », `CT_EXIT_BUTTON1` « Options »,
//!   `CONTROL_21965` « Custom Keys », `CT_EXIT_BUTTON4` « Exit Game », croix `CT_EXIT_CLOSE`.
//!
//! Libellés fixes du dossier de langue (`ini_fr/UI_1024X768.ini` : « Touches personnalisees »…).

use super::npc_ui::frame;
use super::portal_ui::{control_text, state_button};
use super::*;

/// Fenêtres chargées avec le HUD.
pub(super) const HOTKEY_WINDOWS: [&str; 2] = ["UI_Hotkey", "UI_EXIT"];

/// Ligne affichée d'`UI_Hotkey`.
#[derive(Debug, Clone, PartialEq)]
pub struct HotkeyRow {
    pub desc: String,
    pub key: String,
}

/// Contenu d'`UI_Hotkey`.
#[derive(Debug, Clone, Default)]
pub struct HotkeyView {
    pub rows: Vec<HotkeyRow>,
    pub first: usize,
    /// Ligne (rang affiché) qui attend sa touche.
    pub capturing: Option<usize>,
}

/// Choix dans `UI_Hotkey`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyUi {
    /// Case de touche cliquée (rang affiché ; entrée de la liste une fois traduit par le moteur).
    Arm(usize),
    Confirm,
    Cancel,
    Reset,
    Close,
    /// Défilement de la liste, en lignes.
    Scroll(i32),
}

/// Choix dans `UI_EXIT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitUi {
    ReturnGame,
    SelectCharacter,
    Options,
    CustomKeys,
    ExitGame,
    Close,
}

/// Lignes visibles (322 px de liste, lignes de 32 px).
const ROWS: usize = 10;

/// Fenêtre `UI_Hotkey`.
pub fn draw_hotkey(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &HotkeyView, out: &mut Vec<HotkeyUi>) {
    let Some(w) = super::npc_ui::window(hud, assets, "UI_Hotkey") else { return };
    let row_h = w.control("CONTROL_10154").map_or(32.0, |c| c.size[1]);
    frame(ctx, hud, assets, &w, "archonia-hotkey", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let first = view.first.min(view.rows.len().saturating_sub(ROWS));
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            let click = match c.name.as_str() {
                "CONTROL_32053" => {
                    control_text(painter, r, c, c.fixed_label(), font, None);
                    None
                }
                "CONTROL_22920" => Some(HotkeyUi::Confirm),
                "CONTROL_10341" => Some(HotkeyUi::Cancel),
                "CONTROL_20945" => Some(HotkeyUi::Reset),
                "CONTROL_HOTKEY_EXIT" => Some(HotkeyUi::Close),
                "scroll_btn_up" => Some(HotkeyUi::Scroll(-1)),
                "scroll_btn_down" => Some(HotkeyUi::Scroll(1)),
                "scroll_bar" => {
                    let span = view.rows.len().saturating_sub(ROWS).max(1) as f32;
                    let (up, down) = (w.control("scroll_btn_up"), w.control("scroll_btn_down"));
                    let travel = match (up, down) {
                        (Some(u), Some(d)) => (d.pos[1] - (u.pos[1] + u.size[1]) - c.size[1]).max(0.0),
                        _ => 0.0,
                    };
                    let y = (first as f32 / span).clamp(0.0, 1.0) * travel;
                    if let Some((_, t)) = c.textures.first() {
                        control_image(ctx, hud, assets, painter, layout.rect(origin, [c.pos[0], c.pos[1] + y], c.size), c.size, t, 1.0);
                    }
                    None
                }
                _ => None,
            };
            if let Some(a) = click {
                let label = if c.kind == 3 { c.fixed_label() } else { "" };
                if state_button(ctx, ui, hud, assets, painter, r, c, label, font, true, ("archonia-hotkey-btn", c.name.as_str())).clicked() {
                    out.push(a);
                }
            }
        }
        // Lignes : gabarit `CONTROL_10154` répété tous les 32 px.
        let (Some(desc), Some(input)) = (w.control("CONTROL_15305"), w.control("CONTROL_29492")) else { return };
        for (k, row) in view.rows.iter().enumerate().skip(first).take(ROWS) {
            let dy = (k - first) as f32 * row_h;
            let rd = layout.rect(origin, [desc.pos[0], desc.pos[1] + dy], desc.size);
            control_text(painter, rd, desc, &row.desc, font, None);
            let ri = layout.rect(origin, [input.pos[0], input.pos[1] + dy], input.size);
            if let Some((_, t)) = input.textures.first() {
                control_image(ctx, hud, assets, painter, ri, input.size, t, 1.0);
            }
            let armed = view.capturing == Some(k);
            let caret = armed && (ctx.input(|i| i.time) * 2.0) as i64 % 2 == 0;
            let text = if caret { format!("{}|", row.key) } else { row.key.clone() };
            control_text(painter, ri, input, &text, font, None);
            if ui.interact(ri, egui::Id::new(("archonia-hotkey-input", k)), egui::Sense::click()).clicked() {
                out.push(HotkeyUi::Arm(k));
            }
        }
        // Molette sur la liste.
        if let Some(list) = w.control("CONTROL_26981") {
            let r = layout.rect(origin, list.pos, list.size);
            if ui.rect_contains_pointer(r) {
                let d = ctx.input(|i| i.raw_scroll_delta.y);
                if d.abs() > 0.5 {
                    out.push(HotkeyUi::Scroll(if d > 0.0 { -1 } else { 1 }));
                }
            }
        }
    });
}

/// Fenêtre `UI_EXIT` ; `server` : texte de `LoginGameServerName`.
pub fn draw_exit(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, server: &str, out: &mut Vec<ExitUi>) {
    let mut clicked: Option<String> = None;
    super::dialogs_ui::draw_dialog(ctx, hud, assets, "UI_EXIT", |ui, hud, painter, c, r, font| {
        if c.name == "CONTROL_8415" {
            control_text(painter, r, c, server, font, None);
            return true;
        }
        if c.kind != 3 {
            return false;
        }
        if state_button(ctx, ui, hud, assets, painter, r, c, c.fixed_label(), font * 0.95, true, ("archonia-exit", c.name.as_str())).clicked() {
            clicked = Some(c.name.clone());
        }
        true
    });
    let choice = match clicked.as_deref() {
        Some("CT_EXIT_BUTTON0") => ExitUi::ReturnGame,
        Some("CT_EXIT_BUTTON2") => ExitUi::SelectCharacter,
        Some("CT_EXIT_BUTTON1") => ExitUi::Options,
        Some("CONTROL_21965") => ExitUi::CustomKeys,
        Some("CT_EXIT_BUTTON4") => ExitUi::ExitGame,
        Some("CT_EXIT_CLOSE") => ExitUi::Close,
        _ => return,
    };
    out.push(choice);
}
