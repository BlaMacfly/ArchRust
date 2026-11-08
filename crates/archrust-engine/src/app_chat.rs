//! Discussion côté moteur : affichage des types d'AgpmChatting et des cris de zone, salons de discussion.
//! Protocole : `archrust_netclient::chat` ; fenêtres d'Archonia : `hud_chat.rs`.
//!
//! Comportement relevé dans Archonia (sha256 e9c7609b…) :
//! * affichage (`FUN_140511e20`, `FUN_1407a2b30`) : un texte qui commence par `/` n'est pas affiché ; classe,
//!   couleur de `ini/color.ini` et format selon le type (`[nom] texte`, `en-tête[nom] texte`, `en-tête texte`,
//!   texte seul pour 0x11, 0x13 et 0x14 que le serveur compose en entier) ; types 8 à 10 : un texte identique
//!   reçu moins de 500 ms après le précédent est ignoré ; suffixe `\x01RRGGBB` : couleur du corps du texte
//!   (après le 2ᵉ `]` pour 0x11, 0x13, 0x14) ;
//! * salons (AgcmUIChannel, `FUN_1406e7530`) : « Create Room » et « Enter Chat » du menu Communauté,
//!   `/join nom` et `/leave nom` (sysstr 530-537) ; création : nom de 1 à 39 caractères, mot de passe exigé
//!   pour un salon privé (`Channel_Need_Password`) ; « The chat room [%s] has been created. » à l'entrée qui suit
//!   la création d'un salon public ou de guilde (`+0x17ac`, `FUN_1406e6240`) ; cinq fenêtres `UI_ChatRoom%d` ;
//!   message sans écho local (`FUN_1406e5a40`), ligne ajoutée au retour du serveur (`FUN_1406e4f70`, tampon de
//!   10 240 octets) ; « %s has entered. » / « %s has left. » dans la discussion pour les autres
//!   (`FUN_1406e4dd0`, `FUN_1406e6a00`) ; invitation (sysstr 286, 283-285) ; mot de passe demandé
//!   (`UI_QueryPassword`) ; recherche d'un joueur (`UI_FindFriend`, « Whisper » met `/w nom ` dans la saisie,
//!   `FUN_1406e6c00`) ; codes de résultat (`FUN_1406e6710`).
//!
//! Chuchotements (AGPMCHATTING ops 3 et 4, `0x140512bf0`, `0x140512d80`) : « `<nom> is blocking whispering.` »
//! (`Chat_Block_Whisper_Msg`) et « `<nom> is not connected.` » (`Chat_Offline_Whisper_Target_Msg`) dans l'onglet
//! système ; ops 1 / 2 : blocage de tous les chuchotements (attache+4, sans texte), demandé par
//! `/chuchotementsoff` et `/chuchotementson` (aucune commande dans Archonia : envoyeurs sans appelant).
//!
//! Écarts : la liste des membres absente ne plante pas (liste vide) ; les raccourcis Z et X d'Archonia
//! (`Hotkey=0:44`, `0:45`) restent le déplacement d'ArchRust ; `UI_FindFriend` s'ouvre par `/trouver [nom]`
//! (l'appel d'Archonia à `FindUser_Open` n'est pas relevé).

use super::App;
use crate::assets::Assets;
use crate::hud::{ChatRoomUi, ChatRoomView, HudState};
use crate::net::NetLink;
use crate::ui::UiState;
use archrust_netclient::chat::{self, channel, class_of, ChannelCommand, ChannelEvent};
use archrust_netclient::{Command, WorldEvent};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Taille du tampon des messages d'un salon (`0x2800`).
const ROOM_BUFFER: usize = 0x2800;
/// Délai sous lequel un message système identique est ignoré.
const SYSTEM_REPEAT: Duration = Duration::from_millis(500);
/// Couleur des mentions du genre dans l'invitation (`0xfffff1c6`, sysstr 286).
const INVITE_KIND_COLOR: u32 = 0xFF_F1C6;

/// Salon ouvert dans une fenêtre `UI_ChatRoom%d`.
#[derive(Debug, Default)]
struct Room {
    name: String,
    kind: i32,
    members: Vec<String>,
    /// Lignes : auteur, texte, écrit par soi.
    lines: Vec<(String, String, bool)>,
    input: String,
    first_member: usize,
}

