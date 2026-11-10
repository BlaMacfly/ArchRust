//! Guildes : paquets AGPMGUILD (0x15) et AGPMEVENTGUILD (0x39), état de la guilde côté client.
//!
//! ```text
//! AGPMGUILD (drapeaux 4) : 0 op, 1 CID (message système : code), 2 nom de la guilde (texte 1),
//!   3 chef ou joueur (texte 2), 4 8 × INT32 (nombre), 6 date de création, 7 taille, 9 mot de passe,
//!   10 annonce (CP1252), 16 membre (mini GUILD_MEMBER : nom, rang, adhésion, niveau, TID, en ligne)
//!   envoyé : 0 création (2, 9), 1 invitation (2, 16 = l'invité), 2 refus (2), 3 adhésion (2) ou accord
//!            d'une demande par le chef (16), 4 demande de départ, 29 départ accepté (16), 5 exclusion
//!            (16), 6 dissolution (9), 9 annonce (10), 7 agrandissement (7 = taille suivante),
//!            28 liste des guildes (21 = GUILD_LIST { 1 page }) ou recherche (2 = nom)
//!   reçu   : 0 la guilde, 1 invitation (2 guilde, 3 inviteur), 3 un membre, 4 départ et 5 exclusion
//!            (16 = le partant), 6 dissolution, 7 nouvelle taille (7), 8 lien CID ↔ guilde (1, 2, 16),
//!            9 annonce, 10 message système (1 code, 2 texte, 4 nombre), 28 page de la liste des guildes
//!            (2 nom cherché, 21 GUILD_LIST : total, page, nombre, 19 GUILD_LIST_ITEM)
//!   op 48 (second, passation) : seule l'op est un champ, la suite est brute (sous-op en +0x18) ;
//!            voir `society`.
//!   combattants (types 3 et 4, `UI_GuildBattleMember`) : envoyé 14 (1 CID, 2 guilde, 18 GUILD_BATTLE_PERSON
//!            {I16 effectif, bloc de (U16 longueur, nom sans NUL)}, FUN_1408c59a0 / 0x140604e70) juste avant la
//!            demande (15) ou l'acceptation (16) ; reçu 26 (2 guilde, 19 GUILD_BATTLE_MEMBER {guilde, nom,
//!            3 × U32}) : une ligne de `UI_GuildBattleResult`, 27 (2 guilde, 20 GUILD_BATTLE_MEMBER_LIST : deux
//!            tableaux d'enregistrements de 0x48 octets, nom à +0, U32 à +0x38, +0x3c, +0x40, I32 à +0x44 avec
//!            -1 = inchangé, FUN_1404dee40) : tous les combattants, 46 (2 guilde, manche au champ 14 du mini
//!            GUILD_BATTLE du champ 17, sinon au premier entier du champ 4) ; 42 : trêve refusée
//!            (`Guild_Hostile_Leave_Reject`)
//! AGPMEVENTGUILD (drapeaux 1) : op 0 (1 EVENT_BASE du gestionnaire, fonction 23 ; 2 CID) → op 1 accord
//! ```
//!
//! À l'entrée en jeu, un membre reçoit sa guilde (op 0), chaque membre (op 3) puis son lien (op 8).
//! Rangs : 1 membre, 2 demande d'adhésion, 3 demande de départ, 10 chef, 11 second.

use archrust_protocol::packet::{build_mini, decode_mini, sparse, Value};
use archrust_protocol::schema::{main, mini, packet_type, Schema};
use std::collections::HashMap;

/// Fonction d'événement « gestionnaire de guilde » des PNJ.
pub const FUNCTION_GUILD: i32 = 23;
/// Entrepôt de guilde, chez les gardiens de banque (AgcmEventGuild FUN_1406a6560, bouton `sysstr` 611).
pub const FUNCTION_GUILD_WAREHOUSE: i32 = 35;
/// Type de source « personnage » d'une désignation d'événement.
const BASE_TYPE_CHARACTER: i8 = 3;

/// Opérations d'AGPMGUILD.
pub mod op {
    pub const CREATE: i8 = 0;
    pub const JOIN_REQUEST: i8 = 1;
    pub const JOIN_REJECT: i8 = 2;
    pub const JOIN: i8 = 3;
    pub const LEAVE: i8 = 4;
    pub const FORCED_LEAVE: i8 = 5;
    pub const DESTROY: i8 = 6;
    pub const UPDATE_MAX_MEMBER_COUNT: i8 = 7;
    pub const CHAR_DATA: i8 = 8;
    pub const UPDATE_NOTICE: i8 = 9;
    pub const SYSTEM_MESSAGE: i8 = 10;
    pub const UPDATE_STATUS: i8 = 11;
    pub const UPDATE_RECORD: i8 = 12;
    pub const BATTLE_INFO: i8 = 13;
    /// Combattants choisis (C → S), avant la demande ou l'acceptation d'un combat des types 3 et 4.
    pub const BATTLE_PERSON: i8 = 14;
    pub const BATTLE_REQUEST: i8 = 15;
    pub const BATTLE_ACCEPT: i8 = 16;
    pub const BATTLE_REJECT: i8 = 17;
    pub const BATTLE_CANCEL_REQUEST: i8 = 18;
    pub const BATTLE_CANCEL_ACCEPT: i8 = 19;
    pub const BATTLE_CANCEL_REJECT: i8 = 20;
    pub const BATTLE_START: i8 = 21;
    pub const BATTLE_UPDATE_TIME: i8 = 22;
    pub const BATTLE_UPDATE_SCORE: i8 = 23;
    pub const BATTLE_WITHDRAW: i8 = 24;
    pub const BATTLE_RESULT: i8 = 25;
    /// Un combattant et ses points (S → C, rappel 0x26).
    pub const BATTLE_MEMBER: i8 = 26;
    /// Tous les combattants des deux guildes (S → C, rappel 0x27).
    pub const BATTLE_MEMBER_LIST: i8 = 27;
    /// Manche du combat (S → C, rappel 0x28).
    pub const BATTLE_ROUND: i8 = 46;
    /// Trêve refusée par le chef adverse (S → C, rappel 0x3c).
    pub const HOSTILE_LEAVE_REJECT: i8 = 42;
    pub const GUILD_LIST: i8 = 28;
    pub const BUY_GUILDMARK: i8 = 32;
    pub const BUY_GUILDMARK_FORCE: i8 = 33;
    pub const JOINT_REQUEST: i8 = 34;
    pub const JOINT_REJECT: i8 = 35;
    pub const JOINT: i8 = 36;
    pub const JOINT_LEAVE: i8 = 37;
    pub const HOSTILE_REQUEST: i8 = 38;
    pub const HOSTILE_REJECT: i8 = 39;
    pub const HOSTILE: i8 = 40;
    pub const HOSTILE_LEAVE_REQUEST: i8 = 41;
    pub const HOSTILE_LEAVE: i8 = 43;
    pub const JOINT_DETAIL: i8 = 44;
    pub const HOSTILE_DETAIL: i8 = 45;
    pub const LEAVE_ALLOW: i8 = 29;
    pub const CLASS_SOCIETY: i8 = 48;
}

/// Op 48 (Archonia : FUN_1406048a0, FUN_1406047d0, FUN_140606d60, FUN_140606c90, FUN_140206df0) : paquet dont
/// la suite est brute. Tailles des structures, pied 0x6B non compris :
///
/// ```text
///   2 → (0x80)  +0x19 soi, +0x4a membre, +0x7c rang (11 nommer second, 1 retirer ce rang)
///   3 ← (0xa0)  +0x19 guilde, +0x4a chef, +0x6b membre, +0x9c rang     → GuildAppointConfirm_Message
///   4 → (0x84)  +0x19 guilde, +0x4a soi, +0x7c rang, +0x80 réponse (1 accepte)
///   5 → (0x7b)  +0x19 soi, +0x4a second (passation)
///   6 ← (0x9c)  +0x19 guilde, +0x4a chef, +0x6b second                → GuildSuccessionConfirm_Message
///   7 → (0x80)  +0x19 guilde, +0x4a soi, +0x7c réponse
///   8 ← (0x70)  +0x19 guilde, +0x3a membre, +0x6c rang                 → GuildMemberRankUpdate_Message
///   1 ←         +0x19 texte (message système)
/// ```
pub mod society {
    pub const TEXT: u8 = 1;
    pub const APPOINT: u8 = 2;
    pub const APPOINT_ASK: u8 = 3;
    pub const APPOINT_ANSWER: u8 = 4;
    pub const SUCCESSION: u8 = 5;
    pub const SUCCESSION_ASK: u8 = 6;
    pub const SUCCESSION_ANSWER: u8 = 7;
    pub const RANK_UPDATE: u8 = 8;
    /// Décalage de la sous-opération.
    pub const SUB: usize = 0x18;
}

/// Rangs d'un membre.
pub mod rank {
    pub const NORMAL: i32 = 1;
    pub const JOIN_REQUEST: i32 = 2;
    pub const LEAVE_REQUEST: i32 = 3;
    pub const MASTER: i32 = 10;
    pub const SUBMASTER: i32 = 11;
}

