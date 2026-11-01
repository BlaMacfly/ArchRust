//! Fenêtres d'amis et d'ignorés d'Archonia (AgcmUIBuddy), d'après `UI_1024X768.ini` :
//!
//! * `UI_BuddyList` (« Friends », 184 × 323) : liste `BUDDYLIST_LISTBOX` de 10 lignes de 16 px
//!   (`BUDDYLIST_ITEM_ID` nom, `BUDDYLIST_ITEM_STATUS` icône `GD_Off` / `GD_On`, `BUDDYLIST_ITEM_BGBTN`
//!   sélection, `BUDDYLIST_SELBAR` barre `Buddylist_SELb.png` de la ligne choisie), barre de défilement
//!   (`…_SCROLL_UPBTN`, `…_SCROLL_DNBTN`, curseur `…_SCROLL_MBTN`), détail `BUDDYLIST_DETAILINFO` (nom et
//!   région), boutons Remove, Add, Party, Whisper et la croix ;
//! * `UI_BanList` (« Ignore list ») : même liste (`BANLIST_*`), cases `BANLIST_BANTALKBTN`
//!   (« Block Chat »), `BANLIST_BANDEALBTN` (« Block Trade »), `BANLIST_BANINVITEBTN` (« Block Invites »)
//!   de l'ignoré choisi, boutons Add et Remove ;
//! * `UI_AddBuddyList` / `UI_AddBanList` : invite, saisie du nom (`ADDBUDDYLIST_EDITBOX`,
//!   `ADDBANLIST_EDIT`), bouton Add ;
//! * `UI_TargetUserMenu` : bouton « Menu » à côté de la cible quand c'est un joueur, qui ouvre
//!   `UI_TargetCharacterMenu` (entrées gérées : Invite, Trade, Whisper, Add Friend, Ignore User).
//!
//! Les demandes reviennent au moteur en [`BuddyUi`] (voir `app_buddy.rs`).

use super::npc_ui::{button, frame, window};
use super::options_ui::checkbox;
use super::portal_ui::{control_text, state_button, static_text};
use super::*;

/// Fenêtres d'amis et d'ignorés, chargées avec le HUD (textes fixes d'`ini_fr`).
pub(super) const BUDDY_WINDOWS: [&str; 5] = ["UI_BuddyList", "UI_BanList", "UI_AddBuddyList", "UI_AddBanList", "UI_TargetUserMenu"];
/// Lignes visibles des listes (`List=10`) et leur hauteur.
pub const LIST_ROWS: usize = 10;
const ROW_H: f32 = 16.0;

/// Ce que montre `UI_BuddyList` : noms et état en ligne, ligne choisie, première ligne, détail.
pub struct BuddyListView<'a> {
    pub rows: &'a [(String, bool)],
    pub selected: Option<usize>,
    pub first: usize,
    pub detail: &'a str,
}

/// Ce que montre `UI_BanList` : noms, ligne choisie, première ligne, cases de l'ignoré choisi.
pub struct BanListView<'a> {
    pub rows: &'a [String],
    pub selected: Option<usize>,
    pub first: usize,
    pub flags: Option<u32>,
}

/// Demandes des fenêtres d'amis et d'ignorés.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuddyUi {
    CloseFriends,
    SelectFriend(usize),
    ScrollFriends(i32),
    /// `BuddyDeleteList_Click`.
    RemoveFriend,
    /// `BuddyAddDlgOpen_Click`.
    OpenAddFriend,
    /// `BuddyInviteFriend_Click`.
    InviteFriend,
    /// `BuddyWhiperMsgFriend_Click`.
    WhisperFriend,
    CloseBans,
    SelectBan(usize),
    ScrollBans(i32),
    OpenAddBan,
    RemoveBan,
    /// Case d'un ignoré cliquée (bit 0x1, 0x2 ou 0x4).
    ToggleBan(u32),
    /// Bouton Add (ou Entrée) de `UI_AddBuddyList` / `UI_AddBanList`.
    AddFriend,
    AddBan,
    CloseAddFriend,
    CloseAddBan,
}

