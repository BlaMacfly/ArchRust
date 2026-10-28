//! Fenêtre du commutateur d'équipement d'Archonia (AgcmItemSet), d'après `UI_1024X768.ini` :
//! `UI_SwitchEditor` (869 × 652, `switch_bg.png`) : onglets `TabMacros` / `TabSwitch`, profil
//! `CmbProfile` (+ flèche), nom du profil `EditProfileName` et `BtnRenameProfile`, configurations
//! `LstRow0…9` (`row_bg.png`), nom `EditName`, raccourci `LblHotkeyValue` et `BtnSetHotkey` (« REC »),
//! `BtnSave` / `BtnDelete`, grilles `GridA` / `GridB` (10 cases de 50 × 49, écart 9) et leurs numéros,
//! `BtnCaptureA` / `BtnCaptureB`, `BtnAdd` / `BtnRemove`, fermeture.
//!
//! Les demandes reviennent au moteur en [`SwitchUi`] (voir `app_itemset.rs`).

use super::items_ui::icon as item_icon;
use super::npc_ui::{frame, window};
use super::portal_ui::{control_text, state_button, static_text};
use super::*;

/// Fenêtre du commutateur, chargée avec le HUD.
pub(super) const ITEMSET_WINDOWS: [&str; 1] = ["UI_SwitchEditor"];

/// Case d'une grille saisie à la souris (déplacement dans la même grille).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SwitchDrag {
    set: usize,
    pos: usize,
}

pub struct SwitchView<'a> {
    /// Nom du profil courant (`CmbProfile`).
    pub profile: &'a str,
    pub profile_edit: &'a mut String,
    /// Nom des configurations (vide : configuration libre).
    pub configs: [&'a str; 10],
    pub selected: Option<usize>,
    pub name_edit: &'a mut String,
    pub hotkey: &'a str,
    /// Objets des jeux A et B de la configuration choisie (TID, quantité).
    pub grids: [[Option<(i32, i32)>; 10]; 2],
    /// Jeu actif de la configuration choisie (0 A, 1 B).
    pub active_set: Option<usize>,
}

/// Demandes de `UI_SwitchEditor`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitchUi {
    Close,
    /// Onglet « Macros » (`SwitchEditor_SwapToMacros`).
    Macros,
    /// Profil suivant (`SwitchEditor_SelectProfile`).
    NextProfile,
    Rename,
    SelectConfig(usize),
    SetHotkey,
    Save,
    Delete,
    Add,
    Capture(usize),
    Move { set: usize, from: usize, to: usize },
    Remove { set: usize, pos: usize },
}

fn edit(ui: &mut egui::Ui, r: Rect, id: &str, text: &mut String, font: f32) {
    let e = egui::TextEdit::singleline(text)
        .id(egui::Id::new(("archonia-switch-edit", id)))
        .frame(false)
        .char_limit(31)
        .font(FontId::proportional(font))
        .text_color(Color32::WHITE);
    ui.put(r.shrink2(Vec2::new(4.0, 2.0)), e);
}

