//! Fenêtre des boss d'Archonia (AgcmUIBoss, init `FUN_140677870`) d'après `UI_1024X768.ini` :
//!
//! * `UI_Boss` (`boss_main.png`, 904 × 623) : 7 filtres de catégorie (`Boss_Filter_*_Btn`, `OP_Tab_a…d.png`),
//!   liste de 9 boss visibles (`Boss_Tab_List`, ligne `boss_tab_button_a/b/c.png`, icône `Boss_Icon_ListItem`
//!   par l'`Icon` d'UIBoss.xml, nom, statut coloré `BossDisplay_BossSpawn`), barre de défilement, détail du boss
//!   choisi (grande icône, nom, lieu, période), butin 4 × 4 (`Boss_Drop_Grid`), classement de 16 lignes, ligne du
//!   joueur, pages, heure du serveur (`Boss_ServerTime_Edit`), croix `Boss_Close_Btn` ;
//! * `UI_BossDPS` (`boos_damage_meter.png`) : « Damage Meter - <boss> », 11 lignes visibles (icône de classe
//!   `party_face_*.png`, nom, guilde, dégâts) ; `UI_BossDPSButton` la rouvre ;
//! * `UI_BossWindowMiniBtn` : petit bouton qui ouvre ou ferme `UI_Boss` (`BOSS_MAIN`).

use super::npc_ui::{frame, window};
use super::portal_ui::{argb, control_text, state_button, static_text};
use super::*;

pub(super) const BOSS_WINDOWS: [&str; 4] = ["UI_Boss", "UI_BossDPS", "UI_BossDPSButton", "UI_BossWindowMiniBtn"];

/// Lignes visibles de la liste des boss (`List=9`), du classement (`List=16`) et du compteur (`List=11`).
pub const BOSS_LIST_ROWS: usize = 9;
const RANK_ROWS: usize = 16;
const DPS_ROWS: usize = 11;

/// Boutons des filtres, dans l'ordre des bits du masque (catégorie − 1).
const FILTERS: [&str; 7] = ["Boss_Filter_CF_Btn", "Boss_Filter_Asylum_Btn", "Boss_Filter_BS_Btn", "Boss_Filter_SW_Btn", "Boss_Filter_HH_Btn", "Boss_Filter_FM_Btn", "Boss_Filter_PG_Btn"];
/// Libellés des filtres (abréviations des noms de contrôle ; le texte d'Archonia n'est pas relevé).
pub const FILTER_LABELS: [&str; 7] = ["CF", "Asylum", "BS", "SW", "HH", "FM", "PG"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BossUi {
    Close,
    /// Filtre d'indice donné (bit du masque) basculé.
    Filter(usize),
    /// Boss choisi dans la liste (TID).
    Select(i32),
    RankPage { next: bool },
    CloseDps,
    OpenDps,
}

pub struct BossEntry {
    pub tid: i32,
    pub name: String,
    pub icon: u32,
    pub status: String,
    /// Couleur ARGB du statut.
    pub status_color: u32,
}

pub struct BossDetail {
    pub icon: u32,
    pub name: String,
    pub place: String,
    pub period: String,
}

pub struct BossView<'a> {
    pub entries: &'a [BossEntry],
    pub first: &'a mut usize,
    pub selected: Option<i32>,
    pub filters: u32,
    pub detail: Option<BossDetail>,
    /// Butin (TID d'objet, quantité) dans l'ordre de la grille.
    pub drops: &'a [(i32, i32)],
    /// Classement (rang, nom, victoires) et ligne du joueur.
    pub ranks: &'a [(String, String, String)],
    pub me: (String, String, String),
    pub page: String,
    pub server_time: String,
}

pub struct DpsRow {
    /// Icône de classe (0 à 11, `party_face_*.png`).
    pub class: usize,
    pub name: String,
    pub guild: String,
    pub damage: String,
}

