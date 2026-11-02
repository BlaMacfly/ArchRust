//! Discussion d'Archonia : types d'AgpmChatting (0x0F), préfixes envoyés, cris de zone (AgpmAreaChatting,
//! 0x4C) et salons de discussion (AgpmChannel, 0x2C).
//!
//! Relevés dans Archonia (sha256 e9c7609b…) :
//! * envoi (`FUN_140786e20`, `FUN_1404d7dc0`) : **tout** part en AGPMCHATTING op 0 **type 0**, le mode de la
//!   barre ajoutant son préfixe au texte (groupe `"/p %s"`, guilde `"/g %s"`, cri `"/s %s"`) ; le serveur
//!   interprète les préfixes (`/race`, `/scream`, `/warn` compris). Seul le chuchotement part en type 5 ;
//! * réception (`FUN_140511e20`) : le type choisit la classe d'affichage, sa couleur (`ini/color.ini`) et son
//!   format (`FUN_1407a2b30`), voir [`class_of`] ;
//! * cri de zone (`FUN_1402b4be0`) : seule l'op 0 compte ; le champ 4 donne le type AgpmChatting du message,
//!   affiché comme une discussion ordinaire (ici en `WorldEvent::Chat`) ; le client n'en envoie jamais ;
//! * salons (`FUN_1402afaf0`, AgcmUIChannel) : ops 1 à 11 et codes de résultat, voir [`channel`] ;
//! * chuchotements (réception `FUN_1402e21e0`) : AGPMCHATTING op 1 / 2 (1 CID) blocage de tous les chuchotements
//!   du personnage mis / levé (attache+4) ; op 3 (5 nom) « `<nom> is blocking whispering.` »
//!   (`Chat_Block_Whisper_Msg`, `0x140512bf0`) ; op 4 (1 CID de l'émetteur, 5 destinataire) « `<nom> is not
//!   connected.` » (`Chat_Offline_Whisper_Target_Msg`, `0x140512d80`), dans l'onglet système. Les envoyeurs des
//!   ops 1 et 2 n'ont pas d'appelant dans Archonia (aucune commande de `chatcmd.xml`) : ArchRust les envoie par
//!   `/chuchotementsoff` et `/chuchotementson`.

use crate::world::WorldEvent;
use archrust_protocol::packet::{sparse, Value};
use archrust_protocol::schema::{main, packet_type, Schema};

/// Types d'AgpmChatting (champ 2), noms d'après l'affichage d'Archonia.
pub mod chat_type {
    pub const NORMAL: i8 = 0;
    /// Cri local « [Shout] » (bulle, couleur `WORD_BALLON`).
    pub const WORD_BALLOON: i8 = 1;
    pub const UNION: i8 = 2;
    pub const GUILD: i8 = 3;
    pub const PARTY: i8 = 4;
    pub const WHISPER: i8 = 5;
    pub const WHISPER2: i8 = 6;
    /// Monde entier : affiché en « [Notice] ».
    pub const WHOLE_WORLD: i8 = 7;
    pub const SYSTEM_LEVEL1: i8 = 8;
    pub const SYSTEM_LEVEL2: i8 = 9;
    /// Système 3 : affiché en « [System] ».
    pub const SYSTEM_LEVEL3: i8 = 10;
    /// Annonce 1 : affichée en « [Notice] ».
    pub const NOTICE_LEVEL1: i8 = 11;
    pub const NOTICE_LEVEL2: i8 = 12;
    pub const GUILD_NOTICE: i8 = 13;
    pub const GUILD_JOINT: i8 = 14;
    pub const EMPHASIS: i8 = 15;
    pub const SIEGEWAR: i8 = 16;
    /// Système sans préfixe, en jaune.
    pub const SYSTEM_NO_PREFIX: i8 = 17;
    /// Trompette de race (`/race`).
    pub const RACE: i8 = 19;
    /// Cri des éléments (`/scream`).
    pub const SCREAM: i8 = 20;
}

/// Longueur maximale d'un message (`FUN_1402e3180` : 0x200 octets).
pub const MAX_MESSAGE: usize = 0x200;

