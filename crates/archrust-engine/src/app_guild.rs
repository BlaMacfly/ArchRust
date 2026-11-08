//! Guildes et discussion de guilde et de groupe côté moteur. Protocole : `archrust_netclient::guild` ;
//! fenêtres : `hud_guild.rs`.
//!
//! Comportement relevé dans Archonia (AgcmUIGuild, sha256 e9c7609b… ; fonctions d'interface
//! enregistrées par FUN_1406a13a0, événements par FUN_1406a09e0) :
//! * gestionnaire de guilde (PNJ à fonction 23, bouton `sysstr` 328 « Guilde » du dialogue,
//!   FUN_1406ca6a0) : AGPMEVENTGUILD op 0, l'accord (op 1, FUN_1406a3890) ouvre `Guild_Main` ;
//!   texte `Guild_Main_Text` (niveau 20, prix de la ligne 50 de `guildmaxmember.txt`, FUN_1408c6ad0) ;
//!   « Créer » actif sans guilde et avec l'or ; « Dissoudre » actif pour un chef seul dans sa guilde ;
//! * création (FUN_1408c67f0, FUN_1408c6e50, FUN_1408c7330, FUN_1408c7290) : déjà membre → message 5,
//!   niveau < 20 → `Guild_Msg_NeedMoreLevel` ; nom 1 à 32 caractères, mot de passe 1 à 12, confirmé
//!   (`Guild_Msg_CheckPassword`) ; `Guild_Create_Request` (`Guild_Guild_Info`, `Guild_Create_Confirm_Text`)
//!   puis op 0 ; la guilde reçue avec le joueur pour chef ouvre `Guild_Create_Complete` (FUN_1406a36b0) ;
//! * dissolution (FUN_1408c6920, FUN_1408c70c0, FUN_1408c7670, FUN_1408c75a0) : `Guild_Destroy`, puis
//!   `Guild_Destroy_Confirm`, puis op 6 avec le mot de passe ; `Guild_Destroy_Complete` à la fin ;
//! * touche G (`Hotkey=0:34`, « Guild ») ou bouton du menu : `Guild_Notice_Open_Request`
//!   (FUN_1408c7f10) ouvre `Guild_Notice` ou ferme les fenêtres ouvertes ; `Guild_NoticeUI_Init`
//!   (FUN_1408c81d0) : annonce (89 caractères au plus, modifiable par le chef), `Guild_Notice_Info`
//!   (guilde, chef, membres / taille, en ligne), `Guild_Notice_Detail_NoGuildMember` sans guilde ;
//!   « Envoyer un avis » : op 9 par le chef (FUN_1408c7fe0) ; « Quitter la guilde » (FUN_1408c7d00) :
//!   le chef reçoit `Guild_Leave_Error_Master`, un membre confirme puis op 4 ;
//! * liste des membres (FUN_1408c14c0, FUN_1408c10f0) : 19 par page ; « Expulser » (chef seulement,
//!   FUN_1408c0c20) confirme puis op 5 ; « Inviter » et « Chuchotement » pour un membre en ligne ;
//!   « Approbation » (`Guild_Join_Allow_Button_Text`, `Guild_Leave_Allow_Button_Text` pour une demande
//!   de départ, FUN_1408bf620 / FUN_1408bf480) confirme puis op 3 ou op 29 ; détail d'un membre
//!   `Guild_GuildMemberInfo` ou `Guild_GuildLeadMemberInfo` (FUN_1408bfa80) ;
//! * invitation reçue (FUN_1406a3d10) : boîte OK / Annuler `Guild_Invite_ConfirmUI_Open` ; OK = op 3,
//!   Annuler = op 2 (FUN_1406a39f0) ;
//! * messages système (op 10) : code → clé de `uimessage.txt` (FUN_1406a44e0), couleur
//!   `Guild_System_Message_Color` pour 47, 48 et 50 ;
//! * discussion : type 3 « [Guilde][Nom] texte » en `GUILD_CHAT`, type 4 « [Groupe][Nom] texte » en
//!   `PARTY_CHAT` (FUN_1407a2b30, couleurs de `ini/color.ini`) ; envoi par `/G texte` et `/P texte`
//!   (modes `sysstr` 818 « Guilde (/G) » et 817 « Groupe (/P) »), `/N texte` pour la discussion normale.
//!
//! * second et passation (FUN_1408c10f0, FUN_1408c0340, FUN_1408c0aa0) : « Nommer » (libellé `sysstr`
//!   771, ou 772 s'il y a un second) pour le chef, sur un membre de niveau 20 en ligne quand la guilde n'a
//!   pas de second, ou sur le second (`GuildMemberAppointCancelConfirm_Message` puis rang 1) ;
//!   « Hériter de la guilde » pour le chef sur le second en ligne ; demandes reçues
//!   `GuildAppointConfirm_Message` et `GuildSuccessionConfirm_Message` (FUN_1408c0200, FUN_1408c09e0),
//!   changement annoncé `GuildMemberRankUpdate_Message` (FUN_1408c06f0, rang : FUN_1406a53a0) ;
//! * agrandissement (FUN_1408cce90, FUN_1408ccfa0, FUN_1408ccd50) : « Augmenter la limite » pour le
//!   chef ; moins de 20 membres → `Guild_MemberUp_Not_Enought_Member`, sinon `Guild_MemberSize`
//!   (`Guild_CurrentMaxMember`, `Guild_NextMaxMember` ou `Guild_CanNot_Increase_MaxMember`) ;
//!   « Appliquer » (or suffisant) confirme `Guild_MaxMemberIncreaseOK` puis op 7 ;
//! * liste des guildes, onglet « Classement » (FUN_1408c90c0, FUN_1408c8e30, FUN_1408c8eb0) : page 0 à
//!   l'ouverture, ← →, recherche ; colonnes nom (« @ » → « ... »), membres, classement (champ 4) ;
//!   pages « page/dernière » ; détail `Guild_GuildDetailInfo`.
//! * combats de guildes : « Demander la guerre » (`Guild_List_BattleJoin`, FUN_14084ea50) ouvre
//!   `Guild_GBattle` (FUN_14084de70 : type, durée, effectif des types 3 et 4) ; « Appliquer » confirme
//!   `Guild_Battle_Request_ConfirmUI_Open` puis op 15 ; défi reçu (FUN_14084f310) : la fenêtre en lecture
//!   seule et `Guild_Battle_Accept_ConfirmUI_Open` (OK op 16, Annuler op 17 et `Guild_Msg_Battle_Reject`) ;
//!   acceptation (FUN_14084ebd0) : `sysstr` 0x151 en jaune au centre de l'écran, `Guild_Msg_Battle_Accept` ;
//!   annulation demandée : `Guild_Battle_Cancel_Accept_ConfirmUI_Open` (op 19 / op 20 et
//!   `Guild_Msg_Battle_Cancel_Reject`), acceptée : `Guild_Msg_Battle_Cancel_Accpet` (FUN_14084ee80) ;
//!   début (FUN_14084f610) : `sysstr` 0x152 en rouge, `Guild_Msg_Battle_Start` ; compte à rebours et
//!   scores (FUN_1408505a0, FUN_140850160) ; résultat (FUN_14084f400) : `Guild_Msg_Battle_Win2`,
//!   `Draw2`, `Lose2` (« %d-%d »), `Win_By_GG2`, `Lose_By_GG2` ; abandon « GG »
//!   (`Guild_Battle_Withdraw_ConfirmUI_Open`, op 24).
//!
//! * alliances et hostilités (FUN_1408cc910, FUN_1408cc7e0, FUN_1408cc5f0, FUN_1408cc530) : « Alliés » et
//!   « Hostiles » de `Guild_List` y affichent les alliées ou les hostiles (détails des ops 44 et 45) ; le bouton
//!   d'alliance (`Guild_Joint_Request_Button_Text`, `Guild_Joint_Leave_Button_Text` dans les alliés) et celui
//!   d'hostilité (`Guild_Hostile_Request_Button_Text`, `Guild_Hostile_Leave_Button_Text` dans les hostiles)
//!   confirment `Guild_OpenJointRequestConfirmUI`, `Guild_OpenJointLeaveConfirmUI`,
//!   `Guild_OpenHostileRequestConfirmUI` ou `Guild_OpenHostileLeaveRequestConfirmUI` (ops 34, 37, 38, 41) ;
//!   demandes reçues par le chef (FUN_1408cc440, FUN_1408cbb00, FUN_1408cba20) : `Guild_OpenJointAcceptConfirmUI`,
//!   `Guild_OpenHostileAcceptConfirmUI`, `Guild_OpenHostileLeaveAcceptConfirmUI` ({{Guild_RelationGuildID}},
//!   {{Guild_RelationMasterID}}), sauf s'il refuse les relations de guilde (options) ; codes 0x3c à 0x59.
//!
//! * entrepôt de guilde (AgcmUIGuildWarehouse) : bouton du gardien de banque (fonction 35, `sysstr` 611) →
//!   AgpmEventGuild op 2, l'accord (op 3, FUN_1405fc020) envoie l'op 1 ; le résultat 2 ouvre `UI_GuildWarehouse`
//!   (FUN_1405fc6a0), l'op 0 donne l'argent et les pages (FUN_1405fb8a0) ; objets au statut 53 ; dépôt (op 5,
//!   FUN_1405fc440), reprise vers le sac (op 6, FUN_1405fd4b0), argent (ops 3 et 4 après `UI_GWareConfirm`,
//!   FUN_1405fbb90), agrandissement (op 7 : `GWARE_EXPAND_CONFIRM` à 999 999 999 999 pièces, sinon
//!   `GWARE_NOT_ENOUGH_MONEY`) ; la fenêtre se ferme (op 2) si le joueur s'éloigne de plus de 149 du point
//!   d'ouverture (FUN_1405fc830).
//!
//! * combattants des types 3 et 4 (survie de groupe, survie individuelle) : après la confirmation du défi ou
//!   l'acceptation, `UI_GuildBattleMember` (AgcmUIGuildBattleMember) : membres en ligne, le chef coché d'office
//!   (`GuildBattleMember_NotEraseMaster`), clic sur une ligne = cocher (`OP_GuildBattleListSelect`), monter et
//!   descendre la ligne choisie (`OP_GuildBattleListUp/Down`), tirage au sort (`OP_GuildBattleListRandomSelect`),
//!   « Terminer » (`OP_GuildBattleListDone`, FUN_1408c59a0) : moins de cochés que l'effectif →
//!   `GuildBattleMember_LackMember`, sinon op 14 avec les premiers cochés puis la demande (op 15) ou l'acceptation
//!   (op 16) ; `GuildBattleMember_MemberSelect` sous la liste ;
//! * combattants et points (ops 26 et 27, FUN_14084f200 / FUN_14084f2c0) : `UI_GuildBattleResult`, sa guilde à
//!   gauche, l'adverse à droite, ouverte au résultat (op 25, FUN_14084f400 → FUN_1404ded30) ; manche (op 46) ;
//!   trêve refusée (op 42) : `Guild_Hostile_Leave_Reject`.
//!
//! Commandes d'ArchRust pour les combats : `/gguerre guilde` (fenêtre du défi), `/gguerre appliquer`,
//! `/gguerre annuler`, `/gg` (abandon) ; pour les relations : `/gliste allies|hostiles`, `/galliance guilde|quitter`,
//! `/ghostile guilde`, `/gtreve guilde`.
//!
//! Non repris (ArchCore ne les gère pas ou ressource absente) : changement de nom, missions ; position
//! exacte de la boîte OK / Annuler ; nom de guilde au-dessus des têtes (affichage d'Archonia non établi) ;
//! durée restante d'une demande de départ (ArchCore n'envoie pas sa date).

use super::App;
use crate::assets::Assets;
use crate::hud::{GridCell, GuildEdit, GuildRow, GuildUi, GuildView, GwareItem, GwareView, HudState, ItemUi, ListSpec, PageColors};
use crate::net::NetLink;
use archrust_netclient::guild::{self, BattleFighter, GuildBattle, GuildCommand, GuildEvent, GuildMember, GuildNotice, GuildState};
use archrust_netclient::party::PartyCommand;
use archrust_netclient::Command;
use std::collections::{HashMap, HashSet};

/// Membres par page de la liste (`Guild_Member_Index`, 0x13).
const PAGE: usize = 19;
/// Niveau requis pour créer une guilde (FUN_1408c67f0).
const CREATE_LEVEL: i64 = 20;
/// Taille de la guilde créée : ligne de `guildmaxmember.txt` dont le prix est demandé (FUN_1408c6ad0).
const CREATE_SIZE: i64 = 50;
/// Masques des onglets de discussion (FUN_140511e20) : guilde, groupe, chuchotement.
pub(super) const MASK_GUILD: u16 = 0x2;
pub(super) const MASK_PARTY: u16 = 0x4;
pub(super) const MASK_WHISPER: u16 = 0x8;

/// Textes du client utiles aux guildes.
#[derive(Default)]
struct GuildData {
    messages: HashMap<String, String>,
    events: HashMap<String, String>,
    /// Prix de création (`guildmaxmember.txt`, ligne de 50 places), 100 000 sans la table.
    create_price: i64,
    /// Tailles de `guildmaxmember.txt` et leur prix (FUN_1402042f0).
    sizes: Vec<(i64, i64)>,
    /// Éléments d'emblème de `guildmark.txt` (FUN_1402050f0).
    marks: Vec<MarkRow>,
}

/// Élément d'emblème : couche (1 fond, 2 motif, 3 symbole, 4 couleur), numéro, nom, prix en or et en
/// argent liquide, couleur ARGB (couche 4).
#[derive(Debug, Clone, PartialEq)]
struct MarkRow {
    layer: u8,
    number: u32,
    name: String,
    gold: i64,
    cash: i64,
    argb: u32,
}

impl MarkRow {
    /// TID : numéro décalé de 8 bits par couche.
    fn tid(&self) -> u32 {
        self.number << (8 * (self.layer as u32 - 1))
    }
}

/// Nom de couche des textures d'emblème (`GM_%s%02u.png`, `GM_%sS%02u.png`, FUN_1406033e0).
const MARK_LAYERS: [&str; 4] = ["Bottom", "Pattern", "Symbol", "Color"];
/// Fenêtre `Guild_Mark` : onglets des couches, grille (5 colonnes, 7 lignes visibles) et couleurs.
const MARK_TABS: [&str; 3] = ["CONTROL_26771", "CONTROL_22044", "CONTROL_164"];
const MARK_GRID: &str = "GUILDMARK_GRID";
const MARK_COLORS: &str = "GUILDMARK_COLOR_GRID";
const MARK_COLS: usize = 5;
const MARK_ROWS: usize = 7;
/// Membres exigés pour l'emblème (FUN_14055b920).
const MARK_MIN_MEMBERS: usize = 10;

fn parse_marks(text: &str) -> Vec<MarkRow> {
    text.lines()
        .skip(1)
        .filter_map(|l| {
            let c: Vec<&str> = l.split('\t').map(str::trim).collect();
            let layer = MARK_LAYERS.iter().position(|n| c.first() == Some(n))? as u8 + 1;
            let num = |i: usize| c.get(i).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            let number = num(1);
            if !(1..=255).contains(&number) {
                return None;
            }
            let argb = if layer == 4 { c.get(4).and_then(|v| u32::from_str_radix(v, 16).ok()).unwrap_or(0xFFFF_FFFF) } else { 0 };
            Some(MarkRow { layer, number: number as u32, name: c.get(2).unwrap_or(&"").to_string(), gold: num(3), cash: num(6), argb })
        })
        .collect()
}

/// Partie d'un emblème pour la couche `layer` (1 à 3), et indice de couleur de cette couche.
fn mark_part(tid: u32, layer: u8) -> u32 {
    tid & (0xff << (8 * (layer as u32 - 1)))
}

fn color_index(color: u32, layer: u8) -> u32 {
    (color >> (8 * (layer as u32 - 1))) & 0xff
}

