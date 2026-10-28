//! Amis et ignorés côté moteur. Protocole : `archrust_netclient::buddy` (AgpmBuddy 0x2B) ; fenêtres
//! d'Archonia : `hud_buddy.rs`.
//!
//! Comportement relevé dans Archonia (AgcmUIBuddy, sha256 e9c7609b…) :
//! * ouverture : menu Communauté (`MenuBtnBuddyList`, `MenuBtnBanList`) ; touche N pour les ignorés
//!   (`Hotkey=0:49` « Ignore list ») ; menu « Menu » d'un joueur ciblé (`UI_TargetUserMenu` →
//!   `UI_TargetCharacterMenu` : « Add Friend » si `NotInBuddyList`, « Ignore User » si `NotInBanList`) ;
//! * ajout d'un ami (`BuddyAddFriend_Click` `FUN_1407338c0`) : nom de moins de 32 octets, sinon
//!   `BuddyIDLengthWarning_Open` ; op 2. Demande reçue (`FUN_140733660`) : ignorée en silence avec l'option
//!   « refuser les amis » (`OP_RefuseBuddy`, bit 0x80), sinon boîte `BuddyAddRequestConfirm_Open` → op 1
//!   (oui) ou op 3 (non) ;
//! * ignoré (`BanAdd_Click` `FUN_140733b50`) : op 1 sans drapeau ; cases (`FUN_140733ef0/dd0/cb0`) posées
//!   d'abord localement, op 5 et messages `BanList_*Option_Enable/Disable` ;
//! * retrait (Remove) : `BuddyList_Select_None` / `BanList_Select_None` sans ligne choisie, sinon
//!   confirmation `BuddyDeleteBuddyUserConfirm_MsgBox` / `BuddyDeleteBanUserConfirm_MsgBox`, op 4 ;
//! * ami choisi : op 7, détail « nom » puis « L : région » (`FUN_140735d80`, rien si la région est
//!   inconnue) ; Party : invitation de groupe par nom (`BuddyList_Invite_Event_Send`) ; Whisper : « /w nom »
//!   dans la saisie (`sysstr` 542) ;
//! * réception : op 1 / 4 avec `BuddyList_Add_User` / `BuddyList_Remove_User` après l'op 9 ; op 6 état en
//!   ligne ; op 8 : 1 `BuddyAlreayExist_MsgBox`, 2 `BuddyOfflineOrNotExist_MsgBox`, 3 et 4 `sysstr` 715 ;
//!   op 3 `AddRejectDlg_Open` ;
//! * op 9 (`FUN_140736730`) : relit `Ini\IgnoreList.txt` (lignes `nom\tdrapeaux`, `FUN_140738dc0`), écarte
//!   son propre nom et les noms déjà connus, les ajoute comme ignorés (30 au plus) et les envoie par l'op 15 ;
//! * discussion (`FUN_140734810`, rappel 13 d'AgpmChatting) : tout message d'un ignoré à « Block Chat » est
//!   écarté.
//!
//! **Fichier** : `IgnoreList.txt` est commun à tous les personnages du PC, comme dans Archonia ; ArchRust le
//! range dans le dossier des SavedVariables (`saved-variables/IgnoreList.txt`). Il est réécrit après la fin
//! de la liste initiale à chaque changement de la liste d'ignorés.
//!
//! Écarts : la touche F d'Archonia (`Hotkey=0:33` « Friends ») reste l'attaque d'ArchRust ; la liste d'amis
//! s'ouvre par le menu Communauté ou `/amis`. Le mentor (« Buddy », ops 10 à 14, `UI_MentorBuddy`) n'est pas
//! fait. Le nom de région est celui de `RegionTemplate.ini` (coréen), sans le glossaire d'Archonia.

use super::App;
use crate::assets::Assets;
use crate::hud::{BanListView, BuddyListView, BuddyUi, HudState, TargetMenuUi, BUDDY_LIST_ROWS};
use crate::net::NetLink;
use crate::ui::UiState;
use archrust_netclient::buddy::{flag, result, BuddyCommand, BuddyEntry, BuddyEvent, MAX_ENTRIES, NAME_MAX};
use archrust_netclient::party::PartyCommand;
use archrust_netclient::{Command, WorldEvent};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