/// Texte envoyé en type 0 pour un mode de discussion d'Archonia : groupe `/p`, guilde `/g`, cri `/s` ;
/// tout autre mode garde le texte tel quel.
pub fn prefixed(kind: i8, message: &str) -> String {
    match kind {
        chat_type::PARTY => format!("/p {message}"),
        chat_type::GUILD => format!("/g {message}"),
        chat_type::WORD_BALLOON => format!("/s {message}"),
        _ => message.to_string(),
    }
}

/// Classe d'affichage d'Archonia (`FUN_140511e20`) : masque de l'onglet, clé de couleur de color.ini, en-tête
/// d'uimessage (`None` : sans en-tête) et présence du nom (`[nom]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatClass {
    pub mask: u16,
    pub color: &'static str,
    pub header: Option<&'static str>,
    pub named: bool,
}

/// Classe d'un type reçu ; `None` : le client ne l'affiche pas (2 union, 15 emphase, 18, inconnus).
pub fn class_of(kind: i8) -> Option<ChatClass> {
    let c = |mask, color, header, named| Some(ChatClass { mask, color, header, named });
    match kind {
        chat_type::NORMAL => c(0x1, "NORMAL_CHAT", None, true),
        chat_type::WORD_BALLOON => c(0x200, "WORD_BALLON", Some("Chat_Area_Header"), true),
        chat_type::GUILD => c(0x2, "GUILD_CHAT", Some("Chat_Guild_Header"), true),
        chat_type::PARTY => c(0x4, "PARTY_CHAT", Some("Chat_Party_Header"), true),
        chat_type::WHISPER | chat_type::WHISPER2 => c(0x8, "WHISPER_SEND", Some("Chat_Whisper_Header"), true),
        chat_type::WHOLE_WORLD | chat_type::NOTICE_LEVEL1 | chat_type::NOTICE_LEVEL2 => c(0x20, "NOTICE_CHAT1", Some("Chat_Notice_Header"), false),
        // Couleur 0 passée à l'affichage : celle de la classe, `SYSTEM_CHAT2`.
        chat_type::SYSTEM_LEVEL1 | chat_type::SYSTEM_LEVEL2 | chat_type::SYSTEM_LEVEL3 => c(0x10, "SYSTEM_CHAT2", Some("Chat_System_Header"), false),
        // Notice de guilde et union : classe de la guilde, couleur propre.
        chat_type::GUILD_NOTICE => c(0x2, "GUILDNOTICE_CHAT", Some("Chat_Guild_Header"), true),
        chat_type::GUILD_JOINT => c(0x2, "GUILDJOINT_CHAT", Some("Chat_Guild_Header"), true),
        chat_type::SIEGEWAR => c(0x80, "SIEGEWARINFO_CHAT", Some("Chat_Shrine_Header"), false),
        // Sans en-tête ni nom : le serveur compose tout le texte.
        chat_type::SYSTEM_NO_PREFIX => c(0x100, "Yellow", None, false),
        chat_type::RACE => c(0x400, "RACE_TRUMPET", None, false),
        chat_type::SCREAM => c(0x800, "Yellow", None, false),
        _ => None,
    }
}

/// Retire le suffixe couleur `\x01RRGGBB` d'un message reçu ; rend le texte et la couleur `0xRRGGBB`.
pub fn split_color_suffix(message: &str) -> (&str, Option<u32>) {
    let b = message.as_bytes();
    if b.len() >= 7 && b[b.len() - 7] == 1 && b[b.len() - 6..].iter().all(u8::is_ascii_hexdigit) {
        let color = u32::from_str_radix(&message[message.len() - 6..], 16).ok();
        return (&message[..message.len() - 7], color);
    }
    (message, None)
}

/// Position où Archonia insère la couleur du suffixe (`FUN_140511e20`) : début du texte, sauf pour les types
/// 0x11, 0x13 et 0x14 (texte `[Titre] [Nom] message`) où elle suit le 2ᵉ `]` (et l'espace qui le suit).
pub fn color_insert_at(kind: i8, text: &str) -> usize {
    if !matches!(kind, chat_type::SYSTEM_NO_PREFIX | chat_type::RACE | chat_type::SCREAM) {
        return 0;
    }
    let mut seen = 0;
    for (i, c) in text.char_indices() {
        if c == ']' {
            seen += 1;
            if seen == 2 {
                let after = i + 1;
                return if text[after..].starts_with(' ') { after + 1 } else { after };
            }
        }
    }
    0
}

