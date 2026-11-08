//! Salons de discussion d'Archonia (AgcmUIChannel) : fenêtres `UI_CreateChatRoom` (nom, mot de passe, genre
//! Public / Private / Guild Room), `UI_EnterChatRoom`, `UI_InviteChatRoom` (invitation reçue),
//! `UI_InviteChatRoom2` (inviter), `UI_QueryPassword`, `UI_FindFriend` (chercher un joueur, chuchoter) et
//! `UI_ChatRoom` (gabarit cloné en cinq fenêtres : titre `ChannelTitle_Display`, membres `UserList_Display`,
//! messages `ChatList_Display`, saisie `ChannelSend_ChatMessage`, « Invite », « Leave » et croix).
//!
//! Tout vient d'`UI_1024X768.ini` (fonds `charcreateroom_base.png`, `chatenterroom_base.png`,
//! `chatroom_base.png`, `chat_base.png`, cases `OP_Check_*`) ; les demandes reviennent au moteur en
//! [`ChatRoomUi`] (voir `app_chat.rs`).

use super::convert_ui::edit_box;
use super::dialogs_ui::draw_dialog;
use super::npc_ui::{button, frame, window};
use super::portal_ui::{control_text, state_button, static_text};
use super::*;

/// Fenêtres chargées avec le HUD.
pub(super) const CHAT_WINDOWS: [&str; 8] = ["UI_CreateChatRoom", "UI_EnterChatRoom", "UI_InviteChatRoom", "UI_InviteChatRoom2", "UI_QueryPassword", "UI_FindFriend", "UI_ChatRoom", TAB_OPTION];
/// Options d'un onglet de la discussion (voir `app_chattabs.rs`).
const TAB_OPTION: &str = "UI_ChatTab_Option";
/// Canaux d'`UI_ChatTab_Option`, bit par bit : sysstr (`FUN_140516e10`) et clé de `color.ini` (`FUN_140516c60`).
const CHANNELS: [(u32, &str); 12] = [(821, "NORMAL_CHAT"), (822, "GUILD_CHAT"), (823, "PARTY_CHAT"), (824, "WHISPER_SEND"), (825, "SYSTEM_CHAT1"), (828, "NOTICE_CHAT1"), (829, "QUEST_CHAT"), (830, "SHRINE_CHAT"), (836, "Yellow"), (838, "WORD_BALLON"), (839, "RACE_TRUMPET"), (840, "Yellow")];
const ALL_CHANNELS: u16 = 0xFFF;

/// Choix d'un menu flottant de la discussion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatMenuAction {
    Font(u8),
    TabEnable(bool),
    AddTab,
    Delete,
    Change,
    Close,
}

/// Clics d'`UI_ChatTab_Option`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabOptionUi {
    Ok,
    Close,
}

/// Une entrée : libellé, sous-menu rattaché (rang), choix, actif (le choix courant est grisé).
struct MenuEntry {
    label: String,
    submenu: Option<u8>,
    action: Option<ChatMenuAction>,
    enabled: bool,
}

/// Fond d'un menu en neuf morceaux (`PopupBtn_Base1…9`, de gauche à droite, de haut en bas : découpage déduit des noms).
fn popup_frame(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, r: Rect) {
    let parts: Vec<_> = (1..=9).map(|i| hud.image(ctx, assets, &format!("PopupBtn_Base{i}.png"))).collect();
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    let (Some((_, c1)), Some((_, c9))) = (parts[0], parts[8]) else {
        painter.rect_filled(r, 2.0, Color32::from_black_alpha(220));
        return;
    };
    let xs = [r.left(), r.left() + c1[0], r.right() - c9[0], r.right()];
    let ys = [r.top(), r.top() + c1[1], r.bottom() - c9[1], r.bottom()];
    for j in 0..3 {
        for i in 0..3 {
            if let Some((tex, _)) = parts[j * 3 + i] {
                painter.image(tex, Rect::from_min_max(Pos2::new(xs[i], ys[j]), Pos2::new(xs[i + 1], ys[j + 1])), uv, Color32::WHITE);
            }
        }
    }
}

