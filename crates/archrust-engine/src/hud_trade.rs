//! Fenêtres de l'échange entre joueurs d'Archonia (AgcmUIItem, partie PrivateTrade), d'après
//! `UI_1024X768.ini` :
//!
//! * `UI_PrivateTrade` (277 × 491, `PT_BASE3.png`) : grille des objets proposés `CONTROL_PT_Grid` (4 × 3 cases
//!   de 50 + 2, dépôt `PT_MoveItem`) et celle de l'autre `CONTROL_PTC_Grid` ; noms `CONTROL_PT_NameStatic` /
//!   `CONTROL_PT_ClientNameStatic` ; or `CONTROL_7122` / `CONTROL_793` ; pièces Chantra
//!   `CONTROL_PrivateTrade_SelfCashDisplay` / `TargetCashDisplay` ; bouton à bascule « Lock » `CONTROL_PT_Lock`
//!   (statut 1 : texture B, texte rouge), « Trade » `CONTROL_PT_ReadyToExchange` (statut 1 après le clic) ;
//!   états de l'autre `CONTROL_PTC_LOCK` et `CONTROL_PTC_ReadyToExchange` (statuts posés par les événements
//!   `PT_ClientLock`, `PT_ClientUnlock`, `PT_ActiveReadyToExchange`, `PT_ClientReadyToExchange`) ; « Gold »
//!   `CONTROL_PT_Ghelld_Button` ; « Coins » `CONTROL_PrivateTrade_Cash_Button` ; croix `CONTROL_PT_Cancel_Button` ;
//! * `UI_PrivateTradeRequest` : « Waiting for a reply from %s. » (`Diaplay_PT_Request`, sysstr 520), « Cancel » et
//!   croix (`PT_RequestCancel`) ;
//! * `UI_PrivateTradeYesNo` : « Do you wish to trade with %s? » (`PrivateTraderName`, sysstr 519), « Yes »
//!   (`PT_Accept`), « No » et croix (`PT_Reject`) ;
//! * `UI_PrivateTrade_Money` : montant `CONTROL_PT_SendGhelldEdit`, « Confirm » (`PT_UpdateMoney`), « Cancel ».
//!
//! Écarts : les grilles Ex (`CONTROL_PT_Grid_Ex`, objets de genre 0x23) restent vides et « Coins » inactif
//! (pièces Chantra inconnues d'ArchCore, `UI_PrivateTrade_Cash` non reprise). Les demandes reviennent au moteur
//! en [`TradeUi`] (voir `app_trade.rs`).

use super::items_ui::icon as item_icon;
use super::npc_ui::{frame, window};
use super::portal_ui::{argb, control_text, state_button, static_text};
use super::*;

/// Fenêtres de l'échange, chargées avec le HUD.
pub(super) const TRADE_WINDOWS: [&str; 4] = ["UI_PrivateTrade", "UI_PrivateTradeRequest", "UI_PrivateTradeYesNo", "UI_PrivateTrade_Money"];
/// Lignes et colonnes de la grille d'échange (`CONTROL_PT_Grid` 210 × 158, ArchCore `TRADE_GRID`).
const GRID_ROWS: i16 = 3;
const GRID_COLS: i16 = 4;
/// Texte rouge des statuts « appuyé » (`Status2=…:-4980736`).
const PRESSED_TEXT: u32 = 0xFF_B4_00_00;

/// Objet d'une grille d'échange.
#[derive(Debug, Clone)]
pub struct TradeSlot {
    pub iid: i32,
    pub tid: i32,
    pub count: i32,
    /// Case de la grille d'échange (sac, ligne, colonne).
    pub grid: [i16; 3],
    pub name: String,
}

