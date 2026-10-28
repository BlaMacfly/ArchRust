//! Guerres de siège des châteaux (AgpmSiegeWar 0x49, AgpmEventSiegeWarNPC 0x50) et tableau d'information des
//! guerres (AgpmWarInfo 0x61).
//!
//! * **0x49** (drapeaux 2, `FUN_14020fe90`, réception `FUN_140213fc0`) : op, résultat, château (mini `CASTLE`), CID,
//!   page de guildes (`GUILD_PAGE` de 15 `GUILD_ENTRY`), CID de l'objet de siège, fiche de l'objet
//!   (`OBJECT_INFO`), IID, message (`STATUS_MSG`), u64 jamais lu, compte à rebours (`TIME`). Ops du client : 1
//!   déclaration (date FILETIME **locale**), 3/4 candidature/retrait de défense, 5/6 d'attaque, 7/8 pages des
//!   candidats, 0xB défenseur retenu, 0xC/0xD listes, 0x10 ouverture d'un objet 9/10, 0x12 activation, 0x14
//!   réparation, 0x16/0x17 chargement/retrait d'un objet, 0x1B carving du trône. Ops du serveur : 0 château, 2
//!   résultat (21 codes, [`result`]), 9/10 pages, 0xE/0xF guildes retenues, 0x11/0x13/0x15 réponses d'objet,
//!   0x18 objet chargé, 0x1A fiche, 0x1C message, 0x1D compte à rebours.
//! * **0x50** (drapeaux 1, `FUN_140284d60`) : op 1 demande au « Siege Commander » (EVENT_BASE de la fonction 33),
//!   op 2 accord (ouvre `UI_SiegeWarMain` pour le château du PNJ). Pas de refus.
//! * **0x61** : schéma déclaré de 4 × INT8, mais les rappels lisent le paquet **brut** : 0x14 op, 0x15 type de
//!   guerre, 0x16 étape, 0x17 paramètre ; op 4/5 deux INT32 en 0x18 et 0x1C ; op 7 trois chaînes de 0x21 octets
//!   en 0x16, 0x37 et 0x58.

use archrust_protocol::packet::{build_mini, decode_mini, sparse, Value};
use archrust_protocol::schema::{main, mini, packet_type, Schema};

/// Fonction de PNJ du « Siege Commander » (`SiegeWarNPC`).
pub const FUNCTION: i32 = 33;
/// Lignes d'une page de guildes.
pub const PAGE_ROWS: usize = 15;

pub mod op {
    pub const CASTLE: i8 = 0;
    pub const DECLARE: i8 = 1;
    pub const RESULT: i8 = 2;
    pub const APPLY_DEFENSE: i8 = 3;
    pub const CANCEL_DEFENSE: i8 = 4;
    pub const APPLY_ATTACK: i8 = 5;
    pub const CANCEL_ATTACK: i8 = 6;
    pub const ATTACK_PAGE: i8 = 7;
    pub const DEFENSE_PAGE: i8 = 8;
    pub const ATTACK_LIST: i8 = 9;
    pub const DEFENSE_LIST: i8 = 10;
    pub const SELECT_DEFENSE: i8 = 0x0B;
    pub const REQUEST_ATTACKERS: i8 = 0x0C;
    pub const REQUEST_DEFENDERS: i8 = 0x0D;
    pub const ATTACKERS: i8 = 0x0E;
    pub const DEFENDERS: i8 = 0x0F;
    pub const OPEN_OBJECT: i8 = 0x10;
    pub const OBJECT_REPLY_1: i8 = 0x11;
    pub const ACTIVATE: i8 = 0x12;
    pub const OBJECT_REPLY: i8 = 0x13;
    pub const REPAIR: i8 = 0x14;
    pub const OBJECT_REPLY_2: i8 = 0x15;
    pub const LOAD_ITEM: i8 = 0x16;
    pub const UNLOAD_ITEM: i8 = 0x17;
    pub const ITEM_LOADED: i8 = 0x18;
    pub const OBJECT_INFO: i8 = 0x1A;
    pub const CARVE: i8 = 0x1B;
    pub const STATUS_MSG: i8 = 0x1C;
    pub const TIME: i8 = 0x1D;
}

/// Codes de résultat (op 2, `FUN_14062a0b0`).
pub mod result {
    pub const OK: i8 = 0;
    pub const FAILED: i8 = 1;
    pub const OUT_OF_TIME: i8 = 2;
    pub const REQUIRE_CASTLE_OWNER: i8 = 3;
    pub const REQUIRE_GUILD_MASTER: i8 = 4;
    pub const REQUIRE_GUILD_CONDITION: i8 = 5;
    pub const ALREADY_APPLIED: i8 = 6;
    pub const UNUSED_7: i8 = 7;
    pub const DEFENSE_APPLIED: i8 = 8;
    pub const ATTACK_APPLIED: i8 = 9;
    pub const DEFENDER_SELECTED: i8 = 10;
    pub const NEXT_TIME_SET: i8 = 0x0B;
    pub const ALREADY_USED: i8 = 0x0C;
    pub const ALREADY_REPAIRING: i8 = 0x0D;
    /// Réponse d'objet (op 0x13) : ouvrir la fenêtre de l'objet.
    pub const OPEN_OBJECT_UI: i8 = 0x0E;
    pub const NOT_ENOUGH_MONEY: i8 = 0x0F;
    pub const CARVE_START: i8 = 0x10;
    pub const CARVE_CANCEL: i8 = 0x11;
    pub const CARVE_END: i8 = 0x12;
    pub const ALREADY_ACTIVE: i8 = 0x13;
    pub const NO_MORE_REPAIR: i8 = 0x14;
    pub const LIMIT_REACHED: i8 = 0x15;