/// Texte d'un champ `CHAR[n]` ou d'un bloc, en CP1252, coupé au premier zéro.
pub(crate) fn cp1252(v: Option<&Value>) -> String {
    v.and_then(Value::as_bytes)
        .map(|b| {
            let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
            encoding_rs::WINDOWS_1252.decode(&b[..end]).0.into_owned()
        })
        .unwrap_or_default()
}

/// Cri de zone (0x4C op 0) en ligne de discussion ; toute autre opération est ignorée.
pub fn translate_area(ptype: u8, values: &[Option<Value>]) -> Option<WorldEvent> {
    if ptype != packet_type::AREA_CHATTING {
        return None;
    }
    let field = |i: usize| values.get(i).and_then(Option::as_ref);
    if field(0).and_then(Value::as_i64) != Some(0) {
        return Some(WorldEvent::Unhandled { packet_type: ptype, op: field(0).and_then(Value::as_i64) });
    }
    let message = cp1252(field(6));
    if message.len() > MAX_MESSAGE {
        return Some(WorldEvent::Unhandled { packet_type: ptype, op: Some(0) });
    }
    Some(WorldEvent::Chat {
        from_cid: field(1).and_then(Value::as_i64).unwrap_or(0) as i32,
        channel: field(4).and_then(Value::as_i64).unwrap_or(0) as i8,
        from: cp1252(field(5)),
        to: String::new(),
        message,
    })
}

/// Salons de discussion (AgpmChannel).
pub mod channel {
    pub mod op {
        pub const CREATE: i8 = 1;
        pub const JOIN: i8 = 3;
        pub const LEAVE: i8 = 4;
        pub const MEMBER_JOINED: i8 = 5;
        pub const MEMBER_LEFT: i8 = 6;
        pub const INVITE: i8 = 7;
        pub const PASSWORD: i8 = 8;
        pub const MESSAGE: i8 = 9;
        pub const FIND: i8 = 10;
        pub const RESULT: i8 = 11;
    }
    /// Genres de salon (champ 5).
    pub mod kind {
        pub const PUBLIC: i32 = 1;
        pub const PRIVATE: i32 = 2;
        pub const GUILD: i32 = 3;
    }
    /// Codes de résultat (op 11, champ 6) et fenêtre d'Archonia de chacun (`FUN_1406e6710`).
    pub mod result {
        pub const ALREADY_EXIST: i32 = 1;
        pub const NOT_EXIST: i32 = 2;
        pub const NO_CHARACTER: i32 = 3;
        pub const COUNT_FULL: i32 = 4;
        pub const NOT_GUILD_MASTER: i32 = 5;
        pub const NOT_GUILD_MEMBER: i32 = 6;
        pub const GUILD_ALREADY_EXIST: i32 = 7;
        pub const INVALID_PASSWORD: i32 = 8;
        /// « You already joined the channel » (sysstr 620) dans la discussion.
        pub const ALREADY_JOINED: i32 = 11;
    }
    /// Salons ouverts à la fois (5 fenêtres `UI_ChatRoom%d`).
    pub const MAX_CHANNELS: usize = 5;
    /// Nom : 39 caractères à la saisie (champ de 40) ; mot de passe : 8.
    pub const NAME_MAX: usize = 39;
    pub const PASSWORD_MAX: usize = 8;
    /// Texte d'un message (champ 7, `CHAR[121]`).
    pub const MESSAGE_MAX: usize = 120;

    /// Membres au plus selon le genre (`FUN_1406e6d00`) : 15 public ou privé, 100 guilde.
    pub fn max_members(kind: i32) -> usize {
        if kind == self::kind::GUILD {
            100
        } else {
            15
        }
    }