pub struct TradeView<'a> {
    pub me: &'a str,
    pub peer: &'a str,
    pub mine: &'a [TradeSlot],
    pub theirs: &'a [TradeSlot],
    pub my_gold: i64,
    pub peer_gold: i64,
    pub my_cash: i64,
    pub peer_cash: i64,
    /// `CONTROL_PT_Lock` au statut 1 (verrouillé).
    pub locked: bool,
    pub lock_enabled: bool,
    pub ready_enabled: bool,
    /// `CONTROL_PT_ReadyToExchange` au statut 1 (prêt envoyé).
    pub ready_pressed: bool,
    /// Statuts de `CONTROL_PTC_LOCK` (0 à 3) et de `CONTROL_PTC_ReadyToExchange` (0 à 4).
    pub peer_lock: usize,
    pub peer_ready: usize,
}

/// Demandes des fenêtres de l'échange.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeUi {
    Lock,
    Unlock,
    Ready,
    /// Croix de `UI_PrivateTrade` (`PT_Cancel`).
    Close,
    /// « Gold » : `UI_PrivateTrade_Money`.
    Gold,
    /// Objet lâché sur la case `cell` de sa grille d'échange (`PT_MoveItem`).
    Offer { drag: ItemDrag, cell: [i16; 3] },
}

/// Texture (indice de `Texture=`) et couleur du texte des statuts de `CONTROL_PTC_LOCK`
/// (`Unlock`, `Lock`, `LCPU`, `LCPD`) et de `CONTROL_PTC_ReadyToExchange` (`Activate`, `De-activate`,
/// `Incompetency`, `TCBU`, `TCBD`), relevés dans `Status2=`.
fn peer_status(lock: bool, status: usize) -> (u32, u32) {
    let white = 0xFF_FF_FF_FF;
    if lock {
        match status {
            1 => (2, white),
            3 => (2, PRESSED_TEXT),
            _ => (1, white),
        }
    } else {
        match status {
            1 | 3 => (1, white),
            4 => (2, PRESSED_TEXT),
            _ => (2, white),
        }
    }
}

/// Bouton dessiné à l'état « appuyé » de son statut 1 : texture B, texte rouge.
fn pressed(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, r: Rect, c: &UiControl, font: f32) {
    if let Some((_, t)) = c.textures.iter().find(|(i, _)| *i == 2) {
        control_image(ctx, hud, assets, painter, r, c.size, t, 1.0);
    }
    shadow_text(painter, r.center(), Align2::CENTER_CENTER, static_text(c), font, argb(PRESSED_TEXT));
}

/// Cases d'une grille d'échange : objets, survol ; la sienne accepte un objet du sac et se vide par glisser.
#[allow(clippy::too_many_arguments)]
fn trade_grid(ctx: &egui::Context, ui: &mut egui::Ui, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, layout: &Layout, origin: Pos2, c: &UiControl, items: &[TradeSlot], mine: bool, font: f32, out: &mut Vec<TradeUi>) {
    let s = layout.scale;
    let [gx, gy, cw, ch] = c.grid.unwrap_or([2.0, 2.0, 50.0, 50.0]);
    let cols = (((c.size[0] + gx) / (cw + gx)).floor() as i16).clamp(1, GRID_COLS);
    let rows = (((c.size[1] + gy) / (ch + gy)).floor() as i16).clamp(1, GRID_ROWS);
    let dragging = egui::DragAndDrop::payload::<ItemDrag>(ctx).map(|p| *p);
    for row in 0..rows {
        for col in 0..cols {
            let cell = layout.rect(origin, [c.pos[0] + col as f32 * (cw + gx), c.pos[1] + row as f32 * (ch + gy)], [cw, ch]);
            let sense = if mine { egui::Sense::click_and_drag() } else { egui::Sense::hover() };
            let resp = ui.interact(cell, egui::Id::new(("trade-cell", mine, row, col)), sense);
            if let Some(item) = items.iter().find(|i| i.grid == [0, row, col]) {
                let lifted = dragging.is_some_and(|d| d.iid == item.iid);
                item_icon(ctx, hud, assets, painter, cell, Some(item.tid), if lifted { Color32::from_white_alpha(70) } else { Color32::WHITE });
                if item.count > 1 && !lifted {
                    shadow_text(painter, cell.right_bottom() - Vec2::new(3.0, 2.0) * s, Align2::RIGHT_BOTTOM, &item.count.to_string(), font, Color32::WHITE);
                }
                if mine && resp.dragged() {
                    resp.dnd_set_drag_payload(ItemDrag { iid: item.iid, tid: item.tid, worn: false, bank: false });
                }
                if dragging.is_none() {
                    let label = item.name.clone();
                    resp.clone().on_hover_text(label);
                }
            }
            if !mine {
                continue;
            }
            if let Some(drag) = resp.dnd_release_payload::<ItemDrag>() {
                out.push(TradeUi::Offer { drag: *drag, cell: [0, row, col] });
            }
            let target = dragging.is_some() && resp.contains_pointer();
            if resp.hovered() || target {
                let color = if target { Color32::from_rgb(255, 210, 110) } else { Color32::from_rgb(120, 200, 255) };
                painter.rect_stroke(cell, 2.0, egui::Stroke::new(1.0_f32, color), egui::StrokeKind::Inside);
            }
        }
    }
}