impl Room {
    fn push(&mut self, who: String, text: String, mine: bool) {
        self.lines.push((who, text, mine));
        while self.lines.iter().map(|(w, t, _)| w.len() + t.len() + 16).sum::<usize>() > ROOM_BUFFER && self.lines.len() > 1 {
            self.lines.remove(0);
        }
    }
}

#[derive(Debug, Default)]
struct CreateForm {
    title: String,
    password: String,
    kind: i32,
}

#[derive(Default)]
pub struct ChatGlue {
    /// Tous les chuchotements reçus bloqués (AGPMCHATTING op 1 / 2 pour le joueur).
    pub whispers_blocked: bool,
    rooms: [Option<Room>; channel::MAX_CHANNELS],
    create: Option<CreateForm>,
    /// `UI_EnterChatRoom` ouverte et son texte.
    enter: Option<String>,
    /// `UI_QueryPassword` : salon et mot de passe saisi.
    password: Option<(String, String)>,
    /// `UI_InviteChatRoom2` : fenêtre du salon et nom saisi.
    invite_send: Option<(usize, String)>,
    /// `UI_InviteChatRoom` : salon, invitant, genre.
    invite_received: Option<(String, String, i32)>,
    /// `UI_FindFriend` : nom saisi, joueur trouvé (bouton « Whisper »).
    find: Option<String>,
    found: Option<String>,
    /// Entrée qui suit une création publique ou de guilde (`+0x17ac`).
    just_created: bool,
    /// `chatcmd.xml` : commande → rappel.
    commands: Option<std::collections::HashMap<String, String>>,
    /// Boîte d'information (`Action=4:0`).
    info: Option<String>,
    /// Dernier message système (types 8 à 10) et son heure.
    last_system: Option<(String, Instant)>,
    colors: Option<HashMap<String, u32>>,
    events: Option<HashMap<String, String>>,
}

impl ChatGlue {
    /// Couleur `0xRRGGBB` d'une clé de color.ini (blanc si elle manque).
    fn color(&mut self, assets: &Assets, key: &str) -> u32 {
        let map = self.colors.get_or_insert_with(|| {
            let mut m = HashMap::new();
            if let Ok(t) = assets.table("ini", "color.ini") {
                for s in &t.sections {
                    for (k, v) in &s.entries {
                        if let Ok(c) = u32::from_str_radix(v.trim(), 16) {
                            m.entry(k.trim().to_string()).or_insert(c);
                        }
                    }
                }
            }
            m
        });
        map.get(key).copied().unwrap_or(0xFF_FFFF)
    }

    fn event(&mut self, assets: &Assets, name: &str) -> String {
        self.events.get_or_insert_with(|| super::portal_glue::ui_events(assets)).get(name).cloned().unwrap_or_default()
    }

    fn slot_of(&self, name: &str) -> Option<usize> {
        self.rooms.iter().position(|r| r.as_ref().is_some_and(|r| r.name.eq_ignore_ascii_case(name)))
    }

    /// Déconnexion : plus de salon ni de fenêtre.
    fn reset(&mut self) {
        let (colors, events) = (self.colors.take(), self.events.take());
        *self = ChatGlue { colors, events, ..Default::default() };
    }

    pub fn open_create(&mut self) {
        // `CreateChannelOption_Clear` : salon public, champs vides.
        self.create = Some(CreateForm { kind: channel::kind::PUBLIC, ..Default::default() });
    }

    pub fn open_enter(&mut self) {
        self.enter = Some(String::new());
    }
}

/// Texte coloré pour la discussion (codes `|cffRRGGBB…|r`).
fn tint(text: &str, rgb: u32) -> String {
    if text.is_empty() {
        return String::new();
    }
    format!("|cff{:06x}{text}|r", rgb & 0xFF_FFFF)
}