/// Images d'un emblème sur trois contrôles (fond, motif, symbole) : grande (`GM_Bottom03.png`) ou petite
/// (`GM_BottomS03.png`) texture, teinte de la couleur de la couche (FUN_14055d110, FUN_14055c310).
fn mark_images(d: &GuildData, tid: u32, color: u32, controls: [&'static str; 3], small: bool) -> Vec<(&'static str, String, u32)> {
    let mut out = Vec::new();
    for (i, control) in controls.into_iter().enumerate() {
        let layer = i as u8 + 1;
        let part = mark_part(tid, layer);
        let Some(row) = d.marks.iter().find(|m| m.layer == layer && m.tid() == part && part != 0) else { continue };
        let tint = d.marks.iter().find(|m| m.layer == 4 && m.number == color_index(color, layer)).map_or(0xFFFF_FFFF, |m| m.argb);
        out.push((control, format!("GM_{}{}{:02}.png", MARK_LAYERS[i], if small { "S" } else { "" }, row.number), tint));
    }
    out
}

/// Éléments choisis d'un emblème, prix en or et en argent liquide (FUN_14055b200).
fn mark_parts(d: &GuildData, tid: u32) -> (Vec<&MarkRow>, i64, i64) {
    let parts: Vec<&MarkRow> = (1..=3u8).filter_map(|l| d.marks.iter().find(|m| m.layer == l && m.tid() == mark_part(tid, l) && mark_part(tid, l) != 0)).collect();
    let gold = parts.iter().map(|m| m.gold).sum();
    let cash = parts.iter().map(|m| m.cash).sum();
    (parts, gold, cash)
}

impl GuildData {
    fn msg(&self, key: &str) -> String {
        self.messages.get(key).cloned().unwrap_or_default()
    }
    fn color(&self, key: &str, default: u32) -> u32 {
        self.messages.get(key).and_then(|v| v.trim().parse::<u64>().ok()).map_or(default, |v| v as u32)
    }
}

fn load_data(assets: &Assets) -> GuildData {
    let messages: HashMap<String, String> = super::npc_glue::client_text(assets, "uimessage.txt")
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    let mut create_price = 100_000;
    let mut sizes = Vec::new();
    for line in super::npc_glue::client_text_in(assets, "ini", "guildmaxmember.txt").lines() {
        let mut cols = line.split('\t').map(str::trim);
        if let (Some(Ok(size)), Some(Ok(price))) = (cols.next().map(str::parse::<i64>), cols.next().map(str::parse::<i64>)) {
            if size == CREATE_SIZE {
                create_price = price;
            }
            sizes.push((size, price));
        }
    }
    let events = super::portal_glue::ui_events(assets);
    let marks = parse_marks(&super::npc_glue::client_text_in(assets, "ini", "guildmark.txt"));
    GuildData { messages, events, create_price, sizes, marks }
}

/// Une fenêtre de guilde ouverte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Win {
    Main,
    Create,
    Destroy,
    CreateRequest,
    CreateComplete,
    DestroyConfirm,
    DestroyComplete,
    Notice,
    Member,
    List,
    MemberSize,
    Mark,
    MarkConfirm,
    GBattle,
    BattleMember,
    BattleResult,
}

impl Win {
    fn name(self) -> &'static str {
        match self {
            Win::Main => "Guild_Main",
            Win::Create => "Guild_Create",
            Win::Destroy => "Guild_Destroy",
            Win::CreateRequest => "Guild_Create_Request",
            Win::CreateComplete => "Guild_Create_Complete",
            Win::DestroyConfirm => "Guild_Destroy_Confirm",
            Win::DestroyComplete => "Guild_Destroy_Complete",
            Win::Notice => "Guild_Notice",
            Win::Member => "Guild_Member",
            Win::List => "Guild_List",
            Win::MemberSize => "Guild_MemberSize",
            Win::Mark => "Guild_Mark",
            Win::MarkConfirm => "Guild_MarkConfirm",
            Win::GBattle => "Guild_GBattle",
            Win::BattleMember => "UI_GuildBattleMember",
            Win::BattleResult => "UI_GuildBattleResult",
        }
    }
}

/// Question posée dans la boîte OK / Annuler.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Ask {
    Invite { guild: String },
    Leave,
    Kick(String),
    AllowJoin(String),
    AllowLeave(String),
    /// Retirer son rang au second.
    AppointCancel(String),
    /// Agrandir la guilde à cette taille.
    Increase(i64),
    /// Réponse à une nomination ou à une passation reçue.
    AppointAnswer { guild: String, rank: i32 },
    SuccessionAnswer { guild: String },
    /// Demande d'adhésion à la guilde choisie dans la liste.
    JoinRequest(String),
    /// Emblème : remettre à zéro le dessin ; acheter malgré une autre guilde au même emblème.
    MarkReset,
    MarkForce,
    /// Alliance, hostilité et trêve : à demander à cette guilde, reçues de cette guilde ; départ de l'alliance.
    JointRequest(String),
    JointAccept(String),
    JointLeave,
    HostileRequest(String),
    HostileAccept(String),
    HostileLeaveRequest(String),
    HostileLeaveAccept(String),
    /// Combat : défi à envoyer, défi reçu de cette guilde ; annulation à demander, demande reçue ; abandon.
    BattleRequest,
    BattleAnswer(String),
    BattleCancel,
    BattleCancelAnswer,
    BattleWithdraw,
}

/// Entrepôt de guilde : gardien et point d'ouverture, fenêtre ouverte, argent, pages en plus, page affichée,
/// objets (statut 53), montant demandé (dépôt ou retrait).
#[derive(Default)]
struct Gware {
    npc: Option<i32>,
    at: Option<(f32, f32)>,
    open: bool,
    money: i64,
    slots: i32,
    page: i16,
    items: Vec<GwareItem>,
    money_ask: Option<(bool, i64)>,
}

/// Choix de la fenêtre `Guild_GBattle` : guilde visée, type, durée et effectif (indices des listes),
/// lecture seule pour un défi reçu.
#[derive(Default, Clone)]
struct BattleForm {
    enemy: String,
    kind: usize,
    time: usize,
    man: usize,
    received: bool,
}

impl BattleForm {
    /// Valeurs des `{{…}}` des questions de combat (`Guild_Battle_Enemy_GuildID`, type, durée).
    fn named(&self, d: &GuildData) -> Vec<(&'static str, String)> {
        vec![
            ("Guild_Battle_Enemy_GuildID", self.enemy.clone()),
            ("Guild_Battle_Type", d.msg(guild::battle::KINDS[self.kind.min(4)])),
            ("Guild_Battle_Duration", guild::battle::MINUTES[self.time.min(5)].to_string()),
        ]
    }

    fn men(&self) -> &'static [i32] {
        match self.kind {
            3 => &guild::battle::TOTAL_MEN,
            4 => &guild::battle::PRIVATE_MEN,
            _ => &[],
        }
    }
}

/// Remplace chaque `{{Clé.Clé}}` d'un message d'événement par la valeur de sa clé.
fn fill_named(text: &str, values: &[(&str, String)]) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find("{{") {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find("}}") else { break };
        let key = rest[i + 2..i + j].split('.').next_back().unwrap_or("");
        out.push_str(values.iter().find(|(k, _)| *k == key).map_or("", |(_, v)| v.as_str()));
        rest = &rest[i + j + 2..];
    }
    out.push_str(rest);
    out
}

#[derive(Default)]
pub struct GuildGlue {
    pub state: GuildState,
    data: Option<GuildData>,
    open: HashSet<Win>,
    // Saisies.
    name: String,
    password: String,
    password2: String,
    destroy_password: String,
    search: String,
    notice: String,
    /// Création demandée (nom, mot de passe) ; nom de la guilde dissoute pour `Guild_Destroy_Complete`.
    pending_create: Option<(String, String)>,
    pending_destroy: Option<String>,
    complete_name: String,
    selected: Option<usize>,
    page: usize,
    ask: Option<(Ask, String)>,
    /// Boîte d'information (`Action=4:0`) affichée.
    info: Option<String>,
    /// Liste des guildes : ligne choisie et nom cherché ; contenu (0 classement, 1 alliées, 2 hostiles, `+0x2e20`).
    list_selected: Option<usize>,
    list_mode: u8,
    list_search: String,
    /// Emblème en cours de dessin : couche (0 fond, 1 motif, 2 symbole), TID, couleur, ligne de la grille.
    mark_layer: usize,
    mark_tid: u32,
    mark_color: u32,
    mark_scroll: usize,
    /// Combat de guildes : choix de `Guild_GBattle`, annonce du centre de l'écran (texte, couleur ARGB, fin).
    battle_form: BattleForm,
    battle_notice: Option<(String, u32, std::time::Instant)>,
    /// `UI_GuildBattleMember` ouverte : choix des combattants.
    picker: Option<BattlePicker>,
    /// Combattants connus (ops 26 et 27) : sa guilde, la guilde adverse ; première ligne de `UI_GuildBattleResult`.
    fighters: (Vec<BattleFighter>, Vec<BattleFighter>),
    result_first: usize,
    /// Dernier combat terminé (op 25), pour `UI_GuildBattleResult`.
    last_battle: Option<GuildBattle>,
    gware: Gware,
    /// Clics de la dernière image, traités par `guild_frame`.
    clicks: Vec<GuildUi>,
}

/// Choix des combattants (`UI_GuildBattleMember`) : demande (`accept` = `None`) ou acceptation du défi de cette
/// guilde, effectif exigé, membres (nom, niveau, coché), ligne choisie, première ligne affichée.
#[derive(Debug, Clone, Default)]
struct BattlePicker {
    accept: Option<String>,
    count: i32,
    rows: Vec<(String, i64, bool)>,
    cursor: Option<usize>,
    first: usize,
}

/// Lignes visibles des listes de combat (200 / 20 et 160 / 16).
const BATTLE_ROWS: usize = 10;

impl BattlePicker {
    /// Membres en ligne de la guilde, le chef en tête et coché.
    fn new(state: &GuildState, accept: Option<String>, count: i32) -> Self {
        let mut members: Vec<&GuildMember> = state.members.iter().filter(|m| m.active() && m.online).collect();
        members.sort_by_key(|m| m.rank != guild::rank::MASTER);
        let rows = members.iter().map(|m| (m.name.clone(), m.level, m.rank == guild::rank::MASTER)).collect();
        BattlePicker { accept, count, rows, cursor: None, first: 0 }
    }

    /// Noms cochés, dans l'ordre de la liste.
    fn chosen(&self) -> Vec<String> {
        self.rows.iter().filter(|r| r.2).map(|r| r.0.clone()).collect()
    }

    /// Tirage au sort (`OP_GuildBattleListRandomSelect`) : le chef reste, les autres places au hasard.
    fn random(&mut self, master: &str, seed: u64) {
        let mut x = seed | 1;
        let mut others: Vec<usize> = (0..self.rows.len()).filter(|i| !self.rows[*i].0.eq_ignore_ascii_case(master)).collect();
        for i in (1..others.len()).rev() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            others.swap(i, (x % (i as u64 + 1)) as usize);
        }
        let need = (self.count.max(0) as usize).saturating_sub(self.rows.iter().filter(|r| r.0.eq_ignore_ascii_case(master)).count());
        for r in self.rows.iter_mut() {
            r.2 = r.0.eq_ignore_ascii_case(master);
        }
        for i in others.into_iter().take(need) {
            self.rows[i].2 = true;
        }
    }
}

impl GuildGlue {
    fn data(&mut self, assets: &Assets) -> &GuildData {
        self.data.get_or_insert_with(|| load_data(assets))
    }

    /// Lignes de `Guild_List` : la page du classement, les alliées ou les hostiles.
    fn list_entries(&self) -> Vec<guild::GuildListEntry> {
        match self.list_mode {
            1 => self.state.joint_details.clone(),
            2 => self.state.hostile_details.clone(),
            _ => self.state.list.as_ref().map(|l| l.entries.clone()).unwrap_or_default(),
        }
    }

    fn list_picked(&self) -> Option<String> {
        self.list_selected.and_then(|i| self.list_entries().get(i).map(|e| e.name.clone()))
    }
}

fn send(net: Option<&NetLink>, cmd: GuildCommand) {
    if let Some(net) = net {
        net.send(Command::Guild(cmd));
    }
}

/// Nom affiché d'un membre : la suite d'un `@` devient « ... » (FUN_1408bfe60).
fn shown_name(name: &str) -> String {
    match name.split_once('@') {
        Some((head, _)) => format!("{head}..."),
        None => name.to_string(),
    }
}