/// Codes des messages système (op 10) envoyés par ArchCore.
pub mod code {
    pub const EXIST_GUILD_ID: i32 = 0;
    pub const NEED_MORE_LEVEL: i32 = 1;
    pub const NEED_MORE_MONEY: i32 = 2;
    pub const USE_SPECIAL_CHAR: i32 = 4;
    pub const ALREADY_MEMBER: i32 = 5;
    pub const MAX_MEMBER: i32 = 6;
    pub const JOIN_FAIL: i32 = 7;
    pub const LEAVE_FAIL: i32 = 8;
    pub const NOT_MASTER: i32 = 9;
    pub const INVALID_PASSWORD: i32 = 10;
    pub const CREATE_COMPLETE: i32 = 11;
    pub const DESTROY: i32 = 12;
    pub const JOIN: i32 = 13;
    pub const JOIN_REJECT: i32 = 14;
    pub const LEAVE: i32 = 15;
    pub const FORCED_LEAVE: i32 = 16;
    pub const ALREADY_MEMBER2: i32 = 17;
    pub const NOT_ENOUGH_GHELD: i32 = 44;
    pub const NOT_ENOUGH_SKULL: i32 = 45;
    /// Dissolution refusée : guilde trop jeune (« Guild_DestroyFailTooEarly », boîte d'information).
    pub const DESTROY_FAIL_TOO_EARLY: i32 = 46;
    pub const NO_EXIST_SEARCH_GUILD: i32 = 47;
    /// Une autre guilde a le même emblème : Archonia propose de l'acheter quand même (op 33).
    pub const BUY_GUILDMARK_DUPLICATE: i32 = 59;
    pub const JOIN_REQUEST_SELF: i32 = 48;
    pub const LEAVE_REQUEST_SELF: i32 = 50;
    /// Bataille de l'Archlord : défense ou attaque réussie ou manquée (`Guild_Archlord_*`).
    pub const ARCHLORD_DEFENSE_SUCCESS: i32 = 90;
    pub const ARCHLORD_DEFENSE_FAILURE: i32 = 91;
    pub const ARCHLORD_ATTACK_SUCCESS: i32 = 92;
    pub const ARCHLORD_ATTACK_FAILURE: i32 = 93;
    /// Le chef de la guilde visée refuse les relations (option 0x40) : `sysstr` 712.
    pub const REFUSE_RELATION: i32 = 94;
    /// Entrepôt de guilde : effectif trop faible pour déposer, dissolution avec un entrepôt non vide.
    pub const STORE_MEMBER_FAIL: i32 = 95;
    pub const DESTROY_KEEP_FAIL: i32 = 96;
    /// Départ ou exclusion dans une zone de siège en guerre (client de Webzen ; Archonia s'arrête à 96).
    pub const SIEGE_AREA: i32 = 97;
}

/// Opérations d'AGPMEVENTGUILD.
pub mod event_op {
    pub const REQUEST: i8 = 0;
    pub const GRANT: i8 = 1;
    pub const REQUEST_WAREHOUSE: i8 = 2;
    pub const GRANT_WAREHOUSE: i8 = 3;
}

/// Entrepôt de guilde (AgpmGuildWarehouse, 0x54) : opérations, statut de ses objets, grille, résultats.
pub mod warehouse {
    pub const INFO: i8 = 0;
    pub const OPEN: i8 = 1;
    pub const CLOSE: i8 = 2;
    pub const MONEY_IN: i8 = 3;
    pub const MONEY_OUT: i8 = 4;
    pub const ITEM_IN: i8 = 5;
    pub const ITEM_OUT: i8 = 6;
    pub const EXPAND: i8 = 7;
    pub const ITEM_STATUS: i8 = 53;
    /// Pages, lignes, colonnes (`CTL_GRID_GWARE`, 4 pages de 7 × 4).
    pub const GRID: (i16, i16, i16) = (4, 7, 4);
    pub const RESULT_OPEN: i32 = 2;
    pub const RESULT_NOT_ENOUGH_MONEY: i32 = 4;
    /// Droits (FUN_14030b830) : le chef a tout, les membres déposent objets et argent.
    pub const RIGHT_ITEM_OUT: i32 = 0x02;
    pub const RIGHT_MONEY_OUT: i32 = 0x08;
    pub const RIGHTS_MASTER: i32 = 0x1f;
    pub const RIGHTS_MEMBER: i32 = 0x05;
}

/// Membre d'une guilde (mini GUILD_MEMBER).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuildMember {
    pub name: String,
    pub rank: i32,
    /// Date d'adhésion (secondes depuis 1970).
    pub joined_at: i64,
    pub level: i64,
    /// Modèle du personnage (race et classe).
    pub tid: i32,
    pub online: bool,
}

impl GuildMember {
    pub fn leads(&self) -> bool {
        matches!(self.rank, rank::MASTER | rank::SUBMASTER)
    }

    /// Membre à part entière (ni demande d'adhésion).
    pub fn active(&self) -> bool {
        self.rank != rank::JOIN_REQUEST
    }
}

/// Mini GUILD_MEMBER réduit au nom (invitation, départ, exclusion).
fn member_named(name: &str) -> Value {
    let s = mini::GUILD_MEMBER;
    Value::Packet(build_mini(s, &sparse(s, &[(0, Value::str(name.as_bytes()))])).unwrap_or_default())
}

fn int(values: &[Option<Value>], i: usize) -> Option<i64> {
    values.get(i).and_then(Option::as_ref).and_then(Value::as_i64)
}

fn text(values: &[Option<Value>], i: usize) -> String {
    values.get(i).and_then(Option::as_ref).and_then(Value::as_text).unwrap_or_default().trim().to_string()
}

/// Texte CP1252 d'un bloc (annonce), jusqu'au premier octet nul.
fn cp1252(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    encoding_rs::WINDOWS_1252.decode(&bytes[..end]).0.into_owned()
}

/// Encode un texte saisi en CP1252 (caractère inconnu : `?`).
pub fn to_cp1252(s: &str) -> Vec<u8> {
    let (bytes, _, errors) = encoding_rs::WINDOWS_1252.encode(s);
    if !errors {
        return bytes.into_owned();
    }
    s.chars()
        .map(|c| {
            let mut buf = [0u8; 4];
            let (b, _, err) = encoding_rs::WINDOWS_1252.encode(c.encode_utf8(&mut buf));
            if err || b.len() != 1 { b'?' } else { b[0] }
        })
        .collect()
}

fn decode_member(values: &[Option<Value>]) -> Option<GuildMember> {
    let b = values.get(16).and_then(Option::as_ref).and_then(Value::as_bytes)?;
    let (_, m) = decode_mini(mini::GUILD_MEMBER, b).ok()?;
    let name = text(&m, 0);
    if name.is_empty() {
        return None;
    }
    Some(GuildMember {
        name,
        rank: int(&m, 1).unwrap_or(0) as i32,
        joined_at: int(&m, 2).unwrap_or(0),
        level: int(&m, 3).unwrap_or(0),
        tid: int(&m, 4).unwrap_or(0) as i32,
        online: int(&m, 5).unwrap_or(0) != 0,
    })
}

/// Fiche d'une guilde (op 0).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuildInfo {
    pub name: String,
    pub master: String,
    pub created_at: i64,
    pub max_members: i32,
    pub notice: String,
    /// Emblème : TID (fond | motif << 8 | symbole << 16) et couleur (indice par couche).
    pub mark_tid: i32,
    pub mark_color: i32,
}

/// Demande d'alliance, d'hostilité ou de trêve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    Joint,
    Hostile,
    HostileLeave,
}

/// Autre guilde d'un paquet d'alliance ou d'hostilité (champ 17, GUILD_BATTLE champ 0).
fn other_guild(values: &[Option<Value>]) -> String {
    values.get(17).and_then(Option::as_ref).and_then(Value::as_bytes).and_then(|b| decode_mini(mini::GUILD_BATTLE, b).ok()).map(|(_, m)| text(&m, 0)).unwrap_or_default()
}

/// Une guilde de la liste (mini GUILD_LIST_ITEM).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuildListEntry {
    pub name: String,
    pub master: String,
    pub submaster: String,
    pub level: i32,
    /// Colonne « Classement » de la liste (champ 4, FUN_1408c8ca0).
    pub ranking: i32,
    pub members: i32,
    pub max_members: i32,
    /// Emblème : TID (fond | motif << 8 | symbole << 16) et couleur.
    pub mark_tid: i32,
    pub mark_color: i32,
    pub points: i32,
}

/// Page de la liste des guildes (op 28).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuildList {
    /// Nombre total de guildes.
    pub total: i32,
    pub page: i32,
    /// Nom cherché (recherche), vide sinon.
    pub searched: String,
    pub entries: Vec<GuildListEntry>,
}

/// Combat de guildes (mini GUILD_BATTLE) : temps en secondes, sur l'horloge du serveur.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuildBattle {
    pub enemy: String,
    pub enemy_master: String,
    pub accept_at: i64,
    pub ready: i64,
    pub start_at: i64,
    pub kind: i32,
    pub duration: i64,
    pub person: i32,
    pub clock: i64,
    pub score: i32,
    pub enemy_score: i32,
    pub up_score: i32,
    pub enemy_up_score: i32,
    pub result: i32,
    /// Manche (champ 14, combats en manches ; op 46).
    pub round: u32,
}

/// Combattant d'un combat de guildes (ops 26 et 27) : nom et ses entiers (points, puis deux autres, le
/// quatrième n'existe qu'à l'op 27 : -1 = inchangé).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BattleFighter {
    pub name: String,
    pub points: u32,
    pub b: u32,
    pub c: u32,
    pub d: i32,
}

/// Taille d'un enregistrement de l'op 27 (FUN_1404dee40) et décalage de ses entiers.
pub const FIGHTER_RECORD: usize = 0x48;
const FIGHTER_NUMBERS: usize = 0x38;

impl BattleFighter {
    /// Enregistrement de 0x48 octets (nom zéro-terminé, puis U32, U32, U32, I32 à +0x38).
    pub fn encode(&self) -> [u8; FIGHTER_RECORD] {
        let mut out = [0u8; FIGHTER_RECORD];
        let n = to_cp1252(&self.name);
        let len = n.len().min(FIGHTER_NUMBERS - 1);
        out[..len].copy_from_slice(&n[..len]);
        for (k, v) in [self.points, self.b, self.c, self.d as u32].into_iter().enumerate() {
            out[FIGHTER_NUMBERS + 4 * k..FIGHTER_NUMBERS + 4 * k + 4].copy_from_slice(&v.to_le_bytes());
        }
        out
    }