/// Fenêtre de l'échange.
pub fn draw_private_trade(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &TradeView, out: &mut Vec<TradeUi>) {
    let Some(w) = window(hud, assets, "UI_PrivateTrade") else { return };
    frame(ctx, hud, assets, &w, "archonia-private-trade", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match c.name.as_str() {
                "CONTROL_PT_Grid" | "CONTROL_PTC_Grid" => {
                    let mine = c.name == "CONTROL_PT_Grid";
                    trade_grid(ctx, ui, hud, assets, painter, layout, origin, c, if mine { view.mine } else { view.theirs }, mine, font, out);
                }
                "CONTROL_PT_NameStatic" => control_text(painter, r, c, view.me, font, None),
                "CONTROL_PT_ClientNameStatic" => control_text(painter, r, c, view.peer, font, None),
                "CONTROL_7122" | "CONTROL_793" => {
                    let gold = if c.name == "CONTROL_7122" { view.my_gold } else { view.peer_gold };
                    shadow_text(painter, r.right_center(), Align2::RIGHT_CENTER, &gold.to_string(), font, argb(c.font.map_or(u32::MAX, |f| f.0)));
                }
                "CONTROL_PrivateTrade_SelfCashDisplay" | "CONTROL_PrivateTrade_TargetCashDisplay" => {
                    let cash = if c.name.ends_with("SelfCashDisplay") { view.my_cash } else { view.peer_cash };
                    shadow_text(painter, r.right_center(), Align2::RIGHT_CENTER, &cash.to_string(), font, argb(c.font.map_or(u32::MAX, |f| f.0)));
                }
                "CONTROL_PT_Lock" => {
                    if view.locked {
                        // Second clic du bouton à bascule : `PT_Unlock`.
                        let resp = ui.interact(r, egui::Id::new("trade-lock"), if view.lock_enabled { egui::Sense::click() } else { egui::Sense::hover() });
                        pressed(ctx, hud, assets, painter, r, c, font);
                        if resp.clicked() {
                            out.push(TradeUi::Unlock);
                        }
                    } else if state_button(ctx, ui, hud, assets, painter, r, c, static_text(c), font, view.lock_enabled, "trade-lock").clicked() {
                        out.push(TradeUi::Lock);
                    }
                }
                "CONTROL_PT_ReadyToExchange" => {
                    if view.ready_pressed {
                        pressed(ctx, hud, assets, painter, r, c, font);
                    } else if state_button(ctx, ui, hud, assets, painter, r, c, static_text(c), font, view.ready_enabled, "trade-ready").clicked() {
                        out.push(TradeUi::Ready);
                    }
                }
                "CONTROL_PTC_LOCK" | "CONTROL_PTC_ReadyToExchange" => {
                    let lock = c.name == "CONTROL_PTC_LOCK";
                    let (tex, color) = peer_status(lock, if lock { view.peer_lock } else { view.peer_ready });
                    if let Some((_, t)) = c.textures.iter().find(|(i, _)| *i == tex) {
                        control_image(ctx, hud, assets, painter, r, c.size, t, 1.0);
                    }
                    shadow_text(painter, r.center(), Align2::CENTER_CENTER, static_text(c), font, argb(color));
                }
                "CONTROL_PT_Ghelld_Button" | "CONTROL_PrivateTrade_Cash_Button" => {
                    let gold = c.name == "CONTROL_PT_Ghelld_Button";
                    if state_button(ctx, ui, hud, assets, painter, r, c, static_text(c), font, gold, ("trade-money", gold)).clicked() {
                        out.push(TradeUi::Gold);
                    }
                }
                "CONTROL_PT_Cancel_Button" => {
                    if state_button(ctx, ui, hud, assets, painter, r, c, "", font, true, "trade-x").clicked() {
                        out.push(TradeUi::Close);
                    }
                }
                _ => {}
            }
        }
    });
}