/// Entrées du menu d'un joueur ciblé (`UI_TargetCharacterMenu`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetMenuUi {
    /// `CONTROL_828` « Invite » (`PartyInviteMember`).
    Invite,
    /// `CONTROL_32674` « Trade » (`PT_RequestTrade`, montré si `IsActivePrivateTradeMemu`, voir `app_trade.rs`).
    Trade,
    /// `CONTROL_23115` « Whisper » (`Chat_SendWhisper`).
    Whisper,
    /// `CONTROL_BUDDY` « Add Friend » (`BuddyAddFriend_Click`).
    AddFriend,
    /// `CONTROL_BAN` « Ignore User » (`BanAdd_Click`).
    Ignore,
}

/// Liste à lignes de 16 px d'une des deux fenêtres (`prefix` : `BUDDYLIST` ou `BANLIST`) : noms, icône en
/// ligne (amis), barre de la ligne choisie, défilement ; rend la ligne cliquée et le défilement demandé.
#[allow(clippy::too_many_arguments)]
fn name_list(ctx: &egui::Context, ui: &mut egui::Ui, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, layout: &Layout, origin: Pos2, w: &UiWindow, prefix: &str, rows: &[(String, Option<bool>)], selected: Option<usize>, first: usize) -> (Option<usize>, i32) {
    let s = layout.scale;
    let font = (11.0 * s).max(9.0);
    let ctl = |n: &str| w.control(&format!("{prefix}_{n}")).cloned();
    let (mut clicked, mut scroll) = (None, 0);
    if let Some(list) = ctl("LISTBOX") {
        let r = layout.rect(origin, list.pos, list.size);
        if ui.rect_contains_pointer(r) {
            let wheel = ui.input(|i| i.raw_scroll_delta.y);
            if wheel.abs() > 0.5 {
                scroll = if wheel > 0.0 { -1 } else { 1 };
            }
        }
    }
    let (id_c, status_c, bg_c, bar_c) = (ctl("ITEM_ID"), ctl("ITEM_STATUS"), ctl("ITEM_BGBTN"), ctl("SELBAR"));
    for (k, (name, online)) in rows.iter().enumerate().skip(first).take(LIST_ROWS) {
        let dy = (k - first) as f32 * ROW_H;
        let at = |c: &UiControl| layout.rect(origin, [c.pos[0], c.pos[1] + dy], c.size);
        if let Some(c) = bar_c.as_ref().filter(|_| selected == Some(k)) {
            if let Some((_, t)) = c.textures.first() {
                control_image(ctx, hud, assets, painter, at(c), c.size, t, 1.0);
            }
        }
        if let Some(c) = &bg_c {
            // `Button=0:1:1` : texture seulement au survol et à l'appui.
            if state_button(ctx, ui, hud, assets, painter, at(c), c, "", font, true, ("buddy-row", prefix, k)).clicked() {
                clicked = Some(k);
            }
        }
        if let Some(c) = &id_c {
            control_text(painter, at(c), c, name, font, None);
        }
        if let (Some(c), Some(on)) = (&status_c, online) {
            // `Status2` : 0 « ON » → texture 2 (`GD_On`), 1 « OFF » → texture 1 (`GD_Off`).
            let index = if *on { 2 } else { 1 };
            if let Some((_, t)) = c.textures.iter().find(|(i, _)| *i == index) {
                control_image(ctx, hud, assets, painter, at(c), c.size, t, 1.0);
            }
        }
    }
    for (name, step) in [("SCROLL_UPBTN", -1), ("SCROLL_DNBTN", 1)] {
        if let Some(c) = ctl(name) {
            let enabled = if step < 0 { first > 0 } else { first + LIST_ROWS < rows.len() };
            if state_button(ctx, ui, hud, assets, painter, layout.rect(origin, c.pos, c.size), &c, "", font, enabled, ("buddy-scroll", prefix, name)).clicked() {
                scroll = step;
            }
        }
    }
    // Curseur : de 16 à 160 - 16 - 51 px dans la barre.
    if let Some(c) = ctl("SCROLL_MBTN") {
        let span = rows.len().saturating_sub(LIST_ROWS).max(1) as f32;
        let travel = (160.0 - 32.0 - c.size[1]).max(0.0);
        let y = (first as f32 / span).clamp(0.0, 1.0) * travel;
        if let Some((_, t)) = c.textures.first() {
            control_image(ctx, hud, assets, painter, layout.rect(origin, [c.pos[0], c.pos[1] + y], c.size), c.size, t, 1.0);
        }
    }
    (clicked, scroll)
}