/// Image de l'indice `Texture=n:` d'un contrôle, étirée au rectangle.
fn indexed(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, r: Rect, c: &UiControl, n: u32) {
    if let Some((_, t)) = c.textures.iter().find(|(i, t)| *i == n && !t.is_empty()) {
        if let Some((tex, _)) = hud.image(ctx, assets, t) {
            painter.image(tex, r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
    }
}

pub fn draw_boss(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: BossView, out: &mut Vec<BossUi>) {
    let Some(w) = window(hud, assets, "UI_Boss") else { return };
    frame(ctx, hud, assets, &w, "archonia-boss", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let item_h = w.control("Boss_Tab_ListItem").map_or(57.0, |c| c.size[1]);
        let rank_h = w.control("Boss_Rank_ListItem").map_or(25.0, |c| c.size[1]);
        let last_first = view.entries.len().saturating_sub(BOSS_LIST_ROWS);
        *view.first = (*view.first).min(last_first);
        // Molette sur la liste.
        if let Some(list) = w.control("Boss_Tab_List") {
            let lr = layout.rect(origin, list.pos, list.size);
            if ui.rect_contains_pointer(lr) {
                let dy = ui.input(|i| i.raw_scroll_delta.y);
                if dy > 0.0 {
                    *view.first = view.first.saturating_sub(1);
                } else if dy < 0.0 {
                    *view.first = (*view.first + 1).min(last_first);
                }
            }
        }
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            let name = c.name.as_str();
            if let Some(bit) = FILTERS.iter().position(|f| *f == name) {
                let on = view.filters & (1 << bit) != 0;
                let resp = ui.interact(r, egui::Id::new(("boss-filter", bit)), egui::Sense::click());
                let n = if resp.is_pointer_button_down_on() || on { 3 } else if resp.hovered() { 2 } else { 1 };
                indexed(ctx, hud, assets, painter, r, c, n);
                control_text(painter, r, c, FILTER_LABELS[bit], font, Some(if on { Color32::WHITE } else { Color32::GRAY }));
                if resp.clicked() {
                    out.push(BossUi::Filter(bit));
                }
                continue;
            }
            match name {
                "Boss_Title_Base" | "Boss_Rank_Title1_Base" | "Boss_Rank_Title2_Base" => control_text(painter, r, c, static_text(c), font, None),
                "Boss_ServerTime_Edit" => control_text(painter, r, c, &view.server_time, font, None),
                "Boss_Close_Btn" => {
                    if state_button(ctx, ui, hud, assets, painter, r, c, "", font, true, "boss-x").clicked() {
                        out.push(BossUi::Close);
                    }
                }
                "Boss_Tab_Button_ListItem" => {
                    let icon_c = w.control("Boss_Icon_ListItem");
                    let name_c = w.control("Boss_Name_ListItem");
                    let spawn_c = w.control("Boss_Spawn_ListItem");
                    for i in 0..BOSS_LIST_ROWS {
                        let Some(e) = view.entries.get(*view.first + i) else { break };
                        let dy = i as f32 * item_h;
                        let rr = layout.rect(origin, [c.pos[0], c.pos[1] + dy], c.size);
                        let resp = ui.interact(rr, egui::Id::new(("boss-row", e.tid)), egui::Sense::click());
                        let n = if view.selected == Some(e.tid) || resp.is_pointer_button_down_on() { 3 } else if resp.hovered() { 2 } else { 1 };
                        indexed(ctx, hud, assets, painter, rr, c, n);
                        if let Some(ic) = icon_c {
                            indexed(ctx, hud, assets, painter, layout.rect(origin, [ic.pos[0], ic.pos[1] + dy], ic.size), ic, e.icon);
                        }
                        if let Some(nc) = name_c {
                            control_text(painter, layout.rect(origin, [nc.pos[0], nc.pos[1] + dy], nc.size), nc, &e.name, font, None);
                        }
                        if let Some(sc) = spawn_c {
                            control_text(painter, layout.rect(origin, [sc.pos[0], sc.pos[1] + dy], sc.size), sc, &e.status, font, Some(argb(e.status_color)));
                        }
                        if resp.clicked() {
                            out.push(BossUi::Select(e.tid));
                        }
                    }
                }
                "Boss_Tab_Scrollupbtn" | "Boss_Tab_Scrolldnbtn" => {
                    let down = name == "Boss_Tab_Scrolldnbtn";
                    let enabled = if down { *view.first < last_first } else { *view.first > 0 };
                    let resp = ui.interact(r, egui::Id::new(("boss-scroll", down)), egui::Sense::click());
                    // `Button=2:3:4:1` : repos 2, survol 3, appui 4, inactif 1.
                    let n = if !enabled { 1 } else if resp.is_pointer_button_down_on() { 4 } else if resp.hovered() { 3 } else { 2 };
                    indexed(ctx, hud, assets, painter, r, c, n);
                    if enabled && resp.clicked() {
                        *view.first = if down { (*view.first + 1).min(last_first) } else { view.first.saturating_sub(1) };
                    }
                }
                "Boss_Tab_Scrollmbtn" => {
                    // Curseur le long de la glissière (`Scroll=1:17:452`).
                    let track = 452.0 - c.size[1].min(452.0);
                    let t = if last_first == 0 { 0.0 } else { *view.first as f32 / last_first as f32 };
                    let rr = layout.rect(origin, [c.pos[0], c.pos[1] + t * track], c.size);
                    indexed(ctx, hud, assets, painter, rr, c, 2);
                }
                "Boss_Big_Icon_Base" => {
                    if let Some(d) = &view.detail {
                        indexed(ctx, hud, assets, painter, r, c, d.icon);
                    }
                }
                "Boss_Name_Base" | "Boss_Place_Base" | "Boss_Time_Period_Base" => {
                    if let Some(d) = &view.detail {
                        let t = match name {
                            "Boss_Name_Base" => &d.name,
                            "Boss_Place_Base" => &d.place,
                            _ => &d.period,
                        };
                        control_text(painter, r, c, t, font, None);
                    }
                }
                "Boss_Drop_Grid" => {
                    let [gx, gy, cw, ch] = c.grid.unwrap_or([2.0, 2.0, 50.0, 50.0]);
                    for (i, (tid, count)) in view.drops.iter().take(16).enumerate() {
                        let (row, col) = ((i / 4) as f32, (i % 4) as f32);
                        let cell = layout.rect(origin, [c.pos[0] + col * (cw + gx), c.pos[1] + row * (ch + gy)], [cw, ch]);
                        items_ui::icon(ctx, hud, assets, painter, cell, Some(*tid), Color32::WHITE);
                        if *count > 1 {
                            shadow_text(painter, cell.right_bottom() - Vec2::new(3.0, 2.0) * s, Align2::RIGHT_BOTTOM, &count.to_string(), font, Color32::WHITE);
                        }
                        let resp = ui.interact(cell, egui::Id::new(("boss-drop", i)), egui::Sense::hover());
                        if let Some(n) = crate::scene::item_name_for_tid(assets, *tid) {
                            resp.on_hover_text(n);
                        }
                    }
                }
                "Boss_Rank_PlayerRank" | "Boss_Rank_PlayerName" | "Boss_Rank_PlayerKill" => {
                    for (i, (rank, who, kills)) in view.ranks.iter().take(RANK_ROWS).enumerate() {
                        let rr = layout.rect(origin, [c.pos[0], c.pos[1] + i as f32 * rank_h], c.size);
                        let t = match name {
                            "Boss_Rank_PlayerRank" => rank,
                            "Boss_Rank_PlayerName" => who,
                            _ => kills,
                        };
                        control_text(painter, rr, c, t, font, None);
                    }
                }
                "Boss_Rank_Self_Rank_Base" => control_text(painter, r, c, &view.me.0, font, None),
                "Boss_Rank_Self_Name_Base" => control_text(painter, r, c, &view.me.1, font, None),
                "Boss_Rank_Self_Kill_Base" => control_text(painter, r, c, &view.me.2, font, None),
                "Boss_Rank_CurrentPage_Base" => control_text(painter, r, c, &view.page, font, None),
                "Boss_Rank_Page_Left_Btn" | "Boss_Rank_Page_Right_Btn" => {
                    let next = name == "Boss_Rank_Page_Right_Btn";
                    if state_button(ctx, ui, hud, assets, painter, r, c, "", font, view.selected.is_some(), ("boss-rank-page", next)).clicked() {
                        out.push(BossUi::RankPage { next });
                    }
                }
                _ => {}
            }
        }
    });
}