/// Barre de l'entrée survolée (`PopupBtn_Select1…3`).
fn popup_select(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, r: Rect) {
    let parts: Vec<_> = (1..=3).map(|i| hud.image(ctx, assets, &format!("PopupBtn_Select{i}.png"))).collect();
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    let (left, right) = (parts[0].map_or(0.0, |(_, s)| s[0]), parts[2].map_or(0.0, |(_, s)| s[0]));
    let xs = [r.left(), r.left() + left, r.right() - right, r.right()];
    for i in 0..3 {
        if let Some((tex, _)) = parts[i] {
            painter.image(tex, Rect::from_min_max(Pos2::new(xs[i], r.top()), Pos2::new(xs[i + 1], r.bottom())), uv, Color32::WHITE);
        }
    }
}

/// Dessine une colonne d'entrées vers le haut depuis `bottom_left` ; rend (survol d'un sous-menu, clic).
fn popup_column(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, id: &str, bottom_left: Pos2, entries: &[MenuEntry], font: f32) -> (Option<(u8, Pos2)>, Option<ChatMenuAction>, Rect) {
    let width = entries.iter().map(|e| ctx.fonts(|f| f.layout_no_wrap(e.label.clone(), FontId::proportional(font), Color32::WHITE).size().x)).fold(0.0, f32::max) + 32.0;
    let row = font + 8.0;
    let rect = Rect::from_min_size(bottom_left - Vec2::new(0.0, row * entries.len() as f32 + 12.0), Vec2::new(width, row * entries.len() as f32 + 12.0));
    let (mut sub, mut action) = (None, None);
    egui::Area::new(egui::Id::new(("archonia-chat-popup", id))).fixed_pos(rect.min).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.allocate_rect(rect, egui::Sense::click());
        let painter = ui.painter().clone();
        popup_frame(ctx, hud, assets, &painter, rect);
        for (i, e) in entries.iter().enumerate() {
            let r = Rect::from_min_size(rect.min + Vec2::new(4.0, 6.0 + row * i as f32), Vec2::new(width - 8.0, row));
            let resp = ui.interact(r, egui::Id::new(("archonia-chat-popup-row", id, i)), egui::Sense::click());
            if resp.hovered() && e.enabled {
                popup_select(ctx, hud, assets, &painter, r);
            }
            let color = if e.enabled { Color32::WHITE } else { Color32::GRAY };
            shadow_text(&painter, r.left_center() + Vec2::new(6.0, 0.0), Align2::LEFT_CENTER, &e.label, font, color);
            if let Some(index) = e.submenu {
                shadow_text(&painter, r.right_center() - Vec2::new(6.0, 0.0), Align2::RIGHT_CENTER, ">", font, color);
                if resp.hovered() {
                    sub = Some((index, Pos2::new(r.right(), r.bottom())));
                }
            }
            if resp.clicked() && e.enabled {
                action = e.action;
            }
        }
    });
    (sub, action, rect)
}