/// Ligne d'Archonia pour un type reçu : en-tête et nom selon la classe, couleur de la classe, couleur du
/// suffixe sur le corps du texte.
pub(super) fn chat_line(kind: i8, header: &str, class_color: u32, from: &str, message: &str) -> Option<(String, u16)> {
    let class = class_of(kind)?;
    let (text, suffix) = chat::split_color_suffix(message);
    let lead = match (class.header.is_some(), class.named) {
        (true, true) => format!("{header}[{from}] "),
        (false, true) => format!("[{from}] "),
        (true, false) => format!("{header} "),
        (false, false) => String::new(),
    };
    let line = match suffix {
        Some(rgb) => {
            let at = chat::color_insert_at(kind, text);
            format!("{}{}", tint(&format!("{lead}{}", &text[..at]), class_color), tint(&text[at..], rgb))
        }
        None => tint(&format!("{lead}{text}"), class_color),
    };
    Some((line, class.mask))
}

/// Événement d'AddOn d'un type reçu.
fn addon_event(kind: i8) -> &'static str {
    use chat::chat_type as t;
    match kind {
        t::GUILD | t::GUILD_NOTICE | t::GUILD_JOINT => "CHAT_MSG_GUILD",
        t::PARTY => "CHAT_MSG_PARTY",
        t::WORD_BALLOON | t::RACE | t::SCREAM => "CHAT_MSG_YELL",
        t::NORMAL => "CHAT_MSG_SAY",
        _ => "CHAT_MSG_SYSTEM",
    }
}

fn send(net: Option<&NetLink>, cmd: ChannelCommand) {
    if let Some(net) = net {
        net.send(Command::Channel(cmd));
    }
}