/// Compteur de dégâts `UI_BossDPS`.
pub fn draw_boss_dps(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, title: &str, rows: &[DpsRow], out: &mut Vec<BossUi>) {
    let Some(w) = window(hud, assets, "UI_BossDPS") else { return };
    frame(ctx, hud, assets, &w, "archonia-boss-dps", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let row_h = w.control("BossDPS_ListItem").map_or(34.0, |c| c.size[1]);
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match c.name.as_str() {
                "BossDPS_Title_Base" => control_text(painter, r, c, title, font, None),
                "BossDPS_Header_Class" | "BossDPS_Header_Name" | "BossDPS_Header_Guild" | "BossDPS_Header_Damage" => control_text(painter, r, c, static_text(c), font, None),
                "BossDPS_Close_Btn" => {
                    if state_button(ctx, ui, hud, assets, painter, r, c, "", font, true, "boss-dps-x").clicked() {
                        out.push(BossUi::CloseDps);
                    }
                }
                "BossDPS_ClassIcon" | "BossDPS_PlayerName" | "BossDPS_GuildName" | "BossDPS_Damage" => {
                    for (i, row) in rows.iter().take(DPS_ROWS).enumerate() {
                        let dy = i as f32 * row_h;
                        match c.name.as_str() {
                            // `Status2 n:5,6,21,21:n+1` : visage de 21 × 21 à (5, 6) de la ligne.
                            "BossDPS_ClassIcon" => {
                                let rr = layout.rect(origin, [c.pos[0] + 5.0, c.pos[1] + 6.0 + dy], [21.0, 21.0]);
                                indexed(ctx, hud, assets, painter, rr, c, row.class as u32 + 1);
                            }
                            n => {
                                let rr = layout.rect(origin, [c.pos[0], c.pos[1] + dy], c.size);
                                let t = match n {
                                    "BossDPS_PlayerName" => &row.name,
                                    "BossDPS_GuildName" => &row.guild,
                                    _ => &row.damage,
                                };
                                control_text(painter, rr, c, t, font, None);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    });
}

/// Petite fenêtre à un seul bouton (`UI_BossDPSButton`, `UI_BossWindowMiniBtn`) : `true` si le bouton a été cliqué.
pub fn draw_boss_button(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, window_name: &str) -> bool {
    let Some(w) = window(hud, assets, window_name) else { return false };
    let Some(c) = w.controls.iter().find(|c| c.kind == 3).cloned() else { return false };
    let layout = Layout::new(ctx.screen_rect());
    let origin = layout.window_origin(&w);
    let r = layout.rect(origin, c.pos, c.size);
    let mut clicked = false;
    egui::Area::new(egui::Id::new(("archonia-boss-button", window_name))).fixed_pos(r.min).order(egui::Order::Middle).show(ctx, |ui| {
        let (rect, resp) = ui.allocate_exact_size(r.size(), egui::Sense::click());
        let n = if resp.is_pointer_button_down_on() { 3 } else if resp.hovered() { 2 } else { 1 };
        indexed(ctx, hud, assets, ui.painter(), rect, &c, n);
        clicked = resp.clicked();
    });
    clicked
}