    fn decode(rec: &[u8]) -> Self {
        let u = |o: usize| rec.get(o..o + 4).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()));
        BattleFighter { name: cp1252(&rec[..FIGHTER_NUMBERS.min(rec.len())]).trim().to_string(), points: u(0x38), b: u(0x3c), c: u(0x40), d: u(0x44) as i32 }
    }
}

/// `count` enregistrements de 0x48 octets d'un bloc (moins si le bloc est court).
fn fighters(block: Option<&[u8]>, count: i64) -> Vec<BattleFighter> {
    let Some(b) = block else { return Vec::new() };
    b.chunks_exact(FIGHTER_RECORD).take(count.max(0) as usize).map(BattleFighter::decode).collect()
}

/// Bloc des combattants choisis (op 14) : (U16 longueur, nom sans NUL) mis bout à bout.
pub fn encode_person_block(names: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    for n in names {
        let b = to_cp1252(n);
        out.extend_from_slice(&(b.len() as u16).to_le_bytes());
        out.extend_from_slice(&b);
    }
    out
}

/// Noms d'un bloc de l'op 14.
pub fn decode_person_block(mut b: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    while b.len() >= 2 {
        let len = u16::from_le_bytes([b[0], b[1]]) as usize;
        let Some(name) = b.get(2..2 + len) else { break };
        out.push(cp1252(name));
        b = &b[2 + len..];
    }
    out
}

impl GuildBattle {
    /// Secondes restantes : avant le début (statut 2), ou du combat (statut 3) (FUN_1408505a0).
    pub fn remaining(&self, status: i32) -> i64 {
        let left = if status == battle::status::READY { self.ready + self.accept_at - self.clock } else { self.duration + self.start_at - self.clock };
        left.max(0)
    }
}

/// Combats de guildes : statuts, types, résultats.
pub mod battle {
    pub mod status {
        pub const NONE: i32 = 0;
        pub const DECLARE: i32 = 1;
        pub const READY: i32 = 2;
        pub const BATTLE: i32 = 3;
    }
    /// Types (`GuildBattleMode_*`) : points, élimination, mort du chef, survie de groupe, survie individuelle.
    pub const KINDS: [&str; 5] = ["GuildBattleMode_Point", "GuildBattleMode_KnockoutPoint", "GuildBattleMode_ChiefDeath", "GuildBattleMode_TotalSurvival", "GuildBattleMode_PrivateSurvival"];
    /// Durées proposées (minutes) et effectifs (groupe, individuel) : listes d'Archonia (`DAT_140f6b690`…).
    pub const MINUTES: [i64; 6] = [5, 10, 30, 60, 90, 120];
    pub const TOTAL_MEN: [i32; 6] = [5, 10, 15, 20, 25, 50];
    pub const PRIVATE_MEN: [i32; 4] = [3, 5, 7, 9];
}

fn decode_battle(values: &[Option<Value>]) -> Option<GuildBattle> {
    let b = values.get(17).and_then(Option::as_ref).and_then(Value::as_bytes)?;
    let (_, m) = decode_mini(mini::GUILD_BATTLE, b).ok()?;
    let n = |k: usize| int(&m, k).unwrap_or(0);
    Some(GuildBattle {
        enemy: text(&m, 0),
        enemy_master: text(&m, 1),
        accept_at: n(2),
        ready: n(3),
        start_at: n(4),
        kind: n(5) as i32,
        duration: n(6),
        person: n(7) as i32,
        clock: n(8),
        score: n(9) as i32,
        enemy_score: n(10) as i32,
        up_score: n(11) as i32,
        enemy_up_score: n(12) as i32,
        result: n(13) as i32,
        round: n(14) as u32,
    })
}

/// Paquets de guilde reçus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuildEvent {
    /// Statut de la guilde (op 11) : 0 aucun, 1 combat déclaré, 2 préparation, 3 combat.
    Status { guild: String, status: i32 },
    /// Bilan des combats (op 12).
    Record { guild: String, win: i32, draw: i32, lose: i32, points: i32 },
    /// Combats de guildes : `op` = 13 infos, 15 défi reçu, 16 accepté, 18 annulation demandée, 19 annulé,
    /// 21 début, 22 horloge, 23 scores, 25 résultat.
    Battle { op: i8, guild: String, master: String, battle: GuildBattle },
    /// Demande reçue par le chef (ops 34, 38, 41) : sa guilde, la guilde qui demande, son chef.
    RelationAsk { kind: Relation, guild: String, other: String, master: String },
    /// Alliée (op 36) : date d'entrée, rôle (2 fondatrice, 1 autre).
    Joint { guild: String, other: String, date: i64, role: i32 },
    /// Départ de l'alliance (op 37) : la guilde partante (la sienne : plus d'alliance), nouvelle fondatrice.
    JointLeave { guild: String, other: String, leader: String },
    /// Hostile depuis `date` (op 40), plus hostile (op 43).
    Hostile { guild: String, other: String, date: i64 },
    HostileRemoved { guild: String, other: String },
    /// Détails des alliées (op 44) ou des hostiles (op 45), entrées de la liste des guildes.
    RelationDetails { hostile: bool, entries: Vec<GuildListEntry> },
    /// Emblème acheté (op 32 ou 33) : guilde, TID, couleur.
    MarkBought { guild: String, tid: i32, color: i32 },
    /// Nouvelle taille de la guilde (op 7).
    MaxMembers { guild: String, max: i32 },
    /// Page de la liste des guildes (op 28).
    List(GuildList),
    /// Le chef propose au joueur d'être second (op 48, sous-op 3) : guilde, chef, membre, rang.
    AppointAsk { guild: String, master: String, member: String, rank: i32 },
    /// Le chef propose au second de diriger la guilde (op 48, sous-op 6).
    SuccessionAsk { guild: String, master: String, member: String },
    /// Rang d'un membre changé (op 48, sous-op 8) : 1 membre, 10 chef, 11 second.
    RankChanged { guild: String, member: String, rank: i32 },
    /// Texte envoyé par le serveur (op 48, sous-op 1).
    SocietyText(String),
    /// La guilde du joueur (création, entrée en jeu, adhésion).
    Guild(GuildInfo),
    /// Invitation reçue : guilde et inviteur.
    Invited { guild: String, from: String },
    /// Membre ajouté ou mis à jour.
    Member { guild: String, member: GuildMember },
    /// Départ (`forced` : exclusion) d'un membre, joueur compris.
    Left { guild: String, name: String, forced: bool },
    Destroyed { guild: String },
    /// Lien d'un personnage à sa guilde (le joueur ou un autre en vue).
    Linked { cid: i32, guild: String, member: Option<GuildMember>, mark: (i32, i32) },
    Notice { guild: String, text: String },
    /// Message système : code, textes, nombre.
    System { code: i32, text: String, text2: String, number: i64, number2: i64 },
    /// Combattant mis à jour (op 26) : sa guilde (celle du mini), lui et ses points.
    BattleFighter { guild: String, fighter: BattleFighter },
    /// Tous les combattants (op 27) : guilde du champ 2 et les siens, puis ceux de la guilde adverse.
    BattleFighters { guild: String, mine: Vec<BattleFighter>, enemy: Vec<BattleFighter> },
    /// Manche (op 46).
    BattleRound { guild: String, round: u32 },
    /// Trêve refusée (op 42).
    HostileLeaveRejected { guild: String },
    /// Accord d'un gestionnaire de guilde (AGPMEVENTGUILD op 1) : CID du PNJ.
    ManagerGranted { npc: i32 },
    /// Accord d'un gardien pour l'entrepôt de guilde (op 3) : CID du PNJ.
    WarehouseGranted { npc: i32 },
    /// Paquet de l'entrepôt (0x54) : op, argent, pages en plus, résultat.
    Warehouse { op: i8, money: i64, slots: i32, result: i32 },
}

/// Texte CP1252 zéro-terminé lu à `off` sur `max` octets au plus.
fn raw_text(raw: &[u8], off: usize, max: usize) -> String {
    raw.get(off..(off + max).min(raw.len())).map(cp1252).unwrap_or_default().trim().to_string()
}

fn raw_u32(raw: &[u8], off: usize) -> i32 {
    raw.get(off..off + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as i32).unwrap_or(0)
}

/// Op 48 : sous-opérations reçues.
fn translate_society(raw: &[u8]) -> Option<GuildEvent> {
    Some(match *raw.get(society::SUB)? {
        society::TEXT => GuildEvent::SocietyText(raw_text(raw, 0x19, 256)),
        society::APPOINT_ASK => GuildEvent::AppointAsk {
            guild: raw_text(raw, 0x19, 49),
            master: raw_text(raw, 0x4a, 33),
            member: raw_text(raw, 0x6b, 49),
            rank: raw_u32(raw, 0x9c),
        },
        society::SUCCESSION_ASK => GuildEvent::SuccessionAsk { guild: raw_text(raw, 0x19, 49), master: raw_text(raw, 0x4a, 33), member: raw_text(raw, 0x6b, 49) },
        society::RANK_UPDATE => GuildEvent::RankChanged { guild: raw_text(raw, 0x19, 33), member: raw_text(raw, 0x3a, 49), rank: raw_u32(raw, 0x6c) },
        _ => return None,
    })
}