/// Fenêtres des salons (dans la passe egui).
pub fn panel(ctx: &egui::Context, glue: &mut ChatGlue, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>, ui: &mut UiState, me: &str) {
    let mut out = Vec::new();
    if let Some(f) = glue.create.as_mut() {
        crate::hud::draw_create_room(ctx, hud, assets, &mut f.title, &mut f.password, f.kind, &mut out);
    }
    if let Some(t) = glue.enter.as_mut() {
        crate::hud::draw_prompt(ctx, hud, assets, "UI_EnterChatRoom", t, false, &mut out);
    }
    if let Some((_, t)) = glue.password.as_mut() {
        crate::hud::draw_prompt(ctx, hud, assets, "UI_QueryPassword", t, false, &mut out);
    }
    if let Some((_, t)) = glue.invite_send.as_mut() {
        crate::hud::draw_prompt(ctx, hud, assets, "UI_InviteChatRoom2", t, false, &mut out);
    }
    let found = glue.found.is_some();
    if let Some(t) = glue.find.as_mut() {
        crate::hud::draw_prompt(ctx, hud, assets, "UI_FindFriend", t, found, &mut out);
    }
    if let Some((name, from, kind)) = glue.invite_received.clone() {
        let kind_name = match kind {
            channel::kind::PUBLIC => super::npc_glue::sysstr(assets, 283),
            channel::kind::PRIVATE => super::npc_glue::sysstr(assets, 284),
            channel::kind::GUILD => super::npc_glue::sysstr(assets, 285),
            _ => None,
        }
        .unwrap_or_default();
        // sysstr 286 : « '%s' has invited you to\n'%s'\nchat room (<C%d>%s<C%d>) ».
        let format = super::npc_glue::sysstr(assets, 286).unwrap_or_default().replace("\\n", "\n");
        let text = printf(&format, &[&from, &name, &INVITE_KIND_COLOR.to_string(), &kind_name, &0xFF_FFFFu32.to_string()]);
        crate::hud::draw_invite_received(ctx, hud, assets, &text, &mut out);
    }
    for slot in 0..channel::MAX_CHANNELS {
        let Some(room) = glue.rooms[slot].as_mut() else { continue };
        let view = ChatRoomView { title: &room.name, members: &room.members, lines: &room.lines, input: &mut room.input, first_member: &mut room.first_member };
        crate::hud::draw_chat_room(ctx, hud, assets, slot, view, &mut out);
    }
    for r in out {
        match r {
            ChatRoomUi::Kind(k) => {
                if let Some(f) = glue.create.as_mut() {
                    f.kind = k;
                }
            }
            ChatRoomUi::CloseCreate => glue.create = None,
            // `CreateChannel` (FUN_1406e5510).
            ChatRoomUi::Create => {
                let Some(f) = glue.create.as_ref() else { continue };
                let name = f.title.trim().to_string();
                if name.is_empty() || name.chars().count() > channel::NAME_MAX {
                    continue;
                }
                if f.kind == channel::kind::PRIVATE {
                    let pw = f.password.trim().to_string();
                    if pw.is_empty() || pw.chars().count() > channel::PASSWORD_MAX {
                        let text = crate::hud::ui_message(hud, assets, "Channel_Need_Password");
                        ui.push_chat_in(0x10, text);
                        continue;
                    }
                    send(net, ChannelCommand::Create { name, password: Some(pw), kind: f.kind });
                } else {
                    send(net, ChannelCommand::Create { name, password: None, kind: f.kind });
                    glue.just_created = true;
                }
                glue.create = None;
            }
            ChatRoomUi::Confirm(w) => match w {
                // `Channeljoin_click` (FUN_1406e6400).
                "UI_EnterChatRoom" => {
                    let name = glue.enter.take().unwrap_or_default().trim().to_string();
                    if !name.is_empty() {
                        send(net, ChannelCommand::Join { name });
                        glue.just_created = false;
                    }
                }
                "UI_QueryPassword" => {
                    if let Some((name, password)) = glue.password.take() {
                        send(net, ChannelCommand::Password { name, password });
                    }
                }
                "UI_InviteChatRoom2" => {
                    if let Some((slot, target)) = glue.invite_send.take() {
                        let target = target.trim().to_string();
                        if let (Some(room), false) = (glue.rooms[slot].as_ref(), target.is_empty()) {
                            send(net, ChannelCommand::Invite { name: room.name.clone(), target, kind: room.kind });
                        }
                    }
                }
                _ => {
                    let target = glue.find.clone().unwrap_or_default().trim().to_string();
                    if !target.is_empty() {
                        glue.found = None;
                        send(net, ChannelCommand::Find { target });
                    }
                }
            },
            ChatRoomUi::Close(w) => match w {
                "UI_EnterChatRoom" => glue.enter = None,
                "UI_QueryPassword" => glue.password = None,
                "UI_InviteChatRoom2" => glue.invite_send = None,
                _ => {
                    glue.find = None;
                    glue.found = None;
                }
            },
            // `SendWhisper_Click` : « /w nom » dans la saisie de la discussion.
            ChatRoomUi::Whisper => {
                if let Some(name) = glue.found.clone() {
                    let w = super::npc_glue::sysstr(assets, 542).unwrap_or_else(|| "/w".into());
                    ui.chat_input = format!("{w} {name} ");
                    ui.chat_focus = true;
                }
            }
            ChatRoomUi::AcceptInvite => {
                if let Some((name, _, _)) = glue.invite_received.take() {
                    send(net, ChannelCommand::Join { name });
                }
            }
            ChatRoomUi::CloseInvite => glue.invite_received = None,
            ChatRoomUi::Send(slot) => {
                let Some(room) = glue.rooms[slot].as_mut() else { continue };
                let text = std::mem::take(&mut room.input).trim().to_string();
                if !text.is_empty() {
                    send(net, ChannelCommand::Message { name: room.name.clone(), me: me.to_string(), text });
                }
            }
            ChatRoomUi::Leave(slot) => {
                if let Some(room) = glue.rooms[slot].as_ref() {
                    send(net, ChannelCommand::Leave { name: room.name.clone() });
                }
            }
            ChatRoomUi::Invite(slot) => {
                if glue.rooms[slot].is_some() {
                    glue.invite_send = Some((slot, String::new()));
                }
            }
        }
    }
    if let Some(text) = glue.info.clone() {
        if text.is_empty() || crate::hud::info_box(ctx, hud, assets, &text) {
            glue.info = None;
        }
    }
}