/// Commutateur d'équipement.
pub fn draw_switch_editor(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: SwitchView, out: &mut Vec<SwitchUi>) {
    let Some(w) = window(hud, assets, "UI_SwitchEditor") else { return };
    let SwitchView { profile, profile_edit, configs, selected, name_edit, hotkey, grids, active_set } = view;
    frame(ctx, hud, assets, &w, "archonia-switch-editor", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let dragging = egui::DragAndDrop::payload::<SwitchDrag>(ctx).map(|d| *d);
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            let name = c.name.strip_prefix("UI_SwitchEditor_").unwrap_or(&c.name);
            if let Some(k) = name.strip_prefix("LstRow").and_then(|n| n.parse::<usize>().ok()) {
                let label = configs.get(k).copied().unwrap_or("");
                if label.is_empty() {
                    continue;
                }
                if selected == Some(k) {
                    if let Some((_, t)) = c.textures.first() {
                        control_image(ctx, hud, assets, painter, r, c.size, t, 1.0);
                    }
                }
                if state_button(ctx, ui, hud, assets, painter, r, c, "", font, true, ("switch-row", k)).clicked() {
                    out.push(SwitchUi::SelectConfig(k));
                }
                control_text(painter, r, c, label, font, None);
                continue;
            }
            if let Some(set) = match name {
                "GridA" => Some(0usize),
                "GridB" => Some(1),
                _ => None,
            } {
                let [gx, _, cw, ch] = c.grid.unwrap_or([9.0, 0.0, 50.0, 49.0]);
                for pos in 0..10 {
                    let cell = layout.rect(origin, [c.pos[0] + pos as f32 * (cw + gx), c.pos[1]], [cw, ch]);
                    let resp = ui.interact(cell, egui::Id::new(("switch-cell", set, pos)), egui::Sense::click_and_drag());
                    if let Some((tid, count)) = grids[set][pos] {
                        let lifted = dragging.is_some_and(|d| d == SwitchDrag { set, pos });
                        item_icon(ctx, hud, assets, painter, cell, Some(tid), if lifted { Color32::from_white_alpha(70) } else { Color32::WHITE });
                        if count > 1 {
                            shadow_text(painter, cell.right_bottom() - Vec2::new(3.0, 2.0) * s, Align2::RIGHT_BOTTOM, &count.to_string(), font, Color32::WHITE);
                        }
                        if resp.dragged() {
                            resp.dnd_set_drag_payload(SwitchDrag { set, pos });
                        }
                        if resp.secondary_clicked() {
                            out.push(SwitchUi::Remove { set, pos });
                        }
                    }
                    if let Some(d) = resp.dnd_release_payload::<SwitchDrag>() {
                        if d.set == set && d.pos != pos {
                            out.push(SwitchUi::Move { set, from: d.pos, to: pos });
                        }
                    }
                    let target = dragging.is_some_and(|d| d.set == set) && resp.contains_pointer();
                    if resp.hovered() || target {
                        let color = if target { Color32::from_rgb(255, 210, 110) } else { Color32::from_rgb(120, 200, 255) };
                        painter.rect_stroke(cell, 2.0, egui::Stroke::new(1.0_f32, color), egui::StrokeKind::Inside);
                    }
                }
                if active_set == Some(set) {
                    painter.rect_stroke(r.expand(2.0), 3.0, egui::Stroke::new(1.5_f32, Color32::from_rgb(255, 220, 120)), egui::StrokeKind::Outside);
                }
                continue;
            }
            match (c.kind, name) {
                // Onglets sans texture ni texte : dessinés dans `switch_bg.png`.
                (3, "TabSwitch") => {}
                (3, "TabMacros") => {
                    let resp = ui.interact(r, egui::Id::new("switch-tab-macros"), egui::Sense::click());
                    if resp.hovered() {
                        painter.rect_filled(r, 2.0, Color32::from_white_alpha(12));
                    }
                    if resp.clicked() {
                        out.push(SwitchUi::Macros);
                    }
                }
                (5, "CmbProfile") => {
                    if let Some((_, t)) = c.textures.first() {
                        control_image(ctx, hud, assets, painter, r, c.size, t, 1.0);
                    }
                    let resp = ui.interact(r, egui::Id::new("switch-profile"), egui::Sense::click());
                    let tr = Rect::from_min_max(r.min, r.max - Vec2::new(26.0 * s, 0.0));
                    control_text(painter, tr, c, profile, font, None);
                    if resp.clicked() {
                        out.push(SwitchUi::NextProfile);
                    }
                }
                (3, "CmbProfileArrow") => {
                    if state_button(ctx, ui, hud, assets, painter, r, c, "", font, true, "switch-profile-arrow").clicked() {
                        out.push(SwitchUi::NextProfile);
                    }
                }
                (2, "EditProfileName" | "EditName") => {
                    if let Some((_, t)) = c.textures.first() {
                        control_image(ctx, hud, assets, painter, r, c.size, t, 1.0);
                    }
                    if name == "EditName" {
                        edit(ui, r, name, name_edit, font);
                    } else {
                        edit(ui, r, name, profile_edit, font);
                    }
                }
                (1, "LblHotkeyValue") => control_text(painter, r, c, hotkey, font, None),
                (1, _) => control_text(painter, r, c, static_text(c).trim(), font, None),
                (3, _) => {
                    let action = match name {
                        "BtnClose" => SwitchUi::Close,
                        "BtnRenameProfile" => SwitchUi::Rename,
                        "BtnSetHotkey" => SwitchUi::SetHotkey,
                        "BtnSave" => SwitchUi::Save,
                        "BtnDelete" | "BtnRemove" => SwitchUi::Delete,
                        "BtnCaptureA" => SwitchUi::Capture(0),
                        "BtnCaptureB" => SwitchUi::Capture(1),
                        "BtnAdd" => SwitchUi::Add,
                        _ => continue,
                    };
                    let needs_config = matches!(action, SwitchUi::SetHotkey | SwitchUi::Save | SwitchUi::Delete | SwitchUi::Capture(_));
                    let enabled = !needs_config || selected.is_some();
                    if state_button(ctx, ui, hud, assets, painter, r, c, static_text(c), font, enabled, ("switch-btn", name)).clicked() {
                        out.push(action);
                    }
                }
                _ => {}
            }
        }
    });
}