/// Menus flottants de la discussion (voir `app_chattabs.rs`) ; rend le choix, ou `Close` sur un clic ailleurs.
pub fn draw_chat_menu(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, menu: crate::ui::ChatMenu, font_now: u8, tab_enable: bool) -> Option<ChatMenuAction> {
    // Lu une fois : pas de `sysstr.txt` à chaque image.
    let text = |id: u32| ctx.data_mut(|d| d.get_temp_mut_or_insert_with(egui::Id::new(("archonia-chat-menu-text", id)), || crate::app::npc_glue::sysstr(assets, id).unwrap_or_default()).clone());
    let entry = |label: String, action: ChatMenuAction, enabled: bool| MenuEntry { label, submenu: None, action: Some(action), enabled };
    let layout = Layout::new(ctx.screen_rect());
    let font = (11.0 * layout.scale).max(9.0);
    let (at, root) = match menu {
        crate::ui::ChatMenu::Option { at } => (
            at,
            vec![
                MenuEntry { label: text(833), submenu: Some(0), action: None, enabled: true },
                MenuEntry { label: text(834), submenu: Some(1), action: None, enabled: true },
                entry(text(835), ChatMenuAction::AddTab, true),
            ],
        ),
        crate::ui::ChatMenu::Tab { at, .. } => (at, vec![entry(text(814), ChatMenuAction::Delete, true), entry(text(815), ChatMenuAction::Change, true)]),
    };
    let (hover, mut action, root_rect) = popup_column(ctx, hud, assets, "root", at, &root, font);
    // Le sous-menu reste ouvert tant que le pointeur est dans l'un des deux.
    let open_id = egui::Id::new("archonia-chat-popup-open");
    if hover.is_some() {
        ctx.data_mut(|d| d.insert_temp(open_id, hover));
    }
    let open = ctx.data(|d| d.get_temp::<Option<(u8, Pos2)>>(open_id)).flatten();
    let mut sub_rect = None;
    if let Some((index, pos)) = open {
        let entries: Vec<MenuEntry> = if index == 0 {
            crate::app::chattabs_glue::FONT_CHOICES.iter().map(|(f, px)| entry(format!("{px:.0}"), ChatMenuAction::Font(*f), *f != font_now)).collect()
        } else {
            vec![entry("ON".to_string(), ChatMenuAction::TabEnable(true), !tab_enable), entry("OFF".to_string(), ChatMenuAction::TabEnable(false), tab_enable)]
        };
        let (_, a, r) = popup_column(ctx, hud, assets, "sub", pos, &entries, font);
        action = action.or(a);
        sub_rect = Some(r);
    }
    if action.is_some() {
        ctx.data_mut(|d| d.remove::<Option<(u8, Pos2)>>(open_id));
        return action;
    }
    let outside = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !root_rect.contains(p) && !sub_rect.is_some_and(|r: Rect| r.contains(p))));
    if outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ctx.data_mut(|d| d.remove::<Option<(u8, Pos2)>>(open_id));
        return Some(ChatMenuAction::Close);
    }
    None
}

/// Libellé et couleur des 12 canaux.
fn channel_rows(assets: &Assets) -> Vec<(String, Color32)> {
    let mut colors = std::collections::HashMap::new();
    if let Ok(t) = assets.table("ini", "color.ini") {
        for (k, v) in t.sections.iter().flat_map(|s| s.entries.iter()) {
            if let Ok(c) = u32::from_str_radix(v.trim(), 16) {
                colors.entry(k.trim().to_string()).or_insert(c);
            }
        }
    }
    CHANNELS
        .iter()
        .map(|(id, key)| {
            let rgb: u32 = colors.get(*key).copied().unwrap_or(0xFF_FFFF);
            (crate::app::npc_glue::sysstr(assets, *id).unwrap_or_default(), Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8))
        })
        .collect()
}

/// Case de la liste : `Btn_Check_a` vide, `Btn_Check_b` cochée (seules images du contrôle ; sens déduit de `Chat_CheckDisplay`).
#[allow(clippy::too_many_arguments)]
fn tab_check(ctx: &egui::Context, ui: &mut egui::Ui, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, r: Rect, c: &UiControl, on: bool, id: impl std::hash::Hash) -> bool {
    let resp = ui.interact(r, egui::Id::new(id), egui::Sense::click());
    if let Some((_, tex)) = c.textures.get(on as usize).or(c.textures.first()) {
        control_image(ctx, hud, assets, painter, r, c.size, tex, 1.0);
    }
    resp.clicked()
}