fn decode_list(values: &[Option<Value>]) -> Option<GuildList> {
    let b = values.get(21).and_then(Option::as_ref).and_then(Value::as_bytes)?;
    let (_, l) = decode_mini(mini::GUILD_LIST, b).ok()?;
    let count = int(&l, 2).unwrap_or(0).clamp(0, 19) as usize;
    let mut entries = Vec::new();
    for i in 0..count {
        let Some((_, e)) = l.get(3 + i).and_then(Option::as_ref).and_then(Value::as_bytes).and_then(|b| decode_mini(mini::GUILD_LIST_ITEM, b).ok()) else { continue };
        let n = |k: usize| int(&e, k).unwrap_or(0) as i32;
        entries.push(GuildListEntry {
            name: text(&e, 0),
            master: text(&e, 1),
            submaster: text(&e, 2),
            level: n(3),
            ranking: n(4),
            members: n(5),
            max_members: n(6),
            mark_tid: n(8),
            mark_color: n(9),
            points: n(10),
        });
    }
    Some(GuildList { total: int(&l, 0).unwrap_or(0) as i32, page: int(&l, 1).unwrap_or(0) as i32, searched: text(values, 2), entries })
}

/// Traduit un paquet 0x15 ou 0x39 (`raw` : le paquet en clair, pour l'op 48), sinon `None`.
pub fn translate(packet_type: u8, values: &[Option<Value>], raw: &[u8]) -> Option<GuildEvent> {
    if packet_type == packet_type::EVENT_GUILD {
        let o = int(values, 0)? as i8;
        if o != event_op::GRANT && o != event_op::GRANT_WAREHOUSE {
            return None;
        }
        let base = values.get(1).and_then(Option::as_ref).and_then(Value::as_bytes).and_then(|b| decode_mini(mini::EVENT_BASE, b).ok());
        let npc = base.and_then(|(_, v)| int(&v, 1)).unwrap_or(0) as i32;
        return Some(if o == event_op::GRANT { GuildEvent::ManagerGranted { npc } } else { GuildEvent::WarehouseGranted { npc } });
    }
    if packet_type == packet_type::GUILD_WAREHOUSE {
        return Some(GuildEvent::Warehouse {
            op: int(values, 0)? as i8,
            money: int(values, 2).unwrap_or(-1),
            slots: int(values, 3).unwrap_or(0) as i32,
            result: int(values, 8).unwrap_or(-1) as i32,
        });
    }
    if packet_type != packet_type::GUILD {
        return None;
    }
    let guild = text(values, 2);
    Some(match int(values, 0)? as i8 {
        op::CREATE => GuildEvent::Guild(GuildInfo {
            name: guild,
            master: text(values, 3),
            created_at: int(values, 6).unwrap_or(0),
            max_members: int(values, 7).unwrap_or(0) as i32,
            notice: values.get(10).and_then(Option::as_ref).and_then(Value::as_bytes).map(cp1252).unwrap_or_default(),
            mark_tid: int(values, 22).unwrap_or(0) as i32,
            mark_color: int(values, 23).unwrap_or(0) as i32,
        }),
        op::JOIN_REQUEST => GuildEvent::Invited { guild, from: text(values, 3) },
        op::JOIN => GuildEvent::Member { guild, member: decode_member(values)? },
        o @ (op::LEAVE | op::FORCED_LEAVE) => GuildEvent::Left { guild, name: decode_member(values)?.name, forced: o == op::FORCED_LEAVE },
        op::DESTROY => GuildEvent::Destroyed { guild },
        op::UPDATE_MAX_MEMBER_COUNT => GuildEvent::MaxMembers { guild, max: int(values, 7)? as i32 },
        op::UPDATE_STATUS => GuildEvent::Status { guild, status: int(values, 11).unwrap_or(0) as i32 },
        op::UPDATE_RECORD => {
            let n = |k: usize| int(values, k).unwrap_or(0) as i32;
            GuildEvent::Record { guild, win: n(12), draw: n(13), lose: n(14), points: n(15) }
        }
        o @ (op::BATTLE_INFO | op::BATTLE_REQUEST | op::BATTLE_ACCEPT | op::BATTLE_CANCEL_REQUEST | op::BATTLE_CANCEL_ACCEPT | op::BATTLE_START | op::BATTLE_UPDATE_TIME | op::BATTLE_UPDATE_SCORE | op::BATTLE_RESULT) => {
            GuildEvent::Battle { op: o, guild, master: text(values, 3), battle: decode_battle(values).unwrap_or_default() }
        }
        op::GUILD_LIST => GuildEvent::List(decode_list(values)?),
        op::BATTLE_MEMBER => {
            let (_, m) = decode_mini(mini::GUILD_BATTLE_MEMBER, values.get(19).and_then(Option::as_ref).and_then(Value::as_bytes)?).ok()?;
            let n = |k: usize| int(&m, k).unwrap_or(0) as u32;
            let mini_guild = text(&m, 0);
            GuildEvent::BattleFighter { guild: if mini_guild.is_empty() { guild } else { mini_guild }, fighter: BattleFighter { name: text(&m, 1), points: n(2), b: n(3), c: n(4), d: -1 } }
        }
        op::BATTLE_MEMBER_LIST => {
            let (_, l) = decode_mini(mini::GUILD_BATTLE_MEMBER_LIST, values.get(20).and_then(Option::as_ref).and_then(Value::as_bytes)?).ok()?;
            let block = |k: usize| l.get(k).and_then(Option::as_ref).and_then(Value::as_bytes);
            GuildEvent::BattleFighters { guild, mine: fighters(block(1), int(&l, 0).unwrap_or(0)), enemy: fighters(block(3), int(&l, 2).unwrap_or(0)) }
        }
        op::BATTLE_ROUND => {
            let first = match values.get(4).and_then(Option::as_ref) {
                Some(Value::Ints(v)) => v.first().copied(),
                Some(v) => v.as_i64(),
                None => None,
            };
            let round = decode_battle(values).map(|b| b.round).filter(|r| *r != 0).or(first.map(|v| v as u32)).unwrap_or(0);
            GuildEvent::BattleRound { guild, round }
        }
        op::HOSTILE_LEAVE_REJECT => GuildEvent::HostileLeaveRejected { guild },
        o @ (op::JOINT_REQUEST | op::HOSTILE_REQUEST | op::HOSTILE_LEAVE_REQUEST) => {
            let kind = match o {
                op::JOINT_REQUEST => Relation::Joint,
                op::HOSTILE_REQUEST => Relation::Hostile,
                _ => Relation::HostileLeave,
            };
            GuildEvent::RelationAsk { kind, guild, other: other_guild(values), master: text(values, 3) }
        }
        op::JOINT => GuildEvent::Joint { guild, other: other_guild(values), date: int(values, 6).unwrap_or(0), role: int(values, 11).unwrap_or(0) as i32 },
        op::JOINT_LEAVE => GuildEvent::JointLeave { guild, other: other_guild(values), leader: text(values, 3) },
        op::HOSTILE => GuildEvent::Hostile { guild, other: other_guild(values), date: int(values, 6).unwrap_or(0) },
        op::HOSTILE_LEAVE => GuildEvent::HostileRemoved { guild, other: other_guild(values) },
        o @ (op::JOINT_DETAIL | op::HOSTILE_DETAIL) => GuildEvent::RelationDetails { hostile: o == op::HOSTILE_DETAIL, entries: decode_list(values)?.entries },
        op::CLASS_SOCIETY => translate_society(raw)?,
        op::CHAR_DATA => GuildEvent::Linked {
            cid: int(values, 1)? as i32,
            guild,
            member: decode_member(values),
            mark: (int(values, 22).unwrap_or(0) as i32, int(values, 23).unwrap_or(0) as i32),
        },
        op::BUY_GUILDMARK | op::BUY_GUILDMARK_FORCE => GuildEvent::MarkBought { guild, tid: int(values, 22).unwrap_or(0) as i32, color: int(values, 23).unwrap_or(0) as i32 },
        op::UPDATE_NOTICE => GuildEvent::Notice {
            guild,
            text: values.get(10).and_then(Option::as_ref).and_then(Value::as_bytes).map(cp1252).unwrap_or_default(),
        },
        op::SYSTEM_MESSAGE => {
            // Champ 4 : huit entiers ; le code 22 (niveau insuffisant) lit le deuxième.
            let (number, number2) = match values.get(4).and_then(Option::as_ref) {
                Some(Value::Ints(v)) => (v.first().copied().unwrap_or(0), v.get(1).copied().unwrap_or(0)),
                Some(v) => (v.as_i64().unwrap_or(0), 0),
                None => (0, 0),
            };
            GuildEvent::System { code: int(values, 1).unwrap_or(-1) as i32, text: guild, text2: text(values, 3), number, number2 }
        }
        _ => return None,
    })
}