    /// Message d'un code : événement de l'`[EventMap]` (boîte d'information) ou clé d'`uimessage.txt` (centre et
    /// discussion) ; `None` : rien d'affiché (0, 1, 2, 7, 0xE).
    pub fn message(code: i8) -> Option<(bool, &'static str)> {
        Some(match code {
            REQUIRE_CASTLE_OWNER => (true, "SiegeWarRequireCastleMaster"),
            REQUIRE_GUILD_MASTER => (true, "SiegeWarRequireGuildMaster"),
            REQUIRE_GUILD_CONDITION => (true, "SiegeWarRequireGuildCondition"),
            ALREADY_APPLIED => (true, "SiegeWarAlreadyApplGuild"),
            DEFENSE_APPLIED => (true, "SiegeWarDefenseApplSuccess"),
            ATTACK_APPLIED => (true, "SiegeWarAttackApplSuccess"),
            DEFENDER_SELECTED => (true, "SiegeWarDefenseGuildSuccess"),
            NEXT_TIME_SET => (true, "SiegeWarNextSiegeWarTimeSuccess"),
            ALREADY_USED => (false, "SiegeWarMsgAlreadyUse"),
            ALREADY_REPAIRING => (false, "SiegeWarMsgAlreadyRepair"),
            NOT_ENOUGH_MONEY => (true, "SiegeWarNotEnoughMoney"),
            CARVE_START => (false, "SiegeWarStartCarving"),
            CARVE_CANCEL => (false, "SiegeWarCancelCarving"),
            CARVE_END => (false, "SiegeWarEndCarving"),
            ALREADY_ACTIVE => (false, "SiegeWarAlreadyActive"),
            NO_MORE_REPAIR => (true, "SiegeWarNoMoreRepair"),
            LIMIT_REACHED => (false, "SiegeWarStatusReachedLimitation"),
            _ => return None,
        })
    }
}

/// Statuts d'un château (`château+0x17c`).
pub mod status {
    pub const NONE: u8 = 0;
    pub const DECLARED: u8 = 1;
    pub const APPLICATIONS_CLOSED: u8 = 2;
    pub const BATTLE: u8 = 3;
    pub const THRONE_RELEASED: u8 = 4;
    pub const EYES_OPEN: u8 = 5;
    pub const FINISHED: u8 = 6;
    pub const ARCHLORD: u8 = 7;
    pub const CLOSING: u8 = 8;

    /// Château « en guerre » (3 à 8).
    pub fn at_war(s: u8) -> bool {
        (BATTLE..=CLOSING).contains(&s)
    }
}

/// Codes des messages de siège (op 0x1C, `FUN_14062ade0`).
pub mod msg {
    pub const START: u8 = 1;
    pub const OBJECT_DESTROYED: u8 = 4;
    pub const THRONE_RELEASED: u8 = 5;
    pub const EYE_OPENED: u8 = 6;
    pub const EYE_CLOSED: u8 = 7;
    pub const CARVE_START: u8 = 8;
    pub const CARVE_CANCEL: u8 = 9;
    pub const CARVE_DONE: u8 = 10;
    pub const TERMINATE: u8 = 11;
    pub const CAMP_ACTIVE_LEFT: u8 = 12;
    pub const CAMP_ACTIVE_RIGHT: u8 = 13;
    pub const CAMP_DISABLED_LEFT: u8 = 14;
    pub const CAMP_DISABLED_RIGHT: u8 = 15;
    pub const REPAIR_LEFT: u8 = 16;
    pub const REPAIR_RIGHT: u8 = 17;
}

/// Types d'objets de siège (`SiegeWarCharType` de CharacterDataTable.txt).
pub mod object {
    pub const GUARD_TOWER: i32 = 1;
    pub const INNER_GATE: i32 = 2;
    pub const OUTER_GATE: i32 = 3;
    pub const LIFE_TOWER: i32 = 4;
    pub const DEFENSE_RES_TOWER: i32 = 5;
    pub const THRONE: i32 = 6;
    pub const ARCHON_EYE: i32 = 7;
    pub const ARCHON_PROTECTIVE: i32 = 8;
    pub const ATTACK_RES_TOWER: i32 = 9;
    pub const CATAPULT: i32 = 10;
    pub const GUARDIAN: i32 = 11;

    /// Nom d'un type pour « %s has been destroyed » (clés `SiegeWarObject*`).
    pub fn name_key(t: i32) -> Option<&'static str> {
        Some(match t {
            GUARD_TOWER => "SiegeWarObjectGuardTower",
            INNER_GATE => "SiegeWarObjectInnerGate",
            OUTER_GATE => "SiegeWarObjectOuterGate",
            LIFE_TOWER => "SiegeWarObjectLiftTower",
            DEFENSE_RES_TOWER => "SiegeWarObjectDefenseResTower",
            ATTACK_RES_TOWER => "SiegeWarObjectAttackResTower",
            CATAPULT => "SiegeWarObjectCatapult",
            _ => return None,
        })
    }
}

