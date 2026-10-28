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
use super::npc_ui::{button, window};
use super::portal_ui::{control_text, static_text};
use super::*;

/// Fenêtres chargées avec le HUD.
pub(super) const CHAT_WINDOWS: [&str; 7] = ["UI_CreateChatRoom", "UI_EnterChatRoom", "UI_InviteChatRoom", "UI_InviteChatRoom2", "UI_QueryPassword", "UI_FindFriend", "UI_ChatRoom"];
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