/// `UI_ChatTab_Option` : nom, liste des 12 canaux (lignes de 28, `CONTROL_17321`, avec case et nom), « Tout sélectionner »,
/// barre de défilement, OK ; Annuler et la croix ferment. Positions de l'ini, déjà absolues dans la fenêtre.
pub fn draw_tab_option(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, name: &mut String, mask: &mut u16) -> Option<TabOptionUi> {
    let w = window(hud, assets, TAB_OPTION)?;
    let (check, label, row) = (w.control("CONTROL_6171")?.clone(), w.control("CONTROL_11160")?.clone(), w.control("CONTROL_17321")?.clone());
    let list_h = w.control("CONTROL_TABOPTION_LIST")?.size[1];
    // Rangées entières sous la marge de la liste (10).
    let per_page = (((list_h - (row.pos[1] - w.control("CONTROL_TABOPTION_LIST")?.pos[1])) / row.size[1]).floor() as usize).max(1);
    let last = CHANNELS.len().saturating_sub(per_page);
    // Libellés et couleurs lus une fois : ni `sysstr.txt` ni `color.ini` à chaque image.
    let rows = ctx.data_mut(|d| d.get_temp_mut_or_insert_with(egui::Id::new("archonia-chat-tab-channels"), || std::sync::Arc::new(channel_rows(assets))).clone());
    let first_id = egui::Id::new("archonia-chat-tab-first");
    let mut first = ctx.data(|d| d.get_temp::<usize>(first_id)).unwrap_or(0).min(last);
    let out = frame(ctx, hud, assets, &w, "archonia-chat-tab-option", |ui, hud, layout, origin, painter| {
        let font = (11.0 * layout.scale).max(9.0);
        let mut out = None;
        for (i, (text, color)) in rows.iter().enumerate().skip(first).take(per_page) {
            let dy = (i - first) as f32 * row.size[1];
            let bit = 1u16 << i;
            let r = layout.rect(origin, [check.pos[0], check.pos[1] + dy], check.size);
            if tab_check(ctx, ui, hud, assets, painter, r, &check, *mask & bit != 0, ("archonia-chat-tab-channel", i)) {
                *mask ^= bit;
            }
            control_text(painter, layout.rect(origin, [label.pos[0], label.pos[1] + dy], label.size), &label, text, font, Some(*color));
        }
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match c.name.as_str() {
                "CONTROL_TABOPTION_LIST" => {
                    if ui.rect_contains_pointer(r) {
                        let wheel = ui.input(|i| i.raw_scroll_delta.y);
                        if wheel < 0.0 {
                            first = (first + 1).min(last);
                        } else if wheel > 0.0 {
                            first = first.saturating_sub(1);
                        }
                    }
                }
                // Lignes : dessinées ci-dessus ; piste sans image.
                "CONTROL_17321" | "CONTROL_6171" | "CONTROL_11160" | "CONTROL_2654" => {}
                // Curseur, entre les deux boutons (177 → 294).
                "CONTROL_5597" => {
                    let course = (294.0 - c.pos[1] - c.size[1]).max(0.0);
                    let y = first as f32 / last.max(1) as f32 * course;
                    if let Some((_, tex)) = c.textures.first() {
                        control_image(ctx, hud, assets, painter, r.translate(Vec2::new(0.0, y * layout.scale)), c.size, tex, 1.0);
                    }
                }
                "CONTROL_30971" => {
                    if state_button(ctx, ui, hud, assets, painter, r, c, "", font, first > 0, "archonia-chat-tab-up").clicked() {
                        first -= 1;
                    }
                }
                "CONTROL_747" => {
                    if state_button(ctx, ui, hud, assets, painter, r, c, "", font, first < last, "archonia-chat-tab-down").clicked() {
                        first += 1;
                    }
                }
                "CONTROL_TABNAME_EDIT" => {
                    if let Some((_, tex)) = c.textures.first() {
                        control_image(ctx, hud, assets, painter, r, c.size, tex, 1.0);
                    }
                    edit_line(ui, r, "chat-tab-name", name, 32, false, font);
                }
                "CONTROL_TAB_SELECT_ALL" => {
                    let all = *mask & ALL_CHANNELS == ALL_CHANNELS;
                    if tab_check(ctx, ui, hud, assets, painter, r, c, all, "archonia-chat-tab-all") {
                        *mask = if all { 0 } else { ALL_CHANNELS };
                    }
                }
                "CONTROL_TAB_OK" => {
                    if state_button(ctx, ui, hud, assets, painter, r, c, static_text(c), font, true, "archonia-chat-tab-ok").clicked() {
                        out = Some(TabOptionUi::Ok);
                    }
                }
                "CONTROL_TAB_CANCEL" | "CONTROL_TAB_X" => {
                    if state_button(ctx, ui, hud, assets, painter, r, c, static_text(c), font, true, ("archonia-chat-tab-close", c.name.as_str())).clicked() {
                        out = Some(TabOptionUi::Close);
                    }
                }
                _ => control_text(painter, r, c, static_text(c), font, None),
            }
        }
        out
    })
    .flatten();
    ctx.data_mut(|d| d.insert_temp(first_id, first));
    out
}
/// Lignes de la liste des membres (`List=8`).
const MEMBER_ROWS: usize = 8;
/// Couleurs d'une ligne de salon (`FUN_1406e4f70` : `<C-26368>%s: <C%d>%s`) : nom orange, texte gris pour soi.
const NAME_COLOR: u32 = 0xFFFF_9900;
const OWN_TEXT: u32 = 0xFFB8_B8B8;