/// Demandes du joueur concernant les guildes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuildCommand {
    /// S'adresse au gestionnaire de guilde (PNJ).
    TalkToManager(i32),
    /// Demande l'entrepôt de guilde à un gardien de banque (AGPMEVENTGUILD op 2).
    WarehouseRequest(i32),
    /// Entrepôt (0x54) : ouverture, fermeture, argent déposé ou retiré, objet déposé ou repris (case ;
    /// `None` : le serveur choisit), agrandissement.
    WarehouseOpen,
    WarehouseClose,
    WarehouseMoney { deposit: bool, amount: i64 },
    WarehouseStore { iid: i32, grid: [i16; 3] },
    WarehouseTake { iid: i32, grid: Option<[i16; 3]> },
    WarehouseExpand,
    Create { name: String, password: String },
    /// Invite un joueur par son nom (chef ou second).
    Invite { guild: String, name: String },
    /// Refuse l'invitation de cette guilde.
    Reject { guild: String },
    /// Rejoint cette guilde (invitation acceptée) ou demande à la rejoindre.
    Join { guild: String },
    /// Le chef accepte la demande d'adhésion de ce membre.
    AllowJoin { guild: String, name: String },
    /// Demande de départ (le joueur se nomme lui-même).
    Leave { guild: String, name: String },
    /// Le chef accepte la demande de départ de ce membre.
    AllowLeave { guild: String, name: String },
    Kick { guild: String, name: String },
    Destroy { guild: String, password: String },
    Notice { guild: String, text: String },
    /// Le chef agrandit la guilde à `max` places (op 7).
    IncreaseMax { guild: String, max: i32 },
    /// Le chef défie la guilde `enemy` (op 15) : type, durée en secondes, effectif (types 3 et 4).
    BattleRequest { guild: String, enemy: String, kind: i32, duration: i64, person: i32 },
    /// Réponse du chef défié (op 16 accepte, 17 refuse).
    BattleAnswer { guild: String, enemy: String, yes: bool },
    /// Demande d'annulation (op 18), réponse (op 19 accord, 20 refus), abandon (op 24).
    BattleCancel { guild: String },
    BattleCancelAnswer { guild: String, yes: bool },
    BattleWithdraw { guild: String },
    /// Le chef propose l'alliance (op 34) ou déclare l'hostilité (op 38) à `target`, ou lui propose la trêve
    /// (op 41) ; `name` : le joueur.
    JointRequest { guild: String, name: String, target: String },
    HostileRequest { guild: String, name: String, target: String },
    HostileLeaveRequest { guild: String, name: String, target: String },
    /// Réponses : alliance (36 accord, 35 refus), hostilité (40, 39), trêve (43, 39 : FUN_1408cada0).
    JointAnswer { guild: String, other: String, yes: bool },
    HostileAnswer { guild: String, other: String, yes: bool },
    HostileLeaveAnswer { guild: String, other: String, yes: bool },
    /// Départ de l'alliance (op 37).
    JointLeave { guild: String },
    /// Achète l'emblème `tid` / `color` (op 32, ou 33 malgré une autre guilde au même emblème).
    BuyMark { guild: String, tid: i32, color: i32, force: bool },
    /// Page `page` de la liste des guildes (op 28).
    ListPage { page: i32 },
    /// Cherche une guilde par son nom (op 28).
    Search { name: String },
    /// Le chef nomme `name` second (`rank` 11) ou lui retire ce rang (`rank` 1) (op 48, sous-op 2).
    Appoint { name: String, rank: i32 },
    /// Réponse à une nomination (op 48, sous-op 4).
    AppointAnswer { guild: String, rank: i32, yes: bool },
    /// Le chef propose à son second de diriger la guilde (op 48, sous-op 5).
    Succession { name: String },
    /// Réponse à une passation (op 48, sous-op 7).
    SuccessionAnswer { guild: String, yes: bool },
    /// Combattants choisis (op 14) : effectif exigé et les noms (les premiers `count` partent).
    BattlePerson { guild: String, count: i16, names: Vec<String> },
}

/// Paquet de l'op 48 de `size` octets (pied exclu) : en-tête, drapeaux 1, op, sous-op, puis ce que `fill`
/// écrit ; le pied 0x6B est ajouté (FUN_1404a66a0).
fn society_packet(sub: u8, size: usize, fill: impl FnOnce(&mut [u8])) -> Vec<u8> {
    let mut out = vec![0u8; size];
    out[0] = 0xd6;
    out[5] = packet_type::GUILD;
    out[0x13..0x17].copy_from_slice(&1u32.to_le_bytes());
    out[0x17] = op::CLASS_SOCIETY as u8;
    out[society::SUB] = sub;
    fill(&mut out);
    out.push(0x6b);
    let len = out.len() as u32;
    out[1..5].copy_from_slice(&len.to_le_bytes());
    out
}

fn put_text(buf: &mut [u8], off: usize, max: usize, s: &str) {
    let b = to_cp1252(s);
    let n = b.len().min(max - 1);
    buf[off..off + n].copy_from_slice(&b[..n]);
}

/// Paquet brut d'une demande de l'op 48 (`me` = nom du joueur), `None` pour les autres demandes.
pub fn raw_command(cmd: &GuildCommand, me: &str) -> Option<Vec<u8>> {
    Some(match cmd {
        GuildCommand::Appoint { name, rank } => society_packet(society::APPOINT, 0x80, |b| {
            put_text(b, 0x19, 49, me);
            put_text(b, 0x4a, 49, name);
            b[0x7c..0x80].copy_from_slice(&(*rank as u32).to_le_bytes());
        }),
        GuildCommand::AppointAnswer { guild, rank, yes } => society_packet(society::APPOINT_ANSWER, 0x84, |b| {
            put_text(b, 0x19, 49, guild);
            put_text(b, 0x4a, 49, me);
            b[0x7c..0x80].copy_from_slice(&(*rank as u32).to_le_bytes());
            b[0x80..0x84].copy_from_slice(&(*yes as u32).to_le_bytes());
        }),
        GuildCommand::Succession { name } => society_packet(society::SUCCESSION, 0x7b, |b| {
            put_text(b, 0x19, 49, me);
            put_text(b, 0x4a, 49, name);
        }),
        GuildCommand::SuccessionAnswer { guild, yes } => society_packet(society::SUCCESSION_ANSWER, 0x80, |b| {
            put_text(b, 0x19, 49, guild);
            put_text(b, 0x4a, 49, me);
            b[0x7c..0x80].copy_from_slice(&(*yes as u32).to_le_bytes());
        }),
        _ => return None,
    })
}

/// Mini EVENT_BASE désignant le gestionnaire de guilde `npc`.
pub fn event_base(npc: i32) -> Value {
    event_base_for(npc, FUNCTION_GUILD)
}

fn event_base_for(npc: i32, function: i32) -> Value {
    let s = mini::EVENT_BASE;
    let values = sparse(s, &[(0, Value::I8(BASE_TYPE_CHARACTER)), (1, Value::I32(npc)), (3, Value::I32(function))]);
    Value::Packet(build_mini(s, &values).unwrap_or_default())
}

/// Mini GUILD_BATTLE réduit à l'autre guilde (champ 17 des ops 34 à 43).
fn other_mini(name: &str) -> Value {
    let b = mini::GUILD_BATTLE;
    Value::Packet(build_mini(b, &sparse(b, &[(0, Value::str(name.as_bytes()))])).unwrap_or_default())
}

