//! Fenêtres des combats de Shrine d'Archonia (AgcmShrine, `FUN_140727920`) et `UI_1024X768.ini` :
//!
//! * `UI_Shrine_info` (`Shrine_Base.png`, 606 × 653) : carte des 8 Shrines (`C_Event_Panel`, noms `C_Shrine_1…8`,
//!   icônes `C_Shrine_icon_1…8` dessinées par le rendu `ShrineRender` : `Shrine_Icon_Battle.png` en combat,
//!   `Shrine_Icon_Enable_a/b/c.png` ouvert ou bouton de déplacement actif, `Shrine_Icon_Human/Orc/Moon/Dra.png`
//!   d'après la race occupante, sinon `Shrine_Icon_Disable.png`, `FUN_140729620`), onglet « Status »
//!   (`C_Tap_Info`), liste de 3 lignes par page (`C_List`, `List=3` : état, nom, barre et texte d'état, race
//!   occupante, nom, échéance), pages `C_Prev` / `C_Next` / `C_Page`, croix `CONTROL_12082` ;
//! * `UI_SHRINE_HP` (`Shrine_HP_Bar_Base.png`) : jauge de gravure ou d'occupation `CTRL_Shrine_HP` et son texte
//!   `Shrine_Gauge` (`FUN_140725780`), centrée en haut de l'écran (`FUN_14072a350`, `HPUIPos` de shrinemessage.xml) ;
//! * bouton `CONTROL_MINIMAP_SHRINE` de la minicarte : ouvre ou ferme `UI_Shrine_info` (`Func_OpenShrineInfo`).

use super::npc_ui::{frame, window};
use super::portal_ui::{control_text, state_button, static_text};
use super::*;

pub(super) const SHRINE_WINDOWS: [&str; 2] = ["UI_Shrine_info", "UI_SHRINE_HP"];

/// Lignes de la liste par page (`List=3`).
pub const SHRINE_LIST_ROWS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShrineUi {
    Close,
    /// Icône du Shrine d'indice donné (table du client).
    Icon(usize),
    PrevPage,
    NextPage,
}

/// Icône d'un Shrine sur la carte : rendu de `FUN_140729620` (9 combat, 10 fermé, 11 ouvert, 13-16 race).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShrineIcon {
    Battle,
    Disable,
    Enable,
    Race(i32),
}

pub struct ShrineMapEntry {
    pub name: String,
    pub icon: ShrineIcon,
    /// L'icône accepte le clic (`FUN_140729700`).
    pub clickable: bool,
}

pub struct ShrineRow {
    /// Ouvert (`Shrine_Icon2_Open.png`) ou fermé.
    pub open: bool,
    pub name: String,
    /// Barre d'état : 0 combat, 1 occupation, 2 fermé.
    pub bar: usize,
    pub bar_text: String,
    pub race: i32,
    pub pc_name: String,
    pub remain: String,
}

pub struct ShrineInfoView<'a> {
    pub map: &'a [ShrineMapEntry],
    pub rows: &'a [ShrineRow],
    pub page: String,
    /// Info-bulle des icônes cliquables (`Shrine_Move_Button_Click`).
    pub move_hint: &'a str,
}

/// Texture de l'icône d'une race occupante (1 humain, 2 orc, 3 elfe de lune, 4 scion).
fn race_icon(race: i32, list: bool) -> Option<&'static str> {
    Some(match (race, list) {
        (1, false) => "Shrine_Icon_Human.png",
        (2, false) => "Shrine_Icon_Orc.png",
        (3, false) => "Shrine_Icon_Moon.png",
        (4, false) => "Shrine_Icon_Dra.png",
        (1, true) => "Shrine_Icon2_Human.png",
        (2, true) => "Shrine_Icon2_Orc.png",
        (3, true) => "Shrine_Icon2_Moon.png",
        (4, true) => "Shrine_Icon2_Dra.png",
        _ => return None,
    })
}