/// Demandes des fenêtres de salon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatRoomUi {
    /// Genre choisi dans `UI_CreateChatRoom` (1 public, 2 privé, 3 guilde).
    Kind(i32),
    Create,
    CloseCreate,
    /// `UI_EnterChatRoom`, `UI_QueryPassword`, `UI_InviteChatRoom2`, `UI_FindFriend` : validation ou fermeture.
    Confirm(&'static str),
    Close(&'static str),
    /// « Whisper » de `UI_FindFriend`.
    Whisper,
    /// Invitation reçue acceptée ou refusée.
    AcceptInvite,
    CloseInvite,
    /// Salon de la fenêtre `slot` : message, sortie, invitation.
    Send(usize),
    Leave(usize),
    Invite(usize),
}

/// Ligne de saisie d'Archonia (sans cadre egui), focalisée si rien d'autre ne l'est.
fn edit_line(ui: &mut egui::Ui, r: Rect, id: impl std::hash::Hash, text: &mut String, max: usize, password: bool, font: f32) -> egui::Response {
    let edit = egui::TextEdit::singleline(text)
        .id(egui::Id::new(("archonia-chat-edit", id)))
        .frame(false)
        .password(password)
        .char_limit(max)
        .font(FontId::proportional(font))
        .text_color(Color32::WHITE);
    let resp = ui.put(r.shrink2(Vec2::new(3.0, 0.0)), edit);
    if !resp.has_focus() && !resp.lost_focus() && ui.memory(|m| m.focused().is_none()) {
        resp.request_focus();
    }
    resp
}

fn enter_pressed(ui: &egui::Ui, resp: &egui::Response) -> bool {
    resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
}

/// `UI_CreateChatRoom` : nom (39 caractères), mot de passe (8), genre coché.
pub fn draw_create_room(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, title: &mut String, password: &mut String, kind: i32, out: &mut Vec<ChatRoomUi>) {
    let mut pushed = Vec::new();
    let res = draw_dialog(ctx, hud, assets, "UI_CreateChatRoom", |ui, hud, painter, c, r, font| match c.name.as_str() {
        "CreateChatRoom_Title_EDIT" => {
            let resp = edit_line(ui, r, "create-title", title, archrust_netclient::chat::channel::NAME_MAX, false, font);
            if enter_pressed(ui, &resp) {
                pushed.push(ChatRoomUi::Create);
            }
            true
        }
        "CreateChatRoom_PW_EDIT" => {
            edit_line(ui, r, "create-pw", password, archrust_netclient::chat::channel::PASSWORD_MAX, true, font);
            true
        }
        "CreateChatRoom_Property_CheckBtn1" | "CreateChatRoom_Property_CheckBtn2" | "CreateChatRoom_Property_CheckBtn3" => {
            let k = c.name.chars().last().and_then(|d| d.to_digit(10)).unwrap_or(1) as i32;
            if button(ctx, ui, hud, assets, painter, r, c, "", font, true, ("create-kind", k)).clicked() {
                pushed.push(ChatRoomUi::Kind(k));
            }
            // Case cochée : texture « appuyée » (`OP_Check_c`).
            if k == kind {
                if let Some((_, t)) = c.textures.iter().find(|(_, n)| n.to_ascii_lowercase().ends_with("c.png")) {
                    control_image(ctx, hud, assets, painter, r, c.size, t, 1.0);
                }
            }
            true
        }
        _ => false,
    });
    out.append(&mut pushed);
    let Some(res) = res else { return };
    match res.clicked.as_deref() {
        Some("CreateChatRoom_OKBtn") => out.push(ChatRoomUi::Create),
        Some("CreateChatRoom_XBTN") => out.push(ChatRoomUi::CloseCreate),
        _ if res.escape => out.push(ChatRoomUi::CloseCreate),
        _ => {}
    }
}

/// Fenêtres à une ligne de saisie : entrer dans un salon, inviter, mot de passe, chercher un joueur.
/// `found` active le bouton « Whisper » de `UI_FindFriend` (`WhisperBtn_Status`).
pub fn draw_prompt(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, name: &'static str, text: &mut String, found: bool, out: &mut Vec<ChatRoomUi>) {
    let (edit, ok, close, max, password) = match name {
        "UI_EnterChatRoom" => ("EnterChatRoom_Title_EDIT", "EnterChatRoom_OKBtn", "EnterChatRoom_XBTN", archrust_netclient::chat::channel::NAME_MAX, false),
        "UI_InviteChatRoom2" => ("InviteChatRoom2_EDIT1", "InviteChatRoom2_OKBTN", "InviteChatRoom2_XBTN", 32, false),
        "UI_QueryPassword" => ("QueryPassword_EDIT1", "QueryPassword_OKBTN", "QueryPassword_XBTN", archrust_netclient::chat::channel::PASSWORD_MAX, true),
        _ => ("FindFriend_EDIT1", "FindFriend_FindBTN", "FindFriend_XBTN", 32, false),
    };
    let mut pushed = Vec::new();
    let res = draw_dialog(ctx, hud, assets, name, |ui, hud, painter, c, r, font| {
        if c.name == edit {
            let resp = edit_line(ui, r, name, text, max, password, font);
            if enter_pressed(ui, &resp) {
                pushed.push(ChatRoomUi::Confirm(name));
            }
            return true;
        }
        if c.name == "FindFriend_WhisperBTN" {
            if button(ctx, ui, hud, assets, painter, r, c, static_text(c), font * 0.95, found, ("find-whisper", 0)).clicked() {
                pushed.push(ChatRoomUi::Whisper);
            }
            return true;
        }
        false
    });
    out.append(&mut pushed);
    let Some(res) = res else { return };
    match res.clicked.as_deref() {
        Some(n) if n == ok => out.push(ChatRoomUi::Confirm(name)),
        Some(n) if n == close => out.push(ChatRoomUi::Close(name)),
        _ if res.escape => out.push(ChatRoomUi::Close(name)),
        _ => {}
    }
}

/// `UI_InviteChatRoom` : texte de l'invitation (sysstr 286), « Enter » ou croix.
pub fn draw_invite_received(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, text: &str, out: &mut Vec<ChatRoomUi>) {
    let res = draw_dialog(ctx, hud, assets, "UI_InviteChatRoom", |_, _, painter, c, r, font| {
        if c.name == "InviteChatRoom_EDIT1" {
            edit_box(painter, r, text, font);
            return true;
        }
        false
    });
    let Some(res) = res else { return };
    match res.clicked.as_deref() {
        Some("InviteChatRoom_OKBTN") => out.push(ChatRoomUi::AcceptInvite),
        Some("InviteChatRoom_XBTN") => out.push(ChatRoomUi::CloseInvite),
        _ if res.escape => out.push(ChatRoomUi::CloseInvite),
        _ => {}
    }
}

/// Ce que montre une fenêtre `UI_ChatRoom%d`.
pub struct ChatRoomView<'a> {
    pub title: &'a str,
    pub members: &'a [String],
    /// Lignes : auteur, texte, écrit par soi.
    pub lines: &'a [(String, String, bool)],
    pub input: &'a mut String,
    /// Première ligne visible de la liste des membres.
    pub first_member: &'a mut usize,
}