/// Paquet à envoyer pour une demande ; `me` = CID du joueur.
pub fn command_packet(cmd: &GuildCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::GUILD;
    let packet = |o: i8, guild: &str, extra: Vec<(usize, Value)>| {
        let mut pairs = vec![(0, Value::I8(o)), (1, Value::I32(me))];
        if !guild.is_empty() {
            pairs.push((2, Value::str(guild.as_bytes())));
        }
        pairs.extend(extra);
        (s, sparse(s, &pairs))
    };
    match cmd {
        GuildCommand::TalkToManager(npc) => {
            let e = main::EVENT_GUILD;
            (e, sparse(e, &[(0, Value::I8(event_op::REQUEST)), (1, event_base(*npc)), (2, Value::I32(me))]))
        }
        GuildCommand::WarehouseRequest(npc) => {
            let e = main::EVENT_GUILD;
            (e, sparse(e, &[(0, Value::I8(event_op::REQUEST_WAREHOUSE)), (1, event_base_for(*npc, FUNCTION_GUILD_WAREHOUSE)), (2, Value::I32(me))]))
        }
        GuildCommand::WarehouseOpen | GuildCommand::WarehouseClose | GuildCommand::WarehouseExpand => {
            let o = match cmd {
                GuildCommand::WarehouseOpen => warehouse::OPEN,
                GuildCommand::WarehouseClose => warehouse::CLOSE,
                _ => warehouse::EXPAND,
            };
            let w = main::GUILD_WAREHOUSE;
            (w, sparse(w, &[(0, Value::I8(o)), (1, Value::I32(me))]))
        }
        GuildCommand::WarehouseMoney { deposit, amount } => {
            let w = main::GUILD_WAREHOUSE;
            (w, sparse(w, &[(0, Value::I8(if *deposit { warehouse::MONEY_IN } else { warehouse::MONEY_OUT })), (1, Value::I32(me)), (2, Value::I64(*amount))]))
        }
        // FUN_1405fc440 : IID (4), page (5), ligne (6), colonne (7).
        GuildCommand::WarehouseStore { iid, grid } => {
            let w = main::GUILD_WAREHOUSE;
            let g = |k: usize| Value::I32(grid[k] as i32);
            (w, sparse(w, &[(0, Value::I8(warehouse::ITEM_IN)), (1, Value::I32(me)), (4, Value::I32(*iid)), (5, g(0)), (6, g(1)), (7, g(2))]))
        }
        // FUN_1405fd4b0 : statut visé (3 : inventaire, 2), IID, case.
        GuildCommand::WarehouseTake { iid, grid } => {
            let w = main::GUILD_WAREHOUSE;
            let g = |k: usize| Value::I32(grid.map_or(-1, |x| x[k] as i32));
            (w, sparse(w, &[(0, Value::I8(warehouse::ITEM_OUT)), (1, Value::I32(me)), (3, Value::I32(2)), (4, Value::I32(*iid)), (5, g(0)), (6, g(1)), (7, g(2))]))
        }
        GuildCommand::Create { name, password } => packet(op::CREATE, name, vec![(9, Value::str(password.as_bytes()))]),
        GuildCommand::Invite { guild, name } => packet(op::JOIN_REQUEST, guild, vec![(16, member_named(name))]),
        GuildCommand::Reject { guild } => packet(op::JOIN_REJECT, guild, vec![]),
        GuildCommand::Join { guild } => packet(op::JOIN, guild, vec![]),
        GuildCommand::AllowJoin { guild, name } => packet(op::JOIN, guild, vec![(16, member_named(name))]),
        GuildCommand::Leave { guild, name } => packet(op::LEAVE, guild, vec![(16, member_named(name))]),
        GuildCommand::AllowLeave { guild, name } => packet(op::LEAVE_ALLOW, guild, vec![(16, member_named(name))]),
        GuildCommand::Kick { guild, name } => packet(op::FORCED_LEAVE, guild, vec![(16, member_named(name))]),
        GuildCommand::Destroy { guild, password } => packet(op::DESTROY, guild, vec![(9, Value::str(password.as_bytes()))]),
        GuildCommand::Notice { guild, text } => packet(op::UPDATE_NOTICE, guild, vec![(10, Value::Block(to_cp1252(text)))]),
        GuildCommand::IncreaseMax { guild, max } => packet(op::UPDATE_MAX_MEMBER_COUNT, guild, vec![(7, Value::I32(*max))]),
        GuildCommand::ListPage { page } => {
            let l = mini::GUILD_LIST;
            let list = Value::Packet(build_mini(l, &sparse(l, &[(1, Value::I32(*page))])).unwrap_or_default());
            packet(op::GUILD_LIST, "", vec![(21, list)])
        }
        GuildCommand::Search { name } => packet(op::GUILD_LIST, name, vec![]),
        GuildCommand::BattleRequest { guild, enemy, kind, duration, person } => {
            let b = mini::GUILD_BATTLE;
            let v = sparse(b, &[(0, Value::str(enemy.as_bytes())), (5, Value::U32(*kind as u32)), (6, Value::U32(*duration as u32)), (7, Value::U32(*person as u32))]);
            packet(op::BATTLE_REQUEST, guild, vec![(17, Value::Packet(build_mini(b, &v).unwrap_or_default()))])
        }
        GuildCommand::BattleAnswer { guild, enemy, yes } => {
            let b = mini::GUILD_BATTLE;
            let v = sparse(b, &[(0, Value::str(enemy.as_bytes()))]);
            packet(if *yes { op::BATTLE_ACCEPT } else { op::BATTLE_REJECT }, guild, vec![(17, Value::Packet(build_mini(b, &v).unwrap_or_default()))])
        }
        GuildCommand::BattlePerson { guild, count, names } => {
            let p = mini::GUILD_BATTLE_PERSON;
            let take: Vec<String> = names.iter().take((*count).max(0) as usize).cloned().collect();
            let v = sparse(p, &[(0, Value::I16(*count)), (1, Value::Block(encode_person_block(&take)))]);
            packet(op::BATTLE_PERSON, guild, vec![(18, Value::Packet(build_mini(p, &v).unwrap_or_default()))])
        }
        GuildCommand::BattleCancel { guild } => packet(op::BATTLE_CANCEL_REQUEST, guild, vec![]),
        GuildCommand::BattleCancelAnswer { guild, yes } => packet(if *yes { op::BATTLE_CANCEL_ACCEPT } else { op::BATTLE_CANCEL_REJECT }, guild, vec![]),
        GuildCommand::BattleWithdraw { guild } => packet(op::BATTLE_WITHDRAW, guild, vec![]),
        GuildCommand::JointRequest { guild, name, target } => packet(op::JOINT_REQUEST, guild, vec![(3, Value::str(name.as_bytes())), (17, other_mini(target))]),
        GuildCommand::HostileRequest { guild, name, target } => packet(op::HOSTILE_REQUEST, guild, vec![(3, Value::str(name.as_bytes())), (17, other_mini(target))]),
        GuildCommand::HostileLeaveRequest { guild, name, target } => packet(op::HOSTILE_LEAVE_REQUEST, guild, vec![(3, Value::str(name.as_bytes())), (17, other_mini(target))]),
        GuildCommand::JointAnswer { guild, other, yes } => packet(if *yes { op::JOINT } else { op::JOINT_REJECT }, guild, vec![(17, other_mini(other))]),
        GuildCommand::HostileAnswer { guild, other, yes } => packet(if *yes { op::HOSTILE } else { op::HOSTILE_REJECT }, guild, vec![(17, other_mini(other))]),
        GuildCommand::HostileLeaveAnswer { guild, other, yes } => {
            if *yes {
                packet(op::HOSTILE_LEAVE, guild, vec![(3, Value::str(other.as_bytes())), (17, other_mini(other))])
            } else {
                packet(op::HOSTILE_REJECT, guild, vec![(17, other_mini(other))])
            }
        }
        GuildCommand::JointLeave { guild } => packet(op::JOINT_LEAVE, guild, vec![(17, other_mini(guild))]),
        GuildCommand::BuyMark { guild, tid, color, force } => {
            packet(if *force { op::BUY_GUILDMARK_FORCE } else { op::BUY_GUILDMARK }, guild, vec![(22, Value::I32(*tid)), (23, Value::I32(*color))])
        }
        // Op 48 : voir `raw_command` ; un paquet sans suite si on l'appelle quand même.
        GuildCommand::Appoint { .. } | GuildCommand::AppointAnswer { .. } | GuildCommand::Succession { .. } | GuildCommand::SuccessionAnswer { .. } => {
            packet(op::CLASS_SOCIETY, "", vec![])
        }
    }
}

/// Invitation reçue, en attente de réponse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuildInvitation {
    pub guild: String,
    pub from: String,
}

/// Ce qui a changé, pour l'interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuildNotice {
    /// Le joueur a (de nouveau) une guilde.
    Joined,
    /// Le joueur n'a plus de guilde (départ, exclusion, dissolution).
    Left,
    Roster,
    NoticeChanged,
    Invited(GuildInvitation),
    /// Un membre se connecte (`true`) ou se déconnecte.
    MemberOnline(String, bool),
    /// Nouvelle page de la liste des guildes.
    List,
    /// Statut, bilan ou combat de guildes changé.
    Battle,
    /// Alliées ou hostiles changées.
    Relations,
}

/// État de la guilde du joueur et des liens connus des autres personnages.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuildState {
    pub info: Option<GuildInfo>,
    /// Membres dans l'ordre d'arrivée.
    pub members: Vec<GuildMember>,
    pub invitation: Option<GuildInvitation>,
    /// Guilde de chaque personnage connu (CID → nom de la guilde).
    pub links: HashMap<i32, String>,
    /// Dernière page reçue de la liste des guildes.
    pub list: Option<GuildList>,
    /// Emblème connu de chaque guilde (nom en minuscules → TID, couleur).
    pub marks: HashMap<String, (i32, i32)>,
    /// Statut de la guilde du joueur (op 11), bilan (op 12 : victoires, nuls, défaites, points), combat en cours.
    pub status: i32,
    pub record: (i32, i32, i32, i32),
    pub battle: Option<GuildBattle>,
    /// Alliées (nom, date, rôle) et hostiles (nom, date) ; leurs détails (ops 44 et 45).
    pub joints: Vec<(String, i64, i32)>,
    pub hostiles: Vec<(String, i64)>,
    pub joint_details: Vec<GuildListEntry>,
    pub hostile_details: Vec<GuildListEntry>,
}

impl GuildState {
    pub fn in_guild(&self) -> bool {
        self.info.is_some()
    }

    pub fn name(&self) -> &str {
        self.info.as_ref().map_or("", |i| i.name.as_str())
    }

    pub fn member(&self, name: &str) -> Option<&GuildMember> {
        self.members.iter().find(|m| m.name.eq_ignore_ascii_case(name.trim()))
    }

    /// Personnage de la guilde adverse, pendant un combat de guildes (statut 3).
    pub fn is_enemy(&self, cid: i32) -> bool {
        self.status == battle::status::BATTLE && self.battle.as_ref().is_some_and(|b| self.links.get(&cid).is_some_and(|g| g.eq_ignore_ascii_case(&b.enemy)))
    }

    /// Membres à part entière en ligne, et en tout.
    pub fn counts(&self) -> (usize, usize) {
        let active = self.members.iter().filter(|m| m.active());
        (active.clone().filter(|m| m.online).count(), active.count())
    }

    fn clear_guild(&mut self) -> bool {
        let was = self.in_guild();
        self.info = None;
        self.members.clear();
        self.status = 0;
        self.record = (0, 0, 0, 0);
        self.battle = None;
        self.joints.clear();
        self.hostiles.clear();
        self.joint_details.clear();
        self.hostile_details.clear();
        was
    }