/// Nom du fichier des ignorés d'Archonia (`Ini\IgnoreList.txt`).
pub const IGNORE_FILE: &str = "IgnoreList.txt";
/// Lignes lues au plus dans `IgnoreList.txt` (`FUN_140737e00`).
const IGNORE_FILE_LINES: usize = 512;
/// `sysstr` : « L » (préfixe de la région), « /w », « Cannot add player in the list. ».
const SYS_REGION: u32 = 521;
const SYS_WHISPER: u32 = 542;
const SYS_CANNOT_ADD: u32 = 715;

/// Confirmation en cours.
#[derive(Debug, Clone, PartialEq)]
enum Ask {
    RemoveFriend(BuddyEntry),
    RemoveBan(BuddyEntry),
}

#[derive(Default)]
pub struct BuddyGlue {
    pub friends: Vec<BuddyEntry>,
    pub bans: Vec<BuddyEntry>,
    /// Op 9 reçue : messages d'ajout et de retrait, écriture d'`IgnoreList.txt`.
    list_done: bool,
    pub show_friends: bool,
    pub show_bans: bool,
    /// Boîtes de saisie ouvertes (`UI_AddBuddyList`, `UI_AddBanList`) et leur texte.
    add_friend: Option<String>,
    add_ban: Option<String>,
    sel_friend: Option<usize>,
    sel_ban: Option<usize>,
    first_friend: usize,
    first_ban: usize,
    /// `BuddyDetailInfo_UD` : nom et région de l'ami choisi.
    detail: String,
    /// Demandes d'ami reçues, à confirmer une à une.
    requests: VecDeque<String>,
    ask: Option<(Ask, String)>,
    /// Boîtes « OK » d'Archonia en attente.
    info: VecDeque<String>,
    /// Menu du joueur ciblé ouvert (`UI_TargetCharacterMenu`).
    target_menu: bool,
    /// Fichier `IgnoreList.txt` (dossier des SavedVariables).
    ignore_path: Option<PathBuf>,
    messages: Option<HashMap<String, String>>,
    events: Option<HashMap<String, String>>,
}

/// `%s` remplacé par `value`.
fn fill(text: &str, value: &str) -> String {
    text.replacen("%s", value, 1)
}

/// `{{…}}` remplacé par `value` (messages de l'`[EventMap]`).
fn fill_braces(text: &str, value: &str) -> String {
    match (text.find("{{"), text.find("}}")) {
        (Some(a), Some(b)) if b > a => format!("{}{}{}", &text[..a], value, &text[b + 2..]),
        _ => text.to_string(),
    }
}

/// Lignes `nom\tdrapeaux` d'`IgnoreList.txt` (512 au plus, lignes vides écartées).
pub fn parse_ignore_file(text: &str) -> Vec<(String, u32)> {
    text.lines()
        .take(IGNORE_FILE_LINES)
        .filter_map(|l| {
            let mut cols = l.split('\t');
            let name = cols.next()?.trim();
            let flags = cols.next().and_then(|f| f.trim().parse::<u32>().ok()).unwrap_or(0);
            (!name.is_empty()).then(|| (name.to_string(), flags))
        })
        .collect()
}

/// Texte d'`IgnoreList.txt` (`FUN_140738dc0` : `"%s\t%lu\n"`).
pub fn format_ignore_file(bans: &[BuddyEntry]) -> String {
    bans.iter().map(|b| format!("{}\t{}\n", b.name, b.flags)).collect()
}

/// Fusion d'`IgnoreList.txt` à l'op 9 (`FUN_140736730`) : sans son propre nom ni les noms déjà dans une liste,
/// tant que la liste d'ignorés n'est pas pleine. Rend les entrées à envoyer par l'op 15 (drapeaux du fichier).
pub fn merge_ignore_file(file: &[(String, u32)], me: &str, friends: &[BuddyEntry], bans: &mut Vec<BuddyEntry>) -> Vec<(String, u32)> {
    let mut sent = Vec::new();
    for (name, flags) in file {
        if sent.len() >= MAX_ENTRIES {
            break;
        }
        let known = |l: &[BuddyEntry]| l.iter().any(|e| e.name.eq_ignore_ascii_case(name));
        if name.eq_ignore_ascii_case(me) || known(friends) || known(bans) {
            continue;
        }
        if bans.len() >= MAX_ENTRIES {
            break;
        }
        bans.push(BuddyEntry { name: name.clone(), flags: flags & !flag::FRIEND, relation: 0 });
        sent.push((name.clone(), *flags));
    }
    sent
}