    /// Événement d'interface d'un code de résultat (`None` : sysstr 620 ou aucun message).
    pub fn result_event(code: i32) -> Option<&'static str> {
        Some(match code {
            result::ALREADY_EXIST => "ChannelAlreadyExist_MsgUI",
            result::NOT_EXIST => "ChannelNotExist_MsgUI",
            result::NO_CHARACTER => "NotExistCharacter_MsgUI",
            result::COUNT_FULL => "ChannelCountFull_MsgUI",
            result::NOT_GUILD_MASTER => "NotGuildMaster_MsgUI",
            result::NOT_GUILD_MEMBER => "NOtGuildMember_MsgUI",
            result::GUILD_ALREADY_EXIST => "GuildChannelAlreadyExist_MsgUI",
            result::INVALID_PASSWORD => "InvalidPassword_MsgUI",
            _ => return None,
        })
    }
}

/// Demandes du joueur sur les salons.
#[derive(Debug, Clone, PartialEq)]
pub enum ChannelCommand {
    /// Création (`CreateChannel`) : mot de passe pour un salon privé seulement.
    Create { name: String, password: Option<String>, kind: i32 },
    /// Entrée (`/join`, « Enter », invitation acceptée).
    Join { name: String },
    /// Sortie (« Leave », croix, `/leave`).
    Leave { name: String },
    /// Invitation de `target` dans le salon `name` de genre `kind`.
    Invite { name: String, target: String, kind: i32 },
    /// Mot de passe demandé par le serveur (op 8).
    Password { name: String, password: String },
    /// Message : sans écho local, le serveur le renvoie aussi à l'émetteur.
    Message { name: String, me: String, text: String },
    /// Recherche d'un joueur (`FindUser_Click`).
    Find { target: String },
    /// Blocage de tous les chuchotements reçus (AGPMCHATTING op 1) ou sa levée (op 2).
    BlockWhisper(bool),
}

/// Opérations d'AGPMCHATTING autres que la discussion (op 0).
pub mod chatting_op {
    pub const SET_BLOCK_WHISPER: i8 = 1;
    pub const RELEASE_BLOCK_WHISPER: i8 = 2;
    pub const REPLY_BLOCK_WHISPER: i8 = 3;
    pub const REPLY_OFFLINE_WHISPER: i8 = 4;
}

/// Réponses du serveur sur les salons.
#[derive(Debug, Clone, PartialEq)]
pub enum ChannelEvent {
    /// Entrée dans un salon (op 3) : genre, mot de passe et membres (`nom1;nom2;`).
    Entered { name: String, password: String, kind: i32, members: Vec<String> },
    /// Sortie confirmée (op 4).
    Left { name: String },
    MemberJoined { name: String, member: String },
    MemberLeft { name: String, member: String },
    /// Invitation reçue de `from` (op 7).
    Invited { name: String, from: String, kind: i32 },
    /// Mot de passe demandé (op 8) : `UI_QueryPassword`.
    PasswordRequired { name: String },
    /// Message d'un salon (op 9), le sien compris.
    Message { name: String, from: String, text: String },
    /// Joueur trouvé (op 10) : le bouton « Whisper » devient actif.
    Found { target: String },
    /// Code de résultat (op 11).
    Result(i32),
    /// Blocage des chuchotements du personnage `cid` mis ou levé (AGPMCHATTING op 1 / 2).
    WhisperBlocking { cid: i32, on: bool },
    /// `name` bloque les chuchotements (op 3).
    WhisperBlocked { name: String },
    /// `name` n'est pas en jeu (op 4) ; `from` = CID de l'émetteur du chuchotement.
    WhisperOffline { from: i32, name: String },
}

/// AGPMCHATTING ops 1 à 4 (chuchotements) en événement ; la discussion (op 0) reste un `WorldEvent::Chat`.
pub fn translate_whisper(ptype: u8, values: &[Option<Value>]) -> Option<ChannelEvent> {
    if ptype != packet_type::CHATTING {
        return None;
    }
    let int = |i: usize| values.get(i).and_then(Option::as_ref).and_then(Value::as_i64);
    let name = || values.get(5).and_then(Option::as_ref).and_then(Value::as_text).unwrap_or_default().trim().to_string();
    Some(match int(0)? as i8 {
        o @ (chatting_op::SET_BLOCK_WHISPER | chatting_op::RELEASE_BLOCK_WHISPER) => ChannelEvent::WhisperBlocking { cid: int(1).unwrap_or(0) as i32, on: o == chatting_op::SET_BLOCK_WHISPER },
        chatting_op::REPLY_BLOCK_WHISPER => ChannelEvent::WhisperBlocked { name: name() },
        chatting_op::REPLY_OFFLINE_WHISPER => ChannelEvent::WhisperOffline { from: int(1).unwrap_or(0) as i32, name: name() },
        _ => return None,
    })
}