/// Château tel que l'envoie l'op 0 (complet ou partiel : seuls les champs présents changent).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Castle {
    pub name: String,
    pub owner: Option<String>,
    pub guild: Option<String>,
    pub members: Option<u8>,
    /// Dernier et prochain siège, FILETIME en heure locale (100 ns depuis 1601).
    pub last: Option<u64>,
    pub next: Option<u64>,
    pub status: Option<u8>,
    /// Résurrection des assaillants / des défenseurs disponible, drapeau +0xd7c (2 = absent côté lecteur).
    pub attacker_res: Option<bool>,
    pub defender_res: Option<bool>,
    pub flag: Option<bool>,
    pub carve_guild: Option<String>,
}

impl Castle {
    /// Applique une mise à jour partielle.
    pub fn merge(&mut self, u: &Castle) {
        macro_rules! take {
            ($($f:ident),*) => { $( if u.$f.is_some() { self.$f = u.$f.clone(); } )* };
        }
        take!(owner, guild, members, last, next, status, attacker_res, defender_res, flag, carve_guild);
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuildEntry {
    pub name: String,
    pub master: String,
    pub points: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuildPage {
    /// Page (1-based) et nombre de pages.
    pub page: i16,
    pub pages: i16,
    pub entries: Vec<GuildEntry>,
}

/// Fiche d'un objet de siège (catapulte, camp de résurrection).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ObjectInfo {
    pub repairs_left: Option<u8>,
    pub repair_cost: Option<i32>,
    pub repair_minutes: Option<u8>,
    pub unknown: Option<u8>,
    /// TID des 8 objets chargés (-1 vide).
    pub items: [i32; 8],
    pub repair_elapsed: Option<u8>,
    pub user: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StatusMsg {
    pub code: u8,
    pub castle: String,
    pub guild: String,
    pub name: String,
    pub object_type: u8,
    pub unknown: u32,
    /// 0 Ouest (gauche), 1 Est (droite).
    pub side: u8,
}

/// Paquet AgpmWarInfo, lu à ses décalages fixes.
#[derive(Debug, Clone, PartialEq)]
pub enum WarInfoEvent {
    /// op 0 : afficher l'étape.
    Step { war: u8, step: u8, param: u8 },
    /// op 1 (non lu par Archonia).
    Open { war: u8 },
    /// op 2 : compteur affiché (minutes).
    Counter { war: u8, value: u8 },
    /// op 3 : numéro du message central (1 à 3) de l'étape en cours.
    CenterMessage { war: u8, index: u8 },
    /// op 4 / 5 : deux valeurs (victoires / défaites du joueur, puis de sa race).
    Score { war: u8, race: bool, win: i32, lose: i32 },
    /// op 6 : effacer le tableau.
    Clear { war: u8 },
    /// op 7 : arguments `%s` des messages.
    Args { war: u8, args: [String; 3] },
}

/// Types de guerre de warinfodata.txt.
pub mod war {
    pub const SIEGE: u8 = 0;
    pub const BATTLEGROUND: u8 = 1;
    pub const SHRINE: u8 = 2;
    pub const ARCHLORD: u8 = 3;
}

#[derive(Debug, Clone, PartialEq)]
pub enum SiegeEvent {
    /// op 0 : château (complet ou partiel).
    Castle(Castle),
    /// op 2 : code de résultat.
    Result { code: i8, castle: String },
    /// op 9 : candidats à l'attaque (ou assaillants retenus après la clôture) ; op 10 : candidats à la défense.
    AttackApplicants { castle: String, page: GuildPage },
    DefenseApplicants { castle: String, page: GuildPage },
    /// op 0xE / 0xF : guildes retenues (défenseurs : d'abord la guilde propriétaire).
    Attackers { castle: String, guilds: Vec<String> },
    Defenders { castle: String, guilds: Vec<String> },
    /// ops 0x11 / 0x13 / 0x15 : réponse sur un objet ; 0x13 avec 0x0E ouvre sa fenêtre.
    ObjectReply { op: i8, flag: i8, target: i32, info: Option<ObjectInfo> },
    /// op 0x18 : objet d'inventaire chargé dans l'objet de siège.
    ItemLoaded { target: i32, iid: i32 },
    /// op 0x1A : fiche de l'objet ouvert.
    ObjectInfo { target: i32, info: ObjectInfo },
    /// op 0x1C : message de siège.
    Message(StatusMsg),
    /// op 0x1D : compte à rebours (1 avant le début, 2 avant la fin).
    Countdown { kind: u8, seconds: u16 },
    /// AgpmEventSiegeWarNPC op 2 : ouvrir `UI_SiegeWarMain` pour le château du PNJ.
    NpcGranted { npc: i32 },
    /// AgpmWarInfo.
    WarInfo(WarInfoEvent),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SiegeCommand {
    /// Demande au « Siege Commander » `npc` (0x50 op 1).
    NpcRequest(i32),
    /// Déclaration du prochain siège (FILETIME local).
    Declare { castle: String, date: u64 },
    ApplyDefense(String),
    CancelDefense(String),
    ApplyAttack(String),
    CancelAttack(String),
    /// Pages des candidats (1-based).
    AttackPage { castle: String, page: i16 },
    DefensePage { castle: String, page: i16 },
    /// Le propriétaire retient la guilde défenseuse `guild`.
    SelectDefender { castle: String, guild: String },
    RequestAttackers(String),
    RequestDefenders(String),
    /// Objet de siège visé : ouverture (9/10), activation, réparation, carving du trône.
    OpenObject(i32),
    Activate(i32),
    Repair(i32),
    Carve(i32),
    /// Chargement / retrait d'un objet d'inventaire.
    LoadItem { target: i32, iid: i32 },
    UnloadItem { target: i32, iid: i32 },
    /// Résurrection au camp de siège (AgpmCharacter op 0x13 intérieur, 0x14 extérieur, `FUN_1404ff090`).
    Resurrect { inner: bool },
}

/// Ops d'AgpmCharacter de la résurrection de siège.
pub const RESURRECT_INNER: i8 = 0x13;
pub const RESURRECT_OUTER: i8 = 0x14;

/// Texte d'un champ (latin-1, comme `Value::as_text`).
fn latin1(s: &str) -> Vec<u8> {
    s.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).collect()
}

fn block(s: &str) -> Value {
    Value::Block(latin1(s))
}

fn castle_only(name: &str) -> Value {
    castle_value(&Castle { name: name.to_string(), ..Default::default() })
}

/// Mini CASTLE d'un château (champs présents seulement).
pub fn castle_value(c: &Castle) -> Value {
    let m = mini::SIEGE_CASTLE;
    let mut pairs = vec![(0, block(&c.name))];
    if let Some(v) = &c.owner {
        pairs.push((1, block(v)));
    }
    if let Some(v) = &c.guild {
        pairs.push((2, block(v)));
    }
    if let Some(v) = c.members {
        pairs.push((3, Value::U8(v)));
    }
    if let Some(v) = c.last {
        pairs.push((4, Value::U64(v)));
    }
    if let Some(v) = c.next {
        pairs.push((5, Value::U64(v)));
    }
    if let Some(v) = c.status {
        pairs.push((6, Value::U8(v)));
    }
    for (i, v) in [(7, c.attacker_res), (8, c.defender_res), (9, c.flag)] {
        if let Some(v) = v {
            pairs.push((i, Value::U8(v as u8)));
        }
    }
    if let Some(v) = &c.carve_guild {
        pairs.push((10, block(v)));
    }
    Value::Packet(build_mini(m, &sparse(m, &pairs)).unwrap_or_default())
}

fn page_value(page: i16) -> Value {
    let m = mini::SIEGE_GUILD_PAGE;
    Value::Packet(build_mini(m, &sparse(m, &[(0, Value::I16(page))])).unwrap_or_default())
}

/// Paquet d'une commande (`me` : CID du joueur).
pub fn command_packet(cmd: &SiegeCommand, me: i32) -> (&'static Schema, Vec<Option<Value>>) {
    let s = main::SIEGE_WAR;
    let with_castle = |o: i8, castle: &str| sparse(s, &[(0, Value::I8(o)), (2, castle_only(castle)), (3, Value::I32(me))]);
    let on_object = |o: i8, target: i32| sparse(s, &[(0, Value::I8(o)), (3, Value::I32(me)), (5, Value::I32(target))]);
    let values = match cmd {
        SiegeCommand::Resurrect { inner } => {
            let c = main::CHARACTER;
            return (c, sparse(c, &[(0, Value::I8(if *inner { RESURRECT_INNER } else { RESURRECT_OUTER })), (1, Value::I32(me))]));
        }
        SiegeCommand::NpcRequest(npc) => {
            let n = main::EVENT_SIEGEWAR_NPC;
            return (n, sparse(n, &[(0, Value::I8(1)), (1, crate::npc::event_base(*npc, FUNCTION)), (2, Value::I32(me))]));
        }
        SiegeCommand::Declare { castle, date } => {
            let c = Castle { name: castle.clone(), next: Some(*date), ..Default::default() };
            sparse(s, &[(0, Value::I8(op::DECLARE)), (2, castle_value(&c)), (3, Value::I32(me))])
        }
        SiegeCommand::ApplyDefense(c) => with_castle(op::APPLY_DEFENSE, c),
        SiegeCommand::CancelDefense(c) => with_castle(op::CANCEL_DEFENSE, c),
        SiegeCommand::ApplyAttack(c) => with_castle(op::APPLY_ATTACK, c),
        SiegeCommand::CancelAttack(c) => with_castle(op::CANCEL_ATTACK, c),
        SiegeCommand::AttackPage { castle, page } | SiegeCommand::DefensePage { castle, page } => {
            let o = if matches!(cmd, SiegeCommand::AttackPage { .. }) { op::ATTACK_PAGE } else { op::DEFENSE_PAGE };
            sparse(s, &[(0, Value::I8(o)), (2, castle_only(castle)), (3, Value::I32(me)), (4, page_value(*page))])
        }
        SiegeCommand::SelectDefender { castle, guild } => {
            let c = Castle { name: castle.clone(), guild: Some(guild.clone()), ..Default::default() };
            sparse(s, &[(0, Value::I8(op::SELECT_DEFENSE)), (2, castle_value(&c)), (3, Value::I32(me))])
        }
        SiegeCommand::RequestAttackers(c) => with_castle(op::REQUEST_ATTACKERS, c),
        SiegeCommand::RequestDefenders(c) => with_castle(op::REQUEST_DEFENDERS, c),
        SiegeCommand::OpenObject(t) => on_object(op::OPEN_OBJECT, *t),
        SiegeCommand::Activate(t) => on_object(op::ACTIVATE, *t),
        SiegeCommand::Repair(t) => on_object(op::REPAIR, *t),
        SiegeCommand::Carve(t) => on_object(op::CARVE, *t),
        SiegeCommand::LoadItem { target, iid } | SiegeCommand::UnloadItem { target, iid } => {
            let o = if matches!(cmd, SiegeCommand::LoadItem { .. }) { op::LOAD_ITEM } else { op::UNLOAD_ITEM };
            sparse(s, &[(0, Value::I8(o)), (3, Value::I32(me)), (5, Value::I32(*target)), (7, Value::I32(*iid))])
        }
    };
    (s, values)
}

fn int(v: &[Option<Value>], i: usize) -> Option<i64> {
    v.get(i).and_then(Option::as_ref).and_then(Value::as_i64)
}

fn txt(v: &[Option<Value>], i: usize) -> Option<String> {
    v.get(i).and_then(Option::as_ref).and_then(Value::as_text)
}

fn flag(v: &[Option<Value>], i: usize) -> Option<bool> {
    // Le lecteur ne prend que 0 et 1 (2 par défaut : absent).
    int(v, i).filter(|x| *x == 0 || *x == 1).map(|x| x == 1)
}

pub fn decode_castle(bytes: &[u8]) -> Option<Castle> {
    let (_, v) = decode_mini(mini::SIEGE_CASTLE, bytes).ok()?;
    Some(Castle {
        name: txt(&v, 0).unwrap_or_default(),
        owner: txt(&v, 1),
        guild: txt(&v, 2),
        members: int(&v, 3).map(|x| x as u8),
        last: int(&v, 4).map(|x| x as u64),
        next: int(&v, 5).map(|x| x as u64),
        status: int(&v, 6).map(|x| x as u8),
        attacker_res: flag(&v, 7),
        defender_res: flag(&v, 8),
        flag: flag(&v, 9),
        carve_guild: txt(&v, 10),
    })
}

pub fn decode_page(bytes: &[u8]) -> Option<GuildPage> {
    let (_, v) = decode_mini(mini::SIEGE_GUILD_PAGE, bytes).ok()?;
    let rows: Vec<Vec<u8>> = match v.get(2).and_then(Option::as_ref) {
        Some(Value::PacketList(l)) => l.clone(),
        Some(Value::Packet(p)) => vec![p.clone()],
        _ => Vec::new(),
    };
    let entries = rows
        .iter()
        .filter_map(|r| decode_mini(mini::SIEGE_GUILD_ENTRY, r).ok())
        .map(|(_, e)| GuildEntry { name: txt(&e, 0).unwrap_or_default(), master: txt(&e, 1).unwrap_or_default(), points: int(&e, 2).unwrap_or(0) as u32 })
        .filter(|e| !e.name.is_empty())
        .collect();
    Some(GuildPage { page: int(&v, 0).unwrap_or(1) as i16, pages: int(&v, 1).unwrap_or(1) as i16, entries })
}

/// Page de guildes telle que l'envoie un serveur (essais, outils).
pub fn page_packet(page: &GuildPage) -> Value {
    let e = mini::SIEGE_GUILD_ENTRY;
    let rows = page
        .entries
        .iter()
        .take(PAGE_ROWS)
        .map(|g| build_mini(e, &sparse(e, &[(0, block(&g.name)), (1, block(&g.master)), (2, Value::U32(g.points))])).unwrap_or_default())
        .collect();
    let m = mini::SIEGE_GUILD_PAGE;
    Value::Packet(build_mini(m, &sparse(m, &[(0, Value::I16(page.page)), (1, Value::I16(page.pages)), (2, Value::PacketList(rows))])).unwrap_or_default())
}

pub fn decode_object_info(bytes: &[u8]) -> Option<ObjectInfo> {
    let (_, v) = decode_mini(mini::SIEGE_OBJECT_INFO, bytes).ok()?;
    let mut items = [-1; 8];
    for (k, it) in items.iter_mut().enumerate() {
        if let Some(t) = int(&v, 4 + k) {
            *it = t as i32;
        }
    }
    Some(ObjectInfo {
        repairs_left: int(&v, 0).map(|x| x as u8),
        repair_cost: int(&v, 1).map(|x| x as i32),
        repair_minutes: int(&v, 2).map(|x| x as u8),
        unknown: int(&v, 3).map(|x| x as u8),
        items,
        repair_elapsed: int(&v, 12).map(|x| x as u8),
        user: txt(&v, 13),
    })
}

/// Fiche d'objet telle que l'envoie un serveur.
pub fn object_info_value(o: &ObjectInfo) -> Value {
    let m = mini::SIEGE_OBJECT_INFO;
    let mut pairs = Vec::new();
    if let Some(x) = o.repairs_left {
        pairs.push((0, Value::U8(x)));
    }
    if let Some(x) = o.repair_cost {
        pairs.push((1, Value::I32(x)));
    }
    if let Some(x) = o.repair_minutes {
        pairs.push((2, Value::U8(x)));
    }
    if let Some(x) = o.unknown {
        pairs.push((3, Value::U8(x)));
    }
    for (k, t) in o.items.iter().enumerate() {
        pairs.push((4 + k, Value::I32(*t)));
    }
    if let Some(x) = o.repair_elapsed {
        pairs.push((12, Value::U8(x)));
    }
    if let Some(x) = &o.user {
        pairs.push((13, block(x)));
    }
    Value::Packet(build_mini(m, &sparse(m, &pairs)).unwrap_or_default())
}

pub fn decode_status_msg(bytes: &[u8]) -> Option<StatusMsg> {
    let (_, v) = decode_mini(mini::SIEGE_STATUS_MSG, bytes).ok()?;
    Some(StatusMsg {
        code: int(&v, 0)? as u8,
        castle: txt(&v, 1).unwrap_or_default(),
        guild: txt(&v, 2).unwrap_or_default(),
        name: txt(&v, 3).unwrap_or_default(),
        object_type: int(&v, 4).unwrap_or(0) as u8,
        unknown: int(&v, 5).unwrap_or(0) as u32,
        side: int(&v, 6).unwrap_or(0) as u8,
    })
}

pub fn status_msg_value(m: &StatusMsg) -> Value {
    let s = mini::SIEGE_STATUS_MSG;
    // Bloc vide omis : l'encodeur écrirait sa longueur sans poser le drapeau.
    let mut pairs = vec![(0, Value::U8(m.code)), (4, Value::U8(m.object_type)), (5, Value::U32(m.unknown)), (6, Value::U8(m.side))];
    for (i, t) in [(1, &m.castle), (2, &m.guild), (3, &m.name)] {
        if !t.is_empty() {
            pairs.push((i, block(t)));
        }
    }
    Value::Packet(build_mini(s, &sparse(s, &pairs)).unwrap_or_default())
}

pub fn time_value(kind: u8, seconds: u16) -> Value {
    let m = mini::SIEGE_TIME;
    Value::Packet(build_mini(m, &sparse(m, &[(0, Value::U8(kind)), (1, Value::U16(seconds))])).unwrap_or_default())
}

fn names(page: Option<GuildPage>) -> Vec<String> {
    page.map(|p| p.entries.into_iter().map(|e| e.name).collect()).unwrap_or_default()
}

/// Paquets 0x49 et 0x50 reçus.
pub fn translate(ptype: u8, values: &[Option<Value>]) -> Option<SiegeEvent> {
    let bytes = |i: usize| values.get(i).and_then(Option::as_ref).and_then(Value::as_bytes);
    if ptype == packet_type::EVENT_SIEGEWAR_NPC {
        if int(values, 0)? != 2 {
            return None;
        }
        let npc = bytes(1).and_then(|b| decode_mini(mini::EVENT_BASE, b).ok()).and_then(|(_, v)| int(&v, 1)).unwrap_or(0) as i32;
        return Some(SiegeEvent::NpcGranted { npc });
    }
    if ptype != packet_type::SIEGE_WAR {
        return None;
    }
    let castle = || bytes(2).and_then(decode_castle);
    let castle_name = || castle().map(|c| c.name).unwrap_or_default();
    let page = || bytes(4).and_then(decode_page);
    let target = int(values, 5).unwrap_or(0) as i32;
    let info = || bytes(6).and_then(decode_object_info);
    Some(match int(values, 0)? as i8 {
        op::CASTLE => SiegeEvent::Castle(castle()?),
        op::RESULT => SiegeEvent::Result { code: int(values, 1).unwrap_or(0) as i8, castle: castle_name() },
        op::ATTACK_LIST => SiegeEvent::AttackApplicants { castle: castle_name(), page: page().unwrap_or_default() },
        op::DEFENSE_LIST => SiegeEvent::DefenseApplicants { castle: castle_name(), page: page().unwrap_or_default() },
        op::ATTACKERS => SiegeEvent::Attackers { castle: castle_name(), guilds: names(page()) },
        op::DEFENDERS => SiegeEvent::Defenders { castle: castle_name(), guilds: names(page()) },
        o @ (op::OBJECT_REPLY_1 | op::OBJECT_REPLY | op::OBJECT_REPLY_2) => SiegeEvent::ObjectReply { op: o, flag: int(values, 1).unwrap_or(0) as i8, target, info: info() },
        op::ITEM_LOADED => SiegeEvent::ItemLoaded { target, iid: int(values, 7).unwrap_or(0) as i32 },
        op::OBJECT_INFO => SiegeEvent::ObjectInfo { target, info: info()? },
        op::STATUS_MSG => SiegeEvent::Message(bytes(8).and_then(decode_status_msg)?),
        op::TIME => {
            let (_, v) = decode_mini(mini::SIEGE_TIME, bytes(10)?).ok()?;
            SiegeEvent::Countdown { kind: int(&v, 0).unwrap_or(0) as u8, seconds: int(&v, 1).unwrap_or(0) as u16 }
        }
        _ => return None,
    })
}

/// Paquet AgpmWarInfo brut (en-tête de 19 octets, drapeaux en 0x13).
pub fn decode_war_info(raw: &[u8]) -> Option<WarInfoEvent> {
    if raw.get(5) != Some(&packet_type::WAR_INFO) || raw.len() < 0x18 {
        return None;
    }
    let (o, war, step, param) = (raw[0x14], raw[0x15], raw[0x16], raw[0x17]);
    let i32_at = |at: usize| raw.get(at..at + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap()));
    let str_at = |at: usize| {
        let b = raw.get(at..(at + 0x21).min(raw.len())).unwrap_or(&[]);
        let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        b[..end].iter().map(|&c| c as char).collect::<String>()
    };
    Some(match o {
        0 => WarInfoEvent::Step { war, step, param },
        1 => WarInfoEvent::Open { war },
        2 => WarInfoEvent::Counter { war, value: step },
        3 => WarInfoEvent::CenterMessage { war, index: step },
        4 | 5 => WarInfoEvent::Score { war, race: o == 5, win: i32_at(0x18)?, lose: i32_at(0x1C)? },
        6 => WarInfoEvent::Clear { war },
        7 => WarInfoEvent::Args { war, args: [str_at(0x16), str_at(0x37), str_at(0x58)] },
        _ => return None,
    })
}

/// Paquet AgpmWarInfo tel que l'envoie un serveur (essais, outils) : les 4 champs déclarés (drapeaux 0x0F), puis
/// les données ajoutées de l'op.
pub fn war_info_packet(ev: &WarInfoEvent) -> Vec<u8> {
    let mut out = vec![0u8; 0x18];
    out[0] = 0xd6;
    out[5] = packet_type::WAR_INFO;
    out[0x13] = 0x0F;
    let (o, war, step, param) = match ev {
        WarInfoEvent::Step { war, step, param } => (0, *war, *step, *param),
        WarInfoEvent::Open { war } => (1, *war, 0, 0),
        WarInfoEvent::Counter { war, value } => (2, *war, *value, 0),
        WarInfoEvent::CenterMessage { war, index } => (3, *war, *index, 0),
        WarInfoEvent::Score { war, race, .. } => (if *race { 5 } else { 4 }, *war, 0, 0),
        WarInfoEvent::Clear { war } => (6, *war, 0, 0),
        WarInfoEvent::Args { war, .. } => (7, *war, 0, 0),
    };
    out[0x14..0x18].copy_from_slice(&[o, war, step, param]);
    match ev {
        WarInfoEvent::Score { win, lose, .. } => {
            out.extend_from_slice(&win.to_le_bytes());
            out.extend_from_slice(&lose.to_le_bytes());
        }
        WarInfoEvent::Args { args, .. } => {
            out.resize(0x16 + 3 * 0x21, 0);
            for (k, a) in args.iter().enumerate() {
                let b = latin1(a);
                let n = b.len().min(0x20);
                out[0x16 + k * 0x21..0x16 + k * 0x21 + n].copy_from_slice(&b[..n]);
            }
        }
        _ => {}
    }
    out.push(0x6b);
    let len = out.len() as u32;
    out[1..5].copy_from_slice(&len.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_protocol::packet::{build_packet, decode_packet};

    fn round(schema: &'static Schema, values: &[Option<Value>]) -> Vec<Option<Value>> {
        decode_packet(schema, &build_packet(schema, values).unwrap()).unwrap().values
    }

    #[test]
    fn commandes_du_client() {
        let (s, v) = command_packet(&SiegeCommand::Declare { castle: "Inflame".into(), date: 0x01DC_0000_1234_5678 }, 7);
        assert_eq!(s.id, 0x49);
        let v = round(s, &v);
        assert_eq!(int(&v, 0), Some(1));
        assert_eq!(int(&v, 3), Some(7));
        let c = decode_castle(v[2].as_ref().unwrap().as_bytes().unwrap()).unwrap();
        assert_eq!((c.name.as_str(), c.next, c.owner), ("Inflame", Some(0x01DC_0000_1234_5678), None));

        let (s, v) = command_packet(&SiegeCommand::AttackPage { castle: "Arthian".into(), page: 2 }, 7);
        let v = round(s, &v);
        assert_eq!(int(&v, 0), Some(7));
        assert_eq!(decode_page(v[4].as_ref().unwrap().as_bytes().unwrap()).unwrap().page, 2);

        let (s, v) = command_packet(&SiegeCommand::SelectDefender { castle: "Traqua".into(), guild: "Lions".into() }, 7);
        let v = round(s, &v);
        let c = decode_castle(v[2].as_ref().unwrap().as_bytes().unwrap()).unwrap();
        assert_eq!((int(&v, 0), c.guild.as_deref()), (Some(0x0B), Some("Lions")));

        let (s, v) = command_packet(&SiegeCommand::LoadItem { target: 0x4000_0010, iid: 99 }, 7);
        let v = round(s, &v);
        assert_eq!((int(&v, 0), int(&v, 5), int(&v, 7)), (Some(0x16), Some(0x4000_0010), Some(99)));

        let (s, v) = command_packet(&SiegeCommand::Resurrect { inner: false }, 7);
        let v = round(s, &v);
        assert_eq!((s.id, int(&v, 0), int(&v, 1)), (2, Some(0x14), Some(7)));

        let (s, v) = command_packet(&SiegeCommand::Carve(0x4000_0020), 7);
        assert_eq!((int(&round(s, &v), 0), int(&v, 5)), (Some(0x1B), Some(0x4000_0020)));

        let (s, v) = command_packet(&SiegeCommand::NpcRequest(0x4000_0003), 7);
        assert_eq!(s.id, 0x50);
        let v = round(s, &v);
        assert_eq!((int(&v, 0), int(&v, 2)), (Some(1), Some(7)));
        let (_, b) = decode_mini(mini::EVENT_BASE, v[1].as_ref().unwrap().as_bytes().unwrap()).unwrap();
        assert_eq!((int(&b, 1), int(&b, 3)), (Some(0x4000_0003), Some(33)));
    }

    #[test]
    fn paquets_du_serveur() {
        let s = main::SIEGE_WAR;
        // op 0 complet, puis partiel fusionné.
        let full = Castle { name: "Inflame".into(), owner: Some("Bob".into()), guild: Some("Lions".into()), members: Some(31), last: Some(5), next: Some(9), status: Some(1), attacker_res: Some(false), defender_res: Some(true), flag: None, carve_guild: Some("Ours".into()) };
        let v = round(s, &sparse(s, &[(0, Value::I8(0)), (2, castle_value(&full))]));
        let Some(SiegeEvent::Castle(c)) = translate(0x49, &v) else { panic!() };
        assert_eq!(c, full);
        let mut c2 = c.clone();
        c2.merge(&Castle { name: "Inflame".into(), status: Some(3), ..Default::default() });
        assert_eq!((c2.status, c2.owner.as_deref()), (Some(3), Some("Bob")));

        // op 2 : résultat et message.
        let v = round(s, &sparse(s, &[(0, Value::I8(2)), (1, Value::I8(0x15)), (2, castle_only("Arthian"))]));
        assert_eq!(translate(0x49, &v), Some(SiegeEvent::Result { code: 0x15, castle: "Arthian".into() }));
        assert_eq!(result::message(0x15), Some((false, "SiegeWarStatusReachedLimitation")));
        assert_eq!(result::message(7), None);
        let codes = (0..=0x15).filter(|c| result::message(*c).is_some()).count();
        assert_eq!(codes, 17);

        // op 9 : page de candidats.
        let page = GuildPage { page: 1, pages: 2, entries: vec![GuildEntry { name: "Lions".into(), master: "Bob".into(), points: 1200 }, GuildEntry { name: "Ours".into(), master: "Ann".into(), points: 3 }] };
        let v = round(s, &sparse(s, &[(0, Value::I8(9)), (2, castle_only("Inflame")), (4, page_packet(&page))]));
        assert_eq!(translate(0x49, &v), Some(SiegeEvent::AttackApplicants { castle: "Inflame".into(), page: page.clone() }));
        let v = round(s, &sparse(s, &[(0, Value::I8(0x0F)), (2, castle_only("Inflame")), (4, page_packet(&page))]));
        assert_eq!(translate(0x49, &v), Some(SiegeEvent::Defenders { castle: "Inflame".into(), guilds: vec!["Lions".into(), "Ours".into()] }));

        // op 0x13 (ouverture) et 0x1A (fiche).
        let info = ObjectInfo { repairs_left: Some(5), repair_cost: Some(500_000), repair_minutes: Some(5), unknown: Some(0), items: [4553, -1, -1, -1, -1, -1, -1, -1], repair_elapsed: Some(12), user: Some("Bob".into()) };
        let v = round(s, &sparse(s, &[(0, Value::I8(0x13)), (1, Value::I8(0x0E)), (3, Value::I32(7)), (5, Value::I32(0x4000_0010)), (6, object_info_value(&info))]));
        assert_eq!(translate(0x49, &v), Some(SiegeEvent::ObjectReply { op: 0x13, flag: 0x0E, target: 0x4000_0010, info: Some(info.clone()) }));
        let v = round(s, &sparse(s, &[(0, Value::I8(0x1A)), (3, Value::I32(7)), (6, object_info_value(&info))]));
        assert_eq!(translate(0x49, &v), Some(SiegeEvent::ObjectInfo { target: 0, info }));

        // op 0x1C et 0x1D.
        let m = StatusMsg { code: msg::EYE_OPENED, castle: "Inflame".into(), side: 1, ..Default::default() };
        let v = round(s, &sparse(s, &[(0, Value::I8(0x1C)), (8, status_msg_value(&m))]));
        assert_eq!(translate(0x49, &v), Some(SiegeEvent::Message(m)));
        let v = round(s, &sparse(s, &[(0, Value::I8(0x1D)), (10, time_value(2, 3599))]));
        assert_eq!(translate(0x49, &v), Some(SiegeEvent::Countdown { kind: 2, seconds: 3599 }));

        // 0x50 op 2 : accord.
        let n = main::EVENT_SIEGEWAR_NPC;
        let v = round(n, &sparse(n, &[(0, Value::I8(2)), (1, crate::npc::event_base(0x4000_0003, FUNCTION)), (2, Value::I32(7))]));
        assert_eq!(translate(0x50, &v), Some(SiegeEvent::NpcGranted { npc: 0x4000_0003 }));
    }

    #[test]
    fn tableau_des_guerres() {
        for ev in [
            WarInfoEvent::Step { war: war::SIEGE, step: 3, param: 0 },
            WarInfoEvent::Counter { war: war::SIEGE, value: 42 },
            WarInfoEvent::CenterMessage { war: war::SIEGE, index: 2 },
            WarInfoEvent::Score { war: war::SHRINE, race: true, win: 12, lose: -1 },
            WarInfoEvent::Clear { war: war::SIEGE },
            WarInfoEvent::Args { war: war::SIEGE, args: ["Lions".into(), "Bob".into(), "Inflame".into()] },
        ] {
            let p = war_info_packet(&ev);
            assert_eq!((p[0x13], *p.last().unwrap()), (0x0F, 0x6b));
            assert_eq!(decode_war_info(&p), Some(ev));
        }
        // Les 4 champs déclarés se décodent aussi avec le schéma.
        let p = war_info_packet(&WarInfoEvent::Step { war: 0, step: 5, param: 1 });
        let d = decode_packet(main::WAR_INFO, &p).unwrap();
        assert_eq!(d.get(2).and_then(Value::as_i64), Some(5));
    }
}