impl BuddyGlue {
    fn text(&mut self, assets: &Assets, key: &str) -> String {
        let messages = self.messages.get_or_insert_with(|| {
            super::npc_glue::client_text(assets, "uimessage.txt").lines().filter_map(|l| l.split_once('\t')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect()
        });
        messages.get(key).cloned().unwrap_or_default()
    }

    fn event(&mut self, assets: &Assets, key: &str) -> String {
        self.events.get_or_insert_with(|| super::portal_glue::ui_events(assets)).get(key).cloned().unwrap_or_default()
    }

    fn in_friends(&self, name: &str) -> bool {
        self.friends.iter().any(|e| e.name.eq_ignore_ascii_case(name))
    }

    fn in_bans(&self, name: &str) -> bool {
        self.bans.iter().any(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// Message d'un ignoré à « Block Chat » : à écarter.
    pub fn blocks_chat(&self, name: &str) -> bool {
        !name.is_empty() && self.bans.iter().any(|e| e.flags & flag::BLOCK_CHAT != 0 && e.name.eq_ignore_ascii_case(name))
    }

    fn save_ignore_file(&self) {
        let Some(path) = self.ignore_path.as_ref().filter(|_| self.list_done) else { return };
        // Octets CP1252, comme le fichier d'Archonia.
        if let Err(e) = std::fs::write(path, archrust_netclient::guild::to_cp1252(&format_ignore_file(&self.bans))) {
            log::warn!("{} : {e}", path.display());
        }
    }

    /// Ferme les fenêtres ouvertes (Échap) ; vrai s'il y en avait.
    pub fn close_windows(&mut self) -> bool {
        let open = self.show_friends || self.show_bans || self.add_friend.is_some() || self.add_ban.is_some();
        self.show_friends = false;
        self.show_bans = false;
        self.add_friend = None;
        self.add_ban = None;
        open
    }

    fn reset(&mut self) {
        let path = self.ignore_path.take();
        let (show_friends, show_bans) = (self.show_friends, self.show_bans);
        *self = BuddyGlue { ignore_path: path, show_friends, show_bans, ..Default::default() };
    }

    /// Ouverture d'une liste (`BuddyList_Clear` / `BanList_Clear` : sélection et détail remis à zéro).
    pub fn toggle_friends(&mut self) {
        self.show_friends = !self.show_friends;
        self.sel_friend = None;
        self.detail.clear();
    }

    pub fn toggle_bans(&mut self) {
        self.show_bans = !self.show_bans;
        self.sel_ban = None;
    }
}

/// Envoi d'une demande d'ami ou d'un ignoré par nom (`BuddyAddFriend_Click`, `BanAdd_Click`).
fn add_by_name(glue: &mut BuddyGlue, assets: &Assets, name: &str, ban: bool, net: Option<&NetLink>) {
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    if name.len() > NAME_MAX {
        let text = glue.event(assets, "BuddyIDLengthWarning_Open");
        glue.info.push_back(text);
        return;
    }
    if let Some(net) = net {
        net.send(Command::Buddy(if ban { BuddyCommand::AddIgnore(name.to_string()) } else { BuddyCommand::RequestFriend(name.to_string()) }));
    }
}

/// Fenêtres d'amis et d'ignorés, menu du joueur ciblé (`target` : CID et nom d'un autre joueur), demandes
/// d'ami et boîtes en attente.
#[allow(clippy::too_many_arguments)]
pub fn panel(ctx: &egui::Context, glue: &mut BuddyGlue, target: Option<(i32, String)>, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>, ui: &mut UiState) {
    let mut out = Vec::new();
    if glue.show_friends {
        let rows: Vec<(String, bool)> = glue.friends.iter().map(|e| (e.name.clone(), e.online())).collect();
        let view = BuddyListView { rows: &rows, selected: glue.sel_friend, first: glue.first_friend, detail: &glue.detail };
        crate::hud::draw_buddy_list(ctx, hud, assets, &view, &mut out);
    }
    if glue.show_bans {
        let rows: Vec<String> = glue.bans.iter().map(|e| e.name.clone()).collect();
        let flags = glue.sel_ban.and_then(|i| glue.bans.get(i)).map(|e| e.flags);
        let view = BanListView { rows: &rows, selected: glue.sel_ban, first: glue.first_ban, flags };
        crate::hud::draw_ban_list(ctx, hud, assets, &view, &mut out);
    }
    if let Some(text) = glue.add_friend.as_mut() {
        crate::hud::draw_add_name(ctx, hud, assets, false, text, &mut out);
    }
    if let Some(text) = glue.add_ban.as_mut() {
        crate::hud::draw_add_name(ctx, hud, assets, true, text, &mut out);
    }
    let scroll = |first: &mut usize, step: i32, count: usize| {
        *first = (*first as i32 + step).clamp(0, count.saturating_sub(BUDDY_LIST_ROWS) as i32) as usize;
    };
    for a in out {
        match a {
            BuddyUi::CloseFriends => glue.show_friends = false,
            BuddyUi::CloseBans => glue.show_bans = false,
            BuddyUi::ScrollFriends(step) => scroll(&mut glue.first_friend, step, glue.friends.len()),
            BuddyUi::ScrollBans(step) => scroll(&mut glue.first_ban, step, glue.bans.len()),
            BuddyUi::SelectFriend(i) => {
                glue.sel_friend = Some(i);
                glue.detail.clear();
                if let (Some(e), Some(net)) = (glue.friends.get(i), net) {
                    net.send(Command::Buddy(BuddyCommand::Region(e.name.clone())));
                }
            }
            BuddyUi::SelectBan(i) => glue.sel_ban = Some(i),
            BuddyUi::OpenAddFriend => glue.add_friend = Some(String::new()),
            BuddyUi::OpenAddBan => glue.add_ban = Some(String::new()),
            BuddyUi::CloseAddFriend => glue.add_friend = None,
            BuddyUi::CloseAddBan => glue.add_ban = None,
            BuddyUi::AddFriend => {
                let name = glue.add_friend.take().unwrap_or_default();
                add_by_name(glue, assets, &name, false, net);
            }
            BuddyUi::AddBan => {
                let name = glue.add_ban.take().unwrap_or_default();
                add_by_name(glue, assets, &name, true, net);
            }
            BuddyUi::RemoveFriend => match glue.sel_friend.and_then(|i| glue.friends.get(i)).cloned() {
                Some(e) => {
                    let text = glue.event(assets, "BuddyDeleteBuddyUserConfirm_MsgBox");
                    glue.ask = Some((Ask::RemoveFriend(e), text));
                }
                None => ui.push_chat(glue.text(assets, "BuddyList_Select_None")),
            },
            BuddyUi::RemoveBan => match glue.sel_ban.and_then(|i| glue.bans.get(i)).cloned() {
                Some(e) => {
                    let text = glue.event(assets, "BuddyDeleteBanUserConfirm_MsgBox");
                    glue.ask = Some((Ask::RemoveBan(e), text));
                }
                None => ui.push_chat(glue.text(assets, "BanList_Select_None")),
            },
            BuddyUi::InviteFriend => match glue.sel_friend.and_then(|i| glue.friends.get(i)).map(|e| e.name.clone()).filter(|n| !n.is_empty()) {
                Some(name) => {
                    if let Some(net) = net {
                        net.send(Command::Party(PartyCommand::Invite { cid: None, name: Some(name.clone()) }));
                    }
                    ui.push_chat(fill(&glue.text(assets, "BuddyList_Invite_Event_Send"), &name));
                }
                None => ui.push_chat(glue.text(assets, "BuddyList_Select_None")),
            },
            BuddyUi::WhisperFriend => match glue.sel_friend.and_then(|i| glue.friends.get(i)).map(|e| e.name.clone()).filter(|n| !n.is_empty()) {
                Some(name) => {
                    let w = super::npc_glue::sysstr(assets, SYS_WHISPER).unwrap_or_else(|| "/w".into());
                    ui.whisper_target = None;
                    ui.chat_input = format!("{w} {name} ");
                    ui.chat_focus = true;
                }
                None => ui.push_chat(glue.text(assets, "BuddyList_Select_None")),
            },
            BuddyUi::ToggleBan(bit) => {
                let Some(i) = glue.sel_ban.filter(|i| *i < glue.bans.len()) else {
                    ui.push_chat(glue.text(assets, "BanList_Select_None"));
                    continue;
                };
                glue.bans[i].flags ^= bit;
                let e = glue.bans[i].clone();
                let on = e.flags & bit != 0;
                let kind = match bit {
                    flag::BLOCK_CHAT => "Whisper",
                    flag::BLOCK_TRADE => "Trade",
                    _ => "Invite",
                };
                let key = format!("BanList_{kind}Option_{}", if on { "Enable" } else { "Disable" });
                ui.push_chat(fill(&glue.text(assets, &key), &e.name));
                if let Some(net) = net {
                    net.send(Command::Buddy(BuddyCommand::Options(e)));
                }
                glue.save_ignore_file();
            }
        }
    }
    if let Some(sel) = glue.sel_friend.filter(|i| *i >= glue.friends.len()) {
        glue.sel_friend = sel.checked_sub(1).filter(|_| !glue.friends.is_empty());
    }
    if let Some(sel) = glue.sel_ban.filter(|i| *i >= glue.bans.len()) {
        glue.sel_ban = sel.checked_sub(1).filter(|_| !glue.bans.is_empty());
    }

    // Menu du joueur ciblé.
    match target {
        Some((cid, name)) => {
            let mut entries = vec![TargetMenuUi::Invite, TargetMenuUi::Whisper];
            if !glue.in_friends(&name) {
                entries.push(TargetMenuUi::AddFriend);
            }
            if !glue.in_bans(&name) {
                entries.push(TargetMenuUi::Ignore);
            }
            let mut chosen = Vec::new();
            crate::hud::draw_target_menu(ctx, hud, assets, &mut glue.target_menu, &entries, &mut chosen);
            for c in chosen {
                match c {
                    TargetMenuUi::Invite => {
                        if let Some(net) = net {
                            net.send(Command::Party(PartyCommand::Invite { cid: Some(cid), name: None }));
                        }
                    }
                    TargetMenuUi::Whisper => {
                        ui.whisper_target = Some(name.clone());
                        ui.chat_focus = true;
                    }
                    TargetMenuUi::AddFriend => add_by_name(glue, assets, &name, false, net),
                    TargetMenuUi::Ignore => add_by_name(glue, assets, &name, true, net),
                }
            }
        }
        None => glue.target_menu = false,
    }

    // Confirmations, demandes d'ami et boîtes « OK ».
    if let Some((ask, text)) = glue.ask.clone() {
        if let Some(yes) = crate::hud::confirm_box(ctx, hud, assets, "buddy-remove", &text) {
            glue.ask = None;
            if let (true, Some(net)) = (yes, net) {
                let e = match ask {
                    Ask::RemoveFriend(e) | Ask::RemoveBan(e) => e,
                };
                net.send(Command::Buddy(BuddyCommand::Remove(e)));
            }
        }
    } else if let Some(name) = glue.requests.front().cloned() {
        let text = fill_braces(&glue.event(assets, "BuddyAddRequestConfirm_Open"), &name);
        if let Some(accept) = crate::hud::confirm_box(ctx, hud, assets, ("buddy-request", &name), &text) {
            glue.requests.pop_front();
            if let Some(net) = net {
                net.send(Command::Buddy(BuddyCommand::Answer { name, accept }));
            }
        }
    } else if let Some(text) = glue.info.front().cloned() {
        if crate::hud::info_box(ctx, hud, assets, &text) {
            glue.info.pop_front();
        }
    }
}

impl App {
    /// Fichier `IgnoreList.txt` du dossier des SavedVariables.
    fn ignore_path(saved_dir: &Path) -> PathBuf {
        saved_dir.join(IGNORE_FILE)
    }

    /// Message reçu d'un ignoré à « Block Chat » (`FUN_140734810`) : écarté avant tout traitement.
    pub(super) fn buddy_blocks(&self, ev: &WorldEvent) -> bool {
        let WorldEvent::Chat { from_cid, from, .. } = ev else { return false };
        if self.buddy.bans.is_empty() {
            return false;
        }
        let me = self.player.as_ref().map(|p| p.name.as_str()).unwrap_or("");
        let name = if from.is_empty() { self.entities.iter().find(|e| e.cid == *from_cid).map(|e| e.name.as_str()).unwrap_or("") } else { from.as_str() };
        !name.eq_ignore_ascii_case(me) && self.buddy.blocks_chat(name)
    }

    /// Paquet AgpmBuddy.
    pub(super) fn on_buddy_event(&mut self, ev: BuddyEvent) {
        let assets = self.assets.clone();
        let glue = &mut self.buddy;
        glue.ignore_path.get_or_insert_with(|| Self::ignore_path(&self.config.saved_dir));
        match ev {
            BuddyEvent::Added(e) => {
                let friend = e.friend();
                let list = if friend { &mut glue.friends } else { &mut glue.bans };
                if list.len() >= MAX_ENTRIES {
                    return;
                }
                let name = e.name.clone();
                list.push(e);
                if glue.list_done {
                    self.ui.push_chat(fill(&glue.text(&assets, "BuddyList_Add_User"), &name));
                    if !friend {
                        glue.save_ignore_file();
                    }
                }
            }
            BuddyEvent::Removed(e) => {
                let friend = e.friend();
                let list = if friend { &mut glue.friends } else { &mut glue.bans };
                let before = list.len();
                list.retain(|x| !x.name.eq_ignore_ascii_case(&e.name));
                if before != list.len() && glue.list_done {
                    self.ui.push_chat(fill(&glue.text(&assets, "BuddyList_Remove_User"), &e.name));
                    if !friend {
                        glue.save_ignore_file();
                    }
                }
                if friend {
                    glue.detail.clear();
                }
            }
            BuddyEvent::Options(e) => {
                if let Some(x) = glue.bans.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&e.name)) {
                    x.flags = e.flags & !flag::FRIEND;
                    glue.save_ignore_file();
                }
            }
            BuddyEvent::Online { name, flags } => {
                for x in glue.friends.iter_mut().chain(glue.bans.iter_mut()).filter(|x| x.name.eq_ignore_ascii_case(&name)) {
                    x.flags = (x.flags & !flag::ONLINE) | (flags & flag::ONLINE);
                }
            }
            BuddyEvent::Region { name, region } => {
                // Nom puis « L : région » ; rien si la région est inconnue.
                glue.detail.clear();
                let region_name = u16::try_from(region).ok().and_then(|r| crate::zones::Zones::region_name(&assets, r));
                if let Some(region_name) = region_name {
                    let l = super::npc_glue::sysstr(&assets, SYS_REGION).unwrap_or_else(|| "L".into());
                    glue.detail = format!("{name}\n{l} : {region_name}");
                }
            }
            BuddyEvent::Requested(name) => {
                // Option « refuser les demandes d'ami » : ignorée en silence.
                if !self.options.refuse.buddy && !glue.requests.iter().any(|r| r.eq_ignore_ascii_case(&name)) {
                    glue.requests.push_back(name);
                }
            }
            BuddyEvent::Rejected(_) => {
                let text = glue.event(&assets, "AddRejectDlg_Open");
                glue.info.push_back(text);
            }
            BuddyEvent::Result(code) => match code {
                result::ALREADY_EXIST => {
                    let text = glue.event(&assets, "BuddyAlreayExist_MsgBox");
                    glue.info.push_back(text);
                }
                result::OFFLINE_OR_NOT_EXIST => {
                    let text = glue.event(&assets, "BuddyOfflineOrNotExist_MsgBox");
                    glue.info.push_back(text);
                }
                result::LIST_FULL | result::INVALID => {
                    if let Some(t) = super::npc_glue::sysstr(&assets, SYS_CANNOT_ADD) {
                        self.ui.push_chat(t);
                    }
                }
                12 => self.ui.push_chat(glue.text(&assets, "MentorInvalidProcess")),
                14 => self.ui.push_chat(glue.text(&assets, "MentorNotInviteBuddy")),
                _ => {}
            },
            BuddyEvent::ListEnd => {
                glue.list_done = true;
                let me = self.player.as_ref().map(|p| p.name.clone()).unwrap_or_default();
                let file = glue.ignore_path.as_ref().and_then(|p| std::fs::read(p).ok()).map(|b| parse_ignore_file(&archrust_data::ini::decode_text(&b, archrust_data::ini::CodePage::Windows1252))).unwrap_or_default();
                let sent = merge_ignore_file(&file, &me, &glue.friends, &mut glue.bans);
                if !sent.is_empty() {
                    if let Some(net) = &self.net {
                        net.send(Command::Buddy(BuddyCommand::IgnoreList(sent)));
                    }
                }
                glue.save_ignore_file();
            }
        }
    }

    /// Connexion au monde perdue ou reprise : listes remises à zéro (le serveur les renvoie).
    pub(super) fn buddy_reset(&mut self) {
        self.buddy.reset();
    }

    /// `/amis` (`/friends`) et `/ignores` (`/ignore`) : fenêtres de la liste d'amis et d'ignorés.
    pub(super) fn buddy_slash(&mut self, cmd: &str) -> bool {
        match cmd {
            "/amis" | "/friends" => self.buddy.toggle_friends(),
            "/ignores" | "/ignore" => self.buddy.toggle_bans(),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ban(name: &str, flags: u32) -> BuddyEntry {
        BuddyEntry { name: name.into(), flags, relation: 0 }
    }

    #[test]
    fn fichier_des_ignores() {
        let bans = vec![ban("Gêneur", 1), ban("Spam", 7)];
        let text = format_ignore_file(&bans);
        assert_eq!(text, "Gêneur\t1\nSpam\t7\n");
        assert_eq!(parse_ignore_file(&text), vec![("Gêneur".to_string(), 1), ("Spam".to_string(), 7)]);
        assert_eq!(parse_ignore_file("\nSeul\n\tx\n"), vec![("Seul".to_string(), 0)]);
    }

    #[test]
    fn fusion_a_la_fin_de_liste() {
        let friends = vec![ban("Ami", flag::FRIEND)];
        let mut bans = vec![ban("Deja", 1)];
        let file = vec![("Moi".to_string(), 1), ("ami".to_string(), 1), ("DEJA".to_string(), 2), ("Nouveau".to_string(), 0x0200_0003)];
        let sent = merge_ignore_file(&file, "moi", &friends, &mut bans);
        // Son propre nom et les noms connus (sans casse) écartés ; drapeau d'ami retiré de l'entrée locale.
        assert_eq!(sent, vec![("Nouveau".to_string(), 0x0200_0003)]);
        assert_eq!(bans.last(), Some(&ban("Nouveau", 3)));
        // Liste pleine : arrêt.
        let mut full: Vec<BuddyEntry> = (0..MAX_ENTRIES).map(|i| ban(&format!("B{i}"), 0)).collect();
        assert!(merge_ignore_file(&[("Autre".into(), 0)], "moi", &[], &mut full).is_empty());
    }

    #[test]
    fn messages_remplis() {
        assert_eq!(fill("You have added %s to the selected list.", "Arthas"), "You have added Arthas to the selected list.");
        assert_eq!(fill_braces("{{AddRequest_UD.AddRequestBuddy_Display}} has invited you on friend list.", "Jaina"), "Jaina has invited you on friend list.");
        let mut g = BuddyGlue::default();
        g.bans = vec![ban("Gêneur", flag::BLOCK_CHAT), ban("Marchand", flag::BLOCK_TRADE)];
        assert!(g.blocks_chat("gêneur") || g.blocks_chat("Gêneur"));
        assert!(!g.blocks_chat("Marchand"));
    }
}