/// Fenêtre d'un salon (`slot` 0 à 4, une fenêtre clonée par salon).
pub fn draw_chat_room(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, slot: usize, view: ChatRoomView, out: &mut Vec<ChatRoomUi>) {
    let Some(w) = window(hud, assets, "UI_ChatRoom") else { return };
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let size = Vec2::new(w.size[0], w.size[1]) * s;
    let font = (11.0 * s).max(9.0);
    // Les clones se posent au même endroit dans Archonia ; un léger décalage les laisse voir.
    let at = layout.window_origin(&w) + Vec2::splat(18.0 * slot as f32) * s;
    let ChatRoomView { title, members, lines, input, first_member } = view;
    egui::Area::new(egui::Id::new(("archonia-chatroom", slot)))
        .default_pos(at)
        .movable(true)
        .constrain(true)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let painter = ui.painter().clone();
            match w.textures.first() {
                Some((_, tex)) if hud.image(ctx, assets, tex).is_some() => control_image(ctx, hud, assets, &painter, rect, w.size, tex, 1.0),
                _ => {
                    painter.rect_filled(rect, 4.0, Color32::from_black_alpha(220));
                }
            }
            for c in &w.controls {
                let r = layout.rect(rect.min, c.pos, c.size);
                match c.name.as_str() {
                    "ChatRoom_STATIC1" => control_text(&painter, r, c, title, font, None),
                    "ChatRoom_LIST2" => {
                        let row_h = r.height() / MEMBER_ROWS as f32;
                        *first_member = (*first_member).min(members.len().saturating_sub(MEMBER_ROWS));
                        for (i, m) in members.iter().skip(*first_member).take(MEMBER_ROWS).enumerate() {
                            let p = Pos2::new(r.left() + 3.0 * s, r.top() + row_h * (i as f32 + 0.5));
                            shadow_text(&painter, p, Align2::LEFT_CENTER, m, font, Color32::WHITE);
                        }
                    }
                    "ChatRoom_Edit1" => {
                        let resp = edit_line(ui, r, ("room-input", slot), input, archrust_netclient::chat::channel::MESSAGE_MAX - 1, false, font);
                        if enter_pressed(ui, &resp) {
                            out.push(ChatRoomUi::Send(slot));
                            resp.request_focus();
                        }
                    }
                    "ChatRoom_ChatMsgEdit" => {
                        ui.scope_builder(egui::UiBuilder::new().max_rect(r.shrink2(Vec2::new(4.0, 2.0) * s)), |ui| {
                            egui::ScrollArea::vertical().id_salt(("archonia-chatroom-lines", slot)).stick_to_bottom(true).scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                                ui.set_width(r.width() - 24.0 * s);
                                for (who, text, mine) in lines {
                                    let mut job = egui::text::LayoutJob::default();
                                    let fmt = |c: u32| egui::TextFormat { font_id: FontId::proportional(font), color: super::portal_ui::argb(c), ..Default::default() };
                                    job.append(&format!("{who}: "), 0.0, fmt(NAME_COLOR));
                                    job.append(text, 0.0, fmt(if *mine { OWN_TEXT } else { 0xFFFF_FFFF }));
                                    job.wrap.max_width = r.width() - 24.0 * s;
                                    ui.label(job);
                                }
                            });
                        });
                    }
                    "CharRoom_XBTN" | "CharRoom_CloseBTN" | "CharRoom_InviteBTN" => {
                        let label = if c.name == "CharRoom_XBTN" { "" } else { static_text(c) };
                        if button(ctx, ui, hud, assets, &painter, r, c, label, font * 0.95, true, ("room-btn", slot, c.name.as_str())).clicked() {
                            out.push(if c.name == "CharRoom_InviteBTN" { ChatRoomUi::Invite(slot) } else { ChatRoomUi::Leave(slot) });
                        }
                    }
                    "ChatRoom_LIST_ScrollBar_UP" | "ChatRoom_LIST_ScrollBar_DN" => {
                        let up = c.name.ends_with("_UP");
                        if button(ctx, ui, hud, assets, &painter, r, c, "", font, true, ("room-list-scroll", slot, up)).clicked() {
                            *first_member = if up { first_member.saturating_sub(1) } else { (*first_member + 1).min(members.len().saturating_sub(MEMBER_ROWS)) };
                        }
                    }
                    "ChatRoom_ChatMsgEdit_ScrollUP" | "ChatRoom_ChatMsgEdit_ScrollDN" => {
                        // La molette fait défiler les messages ; les flèches restent l'habillage d'Archonia.
                        button(ctx, ui, hud, assets, &painter, r, c, "", font, true, ("room-chat-scroll", slot, c.name.as_str()));
                    }
                    _ => {}
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Avec les données du client : `UI_ChatTab_Option`, 12 canaux nommés et colorés, textures des menus.
    #[test]
    fn options_d_onglet() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let mut hud = HudState::default();
        let w = window(&mut hud, &assets, TAB_OPTION).expect("UI_ChatTab_Option absente");
        for c in ["CONTROL_TABOPTION_LIST", "CONTROL_17321", "CONTROL_6171", "CONTROL_11160", "CONTROL_TABNAME_EDIT", "CONTROL_TAB_SELECT_ALL", "CONTROL_5597", "CONTROL_30971", "CONTROL_747", "CONTROL_TAB_OK", "CONTROL_TAB_CANCEL", "CONTROL_TAB_X"] {
            assert!(w.control(c).is_some(), "{c} absent");
        }
        for t in ["MainChat_Btn_Option_a.png", "MainChat_Btn_Option_c.png", "PopupBtn_Base1.png", "PopupBtn_Base9.png", "PopupBtn_Select1.png", "PopupBtn_Select3.png"] {
            assert!(assets.texture_found(t), "{t}");
        }
        let rows = channel_rows(&assets);
        assert_eq!(rows[0].1, Color32::WHITE);
        assert_eq!(rows[1].1, Color32::from_rgb(0xFF, 0xA5, 0x00));
        assert_eq!(rows[8].1, Color32::from_rgb(0xFF, 0xFF, 0x00), "Yellow");
        assert!(rows.iter().all(|(l, _)| !l.is_empty()));
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : fenêtres des salons, du coiffeur et des avis, et les
    /// contrôles dont dépendent les fonctions de dessin.
    #[test]
    fn fenetres_d_archonia() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let mut hud = HudState::default();
        for (w, controls) in [
            ("UI_ChatRoom", &["ChatRoom_LIST2", "ChatRoom_Edit1", "ChatRoom_ChatMsgEdit", "CharRoom_InviteBTN"][..]),
            ("UI_CreateChatRoom", &["CreateChatRoom_Title_EDIT", "CreateChatRoom_Property_CheckBtn3", "CreateChatRoom_OKBtn"][..]),
            ("UI_FindFriend", &["FindFriend_EDIT1", "FindFriend_WhisperBTN"][..]),
            ("UI_Customizing", &["CONTROL_17928", "CONTROL_24801", "CONTROL_9217", "CONTROL_5228"][..]),
            ("UI_CustomizeBuyConfirm", &["CONTROL_24097", "CONTROL_8150"][..]),
            ("UI_WantedCriminal", &["CTL_ST_RANK", "CTL_ST_ONOFF", "CTL_BTN_PAGE10"][..]),
            ("UI_CriminalArrange2", &["CTL_ED_BOUNTY", "CTL_ST_MONEY"][..]),
            ("UI_SystemMessageGrid", &["CONTROL_32129"][..]),
        ] {
            let win = window(&mut hud, &assets, w).unwrap_or_else(|| panic!("{w} absente"));
            for c in controls {
                assert!(win.control(c).is_some(), "{w} : {c} absent");
            }
        }
        // Rangée de la liste des coiffures : barre posée sur la ligne (parent CONTROL_27760 dans CONTROL_27485).
        let w = window(&mut hud, &assets, "UI_Customizing").unwrap();
        assert_eq!(w.control("CONTROL_17928").map(|c| c.pos), Some([29.0, 59.0]));
    }
}