/// `%s`, `%d`, `%I64d` remplacés dans l'ordre.
pub(super) fn printf(format: &str, args: &[&str]) -> String {
    let mut out = String::new();
    let mut rest = format;
    let mut args = args.iter();
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 1..];
        let spec_len = if tail.starts_with("I64") { 4 } else { tail.find(|c: char| c.is_ascii_alphabetic()).map_or(0, |j| j + 1) };
        if spec_len == 0 {
            out.push('%');
            rest = tail;
            continue;
        }
        out.push_str(args.next().copied().unwrap_or(""));
        rest = &tail[spec_len..];
    }
    out.push_str(rest);
    out
}

/// `chatcmd.xml` : `<ChatCmd CmdString="…" CmdFunc="…">`, commande sans la barre → rappel.
pub(super) fn parse_chatcmd(text: &str) -> HashMap<String, String> {
    let attr = |line: &str, key: &str| -> Option<String> {
        let start = line.find(key)? + key.len();
        let rest = line[start..].trim_start().strip_prefix('=')?.trim_start().strip_prefix('"')?;
        Some(rest[..rest.find('"')?].to_string())
    };
    text.lines().filter(|l| l.contains("<ChatCmd")).filter_map(|l| Some((attr(l, "CmdString")?, attr(l, "CmdFunc")?))).collect()
}

/// Ligne reçue écartée : vide, ou écho d'une commande en `/` dans une discussion de joueurs. Les messages du serveur restent :
/// ArchCore répond à une commande MJ refusée par `/create : objet inconnu : TID 15151. Syntaxe : …` (`send_system`, type
/// `WHOLE_WORLD`), sans quoi le refus était muet, pour le joueur comme pour les AddOns.
fn line_hidden(kind: i8, message: &str) -> bool {
    use archrust_netclient::chat::chat_type as t;
    let server = matches!(kind, t::WHOLE_WORLD | t::SYSTEM_LEVEL1 | t::SYSTEM_LEVEL2 | t::SYSTEM_LEVEL3 | t::SYSTEM_NO_PREFIX);
    message.is_empty() || (message.starts_with('/') && !server)
}

impl App {
    /// Connexion au monde : plus de salon.
    pub(super) fn chat_observe(&mut self, ev: &WorldEvent) {
        if matches!(ev, WorldEvent::Connected | WorldEvent::Disconnected(_)) {
            self.chat.reset();
        }
    }

    /// Ligne reçue (AGPMCHATTING ou cri de zone 0x4C), hors chuchotements : filtres des AddOns, ligne
    /// d'Archonia dans son onglet, puis événement `CHAT_MSG_*`.
    pub(super) fn chat_received(&mut self, kind: i8, from: &str, message: &str) {
        use archrust_netclient::chat::chat_type as t;
        let Some(class) = class_of(kind) else { return };
        if line_hidden(kind, message) {
            return;
        }
        if matches!(kind, t::SYSTEM_LEVEL1 | t::SYSTEM_LEVEL2 | t::SYSTEM_LEVEL3) {
            let now = Instant::now();
            if self.chat.last_system.as_ref().is_some_and(|(m, at)| m == message && now.duration_since(*at) < SYSTEM_REPEAT) {
                return;
            }
            self.chat.last_system = Some((message.to_string(), now));
        }
        let event = addon_event(kind);
        let Some((msg, who)) = self.addons.filter_chat(event, message, from) else { return };
        let assets = self.assets.clone();
        let header = class.header.map(|h| crate::hud::ui_message(&mut self.hud, &assets, h)).unwrap_or_default();
        let color = self.chat.color(&assets, class.color);
        if let Some((line, mask)) = chat_line(kind, &header, color, &who, &msg) {
            self.ui.push_chat_in(mask, line);
        }
        self.addons.fire_event_str(event, &[message, from, "", "", from]);
    }