/// Fenêtre de la liste d'amis.
pub fn draw_buddy_list(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &BuddyListView, out: &mut Vec<BuddyUi>) {
    let Some(w) = window(hud, assets, "UI_BuddyList") else { return };
    let rows: Vec<(String, Option<bool>)> = view.rows.iter().map(|(n, on)| (n.clone(), Some(*on))).collect();
    frame(ctx, hud, assets, &w, "archonia-buddy-list", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let (clicked, scroll) = name_list(ctx, ui, hud, assets, painter, layout, origin, &w, "BUDDYLIST", &rows, view.selected, view.first);
        out.extend(clicked.map(BuddyUi::SelectFriend));
        if scroll != 0 {
            out.push(BuddyUi::ScrollFriends(scroll));
        }
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            let action = match c.name.as_str() {
                "BUDDYLIST_TITLE1" => {
                    control_text(painter, r, c, static_text(c), font, None);
                    None
                }
                "BUDDYLIST_DETAILINFO" => {
                    super::convert_ui::edit_box_aligned(painter, r, view.detail, font, egui::Align::Min);
                    None
                }
                "BUDDYLIST_DELETEBTN" => Some(BuddyUi::RemoveFriend),
                "BUDDYLIST_ADDBTN" => Some(BuddyUi::OpenAddFriend),
                "BUDDYLIST_INVITEBTN" => Some(BuddyUi::InviteFriend),
                "BUDDYLIST_WHISPERBTN" => Some(BuddyUi::WhisperFriend),
                "BUDDYLIST_EXITBTN" => Some(BuddyUi::CloseFriends),
                _ => None,
            };
            if let Some(a) = action {
                if state_button(ctx, ui, hud, assets, painter, r, c, static_text(c), font, true, ("buddy-list-btn", c.name.as_str())).clicked() {
                    out.push(a);
                }
            }
        }
    });
}

/// Fenêtre de la liste d'ignorés.
pub fn draw_ban_list(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &BanListView, out: &mut Vec<BuddyUi>) {
    let Some(w) = window(hud, assets, "UI_BanList") else { return };
    let rows: Vec<(String, Option<bool>)> = view.rows.iter().map(|n| (n.clone(), None)).collect();
    frame(ctx, hud, assets, &w, "archonia-ban-list", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let (clicked, scroll) = name_list(ctx, ui, hud, assets, painter, layout, origin, &w, "BANLIST", &rows, view.selected, view.first);
        out.extend(clicked.map(BuddyUi::SelectBan));
        if scroll != 0 {
            out.push(BuddyUi::ScrollBans(scroll));
        }
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            let bit = match c.name.as_str() {
                "BANLIST_BANTALKBTN" => Some(archrust_netclient::buddy::flag::BLOCK_CHAT),
                "BANLIST_BANDEALBTN" => Some(archrust_netclient::buddy::flag::BLOCK_TRADE),
                "BANLIST_BANINVITEBTN" => Some(archrust_netclient::buddy::flag::BLOCK_INVITE),
                _ => None,
            };
            if let Some(bit) = bit {
                let checked = view.flags.is_some_and(|f| f & bit != 0);
                if checkbox(ctx, ui, hud, assets, painter, r, c, checked, true, ("ban-check", c.name.as_str())).clicked() {
                    out.push(BuddyUi::ToggleBan(bit));
                }
                continue;
            }
            let action = match c.name.as_str() {
                "BANLIST_TITLE1" | "BANLIST_STATIC1" | "BANLIST_STATIC2" | "BANLIST_STATIC3" => {
                    control_text(painter, r, c, static_text(c), font, None);
                    None
                }
                "BANLIST_ADDBTN" => Some(BuddyUi::OpenAddBan),
                "BANLIST_DELETEBTN" => Some(BuddyUi::RemoveBan),
                "BANLIST_EXITBTN" => Some(BuddyUi::CloseBans),
                _ => None,
            };
            if let Some(a) = action {
                if state_button(ctx, ui, hud, assets, painter, r, c, static_text(c), font, true, ("ban-list-btn", c.name.as_str())).clicked() {
                    out.push(a);
                }
            }
        }
    });
}