/// `UI_PrivateTradeRequest` : rend `true` à l'annulation (« Cancel », croix ou Échap).
pub fn draw_trade_request(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, text: &str) -> bool {
    let out = super::dialogs_ui::draw_dialog(ctx, hud, assets, "UI_PrivateTradeRequest", |_, _, painter, c, r, font| {
        if c.name == "CONTROL_27869" {
            shadow_text(painter, r.center(), Align2::CENTER_CENTER, text, font, Color32::WHITE);
            return true;
        }
        false
    });
    out.is_some_and(|o| matches!(o.clicked.as_deref(), Some("CONTROL_9655" | "CONTROL_23785")) || o.escape)
}

/// `UI_PrivateTradeYesNo` : `Some(true)` « Yes », `Some(false)` « No », croix ou Échap.
pub fn draw_trade_yes_no(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, text: &str) -> Option<bool> {
    let out = super::dialogs_ui::draw_dialog(ctx, hud, assets, "UI_PrivateTradeYesNo", |_, _, painter, c, r, font| {
        if c.name == "CONTROL_30890" {
            shadow_text(painter, r.center(), Align2::CENTER_CENTER, text, font, Color32::WHITE);
            return true;
        }
        false
    })?;
    match out.clicked.as_deref() {
        Some("CONTROL_PT_YesButton") => Some(true),
        Some("CONTROL_PT_NoButton" | "CONTROL_3422") => Some(false),
        _ => out.escape.then_some(false),
    }
}

/// `UI_PrivateTrade_Money` : montant saisi (borné à l'or porté) ; `Some(true)` « Confirm » ou Entrée,
/// `Some(false)` « Cancel », croix ou Échap.
pub fn draw_trade_money(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, amount: &mut i64, max: i64) -> Option<bool> {
    let out = super::dialogs_ui::draw_dialog(ctx, hud, assets, "UI_PrivateTrade_Money", |ui, hud, painter, c, r, font| {
        if c.name == "CONTROL_PT_SendGhelldEdit" {
            if let Some((_, t)) = c.textures.first() {
                control_image(ctx, hud, assets, painter, r, c.size, t, 1.0);
            }
            super::dialogs_ui::number_edit(ui, r, "trade-money", amount, max, font);
            return true;
        }
        false
    })?;
    match out.clicked.as_deref() {
        Some("CONTROL_PT_SendGhelldYesButton") => return Some(true),
        Some("CONTROL_PT_SendGhelldCancelButton" | "CONTROL_11593") => return Some(false),
        _ => {}
    }
    if out.enter {
        return Some(true);
    }
    out.escape.then_some(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuts_de_l_autre() {
        // `PT_ClientLock` : statut 3 (texture B, rouge) ; `PT_ClientUnlock` : 2 ; au départ 0.
        assert_eq!(peer_status(true, 0), (1, 0xFF_FF_FF_FF));
        assert_eq!(peer_status(true, 3), (2, PRESSED_TEXT));
        assert_eq!(peer_status(true, 2), (1, 0xFF_FF_FF_FF));
        // « Trade » de l'autre : 2 au départ, 3 quand les deux sont verrouillés, 4 quand il est prêt.
        assert_eq!(peer_status(false, 2), (2, 0xFF_FF_FF_FF));
        assert_eq!(peer_status(false, 3), (1, 0xFF_FF_FF_FF));
        assert_eq!(peer_status(false, 4), (2, PRESSED_TEXT));
    }
}