    /// Paquet AgpmChannel reçu.
    pub(super) fn on_channel_event(&mut self, ev: ChannelEvent) {
        let assets = self.assets.clone();
        let me = self.player.as_ref().map(|p| p.name.clone()).unwrap_or_default();
        // Chuchotements : textes d'uimessage précédés du nom, onglet système.
        let whisper = match &ev {
            ChannelEvent::WhisperBlocked { name } => Some((name.clone(), "Chat_Block_Whisper_Msg")),
            ChannelEvent::WhisperOffline { from, name } => {
                let mine = self.player.as_ref().is_some_and(|p| p.cid == *from);
                let known = mine || self.entities.iter().any(|e| e.cid == *from);
                known.then(|| (name.clone(), "Chat_Offline_Whisper_Target_Msg"))
            }
            ChannelEvent::WhisperBlocking { cid, on } => {
                log::info!("chuchotements du personnage {cid} : {}", if *on { "bloqués" } else { "permis" });
                if self.player.as_ref().is_some_and(|p| p.cid == *cid) {
                    self.chat.whispers_blocked = *on;
                }
                return;
            }
            _ => None,
        };
        if let Some((name, key)) = whisper {
            let text = crate::hud::ui_message(&mut self.hud, &assets, key);
            self.ui.push_chat_in(0x10, format!("{name}{text}"));
            return;
        }
        let glue = &mut self.chat;
        match ev {
            // FUN_1406e6240 : fenêtre libre, liste des membres.
            ChannelEvent::Entered { name, kind, mut members, .. } => {
                members.truncate(channel::max_members(kind));
                let slot = glue.slot_of(&name).or_else(|| glue.rooms.iter().position(Option::is_none));
                let Some(slot) = slot else {
                    log::info!("salon {name} : plus de fenêtre libre");
                    return;
                };
                glue.rooms[slot] = Some(Room { name: name.clone(), kind, members, ..Default::default() });
                if std::mem::take(&mut glue.just_created) {
                    let text = printf(&crate::hud::ui_message(&mut self.hud, &assets, "Channel_Create_Message"), &[&name]);
                    self.ui.push_chat_in(0x10, text);
                }
                log::info!("salon {name} (genre {kind}) ouvert dans la fenêtre {slot}");
            }
            ChannelEvent::Left { name } => {
                if let Some(slot) = glue.slot_of(&name) {
                    glue.rooms[slot] = None;
                    if glue.invite_send.as_ref().is_some_and(|(s, _)| *s == slot) {
                        glue.invite_send = None;
                    }
                }
            }
            ChannelEvent::MemberJoined { name, member } => {
                let Some(slot) = glue.slot_of(&name) else { return };
                if !member.eq_ignore_ascii_case(&me) {
                    let text = printf(&crate::hud::ui_message(&mut self.hud, &assets, "Channel_Join_Message"), &[&member]);
                    self.ui.push_chat_in(0x10, text);
                }
                if let Some(room) = self.chat.rooms[slot].as_mut() {
                    if !room.members.iter().any(|m| m == &member) && room.members.len() < channel::max_members(room.kind) {
                        room.members.push(member);
                    }
                }
            }
            ChannelEvent::MemberLeft { name, member } => {
                let Some(slot) = glue.slot_of(&name) else { return };
                if !member.eq_ignore_ascii_case(&me) {
                    let text = printf(&crate::hud::ui_message(&mut self.hud, &assets, "Channel_Leave_Message"), &[&member]);
                    self.ui.push_chat_in(0x10, text);
                }
                if let Some(room) = self.chat.rooms[slot].as_mut() {
                    room.members.retain(|m| m != &member);
                }
            }
            ChannelEvent::Invited { name, from, kind } => glue.invite_received = Some((name, from, kind)),
            ChannelEvent::PasswordRequired { name } => glue.password = Some((name, String::new())),
            ChannelEvent::Message { name, from, text } => {
                let Some(slot) = glue.slot_of(&name) else { return };
                let mine = from == me;
                if let Some(room) = glue.rooms[slot].as_mut() {
                    room.push(from, text, mine);
                }
            }
            // FUN_1406e5d40 : nom retenu, bouton « Whisper » actif.
            ChannelEvent::Found { target } => glue.found = Some(target),
            ChannelEvent::Result(code) => {
                log::info!("salon : résultat {code}");
                if code == channel::result::ALREADY_JOINED {
                    let text = super::npc_glue::sysstr(&assets, 620).unwrap_or_default();
                    self.ui.push_chat_in(0x10, text);
                } else if let Some(e) = channel::result_event(code) {
                    let text = glue.event(&assets, e);
                    glue.info = Some(text);
                }
            }
            ChannelEvent::WhisperBlocking { .. } | ChannelEvent::WhisperBlocked { .. } | ChannelEvent::WhisperOffline { .. } => {}
        }
    }