/// Membres d'un salon : `nom1;nom2;…;` ; un nom n'est pris que s'il est suivi de `;` (`FUN_1406e6d00`). Une
/// liste absente donne une liste vide (Archonia, lui, plante).
pub fn parse_members(block: Option<&[u8]>) -> Vec<String> {
    let Some(b) = block else { return Vec::new() };
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    let text = encoding_rs::WINDOWS_1252.decode(&b[..end]).0.into_owned();
    let mut parts: Vec<&str> = text.split(';').collect();
    // Le dernier morceau n'est suivi d'aucun `;`.
    parts.pop();
    parts.into_iter().filter(|n| !n.is_empty() && n.len() <= 32).map(str::to_string).collect()
}

fn cut(s: &str, max: usize) -> Vec<u8> {
    let mut b = crate::guild::to_cp1252(s);
    b.truncate(max);
    b
}

/// Paquet d'une demande, champs présents comme ceux d'Archonia (genre et résultat à 0 quand le client les
/// passe ainsi).
pub fn command_packet(cmd: &ChannelCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    use channel::op;
    if let ChannelCommand::BlockWhisper(on) = cmd {
        let o = if *on { chatting_op::SET_BLOCK_WHISPER } else { chatting_op::RELEASE_BLOCK_WHISPER };
        return (main::CHATTING, sparse(main::CHATTING, &[(0, Value::I8(o)), (1, Value::I32(me))]));
    }
    let s = main::CHANNEL;
    let name = |n: &str| Value::Str(cut(n, 40));
    let who = |n: &str| Value::Str(cut(n, 32));
    let values = match cmd {
        ChannelCommand::Create { name: n, password, kind } => {
            let mut v = vec![(0, Value::I8(op::CREATE)), (1, Value::I32(me)), (2, name(n)), (5, Value::I32(*kind)), (6, Value::I32(0))];
            if let Some(pw) = password {
                v.push((3, Value::Str(cut(pw, channel::PASSWORD_MAX))));
            }
            v
        }
        ChannelCommand::Join { name: n } => vec![(0, Value::I8(op::JOIN)), (1, Value::I32(me)), (2, name(n)), (5, Value::I32(0)), (6, Value::I32(0))],
        ChannelCommand::Leave { name: n } => vec![(0, Value::I8(op::LEAVE)), (1, Value::I32(me)), (2, name(n)), (5, Value::I32(0)), (6, Value::I32(0))],
        ChannelCommand::Invite { name: n, target, kind } => {
            vec![(0, Value::I8(op::INVITE)), (1, Value::I32(me)), (2, name(n)), (4, who(target)), (5, Value::I32(*kind)), (6, Value::I32(0))]
        }
        ChannelCommand::Password { name: n, password } => {
            vec![(0, Value::I8(op::PASSWORD)), (1, Value::I32(me)), (2, name(n)), (3, Value::Str(cut(password, channel::PASSWORD_MAX))), (5, Value::I32(0)), (6, Value::I32(0))]
        }
        ChannelCommand::Message { name: n, me: mine, text } => vec![
            (0, Value::I8(op::MESSAGE)),
            (1, Value::I32(me)),
            (2, name(n)),
            (4, who(mine)),
            (5, Value::I32(0)),
            (6, Value::I32(0)),
            (7, Value::Str(cut(text, channel::MESSAGE_MAX))),
        ],
        ChannelCommand::Find { target } => vec![(0, Value::I8(op::FIND)), (1, Value::I32(me)), (4, who(target)), (5, Value::I32(0)), (6, Value::I32(0))],
        ChannelCommand::BlockWhisper(_) => unreachable!("traité plus haut"),
    };
    (s, sparse(s, &values))
}