/// Remplace les `{{…}}` d'un message d'événement par `value`.
pub(super) fn fill_braces(text: &str, value: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find("{{") {
        out.push_str(&rest[..i]);
        match rest[i..].find("}}") {
            Some(j) => {
                out.push_str(value);
                rest = &rest[i + j + 2..];
            }
            None => {
                rest = &rest[i..];
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Remplace dans l'ordre les `%s` et `%d` d'un texte d'Archonia.
fn printf(fmt: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::new();
    let mut it = args.iter();
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' && matches!(chars.peek(), Some('s' | 'd')) {
            chars.next();
            if let Some(a) = it.next() {
                out.push_str(&a.to_string());
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Message d'un code de guilde (op 10) : texte et couleur ARGB (FUN_1406a44e0) ; `None` = rien.
fn system_text(d: &GuildData, code: i32, text: &str, number: i64, number2: i64) -> Option<(String, u32)> {
    let white = 0xFFFF_FFFF;
    let guild_color = d.color("Guild_System_Message_Color", 0xFFFF_FF33);
    let key = match code {
        0 => "Guild_Msg_ExistGuildID",
        1 => "Guild_Msg_NeedMoreLevel",
        2 => return Some((printf(&d.msg("Guild_Msg_NeedMoreMoney"), &[&number]), white)),
        4 => "Guild_Msg_UseSpecialChar",
        5 => "Guild_Msg_AlreadyMember",
        6 => "Guild_Msg_MaxMember",
        7 => "Guild_Msg_JoinFail",
        10 => "Guild_Msg_InvalidPassword",
        11 => "Guild_Msg_CreateComplete",
        12 => "Guild_Msg_Destroy",
        13 => return Some((printf(&d.msg("Guild_Msg_Join"), &[&text]), white)),
        14 => "Guild_Msg_JoinReject",
        15 => return Some((printf(&d.msg("Guild_Msg_Leave"), &[&text]), white)),
        16 => return Some((printf(&d.msg("Guild_Msg_ForcedLeave"), &[&text]), white)),
        17 => "Guild_Msg_AlreadyMember2",
        18 => "Guild_Msg_ForcedLeave2",
        // Invité qui refuse les invitations de guilde (options) : couleur `Negative_Text_Color`.
        42 => return Some((printf(&d.msg("Option_Guild_Off_User"), &[&text]), d.color("Negative_Text_Color", 0xFFFF_0000))),
        44 => "Guild_Not_Enought_Gheld",
        45 => "Guild_Not_Enought_Skull",
        // Combats de guildes (FUN_1406a44e0) : le code 22 lit le second nombre.
        19 => return Some((printf(&d.msg("Guild_Msg_Battle_Both_Member_Count"), &[&number]), white)),
        20 => return Some((printf(&d.msg("Guild_Msg_Battle_Not_Enough_Member"), &[&number]), white)),
        22 => return Some((printf(&d.msg("Guild_Msg_Battle_Not_Enough_Level"), &[&number2]), white)),
        21 | 23 | 25 => "Guild_Msg_Battle_Not_Request_Status2",
        24 => "Guild_Msg_Battle_Not_Request_Status",
        26 => "Guild_Msg_Battle_Master_Offline",
        27 => "Guild_Msg_Battle_Not_Exist_Guild",
        28 => "Guild_Msg_Battle_Request",
        30 => "Guild_Msg_Battle_Reject_By_Other",
        31 => return Some((printf(&d.msg("Guild_Msg_Battle_Cancel_Passed_Time"), &[&(number / 60)]), white)),
        33 => "Guild_Msg_Battle_Cancel_Reject_By_Other",
        35 => "Guild_Msg_Battle_Cannot_Request_GG",
        41 => "Guild_Msg_Battle_Not_Usable",
        43 => return Some((printf(&d.msg("Option_GBattle_Off_User"), &[&text]), d.color("Negative_Text_Color", 0xFFFF_0000))),
        47 => return Some((d.msg("Guild_No_Exist_Search_Guild"), guild_color)),
        // Alliances et hostilités : 0x41 et 0x4f écrivent 20 et 10, 0x48 et 0x4a le nom de la guilde.
        0x3c => "Guild_Joint_Disable",
        0x3d => "Guild_Joint_Not_Leader",
        0x3e => "Guild_Joint_Already_Joint",
        0x3f => "Guild_Joint_Max",
        0x40 => "Guild_Joint_Maste_Offline",
        0x41 => return Some((printf(&d.msg("Guild_Joint_Not_Enough_Member"), &[&20]), white)),
        0x42 => "Guild_Joint_Other_Not_Enough_Member",
        0x43 => "Guild_Joint_Wait",
        0x44 => "Guild_Joint_Reject",
        0x45 => "Guild_Joint_Failure",
        0x46 => "Guild_Joint_Success",
        0x47 => "Guild_Joint_Leave_Success",
        0x48 => return Some((printf(&d.msg("Guild_Joint_Leave_Other_Guild"), &[&text]), white)),
        0x49 => "Guild_Joint_Destroy",
        0x4a => return Some((printf(&d.msg("Guild_Joint_Success2"), &[&text]), white)),
        0x4b => "Guild_Hostile_Disable",
        0x4c => "Guild_Hostile_Already_Hostile",
        0x4d => "Guild_Hostile_Max",
        0x4e => "Guild_Hostile_Master_Offline",
        0x4f => return Some((printf(&d.msg("Guild_Hostile_Not_Enough_Member"), &[&10]), white)),
        0x50 => "Guild_Hostile_Other_Not_Enough_Member",
        0x51 => "Guild_Hostile_Wait",
        0x52 => "Guild_Hostile_Reject",
        0x53 => "Guild_Hostile_Failure",
        0x54 => "Guild_Hostile_Success",
        0x55 => "Guild_Hostile_Leave_Disable",
        0x56 => "Guild_Hostile_Leave_Wait",
        0x57 => "Guild_Hostile_Leave_Reject",
        0x58 => "Guild_Hostile_Leave_Failure",
        0x59 => "Guild_Hostile_Leave_Success",
        48 => return Some((printf(&d.msg("Guild_Join_Request_Self"), &[&text]), guild_color)),
        50 => return Some((printf(&d.msg("Guild_Leave_Request_Self"), &[&text]), guild_color)),
        _ => return None,
    };
    Some((d.msg(key), white))
}

/// Titre d'un rang (FUN_1406a53a0).
fn rank_title(d: &GuildData, r: i32) -> String {
    match r {
        guild::rank::NORMAL => d.msg("Guild_Member"),
        guild::rank::MASTER => d.msg("Guild_Master"),
        guild::rank::SUBMASTER => d.msg("Guild_SubMaster"),
        _ => String::new(),
    }
}

/// Somme d'or avec une virgule tous les trois chiffres (FUN_14055cca0).
fn gold(n: i64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 && c.is_ascii_digit() {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Taille suivante (dizaine supérieure) et son prix, si `guildmaxmember.txt` la prévoit (FUN_1408ccfa0).
fn next_size(d: &GuildData, max: i64) -> (i64, Option<i64>) {
    let next = if max < 1 { 10 } else { (max / 10 + 1) * 10 };
    (next, d.sizes.iter().find(|(s, _)| *s == next).map(|(_, g)| *g))
}

/// Nom de race et de classe d'un modèle (clés de `uimessage.txt`).
fn race_class(assets: &Assets, d: &GuildData, tid: i32) -> (String, String) {
    let (race, class) = crate::login_settings::template_identity(assets, tid);
    let tr = |k: &str| d.messages.get(k).cloned().filter(|v| !v.is_empty()).unwrap_or_else(|| k.to_string());
    (tr(race), if class.is_empty() { String::new() } else { tr(&class) })
}

/// Ligne colorée pour la discussion (codes `|cffRRGGBB…|r`).
fn colored(text: &str, argb: u32) -> String {
    format!("|cff{:06x}{text}|r", argb & 0xFF_FFFF)
}

/// Fenêtres et boîte OK / Annuler (dans la passe egui) ; les clics sont traités par `guild_frame`.
pub fn panel(ctx: &egui::Context, glue: &mut GuildGlue, hud: &mut HudState, assets: &Assets, me: &str) {
    glue.data(assets);
    let money = hud.stats.money.unwrap_or(0);
    let mine = glue.state.member(me).cloned();
    let master = mine.as_ref().is_some_and(|m| m.rank == guild::rank::MASTER);
    let leads = mine.as_ref().is_some_and(GuildMember::leads);
    let info = glue.state.info.clone();
    let guild_name = info.as_ref().map(|i| i.name.clone()).unwrap_or_default();
    let rows = glue.state.members.clone();
    let selected = glue.selected.and_then(|i| rows.get(glue.page * PAGE + i).cloned());
    let (online, total) = glue.state.counts();
    let d = glue.data.take().expect("données lues");
    let colors = PageColors { selected: d.color("Guild_SelectPageColor", 0xFFFF_F0C8), other: d.color("Guild_DeselectPageColor", 0xFF80_8080) };
    let mut out = Vec::new();
    let has_sub = rows.iter().any(|m| m.rank == guild::rank::SUBMASTER);
    let (my_tid, my_color) = info.as_ref().map_or((0, 0), |i| (i.mark_tid as u32, i.mark_color as u32));
    let open: Vec<Win> = [
        Win::Main,
        Win::Create,
        Win::Destroy,
        Win::Notice,
        Win::Member,
        Win::List,
        Win::MemberSize,
        Win::Mark,
        Win::MarkConfirm,
        Win::GBattle,
        Win::BattleMember,
        Win::BattleResult,
        Win::CreateRequest,
        Win::CreateComplete,
        Win::DestroyConfirm,
        Win::DestroyComplete,
    ]
        .into_iter()
        .filter(|w| glue.open.contains(w))
        .collect();
    for win in open {
        let mut v = GuildView::default();
        match win {
            Win::Main => {
                // Prix de création avec la taxe de la région du joueur (`FUN_1408c6ad0`, voir `app_tax.rs`).
                let create_price = super::tax_glue::taxed(d.create_price);
                v.texts.insert("GD_Main_Edit_Info", printf(&d.msg("Guild_Main_Text"), &[&CREATE_LEVEL, &create_price]));
                if info.is_some() || money < create_price {
                    v.disabled.insert("GD_Main_Button_1");
                }
                if !(master && rows.len() == 1) {
                    v.disabled.insert("GD_Main_Button_2");
                }
                // « Emblème » et « Augmenter la limite » : Guild_Enable_Guild_Master. Changement de nom :
                // non géré par ArchCore.
                if !master {
                    v.disabled.extend(["GD_Main_Button_3", "CONTROL_29434"]);
                }
                v.disabled.insert("CONTROL_10138");
            }
            Win::Create => {
                v.texts.insert("Guild_Create_Edit_Info", d.msg("Guild_Create_Text"));
                v.edits.push(GuildEdit { control: "GD_Create_Edit_Name", text: &mut glue.name, password: false, max: 12, multiline: false });
                v.edits.push(GuildEdit { control: "GD_Create_Edit_PW", text: &mut glue.password, password: true, max: 12, multiline: false });
                v.edits.push(GuildEdit { control: "GD_Create_Edit_PW2", text: &mut glue.password2, password: true, max: 12, multiline: false });
            }
            Win::Destroy => {
                v.texts.insert("GD_Destroy_Edit_Info", d.msg("Guild_Destroy_Text"));
                v.texts.insert("GD_Destroy_Edit_GuildName", guild_name.clone());
                v.edits.push(GuildEdit { control: "GD_Destroy_Edit_PW", text: &mut glue.destroy_password, password: true, max: 12, multiline: false });
            }
            Win::CreateRequest => {
                let name = glue.pending_create.as_ref().map(|p| p.0.clone()).unwrap_or_default();
                v.texts.insert("GD_Create_Request_Edit", printf(&d.msg("Guild_Guild_Info"), &[&name, &me]));
                v.texts.insert("GD_Create_Request_Edit2", d.msg("Guild_Create_Confirm_Text"));
            }
            Win::CreateComplete => {
                let n = &glue.complete_name;
                v.texts.insert("GD_Create_Complete_Edit", printf(&d.msg("Guild_Create_Complete_Text"), &[n, n, &me]));
            }
            Win::DestroyConfirm => {
                v.images = mark_images(&d, my_tid, my_color, ["CONTROL_24108", "CONTROL_28196", "CONTROL_11609"], false);
                v.texts.insert("GD_Destroy_Confirm_Edit", printf(&d.msg("Guild_Guild_Info"), &[&guild_name, &me]));
                v.texts.insert("GD_Destroy_Confirm_Edit2", d.msg("Guild_Destroy_Confirm_Text"));
            }
            Win::DestroyComplete => {
                v.texts.insert("GD_Destroy_Complete_Edit", printf(&d.msg("Guild_Destroy_Complete_Text"), &[&glue.complete_name, &me]));
            }
            Win::Notice => {
                v.active.insert("GD_Notice_Button_Tab1");
                v.disabled.insert("GD_Notice_Button_GvGTournament");
                v.images = mark_images(&d, my_tid, my_color, ["CONTROL_3065", "CONTROL_30928", "CONTROL_19292"], false);
                if info.is_none() {
                    v.disabled.insert("GD_Notice_Button_Tab2");
                }
                match &info {
                    Some(i) => {
                        v.texts.insert("GD_Notice_Edit_Guild", String::new());
                        v.texts.insert("GD_Notice_Edit_Member", printf(&d.msg("Guild_Notice_Info"), &[&i.name, &i.master, &total, &i.max_members, &online]));
                        if master {
                            v.edits.push(GuildEdit { control: "GD_Notice_Notice", text: &mut glue.notice, password: false, max: 84, multiline: true });
                        } else {
                            let n = if i.notice.chars().count() > 89 { String::new() } else { i.notice.clone() };
                            v.texts.insert("GD_Notice_Notice", n);
                        }
                    }
                    None => {
                        v.texts.insert("GD_Notice_Edit_Guild", d.msg("Guild_Notice_Detail_NoGuildMember"));
                        v.texts.insert("GD_Notice_Edit_Member", String::new());
                        v.texts.insert("GD_Notice_Notice", String::new());
                    }
                }
                if !master {
                    v.disabled.insert("GD_Notice_Button_Send");
                }
                if mine.as_ref().is_none_or(|m| m.rank == guild::rank::LEAVE_REQUEST) {
                    v.disabled.insert("CONTROL_18715");
                }
                v.texts.insert("GD_Notice_Edit_Member_UD", String::new());
            }
            Win::Member => {
                v.active.insert("GD_Member_Button_Tab2");
                v.disabled.insert("GD_Member_Button_GvGTournament");
                v.list = Some(ListSpec::MEMBERS);
                let pages = rows.len().div_ceil(PAGE).max(1);
                v.pages = pages;
                v.page = glue.page.min(pages - 1);
                for m in rows.iter().skip(v.page * PAGE).take(PAGE) {
                    let (race, class) = race_class(assets, &d, m.tid);
                    let level = match m.rank {
                        guild::rank::JOIN_REQUEST => d.msg("Guild_Member_Join_Request"),
                        guild::rank::LEAVE_REQUEST => d.msg("Guild_Member_Leave_Request"),
                        _ => m.level.to_string(),
                    };
                    v.rows.push(GuildRow { cols: vec![shown_name(&m.name), level, class, race], online: Some(m.online) });
                }
                v.selected = glue.selected;
                v.texts.insert("CONTROL_2357", pages.to_string());
                // Détail et boutons selon le membre choisi (FUN_1408c10f0, FUN_1408bfa80).
                let mut detail = String::new();
                let mut allow = d.msg("Guild_Join_Allow_Button_Text");
                let (mut kick, mut invite, mut allow_on, mut whisper, mut appoint, mut succession) = (false, false, false, false, false, false);
                if let Some(s) = &selected {
                    let (race, class) = race_class(assets, &d, s.tid);
                    detail = match s.rank {
                        guild::rank::MASTER | guild::rank::SUBMASTER => {
                            let title = d.msg(if s.rank == guild::rank::MASTER { "Guild_Master" } else { "Guild_SubMaster" });
                            printf(&d.msg("Guild_GuildLeadMemberInfo"), &[&shown_name(&s.name), &s.level, &title, &race, &class])
                        }
                        _ => printf(&d.msg("Guild_GuildMemberInfo"), &[&shown_name(&s.name), &s.level, &race, &class]),
                    };
                    kick = master;
                    if leads && matches!(s.rank, guild::rank::JOIN_REQUEST | guild::rank::LEAVE_REQUEST) {
                        allow_on = !(s.rank == guild::rank::LEAVE_REQUEST && !master);
                    }
                    if s.rank == guild::rank::LEAVE_REQUEST {
                        allow = d.msg("Guild_Leave_Allow_Button_Text");
                    }
                    invite = s.online && !s.name.eq_ignore_ascii_case(&me);
                    // Guild_Enable_Chat_Whisper : membre en ligne, sans autre condition (FUN_1408c10f0).
                    whisper = s.online;
                    // Guild_Enable_Appoint_Button / Guild_Enable_Succession_Button (FUN_1408c10f0) : le chef,
                    // sur un membre de niveau 20 (en ligne et sans second dans la guilde, ou le second
                    // lui-même) ; la passation sur le second en ligne.
                    let not_master = s.rank != guild::rank::MASTER;
                    appoint = master && not_master && s.level >= 20 && ((!has_sub && s.online) || s.rank == guild::rank::SUBMASTER);
                    succession = master && s.rank == guild::rank::SUBMASTER && s.online;
                }
                v.texts.insert("CONTROL_5979", detail);
                v.texts.insert("CONTROL_23938", allow);
                // Guild_AppointAndCancel_BtnText : sysstr 0x303 « Nommer », 0x304 s'il y a un second.
                v.texts.insert("CONTROL_31321", super::npc_glue::sysstr(assets, 0x303 + has_sub as u32).unwrap_or_default());
                for (on, c) in [
                    (kick, "GD_Member_Button_Leave"),
                    (invite, "CONTROL_14314"),
                    (allow_on, "CONTROL_23938"),
                    (whisper, "CONTROL_24708"),
                    (appoint, "CONTROL_31321"),
                    (succession, "CONTROL_3394"),
                ] {
                    if !on {
                        v.disabled.insert(c);
                    }
                }
                v.edits.push(GuildEdit { control: "CONTROL_12329", text: &mut glue.search, password: false, max: 48, multiline: false });
            }
            Win::List => {
                v.active.insert("CONTROL_12509");
                v.list = Some(ListSpec::GUILDS);
                if info.is_none() {
                    v.disabled.insert("CONTROL_26096");
                }
                // Tournoi de guildes : pas repris.
                v.disabled.insert("GD_Ranking_Button_GvGTournament");
                let entries = glue.list_entries();
                let (total, page) = match (glue.list_mode, &glue.state.list) {
                    (0, Some(l)) => (l.total, l.page),
                    _ => (entries.len() as i32, 0),
                };
                let pages = ((total.max(1) - 1) / PAGE as i32 + 1) as usize;
                v.pages = pages;
                v.page = (page.max(0) as usize).min(pages - 1);
                for e in &entries {
                    v.rows.push(GuildRow { cols: vec![shown_name(&e.name), e.members.to_string(), e.ranking.to_string()], online: None });
                }
                v.selected = glue.list_selected.filter(|i| *i < entries.len());
                v.texts.insert("CONTROL_22866", if total > 0 { format!("{}/{}", v.page + 1, pages) } else { String::new() });
                let picked = v.selected.and_then(|i| entries.get(i));
                // Boutons d'alliance et d'hostilité (FUN_1408cac30, FUN_1408cabb0) : libellé selon la liste
                // affichée ; le chef, sur une autre guilde (ou, dans les alliés, pour quitter l'alliance).
                let mode = glue.list_mode;
                v.texts.insert("CONTROL_19137", d.msg(if mode == 1 { "Guild_Joint_Leave_Button_Text" } else { "Guild_Joint_Request_Button_Text" }));
                v.texts.insert("CONTROL_18911", d.msg(if mode == 2 { "Guild_Hostile_Leave_Button_Text" } else { "Guild_Hostile_Request_Button_Text" }));
                let picked_other = picked.is_some_and(|e| !e.name.eq_ignore_ascii_case(&guild_name));
                if !(master && ((mode == 0 && picked_other) || (mode == 1 && !glue.state.joints.is_empty()))) {
                    v.disabled.insert("CONTROL_19137");
                }
                if !(master && picked_other && mode != 1) {
                    v.disabled.insert("CONTROL_18911");
                }
                if info.is_none() {
                    v.disabled.extend(["CONTROL_24849", "CONTROL_20569"]);
                }
                let detail = picked.map(|e| printf(&d.msg("Guild_GuildDetailInfo"), &[&e.name, &e.level, &e.master, &e.submaster, &e.members, &e.max_members])).unwrap_or_default();
                v.texts.insert("CONTROL_18338", detail);
                // Emblème de la guilde choisie (FUN_1408c97e0).
                if let Some(e) = picked {
                    v.images = mark_images(&d, e.mark_tid as u32, e.mark_color as u32, ["CONTROL_4449", "CONTROL_30582", "CONTROL_8644"], false);
                }
                // Guild_List_BattleJoin : « Demander la guerre » dans une guilde (le chef, sur une autre guilde,
                // hors combat), « Rejoindre » sinon.
                v.texts.insert("CONTROL_6490", d.msg(if info.is_some() { "Guild_List_Battle_Button_Text" } else { "Guild_List_Join_Request_Button_Text" }));
                let other = picked.is_some_and(|e| !e.name.eq_ignore_ascii_case(&guild_name));
                if picked.is_none() || (info.is_some() && !(master && other && mode == 0 && glue.state.status == guild::battle::status::NONE)) {
                    v.disabled.insert("CONTROL_6490");
                }
                v.edits.push(GuildEdit { control: "CONTROL_12575", text: &mut glue.list_search, password: false, max: 32, multiline: false });
            }
            Win::MemberSize => {
                let max = info.as_ref().map_or(0, |i| i.max_members as i64);
                v.texts.insert("CONTROL_16864", printf(&d.msg("Guild_CurrentMaxMember"), &[&max]));
                let (next, price) = next_size(&d, max);
                let text = match price {
                    Some(p) => printf(&d.msg("Guild_NextMaxMember"), &[&next, &gold(p)]),
                    None => d.msg("Guild_CanNot_Increase_MaxMember"),
                };
                v.texts.insert("CONTROL_22644", text);
                // Guild_Enable_Inc_Max_Member : chef, or suffisant (crânes : aucun dans la table).
                if !(master && price.is_some_and(|p| money >= p)) {
                    v.disabled.insert("CONTROL_10606");
                }
            }
            Win::Mark => {
                let layer = glue.mark_layer.min(2);
                v.active.insert(MARK_TABS[layer]);
                let l = layer as u8 + 1;
                let mut rows: Vec<&MarkRow> = d.marks.iter().filter(|m| m.layer == l).collect();
                rows.sort_by_key(|m| m.number);
                let current = mark_part(glue.mark_tid, l);
                for (slot, m) in rows.iter().skip(glue.mark_scroll * MARK_COLS).take(MARK_COLS * MARK_ROWS).enumerate() {
                    let texture = format!("GM_{}S{:02}.png", MARK_LAYERS[layer], m.number);
                    v.cells.push(GridCell { control: MARK_GRID, slot, id: m.tid() as usize, texture, tint: 0xFFFF_FFFF, selected: m.tid() == current });
                }
                let colors: Vec<&MarkRow> = d.marks.iter().filter(|m| m.layer == 4).collect();
                for (slot, m) in colors.iter().enumerate() {
                    let texture = format!("GM_ColorS{:02}.png", m.number);
                    v.cells.push(GridCell { control: MARK_COLORS, slot, id: m.number as usize, texture, tint: 0xFFFF_FFFF, selected: color_index(glue.mark_color, l) == m.number });
                }
                v.images = mark_images(&d, glue.mark_tid, glue.mark_color, ["CONTROL_29950", "CONTROL_4465", "CONTROL_2086"], false);
                v.images.extend(mark_images(&d, glue.mark_tid, glue.mark_color, ["CONTROL_31034", "CONTROL_10592", "CONTROL_15384"], true));
                // Guild_Mark_DetailInfo (FUN_14055b200) : « couche : nom » de chaque élément, puis le prix.
                let (parts, price, cash) = mark_parts(&d, glue.mark_tid);
                let mut detail: Vec<String> = parts
                    .iter()
                    .map(|m| format!(" {} : {}", super::npc_glue::sysstr(assets, 0x20b + m.layer as u32 - 1).unwrap_or_default(), m.name))
                    .collect();
                if price > 0 {
                    detail.push(format!("[{}]", super::npc_glue::sysstr(assets, 0x20e).unwrap_or_default()));
                    detail.push(format!(" {} {}", gold(price), super::npc_glue::sysstr(assets, 10).unwrap_or_default()));
                }
                if cash > 0 {
                    detail.push(format!(" {} {}", gold(cash), super::npc_glue::sysstr(assets, 10).unwrap_or_default()));
                }
                v.texts.insert("CONTROL_7757", detail.join("___"));
                // Guild_Enable_Buy_GuildMark : le chef, un emblème différent de celui de la guilde, payable.
                if !(master && glue.mark_tid != my_tid && money >= price) {
                    v.disabled.insert("CONTROL_5906");
                }
            }
            Win::MarkConfirm => {
                v.images = mark_images(&d, glue.mark_tid, glue.mark_color, ["GUILDMARK_GRID_BOTTOM", "GUILDMARK_GRID_PATTERN", "GUILDMARK_GRID_SYMBOL"], false);
            }
            // Guild_GBattle (FUN_14084de70) : type, durée, effectif (types 3 et 4) ; lecture seule pour un défi
            // reçu.
            Win::GBattle => {
                let f = &glue.battle_form;
                v.texts.insert("CONTROL_4060", d.msg(guild::battle::KINDS[f.kind.min(4)]));
                v.texts.insert("CONTROL_17192", guild::battle::MINUTES[f.time.min(5)].to_string());
                let men = f.men();
                v.texts.insert("CONTROL_21248", men.get(f.man).map(|n| n.to_string()).unwrap_or_default());
                if men.is_empty() {
                    v.disabled.extend(["CONTROL_21248", "CONTROL_12900"]);
                }
                if f.received {
                    v.disabled.extend(["CONTROL_4060", "CONTROL_29955", "CONTROL_17192", "CONTROL_6484", "CONTROL_21248", "CONTROL_12900", "CONTROL_12454"]);
                }
            }
            // UI_GuildBattleMember : numéro, nom, niveau, coché ; `GuildBattleMember_MemberSelect` (effectif).
            Win::BattleMember => {
                let Some(p) = &glue.picker else { continue };
                v.list = Some(ListSpec::BATTLE_MEMBERS);
                v.texts.insert("CONTROL_8021", printf(&d.msg("GuildBattleMember_MemberSelect"), &[&p.count]));
                v.rows = p.rows.iter().enumerate().skip(p.first).take(BATTLE_ROWS).map(|(i, (name, level, on))| GuildRow { cols: vec![(i + 1).to_string(), name.clone(), level.to_string()], online: Some(*on) }).collect();
                v.selected = p.cursor.and_then(|c| c.checked_sub(p.first)).filter(|c| *c < BATTLE_ROWS);
            }
            // UI_GuildBattleResult : sa guilde à gauche, l'adverse à droite, colonnes de points selon le type
            // (`GuildBattleResult_Point`, ou `GuildBattleResult_Kill/Dead` hors combat aux points), totaux = scores.
            Win::BattleResult => {
                v.list = Some(ListSpec::BATTLE_RESULT);
                let b = glue.state.battle.clone().or_else(|| glue.last_battle.clone()).unwrap_or_default();
                let points_kind = b.kind == 0;
                let cell = |f: &BattleFighter| if points_kind { f.points.to_string() } else { format!("{}/{}", f.b, f.c) };
                v.texts.insert("GuildScoreInfo", d.msg(guild::battle::KINDS[(b.kind.max(0) as usize).min(4)]));
                v.texts.insert("CONTROL_27421", guild_name.clone());
                v.texts.insert("CONTROL_9089", b.enemy.clone());
                let head = d.msg(if points_kind { "GuildBattleResult_Point" } else { "GuildBattleResult_Kill/Dead" });
                v.texts.insert("CONTROL_29653", head.clone());
                v.texts.insert("CONTROL_6008", head);
                v.texts.insert("CONTROL_27273", b.score.to_string());
                v.texts.insert("CONTROL_4379", b.enemy_score.to_string());
                v.texts.insert("GuildScoreWinInfo", String::new());
                v.texts.insert("GuildSroceLoseInfo", String::new());
                let (left, right) = &glue.fighters;
                let n = left.len().max(right.len());
                v.rows = (glue.result_first..n)
                    .take(BATTLE_ROWS)
                    .map(|i| {
                        let l = left.get(i);
                        let r = right.get(i);
                        GuildRow { cols: vec![l.map(|f| f.name.clone()).unwrap_or_default(), l.map(cell).unwrap_or_default(), r.map(|f| f.name.clone()).unwrap_or_default(), r.map(cell).unwrap_or_default()], online: None }
                    })
                    .collect();
            }
        }
        crate::hud::draw_guild_window(ctx, hud, assets, win.name(), v, &colors, &mut out);
    }
    // Combat de guildes : compte à rebours, adversaire, scores (FUN_1408505a0, FUN_140850160).
    let mut lines: Vec<(String, u32, f32)> = Vec::new();
    if let Some(b) = &glue.state.battle {
        let left = b.remaining(glue.state.status);
        let clock = if left < 60 {
            let tenth = 9 - (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_millis()) / 100);
            format!("{left:02}.{tenth}")
        } else {
            format!("{}:{:02}", left / 60, left % 60)
        };
        let sys = |n: u32| super::npc_glue::sysstr(assets, n).unwrap_or_default();
        match glue.state.status {
            guild::battle::status::READY => {
                lines.push((format!("{} {clock} {}", sys(0x153), sys(0x154)), 0xFF_FFFF, 200.0));
                lines.push((format!("vs {}", b.enemy), 0xFF_FFFF, 230.0));
            }
            guild::battle::status::BATTLE => {
                lines.push((format!("vs {}[{}] {clock}", b.enemy, b.enemy_master), 0xFF_FFFF, 230.0));
                lines.push((format!("{} : {}{}", sys(0x155), b.score, if b.up_score > 0 { format!("  +{}", b.up_score) } else { String::new() }), 0x1C_41FF, 280.0));
                lines.push((format!("{} : {}{}", sys(0x156), b.enemy_score, if b.enemy_up_score > 0 { format!("  +{}", b.enemy_up_score) } else { String::new() }), 0xFF_0606, 325.0));
            }
            _ => {}
        }
    }
    glue.battle_notice = glue.battle_notice.take().filter(|(_, _, until)| std::time::Instant::now() < *until);
    if !lines.is_empty() || glue.battle_notice.is_some() {
        crate::hud::draw_battle_texts(ctx, &lines, glue.battle_notice.as_ref().map(|(t, c, _)| (t.as_str(), *c)));
    }
    // Entrepôt de guilde : le chef retire de l'argent (droit 8) ; l'agrandissement n'est jamais payable
    // (FUN_1405fc200 : 999 999 999 999 pièces).
    if glue.gware.open {
        let master = glue.state.member(me).is_some_and(|m| m.rank == guild::rank::MASTER);
        let rights = if master { guild::warehouse::RIGHTS_MASTER } else { guild::warehouse::RIGHTS_MEMBER };
        let pages = (1 + glue.gware.slots.clamp(0, 3)) as i16;
        let g = &mut glue.gware;
        let view = GwareView { items: &g.items, money: g.money, pages, page: &mut g.page, money_out: rights & guild::warehouse::RIGHT_MONEY_OUT != 0, expand: false };
        crate::hud::draw_guild_warehouse(ctx, hud, assets, view, &mut out);
    }
    if let Some((deposit, mut amount)) = glue.gware.money_ask {
        let max = if deposit { hud.stats.money.unwrap_or(0) } else { glue.gware.money };
        let answer = crate::hud::draw_gware_money(ctx, hud, assets, deposit, &mut amount, max);
        glue.gware.money_ask = Some((deposit, amount.clamp(0, max.max(0))));
        // GWARE_MONEY_CONF (FUN_1405fbb90) : op 3 ou 4 avec le montant, passé à `guild_click`.
        if let Some(ok) = answer {
            glue.gware.money_ask = None;
            if ok && amount > 0 {
                out.push(GuildUi::Click { window: "UI_GWareConfirm".into(), control: format!("{}:{amount}", if deposit { "in" } else { "out" }) });
            }
        }
    }
    if let Some(text) = glue.info.clone() {
        if crate::hud::info_box(ctx, hud, assets, &text) {
            glue.info = None;
        }
    }
    if let Some((_, text)) = &glue.ask {
        crate::hud::draw_message_box(ctx, hud, assets, text, &d.msg("UI_Name_OK"), &d.msg("UI_Name_Cancel"), &mut out);
    }
    glue.data = Some(d);
    glue.clicks.extend(out);
}

/// Demandes du sac pendant que l'entrepôt est ouvert (avant celles de la banque) : un objet de l'entrepôt
/// lâché dans le sac est repris (op 6) ; le clic droit sur un objet du sac le dépose dans la première case
/// libre de la page affichée.
pub fn item_requests(glue: &GuildGlue, requests: &mut Vec<ItemUi>, net: Option<&NetLink>) {
    if !glue.gware.open {
        return;
    }
    let w = &glue.gware;
    requests.retain(|r| match *r {
        ItemUi::DropOnBag { drag, grid } if drag.bank && w.items.iter().any(|i| i.iid == drag.iid) => {
            send(net, GuildCommand::WarehouseTake { iid: drag.iid, grid: Some(grid) });
            false
        }
        ItemUi::Activate { iid, worn: false } => {
            let (_, rows, cols) = guild::warehouse::GRID;
            let free = (0..rows).flat_map(|r| (0..cols).map(move |c| (r, c))).map(|(r, c)| [w.page, r, c]).find(|g| !w.items.iter().any(|i| i.grid == *g));
            match free {
                Some(grid) => send(net, GuildCommand::WarehouseStore { iid, grid }),
                None => log::info!("[entrepôt] page {} pleine", w.page + 1),
            }
            false
        }
        _ => true,
    });
}

impl App {
    fn my_name(&self) -> String {
        self.player.as_ref().map(|p| p.name.clone()).unwrap_or_default()
    }

    fn my_member(&self) -> Option<&GuildMember> {
        let me = self.my_name();
        self.guild.state.member(&me)
    }

    fn guild_message(&mut self, text: &str, argb: u32) {
        if text.is_empty() {
            return;
        }
        self.hud.push_system(&text.replace("___", " "));
        self.ui.push_chat_in(MASK_GUILD, colored(&text.replace("___", " "), argb));
    }

    /// Paquets de guilde reçus.
    pub(super) fn on_guild_event(&mut self, ev: GuildEvent) {
        let assets = self.assets.clone();
        self.guild.data(&assets);
        let me = self.my_name();
        let my_cid = self.player.as_ref().map_or(0, |p| p.cid);
        match &ev {
            GuildEvent::ManagerGranted { .. } => {
                self.guild.open.insert(Win::Main);
                return;
            }
            // Accord du gardien (FUN_1405fc020) : point d'ouverture retenu, puis l'op 1.
            // Les objets d'une ouverture précédente sont retirés (FUN_14030b630), le serveur les renvoie.
            GuildEvent::WarehouseGranted { npc } => {
                self.items.retain(|_, i| i.status != Some(guild::warehouse::ITEM_STATUS));
                self.guild.gware.npc = Some(*npc);
                self.guild.gware.at = self.player.as_ref().map(|p| (p.position.x, p.position.z));
                send(self.net.as_ref(), GuildCommand::WarehouseOpen);
                return;
            }
            GuildEvent::Warehouse { op, money, slots, result } => {
                let w = &mut self.guild.gware;
                match *op {
                    guild::warehouse::INFO => {
                        if *money >= 0 {
                            w.money = *money;
                        }
                        w.slots = *slots;
                    }
                    guild::warehouse::OPEN if *result == guild::warehouse::RESULT_OPEN => {
                        w.open = true;
                        w.page = 0;
                    }
                    guild::warehouse::EXPAND if *result == guild::warehouse::RESULT_NOT_ENOUGH_MONEY => {
                        let assets = self.assets.clone();
                        self.guild.info = Some(crate::hud::ui_message(&mut self.hud, &assets, "GWARE_NOT_ENOUGH_MONEY"));
                    }
                    _ => {}
                }
                return;
            }
            // Une autre guilde a le même emblème (code 59) : Guild_GuildMark_Duplicate, OK → op 33.
            GuildEvent::System { code: guild::code::BUY_GUILDMARK_DUPLICATE, .. } => {
                let text = self.guild.data.as_ref().and_then(|d| d.events.get("Guild_GuildMark_Duplicate").cloned()).unwrap_or_default();
                self.guild.ask = Some((Ask::MarkForce, text));
                return;
            }
            // Emblème acheté (FUN_14055b760) : le chef voit Guild_GuildMark_BuyComplete.
            GuildEvent::MarkBought { guild, .. } => {
                let mine = self.guild.state.name().eq_ignore_ascii_case(guild);
                if mine && self.my_member().is_some_and(|m| m.rank == guild::rank::MASTER) {
                    self.guild.open.remove(&Win::MarkConfirm);
                    self.guild.info = self.guild.data.as_ref().and_then(|d| d.events.get("Guild_GuildMark_BuyComplete").cloned());
                }
            }
            // Dissolution trop tôt (code 46, FUN_1406a44e0) : boîte `Guild_DestroyFailTooEarly` (`Action=4:0`).
            GuildEvent::System { code: guild::code::DESTROY_FAIL_TOO_EARLY, .. } => {
                self.guild.pending_destroy = None;
                self.guild.info = self.guild.data.as_ref().and_then(|d| d.events.get("Guild_DestroyFailTooEarly").cloned());
                return;
            }
            GuildEvent::System { code, text, number, number2, .. } => {
                let d = self.guild.data.as_ref().expect("données lues");
                if let Some((t, color)) = system_text(d, *code, text, *number, *number2) {
                    self.guild_message(&t, color);
                }
                return;
            }
            GuildEvent::Guild(info) => {
                // Création confirmée : la guilde demandée, avec le joueur pour chef (FUN_1406a36b0).
                if self.guild.pending_create.as_ref().is_some_and(|(n, _)| n.eq_ignore_ascii_case(&info.name)) && info.master.eq_ignore_ascii_case(&me) {
                    self.guild.pending_create = None;
                    self.guild.complete_name = info.name.clone();
                    self.guild.open.insert(Win::CreateComplete);
                }
            }
            GuildEvent::Destroyed { guild } => {
                if self.guild.pending_destroy.take().is_some() {
                    self.guild.complete_name = guild.clone();
                    self.guild.open.insert(Win::DestroyComplete);
                }
            }
            // Nomination proposée (FUN_1408c0200) : « %s de la guilde [%s] nomme %s %s. Acceptez-vous ? ».
            GuildEvent::AppointAsk { guild, master, member, rank } => {
                let d = self.guild.data.as_ref().expect("données lues");
                let text = printf(&d.msg("GuildAppointConfirm_Message"), &[master, guild, member, &rank_title(d, guild::rank::SUBMASTER)]);
                self.guild.ask = Some((Ask::AppointAnswer { guild: guild.clone(), rank: *rank }, text));
                return;
            }
            // Passation proposée (FUN_1408c09e0).
            GuildEvent::SuccessionAsk { guild, master, member } => {
                let d = self.guild.data.as_ref().expect("données lues");
                let text = printf(&d.msg("GuildSuccessionConfirm_Message"), &[master, guild, member]);
                self.guild.ask = Some((Ask::SuccessionAnswer { guild: guild.clone() }, text));
                return;
            }
            // Rang changé (FUN_1408c06f0) : message système, puis rang mis à jour.
            GuildEvent::RankChanged { guild, member, rank } => {
                let d = self.guild.data.as_ref().expect("données lues");
                let text = printf(&d.msg("GuildMemberRankUpdate_Message"), &[member, guild, &rank_title(d, *rank)]);
                self.guild_message(&text, 0xFFFF_FFFF);
            }
            GuildEvent::SocietyText(text) => {
                let text = text.clone();
                self.guild_message(&text, 0xFFFF_FFFF);
                return;
            }
            GuildEvent::Battle { op, battle, .. } => self.guild_battle_event(*op, battle),
            // Op 26 (FUN_14084f200) : une ligne, à gauche pour sa guilde, à droite sinon.
            GuildEvent::BattleFighter { guild, fighter } => {
                let mine = self.guild.state.name().eq_ignore_ascii_case(guild);
                let side = if mine { &mut self.guild.fighters.0 } else { &mut self.guild.fighters.1 };
                match side.iter_mut().find(|f| f.name.eq_ignore_ascii_case(&fighter.name)) {
                    Some(f) => (f.points, f.b, f.c) = (fighter.points, fighter.b, fighter.c),
                    None => side.push(fighter.clone()),
                }
                return;
            }
            // Op 27 (FUN_1404dee40) : listes entières ; le quatrième entier (-1) ne change pas l'ancien.
            GuildEvent::BattleFighters { guild, mine, enemy } => {
                let (l, r) = if self.guild.state.name().eq_ignore_ascii_case(guild) { (mine, enemy) } else { (enemy, mine) };
                merge_fighters(&mut self.guild.fighters.0, l);
                merge_fighters(&mut self.guild.fighters.1, r);
                return;
            }
            GuildEvent::HostileLeaveRejected { .. } => {
                let t = self.guild.data.as_ref().map(|d| d.msg("Guild_Hostile_Leave_Reject")).unwrap_or_default();
                self.guild_message(&t, 0xFFFF_FFFF);
                return;
            }
            // Demande reçue par le chef (FUN_1408cc440, FUN_1408cbb00, FUN_1408cba20) : rien s'il refuse les
            // relations de guilde (options, bit 0x40).
            GuildEvent::RelationAsk { kind, guild, other, master } => {
                let mine = self.guild.state.name().eq_ignore_ascii_case(guild) && self.my_member().is_some_and(|m| m.rank == guild::rank::MASTER);
                if mine && !self.options.refuse.guild_relation {
                    let (ask, event) = match kind {
                        guild::Relation::Joint => (Ask::JointAccept(other.clone()), "Guild_OpenJointAcceptConfirmUI"),
                        guild::Relation::Hostile => (Ask::HostileAccept(other.clone()), "Guild_OpenHostileAcceptConfirmUI"),
                        guild::Relation::HostileLeave => (Ask::HostileLeaveAccept(other.clone()), "Guild_OpenHostileLeaveAcceptConfirmUI"),
                    };
                    let d = self.guild.data.as_ref().expect("données lues");
                    let text = fill_named(d.events.get(event).map_or("", String::as_str), &[("Guild_RelationGuildID", other.clone()), ("Guild_RelationMasterID", master.clone())]);
                    self.guild.ask = Some((ask, text));
                }
                return;
            }
            GuildEvent::List(l) => {
                if l.page != self.guild.state.list.as_ref().map_or(-1, |x| x.page) || !l.searched.is_empty() {
                    self.guild.list_selected = None;
                }
                // Recherche : la ligne de la guilde trouvée est choisie.
                if !l.searched.is_empty() {
                    self.guild.list_selected = l.entries.iter().position(|e| e.name.eq_ignore_ascii_case(&l.searched));
                }
            }
            _ => {}
        }
        let notice = self.guild.state.apply(&ev, &me, my_cid);
        match notice.clone() {
            // Invitations de guilde bloquées (onglet Fonction des options) : déclinée sans boîte.
            Some(GuildNotice::Invited(i)) if self.options.refuse.guild => {
                log::info!("[guilde] invitation de « {} » déclinée (invitations de guilde bloquées)", i.guild);
                self.guild.state.invitation = None;
                send(self.net.as_ref(), GuildCommand::Reject { guild: i.guild });
            }
            Some(GuildNotice::Invited(i)) => {
                let d = self.guild.data.as_ref().expect("données lues");
                let text = fill_braces(d.events.get("Guild_Invite_ConfirmUI_Open").map_or("", String::as_str), &i.guild);
                self.guild.ask = Some((Ask::Invite { guild: i.guild }, text));
            }
            Some(GuildNotice::Left) => {
                self.guild.selected = None;
                self.guild.open.remove(&Win::Member);
            }
            Some(GuildNotice::Joined | GuildNotice::NoticeChanged) => {
                self.guild.notice = self.guild.state.info.as_ref().map(|i| i.notice.clone()).unwrap_or_default();
            }
            _ => {}
        }
        // Les AddOns voient la liste comme sous WoW.
        if notice.is_some() {
            self.addons.fire_event_str("GUILD_ROSTER_UPDATE", &[]);
        }
    }

    /// Combat de guildes reçu (avant sa prise en compte par `GuildState`).
    fn guild_battle_event(&mut self, o: i8, b: &GuildBattle) {
        let assets = self.assets.clone();
        let sys = |n: u32| super::npc_glue::sysstr(&assets, n).unwrap_or_default();
        let d = self.guild.data.as_ref().expect("données lues");
        let white = 0xFFFF_FFFF;
        let until = std::time::Instant::now() + std::time::Duration::from_millis(5000);
        match o {
            // Défi reçu (FUN_14084f310) : Guild_GBattle en lecture seule, puis la question.
            guild::op::BATTLE_REQUEST => {
                let kind = (b.kind.max(0) as usize).min(4);
                let time = guild::battle::MINUTES.iter().position(|m| m * 60 == b.duration).unwrap_or(0);
                let mut form = BattleForm { enemy: b.enemy.clone(), kind, time, man: 0, received: true };
                form.man = form.men().iter().position(|n| *n == b.person).unwrap_or(0);
                let text = fill_named(d.events.get("Guild_Battle_Accept_ConfirmUI_Open").map_or("", String::as_str), &form.named(d));
                self.guild.battle_form = form;
                self.guild.open.insert(Win::GBattle);
                self.guild.ask = Some((Ask::BattleAnswer(b.enemy.clone()), text));
            }
            // Acceptation (FUN_14084ebd0) : sysstr 0x151 en jaune 5 s, Guild_Msg_Battle_Accept (minutes).
            guild::op::BATTLE_ACCEPT => {
                let t = printf(&d.msg("Guild_Msg_Battle_Accept"), &[&(b.ready / 60)]);
                self.guild.battle_notice = Some((printf(&sys(0x151), &[&b.enemy]), 0xFFFF_FF00, until));
                self.guild.open.remove(&Win::GBattle);
                self.guild_message(&t, white);
            }
            // Demande d'annulation de l'adversaire.
            guild::op::BATTLE_CANCEL_REQUEST => {
                let form = BattleForm { enemy: b.enemy.clone(), ..self.guild.battle_form.clone() };
                let text = fill_named(d.events.get("Guild_Battle_Cancel_Accept_ConfirmUI_Open").map_or("", String::as_str), &form.named(d));
                self.guild.ask = Some((Ask::BattleCancelAnswer, text));
            }
            guild::op::BATTLE_CANCEL_ACCEPT => {
                let t = d.msg("Guild_Msg_Battle_Cancel_Accpet");
                self.guild_message(&t, white);
            }
            // Début (FUN_14084f610) : sysstr 0x152 en rouge 5 s, Guild_Msg_Battle_Start ; combattants remis à zéro.
            guild::op::BATTLE_START => {
                self.guild.fighters = Default::default();
                self.guild.result_first = 0;
                let t = d.msg("Guild_Msg_Battle_Start");
                self.guild.battle_notice = Some((sys(0x152), 0xFFFF_0000, until));
                self.guild_message(&t, white);
            }
            // Résultat (FUN_14084f400) : « %d-%d » des scores, ou victoire / défaite par abandon.
            guild::op::BATTLE_RESULT => {
                let key = match b.result {
                    1 => "Guild_Msg_Battle_Win2",
                    2 => "Guild_Msg_Battle_Draw2",
                    3 => "Guild_Msg_Battle_Lose2",
                    4 => "Guild_Msg_Battle_Win_By_GG2",
                    5 => "Guild_Msg_Battle_Lose_By_GG2",
                    _ => return,
                };
                let t = if b.result <= 3 { printf(&d.msg(key), &[&b.score, &b.enemy_score]) } else { d.msg(key) };
                self.guild.battle_notice = None;
                // FUN_1404ded30 : `UI_GuildBattleResult` avec le combat terminé (type et adversaire du combat en cours).
                let mut last = self.guild.state.battle.clone().unwrap_or_default();
                (last.score, last.enemy_score, last.result) = (b.score, b.enemy_score, b.result);
                if last.enemy.is_empty() {
                    last.enemy = b.enemy.clone();
                }
                self.guild.last_battle = Some(last);
                self.guild.open.insert(Win::BattleResult);
                self.guild_message(&t, white);
            }
            _ => {}
        }
    }

    /// Touche G et `/guilde` : `Guild_Notice_Open_Request` (FUN_1408c7f10).
    pub(super) fn guild_toggle(&mut self) {
        if self.guild.open.remove(&Win::Notice) | self.guild.open.remove(&Win::Member) {
            return;
        }
        self.guild.notice = self.guild.state.info.as_ref().map(|i| i.notice.clone()).unwrap_or_default();
        self.guild.open.insert(Win::Notice);
    }

    /// Discussion reçue sur un canal de guilde ou de groupe : ligne d'Archonia et masque d'onglet.
    pub(super) fn guild_chat_line(&mut self, channel: i8, from: &str, message: &str) -> Option<(String, u16)> {
        let assets = self.assets.clone();
        let d = self.guild.data(&assets);
        let (header, color, mask) = match channel {
            3 => (d.msg("Chat_Guild_Header"), 0xFFA500, MASK_GUILD),
            4 => (d.msg("Chat_Party_Header"), 0x7CFC00, MASK_PARTY),
            // Chuchotement reçu / envoyé : WHISPER_RECV / WHISPER_SEND de color.ini (00EE90).
            5 => (d.msg("Chat_Whisper_Header"), 0x00EE90, MASK_WHISPER),
            6 => (d.msg("Chat_WhisperSelf_header"), 0x00EE90, MASK_WHISPER),
            _ => return None,
        };
        Some((colored(&format!("{header}[{from}] {message}"), color), mask))
    }

    /// `/G texte`, `/P texte`, `/N texte` (modes de discussion d'Archonia) et commandes de guilde
    /// d'ArchRust : `/guilde`, `/ginviter [nom]`, `/gquitter`, `/gexclure nom`, `/gannonce texte`,
    /// `/gaccepter`, `/grefuser`, `/gliste [page|nom]`, `/gmembres [nom]`, `/gagrandir`,
    /// `/gsecond nom`, `/gheritier nom`, `/gguerre guilde|appliquer|annuler`, `/gg`, `/galliance guilde|quitter`,
    /// `/ghostile guilde`, `/gtreve guilde`, `/gentrepot deposer|reprendre|argent …`.
    pub(super) fn guild_slash(&mut self, cmd: &str, arg: &str) -> bool {
        let arg = arg.trim();
        let lower = cmd.to_ascii_lowercase();
        let guild = self.guild.state.name().to_string();
        match lower.as_str() {
            "/g" | "/p" | "/n" => {
                if arg.is_empty() {
                    return true;
                }
                let kind = match lower.as_str() {
                    "/g" => 3,
                    "/p" => 4,
                    _ => 0,
                };
                if let Some(net) = &self.net {
                    net.send(Command::Chat { kind, message: arg.to_string() });
                }
            }
            // Chuchotement d'Archonia (sysstr 542-547) : « /w nom message », « /r message » répond au
            // dernier qui a chuchoté ; sans message, la barre Chat_Input_Private s'ouvre sur ce nom.
            "/w" | "/ㅈ" | "/r" => {
                let reply = lower == "/r";
                let (to, message) = if reply {
                    (self.ui.last_whisper.clone().unwrap_or_default(), arg.to_string())
                } else {
                    let (n, m) = arg.split_once(char::is_whitespace).unwrap_or((arg, ""));
                    (n.to_string(), m.trim().to_string())
                };
                if to.is_empty() {
                    return true;
                }
                if message.is_empty() {
                    self.ui.whisper_target = Some(to);
                    self.ui.chat_focus = true;
                } else if let Some(net) = &self.net {
                    net.send(Command::Whisper { to, message });
                }
            }
            "/guilde" | "/guild" => self.guild_toggle(),
            "/ginviter" | "/ginvite" => {
                let name = if arg.is_empty() {
                    self.addons.target_cid().and_then(|c| self.entities.iter().find(|e| e.cid == c)).map(|e| e.name.clone()).unwrap_or_default()
                } else {
                    arg.to_string()
                };
                if name.is_empty() || guild.is_empty() {
                    self.ui.push_chat("[guilde] Usage : /ginviter nom (ou un joueur ciblé), dans une guilde.");
                } else {
                    send(self.net.as_ref(), GuildCommand::Invite { guild, name });
                }
            }
            "/gquitter" | "/gquit" => self.guild_leave_request(),
            "/gexclure" | "/gkick" => {
                if !arg.is_empty() {
                    self.guild_kick_request(arg.to_string());
                }
            }
            "/gannonce" | "/gnotice" => {
                self.guild.notice = arg.to_string();
                self.guild_send_notice();
            }
            "/gaccepter" | "/gaccept" => self.guild_answer(true),
            "/grefuser" | "/gdecline" => self.guild_answer(false),
            // Mêmes effets que les boutons d'Archonia : onglet « Classement » (page ou guilde cherchée),
            // onglet « Membres » sur un membre (« Rechercher »), « Augmenter la limite », « Nommer »,
            // « Hériter de la guilde ».
            "/gliste" | "/glist" => {
                self.guild_click(GuildUi::Click { window: "Guild_Member".into(), control: "GD_Member_Button_Tab3".into() }, 0);
                let relation = match arg {
                    "allies" | "alliés" => Some("CONTROL_24849"),
                    "hostiles" => Some("CONTROL_20569"),
                    _ => None,
                };
                if let Some(control) = relation {
                    self.guild_click(GuildUi::Click { window: "Guild_List".into(), control: control.into() }, 0);
                } else if let Ok(page) = arg.parse::<i32>() {
                    send(self.net.as_ref(), GuildCommand::ListPage { page: (page - 1).max(0) });
                } else if !arg.is_empty() {
                    self.guild.list_search = arg.to_string();
                    self.guild_click(GuildUi::Click { window: "Guild_List".into(), control: "CONTROL_854".into() }, 0);
                }
            }
            "/gmembres" | "/gmembers" => {
                self.guild.open.remove(&Win::Notice);
                self.guild.open.remove(&Win::List);
                self.guild.open.insert(Win::Member);
                if !arg.is_empty() {
                    self.guild.search = arg.to_string();
                    self.guild_click(GuildUi::Click { window: "Guild_Member".into(), control: "CONTROL_23904".into() }, 0);
                }
            }
            // « Emblème » de Guild_Main ; `fond motif symbole [couleurs…]` compose l'emblème comme les clics
            // dans les grilles, `acheter` et `confirmer` comme les boutons.
            "/gembleme" | "/gmark" => match arg {
                "acheter" | "buy" => self.guild_click(GuildUi::Click { window: "Guild_Mark".into(), control: "CONTROL_5906".into() }, 0),
                "confirmer" | "confirm" => self.guild_click(GuildUi::Click { window: "Guild_MarkConfirm".into(), control: "CONTROL_30466".into() }, 0),
                _ => {
                    self.guild_click(GuildUi::Click { window: "Guild_Main".into(), control: "GD_Main_Button_3".into() }, 0);
                    let n: Vec<u32> = arg.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                    if n.len() >= 3 && self.guild.open.contains(&Win::Mark) {
                        self.guild.mark_tid = (n[0] & 0xff) | (n[1] & 0xff) << 8 | (n[2] & 0xff) << 16;
                        for (i, c) in n.iter().skip(3).take(3).enumerate() {
                            let shift = 8 * i as u32;
                            self.guild.mark_color = (self.guild.mark_color & !(0xff << shift)) | ((c & 0xff) << shift);
                        }
                    }
                }
            },
            "/gagrandir" | "/gincrease" => self.guild_click(GuildUi::Click { window: "Guild_Main".into(), control: "CONTROL_29434".into() }, 0),
            "/gsecond" | "/gappoint" | "/gheritier" | "/gsuccession" => {
                if !arg.is_empty() {
                    self.guild.search = arg.to_string();
                    self.guild_click(GuildUi::Click { window: "Guild_Member".into(), control: "CONTROL_23904".into() }, 0);
                }
                let control = if matches!(lower.as_str(), "/gsecond" | "/gappoint") { "CONTROL_31321" } else { "CONTROL_3394" };
                self.guild_click(GuildUi::Click { window: "Guild_Member".into(), control: control.into() }, 0);
            }
            // Comme « Demander la guerre » sur la guilde nommée ; `appliquer` : bouton « Appliquer » ; `annuler` :
            // demande d'annulation confirmée (`Guild_Battle_Cancel_Request_ConfirmUI_Open`).
            "/gguerre" | "/gwar" => match arg {
                "" => {}
                "annuler" | "cancel" => {
                    if self.guild.state.battle.is_some() {
                        let text = self.guild.data.as_ref().and_then(|d| d.events.get("Guild_Battle_Cancel_Request_ConfirmUI_Open").cloned()).unwrap_or_default();
                        self.guild.ask = Some((Ask::BattleCancel, text));
                    }
                }
                "appliquer" | "apply" => self.guild_click(GuildUi::Click { window: "Guild_GBattle".into(), control: "CONTROL_12454".into() }, 0),
                name => self.guild_battle_form(name.to_string()),
            },
            // Mêmes questions que les boutons d'alliance et d'hostilité de Guild_List, pour la guilde nommée ;
            // `/galliance quitter` : départ de l'alliance.
            "/galliance" | "/gunion" | "/ghostile" | "/gtreve" | "/gtruce" => {
                let d = self.guild.data.as_ref().expect("données lues");
                let named = [("Guild_RelationGuildID", arg.to_string())];
                let ask = match lower.as_str() {
                    "/galliance" | "/gunion" if matches!(arg, "quitter" | "leave") => (Ask::JointLeave, "Guild_OpenJointLeaveConfirmUI"),
                    "/galliance" | "/gunion" => (Ask::JointRequest(arg.to_string()), "Guild_OpenJointRequestConfirmUI"),
                    "/ghostile" => (Ask::HostileRequest(arg.to_string()), "Guild_OpenHostileRequestConfirmUI"),
                    _ => (Ask::HostileLeaveRequest(arg.to_string()), "Guild_OpenHostileLeaveRequestConfirmUI"),
                };
                if !arg.is_empty() && self.my_member().is_some_and(|m| m.rank == guild::rank::MASTER) {
                    let text = fill_named(d.events.get(ask.1).map_or("", String::as_str), &named);
                    self.guild.ask = Some((ask.0, text));
                }
            }
            // Entrepôt ouvert : mêmes effets que les gestes de la fenêtre (clic droit dans le sac, objet glissé vers le
            // sac, boutons d'argent) ; `argent +N` dépose, `argent -N` retire.
            "/gentrepot" | "/gwarehouse" => {
                let (verb, rest) = arg.split_once(char::is_whitespace).map_or((arg, ""), |(v, r)| (v, r.trim()));
                let assets = self.assets.clone();
                let find = |status: i8| {
                    let wanted = rest.to_lowercase();
                    let mut v: Vec<&archrust_netclient::world::ItemRecord> = self
                        .items
                        .values()
                        .filter(|i| i.status == Some(status))
                        .filter(|i| i.template_id.map(|t| t.to_string()) == Some(wanted.clone()) || i.template_id.and_then(|t| crate::scene::item_name_for_tid(&assets, t)).is_some_and(|n| n.to_lowercase().contains(&wanted)))
                        .collect();
                    v.sort_by_key(|i| i.id);
                    v.first().map(|i| i.id)
                };
                match verb {
                    _ if !self.guild.gware.open => self.ui.push_chat("[entrepôt] Ouvrez l'entrepôt de guilde chez un gardien de banque (/entrepot)."),
                    "deposer" | "store" => {
                        if let Some(iid) = find(2) {
                            let mut req = vec![crate::hud::ItemUi::Activate { iid, worn: false }];
                            item_requests(&self.guild, &mut req, self.net.as_ref());
                        }
                    }
                    "reprendre" | "take" => {
                        if let Some(iid) = find(guild::warehouse::ITEM_STATUS) {
                            send(self.net.as_ref(), GuildCommand::WarehouseTake { iid, grid: None });
                        }
                    }
                    "argent" | "money" => {
                        if let Ok(n) = rest.replace('+', "").parse::<i64>() {
                            send(self.net.as_ref(), GuildCommand::WarehouseMoney { deposit: n > 0, amount: n.abs() });
                        }
                    }
                    _ => self.ui.push_chat("[entrepôt] Usage : /gentrepot deposer|reprendre nom (ou TID), /gentrepot argent +N|-N."),
                }
            }
            // « GG » d'Archonia : abandon confirmé (`Guild_Battle_Withdraw_ConfirmUI_Open`).
            "/gg" => {
                if self.guild.state.battle.is_some() {
                    let text = self.guild.data.as_ref().and_then(|d| d.events.get("Guild_Battle_Withdraw_ConfirmUI_Open").cloned()).unwrap_or_default();
                    self.guild.ask = Some((Ask::BattleWithdraw, text));
                }
            }
            _ => return false,
        }
        true
    }

    /// `Guild_GBattle` pour défier `enemy` (le chef d'une guilde hors combat).
    fn guild_battle_form(&mut self, enemy: String) {
        let master = self.my_member().is_some_and(|m| m.rank == guild::rank::MASTER);
        if !master || self.guild.state.status != guild::battle::status::NONE || enemy.eq_ignore_ascii_case(self.guild.state.name()) {
            return;
        }
        self.guild.battle_form = BattleForm { enemy, ..Default::default() };
        self.guild.open.insert(Win::GBattle);
    }

    /// Réponse à la boîte OK / Annuler.
    fn guild_answer(&mut self, ok: bool) {
        let Some((ask, _)) = self.guild.ask.take() else { return };
        let guild = self.guild.state.name().to_string();
        let net = self.net.as_ref();
        match ask {
            Ask::Invite { guild } => {
                self.guild.state.invitation = None;
                send(net, if ok { GuildCommand::Join { guild } } else { GuildCommand::Reject { guild } });
            }
            Ask::AppointAnswer { guild, rank } => send(net, GuildCommand::AppointAnswer { guild, rank, yes: ok }),
            Ask::SuccessionAnswer { guild } => send(net, GuildCommand::SuccessionAnswer { guild, yes: ok }),
            // Agrandissement (FUN_1408ccd50) : la fenêtre se ferme, que la réponse soit oui ou non.
            Ask::Increase(max) => {
                self.guild.open.remove(&Win::MemberSize);
                if ok {
                    send(net, GuildCommand::IncreaseMax { guild, max: max as i32 });
                }
            }
            // Demandes d'alliance, d'hostilité et de trêve reçues (FUN_1408cafa0, FUN_1408cad50, FUN_1408cada0).
            Ask::JointAccept(other) => send(net, GuildCommand::JointAnswer { guild, other, yes: ok }),
            Ask::HostileAccept(other) => send(net, GuildCommand::HostileAnswer { guild, other, yes: ok }),
            Ask::HostileLeaveAccept(other) => send(net, GuildCommand::HostileLeaveAnswer { guild, other, yes: ok }),
            // Défi reçu (FUN_14084e7a0) : OK → op 16 ; Annuler → op 17 et Guild_Msg_Battle_Reject. Types 3 et 4 :
            // l'acceptation attend le choix des combattants (`UI_GuildBattleMember`, mode 1).
            Ask::BattleAnswer(enemy) if ok && self.guild.battle_form.men().get(self.guild.battle_form.man).is_some() => {
                self.guild.open.remove(&Win::GBattle);
                let count = self.guild.battle_form.men()[self.guild.battle_form.man];
                self.guild.picker = Some(BattlePicker::new(&self.guild.state, Some(enemy), count));
                self.guild.open.insert(Win::BattleMember);
            }
            Ask::BattleAnswer(enemy) => {
                self.guild.open.remove(&Win::GBattle);
                send(net, GuildCommand::BattleAnswer { guild, enemy, yes: ok });
                if !ok {
                    let t = self.guild.data.as_ref().map(|d| d.msg("Guild_Msg_Battle_Reject")).unwrap_or_default();
                    self.guild_message(&t, 0xFFFF_FFFF);
                }
            }
            Ask::BattleCancelAnswer => {
                send(net, GuildCommand::BattleCancelAnswer { guild, yes: ok });
                if !ok {
                    let t = self.guild.data.as_ref().map(|d| d.msg("Guild_Msg_Battle_Cancel_Reject")).unwrap_or_default();
                    self.guild_message(&t, 0xFFFF_FFFF);
                }
            }
            _ if !ok => {}
            Ask::BattleRequest => {
                let f = self.guild.battle_form.clone();
                self.guild.open.remove(&Win::GBattle);
                let duration = guild::battle::MINUTES[f.time.min(5)] * 60;
                let person = f.men().get(f.man).copied().unwrap_or(0);
                // Types 3 et 4 : la demande part après le choix des combattants (mode 0).
                if person > 0 {
                    self.guild.picker = Some(BattlePicker::new(&self.guild.state, None, person));
                    self.guild.open.insert(Win::BattleMember);
                    return;
                }
                send(net, GuildCommand::BattleRequest { guild, enemy: f.enemy, kind: f.kind as i32, duration, person });
            }
            Ask::BattleCancel => send(net, GuildCommand::BattleCancel { guild }),
            Ask::JointRequest(target) => send(net, GuildCommand::JointRequest { guild, name: self.my_name(), target }),
            Ask::JointLeave => send(net, GuildCommand::JointLeave { guild }),
            Ask::HostileRequest(target) => send(net, GuildCommand::HostileRequest { guild, name: self.my_name(), target }),
            Ask::HostileLeaveRequest(target) => send(net, GuildCommand::HostileLeaveRequest { guild, name: self.my_name(), target }),
            Ask::BattleWithdraw => send(net, GuildCommand::BattleWithdraw { guild }),
            Ask::MarkForce => send(net, GuildCommand::BuyMark { guild, tid: self.guild.mark_tid as i32, color: self.guild.mark_color as i32, force: true }),
            // Guild_GuildMark_Initialize (FUN_14055bb90) : dessin remis à zéro, puis Initialize_Complete.
            Ask::MarkReset => {
                self.guild.mark_tid = 0;
                self.guild.mark_color = 0xFFFF_FFFF;
                self.guild.info = self.guild.data.as_ref().and_then(|d| d.events.get("Guild_GuildMark_Initialize_Complete").cloned());
            }
            Ask::AppointCancel(name) => send(net, GuildCommand::Appoint { name, rank: guild::rank::NORMAL }),
            Ask::JoinRequest(guild) => send(net, GuildCommand::Join { guild }),
            Ask::Leave => send(net, GuildCommand::Leave { guild, name: self.my_name() }),
            Ask::Kick(name) => send(net, GuildCommand::Kick { guild, name }),
            Ask::AllowJoin(name) => send(net, GuildCommand::AllowJoin { guild, name }),
            Ask::AllowLeave(name) => send(net, GuildCommand::AllowLeave { guild, name }),
        }
    }

    /// « Quitter la guilde » (FUN_1408c7d00) : le chef ne peut pas ; une demande en cours ne se répète pas.
    fn guild_leave_request(&mut self) {
        let Some(m) = self.my_member().cloned() else { return };
        if m.rank == guild::rank::LEAVE_REQUEST {
            return;
        }
        let assets = self.assets.clone();
        let d = self.guild.data(&assets);
        if m.rank == guild::rank::MASTER {
            let (t, c) = (d.msg("Guild_Leave_Error_Master"), d.color("Guild_System_Message_Color", 0xFFFF_FF33));
            self.guild_message(&t, c);
            return;
        }
        let text = d.events.get("Guild_Leave_ConfirmUI_Open").cloned().unwrap_or_default();
        self.guild.ask = Some((Ask::Leave, text));
    }

    /// « Expulser » (FUN_1408c0c20) : le chef confirme ; un autre membre se voit proposer de partir.
    fn guild_kick_request(&mut self, name: String) {
        let Some(m) = self.my_member().cloned() else { return };
        if m.rank != guild::rank::MASTER {
            self.guild_leave_request();
            return;
        }
        let assets = self.assets.clone();
        let d = self.guild.data(&assets);
        if name.eq_ignore_ascii_case(&m.name) {
            let (t, c) = (d.msg("Guild_Leave_Error_Master"), d.color("Guild_System_Message_Color", 0xFFFF_FF33));
            self.guild_message(&t, c);
            return;
        }
        let text = fill_braces(d.events.get("Guild_ForcedLeave_ConfirmUI_Open").map_or("", String::as_str), &name);
        self.guild.ask = Some((Ask::Kick(name), text));
    }

    /// « Envoyer un avis » : op 9 par le chef (FUN_1408c7fe0).
    fn guild_send_notice(&mut self) {
        if self.my_member().is_some_and(|m| m.rank == guild::rank::MASTER) && !self.guild.notice.trim().is_empty() {
            let guild = self.guild.state.name().to_string();
            send(self.net.as_ref(), GuildCommand::Notice { guild, text: self.guild.notice.trim().to_string() });
        }
    }

    /// Membres à part entière et demandes, dans l'ordre reçu.
    fn guild_rows(&self) -> Vec<GuildMember> {
        self.guild.state.members.clone()
    }

    fn guild_selected(&self) -> Option<GuildMember> {
        let rows = self.guild_rows();
        self.guild.selected.and_then(|i| rows.get(self.guild.page * PAGE + i).cloned())
    }

    fn guild_click(&mut self, r: GuildUi, level: i64) {
        let (window, control) = match r {
            GuildUi::MessageBox(ok) => return self.guild_answer(ok),
            GuildUi::GwareClose => return self.guild_warehouse_close(),
            GuildUi::GwareMoney(deposit) => {
                self.guild.gware.money_ask = Some((deposit, 0));
                return;
            }
            // GWARE_EXPAND (FUN_1405fb920) : confirmation du prix, puis l'op 7.
            GuildUi::GwareExpand => return send(self.net.as_ref(), GuildCommand::WarehouseExpand),
            GuildUi::GwareStore { iid, grid } => return send(self.net.as_ref(), GuildCommand::WarehouseStore { iid, grid }),
            GuildUi::GwareTake(iid) => return send(self.net.as_ref(), GuildCommand::WarehouseTake { iid, grid: None }),
            GuildUi::Click { window, control } if window == "UI_GWareConfirm" => {
                if let Some((dir, amount)) = control.split_once(':') {
                    if let Ok(amount) = amount.parse::<i64>() {
                        send(self.net.as_ref(), GuildCommand::WarehouseMoney { deposit: dir == "in", amount });
                    }
                }
                return;
            }
            // Grille des éléments (FUN_14055be90) : choisir l'élément de la couche, ou le retirer s'il l'est.
            GuildUi::Cell(MARK_GRID, id) => {
                let l = self.guild.mark_layer.min(2) as u8 + 1;
                let part = id as u32;
                let rest = self.guild.mark_tid & !(0xff << (8 * (l as u32 - 1)));
                self.guild.mark_tid = if mark_part(self.guild.mark_tid, l) == part { rest } else { rest | part };
                return;
            }
            // Couleurs (FUN_14055bdc0) : couleur de la couche choisie.
            GuildUi::Cell(MARK_COLORS, id) => {
                let shift = 8 * self.guild.mark_layer.min(2) as u32;
                self.guild.mark_color = (self.guild.mark_color & !(0xff << shift)) | ((id as u32 & 0xff) << shift);
                return;
            }
            GuildUi::Cell(..) => return,
            GuildUi::Row(list, i) if list == ListSpec::GUILDS.list => {
                self.guild.list_selected = (i < self.guild.list_entries().len()).then_some(i);
                return;
            }
            // OP_GuildBattleListSelect : la ligne est choisie et cochée (ou décochée, sauf le chef).
            GuildUi::Row(list, i) if list == ListSpec::BATTLE_MEMBERS.list => {
                let master = self.guild.state.info.as_ref().map(|g| g.master.clone()).unwrap_or_default();
                let Some(p) = self.guild.picker.as_mut() else { return };
                let at = p.first + i;
                let Some(row) = p.rows.get_mut(at) else { return };
                p.cursor = Some(at);
                if row.2 && row.0.eq_ignore_ascii_case(&master) {
                    let t = self.guild.data.as_ref().map(|d| d.msg("GuildBattleMember_NotEraseMaster")).unwrap_or_default();
                    self.guild_message(&t, 0xFFFF_FFFF);
                } else {
                    row.2 = !row.2;
                }
                return;
            }
            GuildUi::Row(list, _) if list == ListSpec::BATTLE_RESULT.list => return,
            GuildUi::Row(_, i) => {
                self.guild.selected = (self.guild.page * PAGE + i < self.guild.state.members.len()).then_some(i);
                return;
            }
            GuildUi::Click { window, control } => (window, control),
        };
        let close = |g: &mut GuildGlue, w: Win| {
            g.open.remove(&w);
        };
        let msg = |g: &GuildGlue, k: &str| g.data.as_ref().map(|d| d.msg(k)).unwrap_or_default();
        match (window.as_str(), control.as_str()) {
            ("Guild_Main", "GD_Main_Button_X") => close(&mut self.guild, Win::Main),
            ("Guild_Main", "GD_Main_Button_1") => {
                if self.guild.state.in_guild() {
                    let t = msg(&self.guild, "Guild_Msg_AlreadyMember");
                    self.guild_message(&t, 0xFFFF_FFFF);
                } else if level < CREATE_LEVEL {
                    let t = msg(&self.guild, "Guild_Msg_NeedMoreLevel");
                    self.guild_message(&t, 0xFFFF_FFFF);
                } else {
                    close(&mut self.guild, Win::Main);
                    self.guild.open.insert(Win::Create);
                }
            }
            ("Guild_Main", "GD_Main_Button_2") => {
                close(&mut self.guild, Win::Main);
                self.guild.open.insert(Win::Destroy);
            }
            // Guild_Create_GuildMark (FUN_14055b920) : le chef, 10 membres au moins ; le dessin part de
            // l'emblème de la guilde (FUN_14055bcb0).
            ("Guild_Main", "GD_Main_Button_3") => {
                if self.my_member().is_none_or(|m| m.rank != guild::rank::MASTER) {
                    return;
                }
                if self.guild.state.counts().1 < MARK_MIN_MEMBERS {
                    self.guild.info = self.guild.data.as_ref().and_then(|d| d.events.get("Guild_GuildMark_Not_Enought_Member").cloned());
                    return;
                }
                let (tid, color) = self.guild.state.info.as_ref().map_or((0, 0), |i| (i.mark_tid as u32, i.mark_color as u32));
                (self.guild.mark_tid, self.guild.mark_color, self.guild.mark_layer, self.guild.mark_scroll) = (tid, color, 0, 0);
                close(&mut self.guild, Win::Main);
                self.guild.open.insert(Win::Mark);
            }
            ("Guild_Mark", c) if MARK_TABS.contains(&c) => {
                self.guild.mark_layer = MARK_TABS.iter().position(|t| *t == c).unwrap_or(0);
                self.guild.mark_scroll = 0;
            }
            ("Guild_Mark", "GUILDMARK_PGUP") => self.guild.mark_scroll = self.guild.mark_scroll.saturating_sub(1),
            ("Guild_Mark", "GUILDMARK_PGDN") => {
                let l = self.guild.mark_layer.min(2) as u8 + 1;
                let n = self.guild.data.as_ref().map_or(0, |d| d.marks.iter().filter(|m| m.layer == l).count());
                if (self.guild.mark_scroll + MARK_ROWS) * MARK_COLS < n {
                    self.guild.mark_scroll += 1;
                }
            }
            // Guild_GuildMarkClickBuy (FUN_14055ba60) : un fond est exigé, puis Guild_MarkConfirm.
            ("Guild_Mark", "CONTROL_5906") => {
                if self.guild.mark_tid & 0xff == 0 {
                    self.guild.info = self.guild.data.as_ref().and_then(|d| d.events.get("Guild_GuildMark_No_Exist_Bottom").cloned());
                } else {
                    self.guild.open.insert(Win::MarkConfirm);
                }
            }
            ("Guild_Mark", "CONTROL_20548") => {
                let text = self.guild.data.as_ref().and_then(|d| d.events.get("Guild_GuildMark_Initialize").cloned()).unwrap_or_default();
                self.guild.ask = Some((Ask::MarkReset, text));
            }
            ("Guild_Mark", "CONTROL_5198" | "CONTROL_18977") => {
                close(&mut self.guild, Win::Mark);
                close(&mut self.guild, Win::MarkConfirm);
            }
            // Guild_GuildMarkClickBuyOK (FUN_14055bad0) : op 32.
            ("Guild_MarkConfirm", "CONTROL_30466") => {
                close(&mut self.guild, Win::MarkConfirm);
                let guild = self.guild.state.name().to_string();
                send(self.net.as_ref(), GuildCommand::BuyMark { guild, tid: self.guild.mark_tid as i32, color: self.guild.mark_color as i32, force: false });
            }
            ("Guild_MarkConfirm", _) => close(&mut self.guild, Win::MarkConfirm),
            // Guild_Main_Increase_Request (FUN_1408cce90) : le chef, 20 membres au moins.
            ("Guild_Main", "CONTROL_29434") => {
                if self.my_member().is_none_or(|m| m.rank != guild::rank::MASTER) {
                    return;
                }
                if self.guild.state.counts().1 < 20 {
                    self.guild.info = self.guild.data.as_ref().and_then(|d| d.events.get("Guild_MemberUp_Not_Enought_Member").cloned());
                } else {
                    close(&mut self.guild, Win::Main);
                    self.guild.open.insert(Win::MemberSize);
                }
            }
            ("Guild_MemberSize", "CONTROL_10606") => {
                let max = self.guild.state.info.as_ref().map_or(0, |i| i.max_members as i64);
                let next = if max < 1 { 10 } else { (max / 10 + 1) * 10 };
                let text = self.guild.data.as_ref().and_then(|d| d.events.get("Guild_MaxMemberIncreaseOK").cloned()).unwrap_or_default();
                self.guild.ask = Some((Ask::Increase(next), text));
            }
            ("Guild_MemberSize", _) => close(&mut self.guild, Win::MemberSize),
            // Onglet « Classement » : Guild_GuildList_Tab_Click (FUN_1408c8f90), page 0 (FUN_1408c90c0).
            ("Guild_Notice", "GD_Notice_Button_Tab3") | ("Guild_Member", "GD_Member_Button_Tab3") => {
                close(&mut self.guild, Win::Notice);
                close(&mut self.guild, Win::Member);
                self.guild.list_selected = None;
                self.guild.list_mode = 0;
                self.guild.open.insert(Win::List);
                send(self.net.as_ref(), GuildCommand::ListPage { page: 0 });
            }
            ("Guild_List", "CONTROL_11612") => {
                close(&mut self.guild, Win::List);
                self.guild.notice = self.guild.state.info.as_ref().map(|i| i.notice.clone()).unwrap_or_default();
                self.guild.open.insert(Win::Notice);
            }
            ("Guild_List", "CONTROL_26096") => {
                close(&mut self.guild, Win::List);
                self.guild.open.insert(Win::Member);
            }
            ("Guild_List", "CONTROL_6149" | "CONTROL_10973") => close(&mut self.guild, Win::List),
            // Guild_OpenJointList, Guild_OpenHostileList (FUN_1408cc7b0, FUN_1408cc780).
            ("Guild_List", "CONTROL_24849" | "CONTROL_20569") => {
                self.guild.list_mode = if control == "CONTROL_24849" { 1 } else { 2 };
                self.guild.list_selected = None;
            }
            ("Guild_List", "CONTROL_12509") => {
                self.guild.list_mode = 0;
                self.guild.list_selected = None;
            }
            // Guild_OnJointRequestBtnClk (FUN_1408cc5f0) : demande sur la guilde choisie ; départ dans les alliés.
            ("Guild_List", "CONTROL_19137") => {
                let d = self.guild.data.as_ref().expect("données lues");
                match (self.guild.list_mode, self.guild.list_picked()) {
                    (0, Some(name)) => {
                        let text = fill_named(d.events.get("Guild_OpenJointRequestConfirmUI").map_or("", String::as_str), &[("Guild_RelationGuildID", name.clone())]);
                        self.guild.ask = Some((Ask::JointRequest(name), text));
                    }
                    (1, _) if !self.guild.state.joints.is_empty() => {
                        let text = d.events.get("Guild_OpenJointLeaveConfirmUI").cloned().unwrap_or_default();
                        self.guild.ask = Some((Ask::JointLeave, text));
                    }
                    _ => {}
                }
            }
            // Guild_OnHostileRequestBtnClk (FUN_1408cc530) : déclaration, ou trêve dans les hostiles.
            ("Guild_List", "CONTROL_18911") => {
                let d = self.guild.data.as_ref().expect("données lues");
                let Some(name) = self.guild.list_picked() else { return };
                let named = [("Guild_RelationGuildID", name.clone())];
                match self.guild.list_mode {
                    0 => self.guild.ask = Some((Ask::HostileRequest(name), fill_named(d.events.get("Guild_OpenHostileRequestConfirmUI").map_or("", String::as_str), &named))),
                    2 => self.guild.ask = Some((Ask::HostileLeaveRequest(name), fill_named(d.events.get("Guild_OpenHostileLeaveRequestConfirmUI").map_or("", String::as_str), &named))),
                    _ => {}
                }
            }
            ("Guild_List", "CONTROL_22721" | "CONTROL_24225") => {
                let (page, total) = self.guild.state.list.as_ref().map_or((0, 0), |l| (l.page, l.total));
                let wanted = if control == "CONTROL_22721" { page - 1 } else { page + 1 };
                if wanted >= 0 && wanted * (PAGE as i32) < total {
                    send(self.net.as_ref(), GuildCommand::ListPage { page: wanted });
                }
            }
            ("Guild_List", "CONTROL_854") => {
                let name = self.guild.list_search.trim().to_string();
                if !name.is_empty() && name.len() <= 32 {
                    send(self.net.as_ref(), GuildCommand::Search { name });
                }
            }
            // Guild_GuildBattleTime_Request (FUN_14084ea50) : Guild_GBattle sur la guilde choisie ; sans
            // guilde, Guild_Confirm_Join_Request.
            ("Guild_List", "CONTROL_6490") => {
                let picked = self.guild.list_picked();
                match (picked, self.guild.state.in_guild()) {
                    (Some(name), true) => self.guild_battle_form(name),
                    (Some(name), false) => {
                        let text = self.guild.data.as_ref().and_then(|d| d.events.get("Guild_Confirm_Join_Request").cloned()).unwrap_or_default();
                        self.guild.ask = Some((Ask::JoinRequest(name), text));
                    }
                    _ => {}
                }
            }
            // Guild_GBattle : chaque liste (ou sa flèche) passe au choix suivant ; l'effectif revient au
            // premier quand le type change.
            ("Guild_GBattle", "CONTROL_4060" | "CONTROL_29955") if !self.guild.battle_form.received => {
                let f = &mut self.guild.battle_form;
                f.kind = (f.kind + 1) % guild::battle::KINDS.len();
                f.man = 0;
            }
            ("Guild_GBattle", "CONTROL_17192" | "CONTROL_6484") if !self.guild.battle_form.received => {
                let f = &mut self.guild.battle_form;
                f.time = (f.time + 1) % guild::battle::MINUTES.len();
            }
            ("Guild_GBattle", "CONTROL_21248" | "CONTROL_12900") if !self.guild.battle_form.received => {
                let f = &mut self.guild.battle_form;
                let n = f.men().len();
                if n > 0 {
                    f.man = (f.man + 1) % n;
                }
            }
            // « Appliquer » (OP_GuildBattleRequest) : Guild_Battle_Request_ConfirmUI_Open, OK → op 15.
            ("Guild_GBattle", "CONTROL_12454") if !self.guild.battle_form.received => {
                let d = self.guild.data.as_ref().expect("données lues");
                let text = fill_named(d.events.get("Guild_Battle_Request_ConfirmUI_Open").map_or("", String::as_str), &self.guild.battle_form.named(d));
                self.guild.ask = Some((Ask::BattleRequest, text));
            }
            ("Guild_GBattle", "CONTROL_9564" | "CONTROL_26698") => close(&mut self.guild, Win::GBattle),
            // UI_GuildBattleMember : monter / descendre la ligne choisie, tirage, défilement, fermeture, fin.
            ("UI_GuildBattleMember", "CONTROL_24557" | "CONTROL_2784") => {
                let Some(p) = self.guild.picker.as_mut() else { return };
                let Some(c) = p.cursor else { return };
                let to = if control == "CONTROL_24557" { c.checked_sub(1) } else { Some(c + 1).filter(|t| *t < p.rows.len()) };
                if let Some(t) = to {
                    p.rows.swap(c, t);
                    p.cursor = Some(t);
                    p.first = p.first.min(t).max((t + 1).saturating_sub(BATTLE_ROWS));
                }
            }
            ("UI_GuildBattleMember", "CONTROL_13140") => {
                let master = self.guild.state.info.as_ref().map(|g| g.master.clone()).unwrap_or_default();
                let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
                if let Some(p) = self.guild.picker.as_mut() {
                    p.random(&master, seed);
                }
            }
            ("UI_GuildBattleMember", "CONTROL_13872" | "CONTROL_2495") => {
                if let Some(p) = self.guild.picker.as_mut() {
                    let last = p.rows.len().saturating_sub(BATTLE_ROWS);
                    p.first = if control == "CONTROL_13872" { p.first.saturating_sub(1) } else { (p.first + 1).min(last) };
                }
            }
            ("UI_GuildBattleMember", "CONTROL_17266") => {
                self.guild.picker = None;
                close(&mut self.guild, Win::BattleMember);
            }
            ("UI_GuildBattleMember", "CONTROL_15682") => self.guild_battle_members_done(),
            ("UI_GuildBattleResult", "CONTROL_29570") => close(&mut self.guild, Win::BattleResult),
            ("UI_GuildBattleResult", "CONTROL_14117" | "CONTROL_15119") => {
                let n = self.guild.fighters.0.len().max(self.guild.fighters.1.len());
                let last = n.saturating_sub(BATTLE_ROWS);
                let f = &mut self.guild.result_first;
                *f = if control == "CONTROL_14117" { f.saturating_sub(1) } else { (*f + 1).min(last) };
            }
            // Guild_Appoint_Click (FUN_1408c0340) : le second se voit retirer son rang après confirmation ;
            // sinon la nomination part aussitôt.
            ("Guild_Member", "CONTROL_31321") => {
                let Some(s) = self.guild_selected() else { return };
                if s.rank == guild::rank::SUBMASTER {
                    let d = self.guild.data.as_ref().expect("données lues");
                    let text = printf(&d.msg("GuildMemberAppointCancelConfirm_Message"), &[&self.guild.state.name(), &s.name, &rank_title(d, guild::rank::SUBMASTER)]);
                    self.guild.ask = Some((Ask::AppointCancel(s.name), text));
                } else {
                    send(self.net.as_ref(), GuildCommand::Appoint { name: s.name, rank: guild::rank::SUBMASTER });
                }
            }
            // Guild_Succession_Click (FUN_1408c0aa0) : la demande part aussitôt.
            ("Guild_Member", "CONTROL_3394") => {
                if let Some(s) = self.guild_selected() {
                    send(self.net.as_ref(), GuildCommand::Succession { name: s.name });
                }
            }
            ("Guild_Create", "GD_Create_Button_OK") => {
                let (n, p, p2) = (self.guild.name.trim().to_string(), self.guild.password.clone(), self.guild.password2.clone());
                if n.is_empty() || n.len() > 32 || p.is_empty() || p.len() > 12 || p2.is_empty() {
                    return;
                }
                if p != p2 {
                    let t = msg(&self.guild, "Guild_Msg_CheckPassword");
                    self.guild_message(&t, 0xFFFF_FFFF);
                    return;
                }
                self.guild.pending_create = Some((n, p));
                self.guild.name.clear();
                self.guild.password.clear();
                self.guild.password2.clear();
                close(&mut self.guild, Win::Create);
                self.guild.open.insert(Win::CreateRequest);
            }
            ("Guild_Create", _) => close(&mut self.guild, Win::Create),
            ("Guild_Create_Request", "GD_Request_Button_Ok") => {
                close(&mut self.guild, Win::CreateRequest);
                if let Some((name, password)) = self.guild.pending_create.clone() {
                    send(self.net.as_ref(), GuildCommand::Create { name, password });
                }
            }
            ("Guild_Create_Request", _) => {
                self.guild.pending_create = None;
                close(&mut self.guild, Win::CreateRequest);
            }
            ("Guild_Create_Complete", _) => close(&mut self.guild, Win::CreateComplete),
            ("Guild_Destroy", "GD_Destroy_Button_Ok") => {
                if self.guild.destroy_password.is_empty() {
                    return;
                }
                self.guild.pending_destroy = Some(std::mem::take(&mut self.guild.destroy_password));
                close(&mut self.guild, Win::Destroy);
                self.guild.open.insert(Win::DestroyConfirm);
            }
            ("Guild_Destroy", _) => close(&mut self.guild, Win::Destroy),
            ("Guild_Destroy_Confirm", "GD_Destroy_Confirm_Button_Ok") => {
                close(&mut self.guild, Win::DestroyConfirm);
                if let Some(password) = self.guild.pending_destroy.clone() {
                    let guild = self.guild.state.name().to_string();
                    send(self.net.as_ref(), GuildCommand::Destroy { guild, password });
                }
            }
            ("Guild_Destroy_Confirm", _) => {
                self.guild.pending_destroy = None;
                close(&mut self.guild, Win::DestroyConfirm);
            }
            ("Guild_Destroy_Complete", _) => close(&mut self.guild, Win::DestroyComplete),
            ("Guild_Notice", "GD_Notice_Button_Send") => self.guild_send_notice(),
            ("Guild_Notice", "GD_Notice_Button_Tab2") => {
                close(&mut self.guild, Win::Notice);
                self.guild.open.insert(Win::Member);
            }
            ("Guild_Notice", "CONTROL_18715") => self.guild_leave_request(),
            ("Guild_Notice", "GD_Notice_Button_X" | "GD_Notice_Button_Tab4") => close(&mut self.guild, Win::Notice),
            ("Guild_Member", "GD_Member_Button_Tab1") => {
                close(&mut self.guild, Win::Member);
                self.guild.notice = self.guild.state.info.as_ref().map(|i| i.notice.clone()).unwrap_or_default();
                self.guild.open.insert(Win::Notice);
            }
            ("Guild_Member", "GD_Member_Button_X" | "GD_Member_Button_Tab4") => close(&mut self.guild, Win::Member),
            ("Guild_Member", "GD_Member_Button_Left") => {
                if self.guild.page > 0 {
                    self.guild.page -= 1;
                    self.guild.selected = None;
                }
            }
            ("Guild_Member", "GD_Member_Button_Right") => {
                if (self.guild.page + 1) * PAGE < self.guild.state.members.len() {
                    self.guild.page += 1;
                    self.guild.selected = None;
                }
            }
            ("Guild_Member", "CONTROL_23904") => {
                let wanted = self.guild.search.trim().to_string();
                match self.guild.state.members.iter().position(|m| m.name.eq_ignore_ascii_case(&wanted)) {
                    Some(i) => {
                        self.guild.page = i / PAGE;
                        self.guild.selected = Some(i % PAGE);
                    }
                    None if !wanted.is_empty() => {
                        let t = printf(&msg(&self.guild, "Guild_No_Exist_Member"), &[&wanted]);
                        self.guild_message(&t, 0xFFFF_FFFF);
                    }
                    None => {}
                }
            }
            ("Guild_Member", "GD_Member_Button_Leave") => {
                if let Some(s) = self.guild_selected() {
                    self.guild_kick_request(s.name);
                }
            }
            ("Guild_Member", "CONTROL_14314") => {
                if let (Some(s), Some(net)) = (self.guild_selected(), &self.net) {
                    net.send(Command::Party(PartyCommand::Invite { cid: None, name: Some(s.name) }));
                }
            }
            // Guild_WhisperChat (FUN_1408c0b40, FUN_140787d30) : « sysstr 542 nom » dans la barre de
            // discussion normale, qui prend la main.
            ("Guild_Member", "CONTROL_24708") => {
                if let Some(s) = self.guild_selected().filter(|s| s.online && !s.name.is_empty()) {
                    let w = super::npc_glue::sysstr(&self.assets, 542).unwrap_or_else(|| "/w".into());
                    self.ui.whisper_target = None;
                    self.ui.chat_input = format!("{w} {} ", s.name);
                    self.ui.chat_focus = true;
                }
            }
            ("Guild_Member", "CONTROL_23938") => {
                if let Some(s) = self.guild_selected() {
                    let assets = self.assets.clone();
                    let d = self.guild.data(&assets);
                    let (ask, event) = if s.rank == guild::rank::LEAVE_REQUEST { (Ask::AllowLeave(s.name), "Guild_Confirm_Allow_Leave") } else { (Ask::AllowJoin(s.name), "Guild_Confirm_Allow_Join") };
                    let text = d.events.get(event).cloned().unwrap_or_default();
                    self.guild.ask = Some((ask, text));
                }
            }
            _ => {}
        }
    }

    /// Clics de la dernière image ; onglets de la discussion (`Chat_Tab_*` de uimessage).
    pub(super) fn guild_frame(&mut self) {
        let assets = self.assets.clone();
        if self.ui.chat_tabs.is_empty() {
            let d = self.guild.data(&assets);
            self.ui.chat_tabs = vec![(d.msg("Chat_Tab_Normal"), u16::MAX), (d.msg("Chat_Tab_Party"), MASK_PARTY), (d.msg("Chat_Tab_Guild"), MASK_GUILD), (d.msg("Chat_Tab_Whisper"), MASK_WHISPER)];
        }
        self.guild_warehouse_frame();
        let level = self.hud.stats.level.map_or(0, |l| l as i64);
        for r in std::mem::take(&mut self.guild.clicks) {
            self.guild_click(r, level);
        }
    }

    /// Fermeture de l'entrepôt (croix, ou joueur éloigné) : op 2.
    fn guild_warehouse_close(&mut self) {
        if std::mem::take(&mut self.guild.gware.open) {
            send(self.net.as_ref(), GuildCommand::WarehouseClose);
        }
        self.guild.gware.money_ask = None;
        self.guild.gware.npc = None;
    }

    /// Objets de l'entrepôt (statut 53) pour la fenêtre ; fermeture à plus de 149 du point d'ouverture.
    fn guild_warehouse_frame(&mut self) {
        let w = &self.guild.gware;
        if !w.open {
            return;
        }
        let here = self.player.as_ref().map(|p| (p.position.x, p.position.z));
        if let (Some((x0, z0)), Some((x, z))) = (w.at, here) {
            if ((x - x0).powi(2) + (z - z0).powi(2)).sqrt() > 149.0 {
                return self.guild_warehouse_close();
            }
        }
        let assets = self.assets.clone();
        let mut items: Vec<GwareItem> = self
            .items
            .values()
            .filter(|i| i.status == Some(guild::warehouse::ITEM_STATUS))
            .filter_map(|i| {
                let tid = i.template_id?;
                let name = crate::scene::item_name_for_tid(&assets, tid).unwrap_or_default();
                Some(GwareItem { iid: i.id, tid, count: i.count.unwrap_or(1), grid: i.grid?, name })
            })
            .collect();
        items.sort_by_key(|i| i.grid);
        self.guild.gware.items = items;
    }

    /// Déconnexion : plus de guilde ni de fenêtre.
    /// « Terminer » d'`UI_GuildBattleMember` (FUN_1408c59a0) : assez de cochés → op 14, puis la demande (mode 0,
    /// FUN_14084e150) ou l'acceptation (mode 1, FUN_14084e080).
    fn guild_battle_members_done(&mut self) {
        let Some(p) = self.guild.picker.clone() else { return };
        let chosen = p.chosen();
        if (chosen.len() as i32) < p.count {
            let t = self.guild.data.as_ref().map(|d| d.msg("GuildBattleMember_LackMember")).unwrap_or_default();
            self.guild_message(&t, 0xFFFF_FFFF);
            return;
        }
        self.guild.picker = None;
        self.guild.open.remove(&Win::BattleMember);
        let guild = self.guild.state.name().to_string();
        let net = self.net.as_ref();
        send(net, GuildCommand::BattlePerson { guild: guild.clone(), count: p.count as i16, names: chosen });
        match p.accept {
            Some(enemy) => send(net, GuildCommand::BattleAnswer { guild, enemy, yes: true }),
            None => {
                let f = self.guild.battle_form.clone();
                let duration = guild::battle::MINUTES[f.time.min(5)] * 60;
                send(net, GuildCommand::BattleRequest { guild, enemy: f.enemy, kind: f.kind as i32, duration, person: p.count });
            }
        }
    }

    pub(super) fn guild_reset(&mut self) {
        let data = self.guild.data.take();
        self.guild = GuildGlue { data, ..Default::default() };
    }
}

/// Op 27 : listes reçues ; un quatrième entier à -1 garde l'ancien (FUN_1404dee40).
fn merge_fighters(side: &mut Vec<BattleFighter>, list: &[BattleFighter]) {
    for f in list {
        match side.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&f.name)) {
            Some(x) => {
                (x.points, x.b, x.c) = (f.points, f.b, f.c);
                if f.d != -1 {
                    x.d = f.d;
                }
            }
            None => side.push(f.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choix_des_combattants() {
        let mut s = GuildState::default();
        for (name, rank, online) in [("Chef", guild::rank::MASTER, true), ("Bo", 1, true), ("Zed", 1, false), ("Al", 1, true), ("Ny", 1, true)] {
            s.members.push(GuildMember { name: name.into(), rank, online, level: 30, ..Default::default() });
        }
        let mut p = BattlePicker::new(&s, None, 3);
        assert_eq!(p.rows.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(), ["Chef", "Bo", "Al", "Ny"]);
        assert_eq!(p.chosen(), vec!["Chef".to_string()]);
        p.random("Chef", 12345);
        let c = p.chosen();
        assert_eq!(c.len(), 3);
        assert!(c.contains(&"Chef".to_string()));
        let mut side = vec![BattleFighter { name: "Al".into(), points: 1, d: 7, ..Default::default() }];
        merge_fighters(&mut side, &[BattleFighter { name: "al".into(), points: 5, d: -1, ..Default::default() }, BattleFighter { name: "Bo".into(), ..Default::default() }]);
        assert_eq!((side.len(), side[0].points, side[0].d), (2, 5, 7));
    }

    #[test]
    fn textes_d_archonia() {
        assert_eq!(printf("Il vous faut %d Or pour creer une guilde.", &[&10_000]), "Il vous faut 10000 Or pour creer une guilde.");
        assert_eq!(fill_braces("An invite has been sent from the guild {{Guild_Join_GuildID.Guild_Join_GuildID}}.  Do you wish to accept?", "Rust"), "An invite has been sent from the guild Rust.  Do you wish to accept?");
        assert_eq!(shown_name("Bob@serveur"), "Bob...");
        assert_eq!(colored("x", 0xFFFF_A500), "|cffffa500x|r");
    }
}