    /// Applique un paquet reçu ; `me` = nom du joueur, `my_cid` = son CID.
    pub fn apply(&mut self, ev: &GuildEvent, me: &str, my_cid: i32) -> Option<GuildNotice> {
        let mine = |g: &str, s: &Self| s.info.as_ref().is_some_and(|i| i.name.eq_ignore_ascii_case(g));
        match ev {
            GuildEvent::Guild(info) => {
                self.marks.insert(info.name.to_lowercase(), (info.mark_tid, info.mark_color));
                let fresh = !mine(&info.name, self);
                if fresh {
                    self.members.clear();
                }
                self.info = Some(info.clone());
                self.invitation = None;
                self.links.insert(my_cid, info.name.clone());
                fresh.then_some(GuildNotice::Joined)
            }
            GuildEvent::Invited { guild, from } => {
                let i = GuildInvitation { guild: guild.clone(), from: from.clone() };
                self.invitation = Some(i.clone());
                Some(GuildNotice::Invited(i))
            }
            GuildEvent::Member { guild, member } => {
                if !mine(guild, self) {
                    return None;
                }
                match self.members.iter_mut().find(|m| m.name.eq_ignore_ascii_case(&member.name)) {
                    Some(m) => *m = member.clone(),
                    None => self.members.push(member.clone()),
                }
                Some(GuildNotice::Roster)
            }
            GuildEvent::Left { guild, name, .. } => {
                if !mine(guild, self) {
                    return None;
                }
                if name.eq_ignore_ascii_case(me) {
                    self.links.remove(&my_cid);
                    return self.clear_guild().then_some(GuildNotice::Left);
                }
                self.members.retain(|m| !m.name.eq_ignore_ascii_case(name));
                Some(GuildNotice::Roster)
            }
            GuildEvent::Destroyed { guild } => {
                if !mine(guild, self) {
                    return None;
                }
                self.links.retain(|_, g| !g.eq_ignore_ascii_case(guild));
                self.clear_guild().then_some(GuildNotice::Left)
            }
            GuildEvent::Linked { cid, guild, member, mark } => {
                self.links.insert(*cid, guild.clone());
                if !guild.is_empty() {
                    self.marks.insert(guild.to_lowercase(), *mark);
                }
                let m = member.as_ref().filter(|_| mine(guild, self))?;
                let was = self.member(&m.name).map(|x| x.online);
                match self.members.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&m.name)) {
                    Some(x) => *x = m.clone(),
                    None => self.members.push(m.clone()),
                }
                match was {
                    Some(o) if o != m.online && !m.name.eq_ignore_ascii_case(me) => Some(GuildNotice::MemberOnline(m.name.clone(), m.online)),
                    _ => Some(GuildNotice::Roster),
                }
            }
            GuildEvent::Notice { guild, text } => {
                let info = self.info.as_mut().filter(|i| i.name.eq_ignore_ascii_case(guild))?;
                info.notice = text.clone();
                Some(GuildNotice::NoticeChanged)
            }
            GuildEvent::Joint { guild, other, date, role } => {
                if !mine(guild, self) {
                    return None;
                }
                self.joints.retain(|(n, ..)| !n.eq_ignore_ascii_case(other));
                self.joints.push((other.clone(), *date, *role));
                Some(GuildNotice::Relations)
            }
            // FUN_1408cc1c0 : sa propre guilde partie, plus d'alliance ; sinon l'alliée partie est retirée et la
            // nouvelle fondatrice prend le rôle 2.
            GuildEvent::JointLeave { guild, other, leader } => {
                if !mine(guild, self) {
                    return None;
                }
                if other.eq_ignore_ascii_case(guild) {
                    self.joints.clear();
                    self.joint_details.clear();
                } else {
                    self.joints.retain(|(n, ..)| !n.eq_ignore_ascii_case(other));
                    self.joint_details.retain(|e| !e.name.eq_ignore_ascii_case(other));
                    if let Some(j) = self.joints.iter_mut().find(|(n, ..)| !leader.is_empty() && n.eq_ignore_ascii_case(leader)) {
                        j.2 = 2;
                    }
                }
                Some(GuildNotice::Relations)
            }
            GuildEvent::Hostile { guild, other, date } => {
                if !mine(guild, self) {
                    return None;
                }
                self.hostiles.retain(|(n, _)| !n.eq_ignore_ascii_case(other));
                self.hostiles.push((other.clone(), *date));
                Some(GuildNotice::Relations)
            }
            GuildEvent::HostileRemoved { guild, other } => {
                if !mine(guild, self) {
                    return None;
                }
                self.hostiles.retain(|(n, _)| !n.eq_ignore_ascii_case(other));
                self.hostile_details.retain(|e| !e.name.eq_ignore_ascii_case(other));
                Some(GuildNotice::Relations)
            }
            // FUN_1408cbd20, FUN_1408cb400 : entrées gardées pour les alliées et hostiles connues, remplacées par nom.
            GuildEvent::RelationDetails { hostile, entries } => {
                for e in entries {
                    let known = if *hostile { self.hostiles.iter().any(|(n, _)| n.eq_ignore_ascii_case(&e.name)) } else { self.joints.iter().any(|(n, ..)| n.eq_ignore_ascii_case(&e.name)) };
                    if !known {
                        continue;
                    }
                    let list = if *hostile { &mut self.hostile_details } else { &mut self.joint_details };
                    match list.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&e.name)) {
                        Some(x) => *x = e.clone(),
                        None => list.push(e.clone()),
                    }
                }
                Some(GuildNotice::Relations)
            }
            GuildEvent::MaxMembers { guild, max } => {
                let info = self.info.as_mut().filter(|i| i.name.eq_ignore_ascii_case(guild))?;
                info.max_members = *max;
                Some(GuildNotice::Roster)
            }
            GuildEvent::Status { guild, status } => {
                if !mine(guild, self) {
                    return None;
                }
                self.status = *status;
                if *status == battle::status::NONE {
                    self.battle = None;
                }
                Some(GuildNotice::Battle)
            }
            GuildEvent::Record { guild, win, draw, lose, points } => {
                if !mine(guild, self) {
                    return None;
                }
                self.record = (*win, *draw, *lose, *points);
                Some(GuildNotice::Battle)
            }
            GuildEvent::Battle { op: o, guild, battle: b, .. } => {
                if !mine(guild, self) {
                    return None;
                }
                match *o {
                    op::BATTLE_INFO | op::BATTLE_ACCEPT | op::BATTLE_START => {
                        self.battle = Some(b.clone());
                        if *o == op::BATTLE_START {
                            self.status = battle::status::BATTLE;
                        } else if *o == op::BATTLE_ACCEPT {
                            self.status = battle::status::READY;
                        }
                    }
                    op::BATTLE_UPDATE_TIME => {
                        if let Some(x) = self.battle.as_mut() {
                            x.clock = b.clock;
                        }
                    }
                    op::BATTLE_UPDATE_SCORE => {
                        if let Some(x) = self.battle.as_mut() {
                            (x.score, x.enemy_score, x.up_score, x.enemy_up_score) = (b.score, b.enemy_score, b.up_score, b.enemy_up_score);
                        }
                    }
                    op::BATTLE_CANCEL_ACCEPT | op::BATTLE_RESULT => {
                        self.battle = None;
                        self.status = battle::status::NONE;
                    }
                    _ => {}
                }
                Some(GuildNotice::Battle)
            }
            GuildEvent::MarkBought { guild, tid, color } => {
                self.marks.insert(guild.to_lowercase(), (*tid, *color));
                let info = self.info.as_mut().filter(|i| i.name.eq_ignore_ascii_case(guild))?;
                info.mark_tid = *tid;
                info.mark_color = *color;
                Some(GuildNotice::Roster)
            }
            GuildEvent::List(list) => {
                for e in &list.entries {
                    self.marks.insert(e.name.to_lowercase(), (e.mark_tid, e.mark_color));
                }
                self.list = Some(list.clone());
                Some(GuildNotice::List)
            }
            // FUN_14031afe0 : rang du membre ; 10 en fait le chef, 11 le second.
            GuildEvent::RankChanged { guild, member, rank } => {
                if !mine(guild, self) {
                    return None;
                }
                if let Some(m) = self.members.iter_mut().find(|m| m.name.eq_ignore_ascii_case(member)) {
                    m.rank = *rank;
                }
                if *rank == rank::MASTER {
                    if let Some(i) = self.info.as_mut() {
                        i.master = member.clone();
                    }
                }
                Some(GuildNotice::Roster)
            }
            // Manche : gardée avec le combat (guilde+0x234, FUN_14084f580).
            GuildEvent::BattleRound { guild, round } => {
                if !mine(guild, self) {
                    return None;
                }
                self.battle.as_mut()?.round = *round;
                Some(GuildNotice::Battle)
            }
            GuildEvent::System { .. } | GuildEvent::RelationAsk { .. } | GuildEvent::ManagerGranted { .. } | GuildEvent::WarehouseGranted { .. } | GuildEvent::Warehouse { .. } | GuildEvent::AppointAsk { .. } | GuildEvent::SuccessionAsk { .. } | GuildEvent::SocietyText(_) => None,
            GuildEvent::BattleFighter { .. } | GuildEvent::BattleFighters { .. } | GuildEvent::HostileLeaveRejected { .. } => None,
        }
    }

    /// Un personnage quitte la vue : son lien est oublié (sauf le joueur).
    pub fn forget(&mut self, cid: i32, my_cid: i32) {
        if cid != my_cid {
            self.links.remove(&cid);
        }
    }

    /// Déconnexion : plus rien.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(name: &str, r: i32, online: bool) -> Value {
        let s = mini::GUILD_MEMBER;
        let v = sparse(s, &[(0, Value::str(name)), (1, Value::I32(r)), (3, Value::U64(20)), (4, Value::I32(96)), (5, Value::I8(online as i8))]);
        Value::Packet(build_mini(s, &v).unwrap())
    }

    fn guild(pairs: &[(usize, Value)]) -> GuildEvent {
        translate(0x15, &sparse(main::GUILD, pairs), &[]).unwrap()
    }

    #[test]
    fn combattants_et_manches() {
        // Op 14 : effectif 2, les deux premiers noms seulement, (U16 longueur, nom).
        let cmd = GuildCommand::BattlePerson { guild: "Lames".into(), count: 2, names: vec!["Alice".into(), "Bo".into(), "Zed".into()] };
        let (s, v) = command_packet(&cmd, 7);
        let raw = archrust_protocol::packet::build_packet(s, &v).unwrap();
        let p = archrust_protocol::packet::decode_packet(main::GUILD, &raw).unwrap();
        assert_eq!((int(&p.values, 0), int(&p.values, 1), text(&p.values, 2).as_str()), (Some(14), Some(7), "Lames"));
        let (_, m) = decode_mini(mini::GUILD_BATTLE_PERSON, p.values[18].as_ref().and_then(Value::as_bytes).unwrap()).unwrap();
        let block = m[1].as_ref().and_then(Value::as_bytes).unwrap();
        assert_eq!(int(&m, 0), Some(2));
        assert_eq!(block, b"\x05\x00Alice\x02\x00Bo");
        assert_eq!(decode_person_block(block), vec!["Alice".to_string(), "Bo".to_string()]);
        // Op 26 : une ligne, guilde du mini.
        let bm = mini::GUILD_BATTLE_MEMBER;
        let one = Value::Packet(build_mini(bm, &sparse(bm, &[(0, Value::str("Corbeaux")), (1, Value::str("Zed")), (2, Value::U32(30)), (3, Value::U32(2)), (4, Value::U32(1))])).unwrap());
        assert_eq!(
            guild(&[(0, Value::I8(26)), (2, Value::str("Lames")), (19, one)]),
            GuildEvent::BattleFighter { guild: "Corbeaux".into(), fighter: BattleFighter { name: "Zed".into(), points: 30, b: 2, c: 1, d: -1 } }
        );
        // Op 27 : deux tableaux de 0x48 octets.
        let a = BattleFighter { name: "Alice".into(), points: 10, b: 1, c: 0, d: -1 };
        let z = BattleFighter { name: "Zed".into(), points: 30, b: 2, c: 1, d: 4 };
        let bl = mini::GUILD_BATTLE_MEMBER_LIST;
        let list = Value::Packet(build_mini(bl, &[Some(Value::U32(1)), Some(Value::Block(a.encode().to_vec())), Some(Value::U32(1)), Some(Value::Block(z.encode().to_vec()))]).unwrap());
        assert_eq!(guild(&[(0, Value::I8(27)), (2, Value::str("Lames")), (20, list)]), GuildEvent::BattleFighters { guild: "Lames".into(), mine: vec![a], enemy: vec![z] });
        // Op 46 : manche du mini GUILD_BATTLE, sinon champ 4 ; gardée avec le combat.
        let b = mini::GUILD_BATTLE;
        let round = Value::Packet(build_mini(b, &sparse(b, &[(14, Value::U32(3))])).unwrap());
        let ev = guild(&[(0, Value::I8(46)), (2, Value::str("Lames")), (17, round)]);
        assert_eq!(ev, GuildEvent::BattleRound { guild: "Lames".into(), round: 3 });
        let mut s = GuildState { info: Some(GuildInfo { name: "Lames".into(), ..Default::default() }), battle: Some(GuildBattle::default()), ..Default::default() };
        assert_eq!(s.apply(&ev, "Alice", 7), Some(GuildNotice::Battle));
        assert_eq!(s.battle.as_ref().map(|b| b.round), Some(3));
        assert_eq!(guild(&[(0, Value::I8(42)), (2, Value::str("Lames"))]), GuildEvent::HostileLeaveRejected { guild: "Lames".into() });
    }

    #[test]
    fn entree_en_jeu_et_membres() {
        let mut s = GuildState::default();
        let g = guild(&[(0, Value::I8(0)), (2, Value::str("SimGuild")), (3, Value::str("GmTest")), (7, Value::I32(50)), (10, Value::Block(b"Bienvenue \xe0 tous".to_vec()))]);
        assert_eq!(s.apply(&g, "GmTest", 7), Some(GuildNotice::Joined));
        assert_eq!(s.info.as_ref().unwrap().notice, "Bienvenue à tous");
        let m = guild(&[(0, Value::I8(3)), (2, Value::str("SimGuild")), (16, member("GmTest", rank::MASTER, true))]);
        assert_eq!(s.apply(&m, "GmTest", 7), Some(GuildNotice::Roster));
        let m = guild(&[(0, Value::I8(3)), (2, Value::str("SimGuild")), (16, member("SimA", rank::NORMAL, false))]);
        s.apply(&m, "GmTest", 7);
        assert_eq!(s.counts(), (1, 2));
        // SimA se connecte : lien op 8 avec le membre en ligne.
        let l = guild(&[(0, Value::I8(8)), (1, Value::I32(9)), (2, Value::str("SimGuild")), (16, member("SimA", rank::NORMAL, true))]);
        assert_eq!(s.apply(&l, "GmTest", 7), Some(GuildNotice::MemberOnline("SimA".into(), true)));
        assert_eq!(s.links.get(&9).map(String::as_str), Some("SimGuild"));
        // Exclusion de SimA, puis dissolution.
        let k = guild(&[(0, Value::I8(5)), (2, Value::str("SimGuild")), (16, member("SimA", 0, false))]);
        assert_eq!(s.apply(&k, "GmTest", 7), Some(GuildNotice::Roster));
        assert_eq!(s.members.len(), 1);
        let d = guild(&[(0, Value::I8(6)), (2, Value::str("SimGuild"))]);
        assert_eq!(s.apply(&d, "GmTest", 7), Some(GuildNotice::Left));
        assert!(!s.in_guild() && s.links.is_empty());
    }

    #[test]
    fn invitation_et_message_systeme() {
        let mut s = GuildState::default();
        let i = guild(&[(0, Value::I8(1)), (2, Value::str("SimGuild")), (3, Value::str("GmTest")), (16, member("SimA", 0, false))]);
        assert_eq!(s.apply(&i, "SimA", 9), Some(GuildNotice::Invited(GuildInvitation { guild: "SimGuild".into(), from: "GmTest".into() })));
        let sys = guild(&[(0, Value::I8(10)), (1, Value::I32(2)), (4, Value::Ints(vec![10_000, 0, 0, 0, 0, 0, 0, 0]))]);
        assert_eq!(sys, GuildEvent::System { code: 2, text: String::new(), text2: String::new(), number: 10_000, number2: 0 });
    }

    #[test]
    fn paquets_envoyes() {
        let (s, v) = command_packet(&GuildCommand::Invite { guild: "SimGuild".into(), name: "SimA".into() }, 7);
        assert_eq!(s.id, 0x15);
        assert_eq!((int(&v, 0), int(&v, 1), text(&v, 2)), (Some(1), Some(7), "SimGuild".to_string()));
        let b = v[16].as_ref().and_then(Value::as_bytes).unwrap();
        assert_eq!(text(&decode_mini(mini::GUILD_MEMBER, b).unwrap().1, 0), "SimA");
        let (_, v) = command_packet(&GuildCommand::Notice { guild: "G".into(), text: "Rendez-vous à Anchorville".into() }, 7);
        assert_eq!(v[10].as_ref().and_then(Value::as_bytes).map(cp1252).as_deref(), Some("Rendez-vous à Anchorville"));
        let (s, v) = command_packet(&GuildCommand::TalkToManager(0x4000_0010), 7);
        assert_eq!((s.id, int(&v, 0), int(&v, 2)), (0x39, Some(0), Some(7)));
        let ok = sparse(main::EVENT_GUILD, &[(0, Value::I8(1)), (1, event_base(0x4000_0010)), (2, Value::I32(7))]);
        assert_eq!(translate(0x39, &ok, &[]), Some(GuildEvent::ManagerGranted { npc: 0x4000_0010 }));
    }

    #[test]
    fn second_passation_et_liste() {
        // Demande de nomination envoyée : 0x80 octets + pied, sous-op 2, noms et rang aux bons décalages.
        let p = raw_command(&GuildCommand::Appoint { name: "SimA".into(), rank: rank::SUBMASTER }, "GmTest").unwrap();
        assert_eq!((p.len(), p[0x17], p[society::SUB], *p.last().unwrap()), (0x81, 48, society::APPOINT, 0x6b));
        assert_eq!((raw_text(&p, 0x19, 49), raw_text(&p, 0x4a, 49), raw_u32(&p, 0x7c)), ("GmTest".into(), "SimA".into(), 11));
        assert!(raw_command(&GuildCommand::ListPage { page: 0 }, "GmTest").is_none());
        // Rang reçu (sous-op 8) : SimA chef.
        let mut s = GuildState::default();
        s.apply(&guild(&[(0, Value::I8(0)), (2, Value::str("SimLead")), (3, Value::str("GmTest")), (7, Value::I32(50))]), "SimA", 9);
        s.apply(&guild(&[(0, Value::I8(3)), (2, Value::str("SimLead")), (16, member("SimA", rank::SUBMASTER, true))]), "SimA", 9);
        let mut up = society_packet(society::RANK_UPDATE, 0x70, |b| {
            put_text(b, 0x19, 33, "SimLead");
            put_text(b, 0x3a, 49, "SimA");
            b[0x6c..0x70].copy_from_slice(&10u32.to_le_bytes());
        });
        let ev = translate(0x15, &sparse(main::GUILD, &[(0, Value::I8(48))]), &up).unwrap();
        assert_eq!(ev, GuildEvent::RankChanged { guild: "SimLead".into(), member: "SimA".into(), rank: 10 });
        s.apply(&ev, "SimA", 9);
        assert_eq!((s.info.as_ref().unwrap().master.as_str(), s.member("SimA").unwrap().rank), ("SimA", 10));
        up[society::SUB] = 99;
        assert!(translate(0x15, &sparse(main::GUILD, &[(0, Value::I8(48))]), &up).is_none());
        // Liste : une entrée.
        let e = mini::GUILD_LIST_ITEM;
        let item = build_mini(e, &sparse(e, &[(0, Value::str("SimLead")), (1, Value::str("GmTest")), (5, Value::I32(20)), (6, Value::I32(50))])).unwrap();
        let l = mini::GUILD_LIST;
        let list = build_mini(l, &sparse(l, &[(0, Value::I32(1)), (1, Value::I32(0)), (2, Value::I16(1)), (3, Value::Packet(item))])).unwrap();
        let ev = guild(&[(0, Value::I8(28)), (1, Value::I32(7)), (21, Value::Packet(list))]);
        let GuildEvent::List(l) = ev else { panic!("{ev:?}") };
        assert_eq!((l.total, l.entries.len(), l.entries[0].members, l.entries[0].max_members, l.entries[0].master.as_str()), (1, 1, 20, 50, "GmTest"));
    }
}