/// Paquet AgpmChannel reçu en événement (ops 0, 1, 2 et > 11 : rien).
pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<ChannelEvent> {
    use channel::op;
    if ptype != packet_type::CHANNEL {
        return None;
    }
    let field = |i: usize| values.get(i).and_then(Option::as_ref);
    let int = |i: usize| field(i).and_then(Value::as_i64).unwrap_or(0);
    let name = cp1252(field(2));
    let who = cp1252(field(4));
    Some(match int(0) as i8 {
        op::JOIN => ChannelEvent::Entered { name, password: cp1252(field(3)), kind: int(5) as i32, members: parse_members(field(8).and_then(Value::as_bytes)) },
        op::LEAVE => ChannelEvent::Left { name },
        op::MEMBER_JOINED => ChannelEvent::MemberJoined { name, member: who },
        op::MEMBER_LEFT => ChannelEvent::MemberLeft { name, member: who },
        op::INVITE => ChannelEvent::Invited { name, from: who, kind: int(5) as i32 },
        op::PASSWORD => ChannelEvent::PasswordRequired { name },
        op::MESSAGE => ChannelEvent::Message { name, from: who, text: cp1252(field(7)) },
        op::FIND => ChannelEvent::Found { target: who },
        op::RESULT => ChannelEvent::Result(int(6) as i32),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet};

    #[test]
    fn chuchotements_bloques_et_absents() {
        let c = main::CHATTING;
        let p = |pairs: &[(usize, Value)]| decode_packet(c, &build_packet(c, &sparse(c, pairs)).unwrap()).unwrap().values;
        assert_eq!(translate_whisper(packet_type::CHATTING, &p(&[(0, Value::I8(4)), (1, Value::I32(7)), (5, Value::str("Bob"))])), Some(ChannelEvent::WhisperOffline { from: 7, name: "Bob".into() }));
        assert_eq!(translate_whisper(packet_type::CHATTING, &p(&[(0, Value::I8(3)), (1, Value::I32(7)), (5, Value::str("Bob"))])), Some(ChannelEvent::WhisperBlocked { name: "Bob".into() }));
        assert_eq!(translate_whisper(packet_type::CHATTING, &p(&[(0, Value::I8(1)), (1, Value::I32(7))])), Some(ChannelEvent::WhisperBlocking { cid: 7, on: true }));
        assert_eq!(translate_whisper(packet_type::CHATTING, &p(&[(0, Value::I8(0)), (2, Value::I8(5))])), None);
        let (s, v) = command_packet(&ChannelCommand::BlockWhisper(false), 7);
        assert_eq!((s.id, v[0].as_ref().and_then(Value::as_i64), v[1].as_ref().and_then(Value::as_i64)), (packet_type::CHATTING, Some(2), Some(7)));
    }

    #[test]
    fn prefixes_d_archonia() {
        assert_eq!(prefixed(chat_type::PARTY, "salut"), "/p salut");
        assert_eq!(prefixed(chat_type::GUILD, "salut"), "/g salut");
        assert_eq!(prefixed(chat_type::WORD_BALLOON, "salut"), "/s salut");
        assert_eq!(prefixed(chat_type::NORMAL, "/race salut"), "/race salut");
    }

    #[test]
    fn classes_d_affichage() {
        assert_eq!(class_of(chat_type::WHOLE_WORLD).map(|c| c.header), Some(Some("Chat_Notice_Header")));
        assert_eq!(class_of(chat_type::SYSTEM_LEVEL3).map(|c| c.mask), Some(0x10));
        assert_eq!(class_of(chat_type::NOTICE_LEVEL1).map(|c| c.mask), Some(0x20));
        assert_eq!(class_of(chat_type::RACE), Some(ChatClass { mask: 0x400, color: "RACE_TRUMPET", header: None, named: false }));
        assert_eq!(class_of(chat_type::SCREAM).map(|c| c.mask), Some(0x800));
        assert_eq!(class_of(chat_type::UNION), None);
    }

    #[test]
    fn suffixe_couleur() {
        assert_eq!(split_color_suffix("bonjour\u{1}FF8000"), ("bonjour", Some(0xFF8000)));
        assert_eq!(split_color_suffix("bonjour"), ("bonjour", None));
        assert_eq!(color_insert_at(chat_type::RACE, "[Race] [Bob] salut"), 13);
        assert_eq!(color_insert_at(chat_type::NORMAL, "[Race] [Bob] salut"), 0);
    }

    #[test]
    fn cri_de_zone() {
        let s = main::AREA_CHATTING;
        let v = sparse(s, &[(0, Value::I8(0)), (1, Value::I32(42)), (2, Value::I8(1)), (3, Value::I8(1)), (4, Value::U32(19)), (5, Value::str("Bob")), (6, Value::Block(b"[Race] [Bob] salut".to_vec()))]);
        let bytes = build_packet(s, &v).unwrap();
        let p = decode_packet(s, &bytes).unwrap();
        match translate_area(packet_type::AREA_CHATTING, &p.values) {
            Some(WorldEvent::Chat { from_cid, channel, from, message, .. }) => {
                assert_eq!((from_cid, channel, from.as_str(), message.as_str()), (42, 19, "Bob", "[Race] [Bob] salut"));
            }
            other => panic!("{other:?}"),
        }
        let autre = sparse(s, &[(0, Value::I8(1))]);
        assert!(matches!(translate_area(packet_type::AREA_CHATTING, &autre), Some(WorldEvent::Unhandled { .. })));
    }

    #[test]
    fn liste_des_membres() {
        assert_eq!(parse_members(Some(b"Bob;Alice;\0")), vec!["Bob", "Alice"]);
        // Dernier nom sans `;` : ignoré, comme Archonia.
        assert_eq!(parse_members(Some(b"Bob;Alice")), vec!["Bob"]);
        assert!(parse_members(None).is_empty());
        assert!(parse_members(Some(b"\0")).is_empty());
    }

    #[test]
    fn salons_aller_retour() {
        let (s, v) = command_packet(&ChannelCommand::Create { name: "Taverne".into(), password: Some("secret".into()), kind: channel::kind::PRIVATE }, 7);
        let p = decode_packet(s, &build_packet(s, &v).unwrap()).unwrap();
        assert_eq!(p.values[0].as_ref().and_then(Value::as_i64), Some(1));
        assert_eq!(cp1252(p.values[2].as_ref()), "Taverne");
        assert_eq!(cp1252(p.values[3].as_ref()), "secret");
        assert_eq!(p.values[5].as_ref().and_then(Value::as_i64), Some(2));
        let (s, v) = command_packet(&ChannelCommand::Message { name: "Taverne".into(), me: "Bob".into(), text: "salut".into() }, 7);
        let p = decode_packet(s, &build_packet(s, &v).unwrap()).unwrap();
        assert_eq!(cp1252(p.values[4].as_ref()), "Bob");
        assert_eq!(cp1252(p.values[7].as_ref()), "salut");
        assert!(p.values[3].is_none());

        let entered = sparse(s, &[(0, Value::I8(3)), (2, Value::str("Taverne")), (5, Value::I32(1)), (8, Value::Block(b"Bob;Alice;\0".to_vec()))]);
        let p = decode_packet(s, &build_packet(s, &entered).unwrap()).unwrap();
        assert_eq!(
            translate(packet_type::CHANNEL, &p.values),
            Some(ChannelEvent::Entered { name: "Taverne".into(), password: String::new(), kind: 1, members: vec!["Bob".into(), "Alice".into()] })
        );
        // Liste absente : pas de plantage.
        let bare = sparse(s, &[(0, Value::I8(3)), (2, Value::str("Taverne"))]);
        assert!(matches!(translate(packet_type::CHANNEL, &bare), Some(ChannelEvent::Entered { members, .. }) if members.is_empty()));
        let res = sparse(s, &[(0, Value::I8(11)), (6, Value::I32(8))]);
        assert_eq!(translate(packet_type::CHANNEL, &res), Some(ChannelEvent::Result(8)));
        assert_eq!(channel::result_event(8), Some("InvalidPassword_MsgUI"));
    }
}