/// Image entière étirée au rectangle.
fn whole(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, r: Rect, name: &str) {
    if let Some((tex, _)) = hud.image(ctx, assets, name) {
        painter.image(tex, r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
    }
}

pub fn draw_shrine_info(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &ShrineInfoView, out: &mut Vec<ShrineUi>) {
    let Some(w) = window(hud, assets, "UI_Shrine_info") else { return };
    frame(ctx, hud, assets, &w, "archonia-shrine-info", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let row_h = w.control("C_List_Element").map_or(20.0, |c| c.size[1]);
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            let name = c.name.as_str();
            if let Some(n) = name.strip_prefix("C_Shrine_icon_").and_then(|n| n.parse::<usize>().ok()) {
                let Some(e) = n.checked_sub(1).and_then(|i| view.map.get(i)) else { continue };
                let resp = ui.interact(r, egui::Id::new(("shrine-icon", n)), if e.clickable { egui::Sense::click() } else { egui::Sense::hover() });
                let tex = match e.icon {
                    ShrineIcon::Battle => "Shrine_Icon_Battle.png",
                    ShrineIcon::Disable => "Shrine_Icon_Disable.png",
                    ShrineIcon::Enable if resp.is_pointer_button_down_on() => "Shrine_Icon_Enable_c.png",
                    ShrineIcon::Enable if resp.hovered() => "Shrine_Icon_Enable_b.png",
                    ShrineIcon::Enable => "Shrine_Icon_Enable_a.png",
                    ShrineIcon::Race(race) => race_icon(race, false).unwrap_or("Shrine_Icon_Disable.png"),
                };
                whole(ctx, hud, assets, painter, r, tex);
                if e.clickable {
                    if !view.move_hint.is_empty() {
                        resp.clone().on_hover_text(view.move_hint);
                    }
                    if resp.clicked() {
                        out.push(ShrineUi::Icon(n - 1));
                    }
                }
                continue;
            }
            if let Some(n) = name.strip_prefix("C_Shrine_").and_then(|n| n.parse::<usize>().ok()) {
                if let Some(e) = n.checked_sub(1).and_then(|i| view.map.get(i)) {
                    control_text(painter, r, c, &e.name, font, None);
                }
                continue;
            }
            match name {
                "CONTROL_19892" | "CONTROL_19577" | "CONTROL_32763" | "CONTROL_3597" | "CONTROL_195" | "CONTROL_23126" => control_text(painter, r, c, static_text(c), font, None),
                "C_Tap_Info" => {
                    // Seul onglet de la fenêtre, toujours choisi (`Btn_Tab_c.png`).
                    if let Some((_, t)) = c.textures.first() {
                        control_image(ctx, hud, assets, painter, r, c.size, t, 1.0);
                    }
                    control_text(painter, r, c, static_text(c), font, None);
                }
                "C_LE_Status" | "C_LE_Name" | "C_LE_Status_bar" | "C_LE_Status_Bar_Text" | "C_LE_Occupy_Race" | "C_LE_PCName" | "C_LE_RemainTime" => {
                    for (i, row) in view.rows.iter().take(SHRINE_LIST_ROWS).enumerate() {
                        let rr = layout.rect(origin, [c.pos[0], c.pos[1] + i as f32 * row_h], c.size);
                        match name {
                            "C_LE_Status" => {
                                let t = if row.open { "Shrine_Icon2_Open.png" } else { "Shrine_Icon2_Closed.png" };
                                whole(ctx, hud, assets, painter, rr, t);
                            }
                            "C_LE_Name" => control_text(painter, rr, c, &row.name, font, None),
                            "C_LE_Status_bar" => {
                                if let Some((_, t)) = c.textures.get(row.bar.min(c.textures.len().saturating_sub(1))) {
                                    control_image(ctx, hud, assets, painter, rr, c.size, t, 1.0);
                                }
                            }
                            "C_LE_Status_Bar_Text" => {
                                // Texte posé sur la barre (parent `C_LE_Status_bar`).
                                let bar = w.control("C_LE_Status_bar").map_or(c.pos, |b| b.pos);
                                let br = layout.rect(origin, [bar[0], bar[1] + i as f32 * row_h], c.size);
                                control_text(painter, br, c, &row.bar_text, font, None);
                            }
                            "C_LE_Occupy_Race" => {
                                if let Some(t) = race_icon(row.race, true) {
                                    whole(ctx, hud, assets, painter, Rect::from_min_size(rr.min, Vec2::splat(15.0 * s)), t);
                                }
                            }
                            "C_LE_PCName" => control_text(painter, rr, c, &row.pc_name, font, None),
                            _ => control_text(painter, rr, c, &row.remain, font, None),
                        }
                    }
                }
                "C_Page" => control_text(painter, r, c, &view.page, font, None),
                "C_Prev" | "C_Next" => {
                    let next = name == "C_Next";
                    if state_button(ctx, ui, hud, assets, painter, r, c, "", font, true, ("shrine-page", next)).clicked() {
                        out.push(if next { ShrineUi::NextPage } else { ShrineUi::PrevPage });
                    }
                }
                "CONTROL_12082" => {
                    if state_button(ctx, ui, hud, assets, painter, r, c, "", font, true, "shrine-x").clicked() {
                        out.push(ShrineUi::Close);
                    }
                }
                _ => {}
            }
        }
    });
}