    /// `/join nom`, `/leave nom` (sysstr 530-537, chatcmd.xml) ; `/trouver [nom]` ouvre `UI_FindFriend`.
    pub(super) fn chat_slash(&mut self, cmd: &str, arg: &str) -> bool {
        let lower = cmd.to_lowercase();
        let arg = arg.trim();
        let assets = self.assets.clone();
        // Rappels actifs de `chatcmd.xml` (table FUN_140782bd0), alias coréens compris.
        let func = self.chat.commands.get_or_insert_with(|| parse_chatcmd(&super::npc_glue::client_text_in(&assets, "ini", "chatcmd.xml"))).get(cmd.trim_start_matches('/')).cloned();
        if let Some(func) = func {
            let refuse = match func.as_str() {
                "CallBack_OnRejectTradeOn" => Some((0, true)),
                "CallBack_OnRejectTradeOff" => Some((0, false)),
                "CallBack_OnRejectPartyOn" => Some((1, true)),
                "CallBack_OnRejectPartyOff" => Some((1, false)),
                "CallBack_OnRejectGuildOn" => Some((2, true)),
                "CallBack_OnRejectGuildOff" => Some((2, false)),
                _ => None,
            };
            if let Some((which, on)) = refuse {
                // FUN_14050f440 : bit de `character+0xbf8`, envoyé au serveur (mêmes cases que les options).
                let r = &mut self.options.refuse;
                *[&mut r.trade, &mut r.party, &mut r.guild].into_iter().nth(which).unwrap() = on;
                let flags = self.options.refuse.flags();
                if let Some(net) = &self.net {
                    net.send(Command::OptionFlags(flags));
                }
                self.save_options_if_needed(true);
                return true;
            }
            match func.as_str() {
                // FUN_1407821b0 → FUN_1406a5440 : confirmation de départ de la guilde, sauf pour le chef.
                "CallBack_OnGuildLeaveOut" => {
                    self.guild_slash("/gquitter", "");
                    return true;
                }
                // FUN_140781fa0 → FUN_1404a6920 : fenêtre de vue de la caméra (largeur, hauteur au prorata de l'écran).
                "CallBack_OnChangePerspective" => {
                    if let (Ok(w), Some(gpu)) = (arg.parse::<f32>(), self.gpu.as_ref()) {
                        if w > 0.0 {
                            self.camera.fov_y = 2.0 * (w / gpu.renderer.aspect()).atan();
                        }
                    }
                    return true;
                }
                "CallBack_OnViewFPS" => {
                    self.ui.show_fps = !self.ui.show_fps;
                    return true;
                }
                _ => {}
            }
        }
        let alias = |ids: std::ops::RangeInclusive<u32>| ids.filter_map(|i| super::npc_glue::sysstr(&assets, i)).any(|s| s == cmd);
        if lower == "/join" || alias(530..=533) {
            // `CallBack_OnSendMsgJoin` (FUN_140782460) : op 3.
            if !arg.is_empty() {
                send(self.net.as_ref(), ChannelCommand::Join { name: arg.to_string() });
                self.chat.just_created = false;
            }
            return true;
        }
        if lower == "/leave" || alias(534..=537) {
            if !arg.is_empty() {
                send(self.net.as_ref(), ChannelCommand::Leave { name: arg.to_string() });
            }
            return true;
        }
        if lower == "/trouver" {
            self.chat.find = Some(arg.to_string());
            self.chat.found = None;
            return true;
        }
        // AGPMCHATTING op 1 / 2 : bloquer ou permettre tous les chuchotements reçus.
        if lower == "/chuchotementsoff" || lower == "/chuchotementson" {
            send(self.net.as_ref(), ChannelCommand::BlockWhisper(lower.ends_with("off")));
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_netclient::chat::chat_type as t;

    #[test]
    fn refus_mj_du_serveur_gardes() {
        let refus = "/create : objet inconnu : TID 15151. Syntaxe : /create \"nom\" <TID> <quantité> <0|1>";
        assert!(!line_hidden(t::WHOLE_WORLD, refus), "message système d'ArchCore (send_system)");
        assert!(!line_hidden(t::SYSTEM_LEVEL1, "/SetExp : argument manquant"));
        assert!(line_hidden(t::NORMAL, "/move 1,2"), "écho d'une commande dans la discussion");
        assert!(line_hidden(t::WHOLE_WORLD, ""));
        assert!(!line_hidden(t::NORMAL, "bonjour"));
    }

    #[test]
    fn commandes_de_chatcmd() {
        let m = parse_chatcmd("<ChatCmdList>\n<ChatCmd CmdString=\"tradeoff\" CmdFunc=\"CallBack_OnRejectTradeOn\">\n<ChatCmd CmdString=\"GOUT\" CmdFunc=\"CallBack_OnGuildLeaveOut\">\n");
        assert_eq!(m["tradeoff"], "CallBack_OnRejectTradeOn");
        assert_eq!(m["GOUT"], "CallBack_OnGuildLeaveOut");
        // Avec les données du client : les commandes demandées pointent bien sur ces rappels.
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = crate::assets::Assets::new(Some(dir.into()), &[]);
        let m = parse_chatcmd(&crate::app::npc_glue::client_text_in(&assets, "ini", "chatcmd.xml"));
        for (cmd, func) in [("tradeoff", "CallBack_OnRejectTradeOn"), ("tradeon", "CallBack_OnRejectTradeOff"), ("pinoff", "CallBack_OnRejectPartyOn"), ("pinon", "CallBack_OnRejectPartyOff"), ("ginoff", "CallBack_OnRejectGuildOn"), ("ginon", "CallBack_OnRejectGuildOff"), ("GOUT", "CallBack_OnGuildLeaveOut"), ("PS", "CallBack_OnChangePerspective"), ("fps", "CallBack_OnViewFPS")] {
            assert_eq!(m.get(cmd).map(String::as_str), Some(func), "{cmd}");
        }
    }

    #[test]
    fn lignes_d_archonia() {
        assert_eq!(chat_line(t::NORMAL, "", 0xFFFFFF, "Bob", "salut"), Some(("|cffffffff[Bob] salut|r".to_string(), 0x1)));
        assert_eq!(chat_line(t::WHOLE_WORLD, "[Notice]", 0x00FF00, "Bob", "Maintenance"), Some(("|cff00ff00[Notice] Maintenance|r".to_string(), 0x20)));
        assert_eq!(chat_line(t::WORD_BALLOON, "[Shout]", 0x99CCFF, "Bob", "ohé"), Some(("|cff99ccff[Shout][Bob] ohé|r".to_string(), 0x200)));
        assert_eq!(chat_line(t::RACE, "", 0xFF80FA, "Bob", "[Race] [Bob] salut"), Some(("|cffff80fa[Race] [Bob] salut|r".to_string(), 0x400)));
        // Suffixe couleur : corps coloré après le 2ᵉ `]`.
        assert_eq!(
            chat_line(t::SCREAM, "", 0xFFFF00, "Bob", "[Cri] [Bob] salut\u{1}FF0000"),
            Some(("|cffffff00[Cri] [Bob] |r|cffff0000salut|r".to_string(), 0x800))
        );
        assert_eq!(chat_line(t::UNION, "", 0, "Bob", "x"), None);
    }

    #[test]
    fn formats_printf() {
        assert_eq!(printf("%s has entered.", &["Bob"]), "Bob has entered.");
        assert_eq!(printf("(%s) pour (%I64d) Gold.", &["Bob", "5"]), "(Bob) pour (5) Gold.");
        assert_eq!(printf("'%s' a '%s' (<C%d>%s<C%d>)", &["A", "B", "1", "Public", "2"]), "'A' a 'B' (<C1>Public<C2>)");
    }

    #[test]
    fn tampon_du_salon() {
        let mut r = Room::default();
        for i in 0..2000 {
            r.push("Bob".into(), format!("message {i}"), false);
        }
        assert!(r.lines.iter().map(|(w, t, _)| w.len() + t.len() + 16).sum::<usize>() <= ROOM_BUFFER);
        assert_eq!(r.lines.last().map(|l| l.1.as_str()), Some("message 1999"));
    }
}