/// Boîte de saisie d'un nom (`UI_AddBuddyList` ou `UI_AddBanList`), au premier plan.
pub fn draw_add_name(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, ban: bool, text: &mut String, out: &mut Vec<BuddyUi>) {
    let name = if ban { "UI_AddBanList" } else { "UI_AddBuddyList" };
    let Some(w) = window(hud, assets, name) else { return };
    let (add, close) = if ban { (BuddyUi::AddBan, BuddyUi::CloseAddBan) } else { (BuddyUi::AddFriend, BuddyUi::CloseAddFriend) };
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let font = (11.0 * s).max(9.0);
    let size = Vec2::new(w.size[0], w.size[1]) * s;
    egui::Area::new(egui::Id::new(("archonia-add-name", ban)))
        .default_pos(layout.window_origin(&w))
        .movable(true)
        .constrain(true)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let painter = ui.painter().clone();
            if let Some((_, tex)) = w.textures.first() {
                control_image(ctx, hud, assets, &painter, rect, w.size, tex, 1.0);
            }
            for c in &w.controls {
                let r = layout.rect(rect.min, c.pos, c.size);
                match c.kind {
                    2 => {
                        if let Some((_, t)) = c.textures.first() {
                            control_image(ctx, hud, assets, &painter, r, c.size, t, 1.0);
                        }
                        let edit = egui::TextEdit::singleline(text)
                            .id(egui::Id::new(("archonia-add-name-edit", ban)))
                            .frame(false)
                            .char_limit(archrust_netclient::buddy::NAME_MAX + 1)
                            .font(FontId::proportional(font))
                            .text_color(Color32::WHITE);
                        let resp = ui.put(r.shrink2(Vec2::new(3.0 * s, 0.0)), edit);
                        if !resp.has_focus() && !resp.lost_focus() && ui.memory(|m| m.focused().is_none()) {
                            resp.request_focus();
                        }
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            out.push(add);
                        }
                    }
                    3 => {
                        let a = if c.name.ends_with("_ADDBTN") { add } else { close };
                        if state_button(ctx, ui, hud, assets, &painter, r, c, static_text(c), font, true, ("add-name-btn", ban, c.name.as_str())).clicked() {
                            out.push(a);
                        }
                    }
                    _ => control_text(&painter, r, c, static_text(c), font, None),
                }
            }
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        out.push(close);
    }
}

/// Bouton « Menu » d'`UI_TargetUserMenu` (cible joueur) et, ouvert, les entrées d'`UI_TargetCharacterMenu`
/// empilées comme dans le fichier d'interface ; `entries` : entrées visibles.
pub fn draw_target_menu(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, open: &mut bool, entries: &[TargetMenuUi], out: &mut Vec<TargetMenuUi>) {
    let Some(bar) = window(hud, assets, "UI_TargetUserMenu") else { return };
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let font = (10.0 * s).max(8.0);
    let origin = layout.window_origin(&bar);
    egui::Area::new(egui::Id::new("archonia-target-user-menu"))
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
                if button(ctx, ui, hud, assets, &painter, r, c, static_text(c), font, true, ("target-user-menu", c.name.as_str())).clicked() {
                    *open = !*open;
                }
            }
        });
    if !*open {
        return;
    }
    let Some(w) = window(hud, assets, "UI_TargetCharacterMenu") else { return };
    let at = layout.window_origin(&w);
    let mut chosen = None;
    egui::Area::new(egui::Id::new("archonia-target-character-menu"))
        .fixed_pos(at)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let painter = ui.painter().clone();
            let mut y = 0.0;
            for e in entries {
                let control = match e {
                    TargetMenuUi::Invite => "CONTROL_828",
                    TargetMenuUi::Trade => "CONTROL_32674",
                    TargetMenuUi::Whisper => "CONTROL_23115",
                    TargetMenuUi::AddFriend => "CONTROL_BUDDY",
                    TargetMenuUi::Ignore => "CONTROL_BAN",
                };
                let Some(c) = w.control(control) else { continue };
                let r = Rect::from_min_size(at + Vec2::new(0.0, y) * s, Vec2::new(c.size[0], c.size[1]) * s);
                y += c.size[1];
                ui.allocate_rect(r, egui::Sense::hover());
                if button(ctx, ui, hud, assets, &painter, r, c, static_text(c), font, true, ("target-character-menu", control)).clicked() {
                    chosen = Some(*e);
                }
            }
        });
    if let Some(e) = chosen {
        out.push(e);
        *open = false;
    } else if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        *open = false;
    }
}