/// Jauge `UI_SHRINE_HP`, centrée à `top` px (1024 × 768) du haut de l'écran.
pub fn draw_shrine_hp(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, text: &str, ratio: f32, top: f32) {
    let Some(w) = window(hud, assets, "UI_SHRINE_HP") else { return };
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let size = Vec2::new(w.size[0], w.size[1]) * s;
    let origin = Pos2::new(ctx.screen_rect().center().x - size.x / 2.0, ctx.screen_rect().min.y + top * s);
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("archonia-shrine-hp")));
    if let Some((_, t)) = w.textures.first().cloned() {
        control_image(ctx, hud, assets, &painter, Rect::from_min_size(origin, size), w.size, &t, 1.0);
    }
    for c in &w.controls {
        let r = layout.rect(origin, c.pos, c.size);
        match c.name.as_str() {
            "CTRL_Shrine_HP" => {
                if let Some((_, t)) = c.textures.first() {
                    control_image(ctx, hud, assets, &painter, r, c.size, t, ratio);
                }
            }
            "Ctrl_HP_DP" => control_text(&painter, r, c, text, (11.0 * s).max(9.0), None),
            _ => {}
        }
    }
}

/// Bouton `CONTROL_MINIMAP_SHRINE` de la minicarte (dessiné avec elle) : `true` s'il a été cliqué.
pub fn minimap_shrine_button(ctx: &egui::Context, hud: &mut HudState, assets: &Assets) -> bool {
    if super::minimap_closed(hud) {
        return false;
    }
    let Some(w) = window(hud, assets, "MiniMap") else { return false };
    let Some(c) = w.control("CONTROL_MINIMAP_SHRINE") else { return false };
    let layout = Layout::new(ctx.screen_rect());
    let r = layout.rect(layout.window_origin(&w), c.pos, c.size);
    let mut clicked = false;
    egui::Area::new(egui::Id::new("archonia-minimap-shrine")).fixed_pos(r.min).order(egui::Order::Middle).show(ctx, |ui| {
        let (rect, resp) = ui.allocate_exact_size(r.size(), egui::Sense::click());
        let suffix = if resp.is_pointer_button_down_on() { "c.png" } else if resp.hovered() { "b.png" } else { "a.png" };
        if let Some((_, t)) = c.textures.iter().find(|(_, n)| n.to_ascii_lowercase().ends_with(suffix)).cloned() {
            control_image(ctx, hud, assets, ui.painter(), rect, c.size, &t, 1.0);
        }
        clicked = resp.clicked();
    });
    clicked
}
