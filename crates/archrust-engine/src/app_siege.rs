//! Guerres de siège des châteaux (AgpmSiegeWar 0x49, AgpmEventSiegeWarNPC 0x50, AgpmWarInfo 0x61) et fenêtres
//! d'AgcmUISiegeWar d'Archonia (voir `hud_siege.rs`).
//!
//! * **Siege Commander** (fonction de PNJ 33, bouton sysstr 529 « Apply for a Siege War ») : op 0x50/1, l'accord
//!   (op 2) ouvre `UI_SiegeWarMain` pour le château de la région du PNJ (`FUN_140628bb0`) : `SiegeWarMainText`
//!   (nom du PNJ sans ses balises `<…>`, château), date du siège (`FUN_14062db10` : prochain siège si statut ≠ 0,
//!   sinon dernier ; `-Siege war time\n M/D/Y H:MM(GST) SiegeWarReserve|SiegeWarComplete`).
//! * **Déclaration** (`UI_SiegeWarSetTIme`, `FUN_1406292c0`) : propriétaire seulement (`SiegeWarRequireCastleMaster`),
//!   statut 0 (`SiegeWarCannotSetSiegeWarTime`) ; aide `SiegeWarSetTimeHelp` + « Expiry : n Hour » ; 2 jours (base =
//!   dernier siège + Duration4nextSW jours, la veille si dimanche ; sans siège : samedi de la semaine de
//!   « maintenant + Duration − 7 j », `FUN_14021da00` / `FUN_14021da70`) × 20, 21, 22 h ; op 1 en FILETIME local.
//! * **Candidatures** : défense (op 0xD à l'ouverture, op 3 après contrôle local du chef et de la condition
//!   30 membres / 14 jours, choix des défenseurs op 8 → `UI_SiegeWarSelectDefense` → op 0xB puis 0xD), attaque
//!   (op 7 page 1 à l'ouverture, op 5).
//! * **Résultats** (op 2, `FUN_14062a0b0`) : boîtes de l'`[EventMap]` ou phrases d'`uimessage.txt` ; 0x10 ouvre
//!   `UI_CarveMessage`, 0x11 / 0x12 la ferment.
//! * **Objets de siège** (`FUN_140713570`, au clic sur un personnage) : trône (type 6) si le carving est permis
//!   (`FUN_14021e680`) → op 0x1B ; catapulte / camp (10 / 9) pour une guilde assaillante → op 0x10, réponse 0x13
//!   (0x0E) → `UI_SiegeWarCatapult` / `UI_SiegeWarAttackRes` (grille, 0x16 / 0x17, 0x12, 0x14), barre
//!   `UI_SiegeWarRepair`.
//! * **Messages** (op 0x1C, `FUN_14062ade0`) : discussion (type SiegeWar, `SIEGEWARINFO_CHAT`) et centre de l'écran ;
//!   sons `SiegeWar_Start.mp3` / `SiegeWar_Close.mp3` ; comptes à rebours (op 0x1D) de même.
//! * **Résurrection** (`FUN_14021db60`, `FUN_1404f9bb0`) : bouton du château (`CONTROL_26364`) dans `UI_Resurrection`
//!   pendant la guerre si un camp (assaillants) ou la tour de résurrection (défenseurs) est debout ; assaillant ou
//!   défenseur de Rensphere : AgpmCharacter op 0x13 ; autre défenseur : `UI_ResurrectSiege` (0x13 / 0x14).
//! * **Tableau** `UI_WarInfo` (AgpmWarInfo) : étapes de `warinfodata.txt`, compteur, scores.
//!
//! Choix d'ArchRust : `UI_SiegeWarMain` d'Archonia n'a pas de bouton pour `SiegeWarOpenSiegeWarTimeSetUI` ; la date
//! (`CONTROL_EDIT_WAR_TIME`) l'ouvre au clic. Les téléportations de siège (`SpecialType` 2, 3, 4) passent par la
//! fenêtre des portails existante (destinations rendues par le serveur).

use super::App;
use crate::assets::Assets;
use crate::hud::{HudState, SiegeGuildRow, SiegeUi, SiegeView};
use crate::net::NetLink;
use crate::ui::UiState;
use archrust_netclient::siege::{self, msg, object, result, status, war, Castle, GuildPage, ObjectInfo, SiegeCommand, SiegeEvent, WarInfoEvent};
use archrust_netclient::world::{ItemRecord, WorldEvent};
use archrust_netclient::Command;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Statut AgpmItem d'un objet chargé dans un objet de siège, d'un objet du sac.
const STATUS_SIEGE_OBJECT: i8 = 90;
const STATUS_INVENTORY: i8 = 2;
/// Couleur des messages de siège (`SIEGEWARINFO_CHAT` de color.ini) et onglet de discussion.
const CHAT_COLOR: u32 = 0xC8FFC8;
const CHAT_MASK: u16 = 0x80;
const CENTER_TIME: Duration = Duration::from_secs(5);
/// Régions château / abords par indice de SiegeWar.txt (RegionTemplate.ini), si les noms ne se retrouvent pas.
const DEFAULT_REGIONS: [[u16; 2]; 5] = [[81, 88], [80, 76], [78, 79], [75, 77], [131, 132]];

/// Ligne de SiegeWar.txt.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CastleData {
    pub index: usize,
    pub name: String,
    pub regions: Vec<u16>,
    pub playing_ms: i64,
    pub stamping_ms: i64,
    pub declaring_hours: i64,
    pub duration_days: i64,
}

#[derive(Debug, Default)]
pub struct SiegeData {
    pub castles: Vec<CastleData>,
    /// `SiegeWarCharType` par TID (CharacterDataTable.txt).
    pub types: HashMap<i32, i32>,
    /// (type de guerre, étape) → message d'étape, 3 messages centraux.
    pub steps: HashMap<(u8, u8), (String, [String; 3])>,
    /// Messages de l'`[EventMap]`.
    pub events: HashMap<String, String>,
}

/// Lit SiegeWar.txt (Index, Castle, Region « château/abords », SWPlayingTime, ArchonEyeOpenTime, StampingTime,
/// NextSWDeclaring, SelectGuildForSW, Duration4nextSW, …) ; `region_id` rend l'indice d'un nom de région.
pub fn parse_castles(text: &str, region_id: impl Fn(&str) -> Option<u16>) -> Vec<CastleData> {
    let mut out = Vec::new();
    for line in text.lines().skip(1) {
        let c: Vec<&str> = line.split('\t').map(str::trim).collect();
        let num = |i: usize| c.get(i).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
        let Some(index) = c.first().and_then(|v| v.parse::<usize>().ok()) else { continue };
        let name = c.get(1).copied().unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        let mut regions: Vec<u16> = c.get(2).map(|r| r.split('/').filter_map(|n| region_id(n.trim())).collect()).unwrap_or_default();
        if regions.is_empty() {
            regions = DEFAULT_REGIONS.get(index).map(|r| r.to_vec()).unwrap_or_default();
        }
        out.push(CastleData { index, name, regions, playing_ms: num(3), stamping_ms: num(5), declaring_hours: num(6), duration_days: num(8) });
    }
    out
}

/// Colonne `SiegeWarCharType` de CharacterDataTable.txt.
pub fn parse_siege_types(text: &str) -> HashMap<i32, i32> {
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap_or_default().split('\t').map(str::trim).collect();
    let (Some(tid), Some(ty)) = (header.iter().position(|h| h.eq_ignore_ascii_case("TID")), header.iter().position(|h| h.eq_ignore_ascii_case("SiegeWarCharType"))) else {
        return HashMap::new();
    };
    lines
        .filter_map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            let t = c.get(ty)?.trim().parse::<i32>().ok().filter(|t| *t > 0)?;
            Some((c.get(tid)?.trim().parse().ok()?, t))
        })
        .collect()
}

/// warinfodata.txt : TYPE (SiegeWar, BattleGround, ShrineBattle, ArchlordBattle), STEP, STEP MESSAGE, 3 CENTER MESSAGE.
pub fn parse_war_steps(text: &str) -> HashMap<(u8, u8), (String, [String; 3])> {
    let mut out = HashMap::new();
    for line in text.lines().skip(1) {
        let c: Vec<&str> = line.split('\t').collect();
        let kind = match c.first().map(|t| t.trim()) {
            Some("SiegeWar") => war::SIEGE,
            Some("BattleGround") => war::BATTLEGROUND,
            Some("ShrineBattle") => war::SHRINE,
            Some("ArchlordBattle") => war::ARCHLORD,
            _ => continue,
        };
        let Some(step) = c.get(1).and_then(|s| s.trim().parse::<u8>().ok()) else { continue };
        let get = |i: usize| c.get(i).map(|s| s.trim().replace("<\\n>", "\n")).unwrap_or_default();
        out.insert((kind, step), (get(2), [get(3), get(4), get(5)]));
    }
    out
}

// ---- Dates FILETIME (100 ns depuis 1601, heure locale comme Archonia) ----

const FILETIME_PER_SEC: u64 = 10_000_000;
const UNIX_TO_1601: i64 = 11_644_473_600;

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { yoe + era * 400 + 1 } else { yoe + era * 400 }, m, d)
}

/// Date d'un FILETIME : (année, mois, jour, heure, minute, jour de la semaine 0 = dimanche).
pub fn filetime_parts(ft: u64) -> (i64, u32, u32, u32, u32, u32) {
    let secs = (ft / FILETIME_PER_SEC) as i64 - UNIX_TO_1601;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    (y, m, d, (rem / 3600) as u32, ((rem % 3600) / 60) as u32, (days + 4).rem_euclid(7) as u32)
}

/// FILETIME d'une date à l'heure `hour` pile.
pub fn filetime_of(y: i64, m: u32, d: u32, hour: u32) -> u64 {
    let secs = days_from_civil(y, m, d) * 86_400 + hour as i64 * 3600 + UNIX_TO_1601;
    secs.max(0) as u64 * FILETIME_PER_SEC
}

const DAY: u64 = 86_400 * FILETIME_PER_SEC;

/// Heure locale actuelle en FILETIME (`FUN_1404964d0`).
pub fn local_now() -> u64 {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::SYSTEMTIME;
        use windows_sys::Win32::System::SystemInformation::GetLocalTime;
        let mut t = SYSTEMTIME { wYear: 0, wMonth: 0, wDayOfWeek: 0, wDay: 0, wHour: 0, wMinute: 0, wSecond: 0, wMilliseconds: 0 };
        // SAFETY : `GetLocalTime` remplit la structure fournie, valide et correctement alignée.
        unsafe { GetLocalTime(&mut t) };
        filetime_of(t.wYear as i64, t.wMonth as u32, t.wDay as u32, t.wHour as u32) + (t.wMinute as u64 * 60 + t.wSecond as u64) * FILETIME_PER_SEC
    }
    #[cfg(not(windows))]
    {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        (secs as i64 + UNIX_TO_1601) as u64 * FILETIME_PER_SEC
    }
}

/// Premier jour proposé à la déclaration (`FUN_14021da00` / `FUN_14021da70`).
pub fn declare_base(last: u64, duration_days: i64, now: u64) -> u64 {
    if last != 0 {
        let base = last.saturating_add_signed(duration_days * DAY as i64);
        if filetime_parts(base).5 == 0 {
            return base - DAY;
        }
        return base;
    }
    let t = now.saturating_add_signed((duration_days - 7) * DAY as i64);
    let wd = filetime_parts(t).5 as u64;
    t + (6 - wd) * DAY
}

/// Les 2 jours (base et lendemain) × 3 heures (20, 21, 22 h) : FILETIME du créneau choisi.
pub fn declare_date(base: u64, day: usize, hour: usize) -> u64 {
    let (y, m, d, ..) = filetime_parts(base + day.min(1) as u64 * DAY);
    filetime_of(y, m, d, 20 + hour.min(2) as u32)
}

/// Joueur, pour les contrôles locaux des fenêtres.
#[derive(Debug, Clone, Default)]
pub struct SiegeMe {
    pub name: String,
    pub guild: String,
    pub master: bool,
    pub members: usize,
    /// Création de la guilde (secondes Unix).
    pub guild_created: i64,
    pub dead: bool,
    /// Libellé du bouton de résurrection de siège (château), s'il est permis.
    pub resurrect: Option<String>,
    /// Résurrection directe (op 0x13) plutôt que `UI_ResurrectSiege`.
    pub resurrect_direct: bool,
}

/// Tableau `UI_WarInfo`.
#[derive(Debug, Clone, Default)]
struct WarBoard {
    war: u8,
    step: u8,
    center: usize,
    args: [String; 3],
    counter: Option<u8>,
    scores: [Option<(i32, i32)>; 2],
}

#[derive(Default)]
pub struct SiegeGlue {
    data: Option<SiegeData>,
    /// Châteaux connus (nom en minuscules).
    castles: HashMap<String, Castle>,
    attackers: HashMap<String, Vec<String>>,
    defenders: HashMap<String, Vec<String>>,
    /// Fenêtres ouvertes : château et PNJ de `UI_SiegeWarMain`.
    castle: String,
    npc_name: String,
    show_main: bool,
    show_set_time: bool,
    show_defense: bool,
    show_attack: bool,
    show_select: bool,
    date_choice: usize,
    hour_choice: usize,
    attack_page: GuildPage,
    defense_page: GuildPage,
    selected: Option<usize>,
    /// Objet de siège ouvert : CID, type, fiche, réception de la fiche.
    object: Option<(i32, i32, ObjectInfo, Instant)>,
    carving: bool,
    resurrect_window: bool,
    boxes: Vec<String>,
    center: Vec<(String, Instant)>,
    board: Option<WarBoard>,
}

impl SiegeGlue {
    fn data(&mut self, assets: &Assets) -> &SiegeData {
        self.data.get_or_insert_with(|| {
            let regions: HashMap<String, u16> = assets
                .table("ini", "RegionTemplate.ini")
                .map(|t| t.sections.iter().filter_map(|s| Some((s.get("Name")?.trim().to_string(), s.name.trim().parse().ok()?))).collect())
                .unwrap_or_default();
            let castles = parse_castles(&super::npc_glue::client_text_in(assets, "ini", "SiegeWar.txt"), |n| regions.get(n).copied());
            let d = SiegeData {
                castles,
                types: parse_siege_types(&super::npc_glue::client_text_in(assets, "ini", "CharacterDataTable.txt")),
                steps: parse_war_steps(&super::npc_glue::client_text(assets, "warinfodata.txt")),
                events: super::portal_glue::ui_events(assets),
            };
            log::info!("siège : {} château(x), {} objet(s) de siège", d.castles.len(), d.types.len());
            d
        })
    }

    fn castle(&self, name: &str) -> Option<&Castle> {
        self.castles.get(&name.to_lowercase())
    }

    fn castle_data(&self, name: &str) -> Option<&CastleData> {
        self.data.as_ref()?.castles.iter().find(|c| c.name.eq_ignore_ascii_case(name))
    }

    fn in_list(list: &HashMap<String, Vec<String>>, castle: &str, guild: &str) -> bool {
        !guild.is_empty() && list.get(&castle.to_lowercase()).is_some_and(|l| l.iter().any(|g| g.eq_ignore_ascii_case(guild)))
    }

    fn is_attacker(&self, castle: &str, guild: &str) -> bool {
        Self::in_list(&self.attackers, castle, guild)
    }

    /// Défenseurs : guilde propriétaire et guildes retenues.
    fn is_defender(&self, castle: &str, guild: &str) -> bool {
        Self::in_list(&self.defenders, castle, guild) || (!guild.is_empty() && self.castle(castle).and_then(|c| c.guild.as_deref()).is_some_and(|g| g.eq_ignore_ascii_case(guild)))
    }

    fn event_box(&mut self, assets: &Assets, event: &str) {
        let t = self.data(assets).events.get(event).cloned().unwrap_or_default();
        if !t.is_empty() {
            self.boxes.push(t);
        }
    }

    fn push_center(&mut self, text: String) {
        if !text.is_empty() {
            self.center.push((text, Instant::now() + CENTER_TIME));
            let excess = self.center.len().saturating_sub(4);
            self.center.drain(..excess);
        }
    }

    fn close_all(&mut self) {
        self.show_main = false;
        self.show_set_time = false;
        self.show_defense = false;
        self.show_attack = false;
        self.show_select = false;
    }
}

fn send(net: Option<&NetLink>, c: SiegeCommand) {
    if let Some(net) = net {
        net.send(Command::Siege(c));
    }
}

fn msg_text(hud: &mut HudState, assets: &Assets, key: &str) -> String {
    crate::hud::ui_message(hud, assets, key)
}

/// Date du siège de `UI_SiegeWarMain` (`FUN_14062db10`, forme « (GST) » d'Archonia).
pub fn war_time_text(c: &Castle, label: &str, reserve: &str, complete: &str) -> String {
    let st = c.status.unwrap_or(0);
    let ft = if st != 0 { c.next.unwrap_or(0) } else { c.last.unwrap_or(0) };
    if c.last.unwrap_or(0) == 0 && c.next.unwrap_or(0) == 0 || ft == 0 {
        return String::new();
    }
    let (y, m, d, h, mi, _) = filetime_parts(ft);
    format!("-{label}\n {m}/{d}/{y} {h}:{mi:02}(GST) {}", if st != 0 { reserve } else { complete })
}

/// Nom du PNJ sans ses balises `<…>` (`FUN_14062d350`).
fn strip_tags(name: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in name.chars().take(0x7f) {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// Fenêtres, boîtes et messages (passe egui).
#[allow(clippy::too_many_arguments)]
pub fn panel(ctx: &egui::Context, glue: &mut SiegeGlue, me: &SiegeMe, items: &HashMap<i32, ItemRecord>, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>, ui: &mut UiState) {
    glue.data(assets);
    let castle = glue.castle(&glue.castle.clone()).cloned().unwrap_or_else(|| Castle { name: glue.castle.clone(), ..Default::default() });
    let war_time = war_time_text(&castle, &msg_text(hud, assets, "SiegeWarDate"), &msg_text(hud, assets, "SiegeWarReserve"), &msg_text(hud, assets, "SiegeWarComplete"));
    let name = castle.name.clone();
    let mut out = Vec::new();

    if glue.show_main {
        let desc = super::pvp_glue::fill(&msg_text(hud, assets, "SiegeWarMainText"), &[strip_tags(&glue.npc_name), name.clone()]);
        let view = SiegeView { texts: vec![("CONTROL_30695", glue.npc_name.clone()), ("CONTROL_EDIT_DESC", desc), ("CONTROL_EDIT_WAR_TIME", war_time.clone())], clickable: vec!["CONTROL_EDIT_WAR_TIME"], ..Default::default() };
        crate::hud::draw_siege_window(ctx, hud, assets, "UI_SiegeWarMain", &view, &mut out);
        for a in out.drain(..) {
            let SiegeUi::Clicked(c) = a else { continue };
            match c.as_str() {
                "CONTROL_4970" => glue.show_main = false,
                // `SiegeWarOpenDefenseApplicationUI` : op 0xD.
                "CONTROL_5460" => {
                    glue.show_main = false;
                    glue.show_defense = true;
                    send(net, SiegeCommand::RequestDefenders(name.clone()));
                }
                // `SiegeWarOpenAttackApplicationUI` : op 7, page 1.
                "CONTROL_3154" => {
                    glue.show_main = false;
                    glue.show_attack = true;
                    send(net, SiegeCommand::AttackPage { castle: name.clone(), page: 1 });
                }
                // `SiegeWarOpenSiegeWarTimeSetUI` (`FUN_1406292c0`).
                "CONTROL_EDIT_WAR_TIME" => {
                    let owner = castle.owner.clone().unwrap_or_default();
                    if owner.is_empty() || !owner.eq_ignore_ascii_case(&me.name) {
                        glue.event_box(assets, "SiegeWarRequireCastleMaster");
                    } else if castle.status.unwrap_or(0) != status::NONE {
                        glue.event_box(assets, "SiegeWarCannotSetSiegeWarTime");
                    } else {
                        glue.show_set_time = true;
                    }
                }
                _ => {}
            }
        }
    }

    if glue.show_set_time {
        let data = glue.castle_data(&name).cloned().unwrap_or_default();
        let now = local_now();
        let last = castle.last.unwrap_or(0);
        let mut help = msg_text(hud, assets, "SiegeWarSetTimeHelp");
        if last != 0 {
            // Échéance de la déclaration : fin du dernier siège + NextSWDeclaring heures.
            let end = last as i64 + (data.playing_ms * 10_000) + data.declaring_hours * 3600 * FILETIME_PER_SEC as i64;
            let left = ((end - now as i64) / FILETIME_PER_SEC as i64).max(0);
            let (n, unit) = if left < 3600 { (left / 60, "Time_Unit_Minute") } else { (left / 3600, "Time_Unit_Hour") };
            help = format!("{help}\n\n{} : {n} {}", msg_text(hud, assets, "SiegeWarRemainTime"), msg_text(hud, assets, unit));
        }
        let base = declare_base(last, data.duration_days, now);
        let mut day_label = |ft: u64| {
            let (y, m, d, ..) = filetime_parts(ft);
            format!("{y}{} {m}{} {d}{}", msg_text(hud, assets, "SiegeWarDateYear"), msg_text(hud, assets, "SiegeWarDateMonth"), msg_text(hud, assets, "SiegeWarDateDay"))
        };
        let days = vec![day_label(base), day_label(base + DAY)];
        let hours: Vec<String> = (8..11).map(|h| format!("{} {h}{}", msg_text(hud, assets, "SiegeWarDatePM"), msg_text(hud, assets, "SiegeWarDateHour"))).collect();
        let view = SiegeView {
            texts: vec![("CONTROL_AttackHelpEdit", help), ("CONTROL_AttackSiegeWarTimeEdit", war_time.clone())],
            combos: vec![("CONTROL_DateCombo", days, glue.date_choice), ("CONTROL_TimeCombo", hours, glue.hour_choice)],
            ..Default::default()
        };
        crate::hud::draw_siege_window(ctx, hud, assets, "UI_SiegeWarSetTIme", &view, &mut out);
        for a in out.drain(..) {
            match a {
                SiegeUi::Combo(c, i) if c == "CONTROL_DateCombo" => glue.date_choice = i,
                SiegeUi::Combo(_, i) => glue.hour_choice = i,
                SiegeUi::Clicked(c) if c == "CONTROL_18664" => glue.show_set_time = false,
                // `SiegeWarConfirmSiegeWarTime` : op 1.
                SiegeUi::Clicked(c) if c == "CONTROL_30841" => {
                    glue.show_set_time = false;
                    send(net, SiegeCommand::Declare { castle: name.clone(), date: declare_date(base, glue.date_choice, glue.hour_choice) });
                }
                _ => {}
            }
        }
    }

    if glue.show_defense {
        let defenders: Vec<String> = glue.defenders.get(&name.to_lowercase()).cloned().unwrap_or_default();
        let info = if defenders.is_empty() { msg_text(hud, assets, "SiegeWarNotSelectedDefenseGuild") } else { format!("{} : {}", msg_text(hud, assets, "SiegeWarDefenseGuild"), defenders.join(", ")) };
        let view = SiegeView { texts: vec![("CONTROL_DefenseHelpEdit", msg_text(hud, assets, "SiegeWarDefenseGuildHelp")), ("CONTROL_DefenseGuildInfoEdit", info)], ..Default::default() };
        crate::hud::draw_siege_window(ctx, hud, assets, "UI_SiegeWarApplDefense", &view, &mut out);
        for a in out.drain(..) {
            let SiegeUi::Clicked(c) = a else { continue };
            match c.as_str() {
                "CONTROL_10206" | "CONTROL_32024" => glue.show_defense = false,
                "CONTROL_11218" => {
                    glue.show_defense = false;
                    glue.show_main = true;
                }
                // `SiegeWarApplicationDefense` : contrôle local du chef et de la condition, puis op 3.
                "CONTROL_23908" => {
                    let days = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64) - me.guild_created) / 86_400;
                    if !me.master {
                        glue.event_box(assets, "SiegeWarRequireGuildMaster");
                    } else if me.members < 30 || days < 14 {
                        glue.event_box(assets, "SiegeWarRequireGuildCondition");
                    } else {
                        send(net, SiegeCommand::ApplyDefense(name.clone()));
                    }
                }
                // `SiegeWarSelectDefenseGuild` : op 8 et `UI_SiegeWarSelectDefense`.
                "CONTROL_23507" => {
                    glue.show_select = true;
                    glue.selected = None;
                    send(net, SiegeCommand::DefensePage { castle: name.clone(), page: 1 });
                }
                _ => {}
            }
        }
    }

    let rows = |p: &GuildPage| p.entries.iter().map(|g| SiegeGuildRow { name: g.name.clone(), master: g.master.clone(), points: g.points.to_string(), disabled: false }).collect::<Vec<_>>();
    if glue.show_attack {
        let view = SiegeView { rows: rows(&glue.attack_page), page: glue.attack_page.page.max(1) as usize, pages: glue.attack_page.pages.max(1) as usize, ..Default::default() };
        crate::hud::draw_siege_window(ctx, hud, assets, "UI_SiegeWarApplAttack", &view, &mut out);
        let (page, pages) = (view.page as i16, view.pages as i16);
        for a in out.drain(..) {
            let SiegeUi::Clicked(c) = a else { continue };
            let go = |p: i16| send(net, SiegeCommand::AttackPage { castle: name.clone(), page: p.clamp(1, pages) });
            match c.as_str() {
                "CONTROL_2049" | "CONTROL_30092" => glue.show_attack = false,
                "CONTROL_18915" => send(net, SiegeCommand::ApplyAttack(name.clone())),
                "CONTROL_9096" => go(page - 1),
                "CONTROL_18167" => go(page + 1),
                p => {
                    if let Some(n) = p.strip_prefix("PAGE:").and_then(|n| n.parse().ok()) {
                        go(n);
                    }
                }
            }
        }
    }

    if glue.show_select {
        let view = SiegeView { rows: rows(&glue.defense_page), selected: glue.selected, page: glue.defense_page.page.max(1) as usize, pages: glue.defense_page.pages.max(1) as usize, ..Default::default() };
        crate::hud::draw_siege_window(ctx, hud, assets, "UI_SiegeWarSelectDefense", &view, &mut out);
        let (page, pages) = (view.page as i16, view.pages as i16);
        for a in out.drain(..) {
            match a {
                SiegeUi::Row(i) => glue.selected = Some(i),
                SiegeUi::Clicked(c) => {
                    let go = |p: i16| send(net, SiegeCommand::DefensePage { castle: name.clone(), page: p.clamp(1, pages) });
                    match c.as_str() {
                        "CONTROL_22160" | "CONTROL_29265" => glue.show_select = false,
                        // `SiegeWarRegisterDefenseGuild` : op 0xB puis 0xD.
                        "CONTROL_21546" => {
                            if let Some(g) = glue.selected.and_then(|i| glue.defense_page.entries.get(i)) {
                                send(net, SiegeCommand::SelectDefender { castle: name.clone(), guild: g.name.clone() });
                                send(net, SiegeCommand::RequestDefenders(name.clone()));
                            }
                        }
                        "CONTROL_1649" => go(page - 1),
                        "CONTROL_26532" => go(page + 1),
                        p => {
                            if let Some(n) = p.strip_prefix("PAGE:").and_then(|n| n.parse().ok()) {
                                go(n);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Catapulte (10) ou camp de résurrection (9).
    if let Some((target, kind, info, at)) = glue.object.clone() {
        let catapult = kind == object::CATAPULT;
        let (win, desc_ctl, desc_key) = if catapult { ("UI_SiegeWarCatapult", "CONTROL_EDIT_CATAPULT_DESC", "SiegeWarCatapultDesc") } else { ("UI_SiegeWarAttackRes", "CONTROL_EDIT_ATKRES_DESC", "SiegeWarAtkResTowerDesc") };
        let args = [info.repairs_left.unwrap_or(0).to_string(), info.repair_cost.unwrap_or(0).to_string(), info.repair_minutes.unwrap_or(0).to_string()];
        let desc = super::pvp_glue::fill(&msg_text(hud, assets, desc_key), &args);
        let view = SiegeView { texts: vec![(desc_ctl, desc)], grid: Some(info.items), ..Default::default() };
        crate::hud::draw_siege_window(ctx, hud, assets, win, &view, &mut out);
        for a in out.drain(..) {
            match a {
                SiegeUi::Clicked(c) => match c.as_str() {
                    "CONTROL_17640" | "CONTROL_15620" => glue.object = None,
                    "CONTROL_11882" | "CONTROL_4004" => send(net, SiegeCommand::Repair(target)),
                    "CONTROL_22021" | "CONTROL_26506" => send(net, SiegeCommand::Activate(target)),
                    _ => {}
                },
                SiegeUi::Drop(drag) => {
                    if items.get(&drag.iid).is_some_and(|i| i.status == Some(STATUS_INVENTORY)) {
                        send(net, SiegeCommand::LoadItem { target, iid: drag.iid });
                    }
                }
                SiegeUi::Unload(cell) => {
                    let tid = info.items[cell];
                    if let Some(i) = items.values().find(|i| i.status == Some(STATUS_SIEGE_OBJECT) && i.template_id == Some(tid)) {
                        send(net, SiegeCommand::UnloadItem { target, iid: i.id });
                    }
                }
                _ => {}
            }
        }
        // Barre de réparation (`SiegeWarOpenRepairBarUI`).
        if let (Some(elapsed), Some(min)) = (info.repair_elapsed.filter(|e| *e > 0), info.repair_minutes.filter(|m| *m > 0)) {
            let done = elapsed as f32 + at.elapsed().as_secs_f32();
            let total = min as f32 * 60.0;
            if done < total {
                crate::hud::draw_siege_window(ctx, hud, assets, "UI_SiegeWarRepair", &SiegeView { bar: Some(done / total), ..Default::default() }, &mut out);
                out.clear();
            }
        }
    }

    if glue.carving {
        crate::hud::draw_carve_message(ctx, hud, assets);
    }

    // Résurrection de siège : bouton du château dans `UI_Resurrection`, puis `UI_ResurrectSiege`.
    if me.dead {
        if let Some(label) = &me.resurrect {
            if crate::hud::draw_resurrection_siege(ctx, hud, assets, label) {
                if me.resurrect_direct {
                    send(net, SiegeCommand::Resurrect { inner: true });
                } else {
                    glue.resurrect_window = true;
                }
            }
        }
        if glue.resurrect_window {
            crate::hud::draw_siege_window(ctx, hud, assets, "UI_ResurrectSiege", &SiegeView::default(), &mut out);
            for a in out.drain(..) {
                let SiegeUi::Clicked(c) = a else { continue };
                match c.as_str() {
                    "CONTROL_27630" => send(net, SiegeCommand::Resurrect { inner: true }),
                    "CONTROL_26467" => send(net, SiegeCommand::Resurrect { inner: false }),
                    _ => {}
                }
                glue.resurrect_window = false;
            }
        }
    } else {
        glue.resurrect_window = false;
    }

    // Tableau des guerres.
    if let Some(b) = glue.board.clone() {
        let (step_msg, centers) = glue.data.as_ref().and_then(|d| d.steps.get(&(b.war, b.step))).cloned().unwrap_or_default();
        let center = centers.get(b.center.saturating_sub(1)).cloned().unwrap_or_default();
        let fill = |t: &str| super::pvp_glue::fill(t, &b.args);
        let mut texts = vec![("Step_Message_Edit", fill(&step_msg)), ("Center_Message_Edit", fill(&center))];
        if let Some(c) = b.counter {
            texts.push(("Time_Edit", format!("{c} {}", msg_text(hud, assets, "UiMsg_WarInfo_Min"))));
        }
        for (k, ctl) in [(0, ["CONTROL_7151", "CONTROL_5592"]), (1, ["CONTROL_28412", "CONTROL_29787"])] {
            if let Some((w, l)) = b.scores[k] {
                texts.push((ctl[0], w.to_string()));
                texts.push((ctl[1], l.to_string()));
            }
        }
        crate::hud::draw_siege_window(ctx, hud, assets, "UI_WarInfo", &SiegeView { texts, ..Default::default() }, &mut out);
        out.clear();
    }

    // Boîtes d'information, messages du centre.
    if let Some(text) = glue.boxes.first().cloned() {
        if crate::hud::info_box(ctx, hud, assets, &text) {
            glue.boxes.remove(0);
        }
    }
    let now = Instant::now();
    glue.center.retain(|(_, until)| now < *until);
    if !glue.center.is_empty() {
        let lines: Vec<(String, u32, f32)> = glue.center.iter().enumerate().map(|(i, (t, _))| (t.clone(), CHAT_COLOR, 230.0 + i as f32 * 20.0)).collect();
        crate::hud::draw_battle_texts(ctx, &lines, None);
    }
    let _ = ui;
}

fn colored(text: &str) -> String {
    format!("|cff{CHAT_COLOR:06x}{text}|r")
}

impl App {
    /// Château dont la région (château ou abords) contient `pos` (`FUN_1402123e0`).
    fn siege_castle_at(&mut self, x: f32, z: f32) -> Option<CastleData> {
        let assets = self.assets.clone();
        let region = self.zones.region_at(&assets, x, z)?;
        self.siege.data(&assets).castles.iter().find(|c| c.regions.contains(&region)).cloned()
    }

    fn siege_my_castle(&mut self) -> Option<CastleData> {
        let region = self.zone_region.as_ref()?.region;
        let assets = self.assets.clone();
        self.siege.data(&assets).castles.iter().find(|c| c.regions.contains(&region)).cloned()
    }

    /// Le joueur se tient dans un château en guerre (statut 3 à 8) dont sa guilde assiège ou défend
    /// (`FUN_1402123e0`, `FUN_14021dda0` / `FUN_14021dd20`) : « Revive in town » sans `UI_ResurrectTownConfirm`
    /// (`FUN_1404f86a0`, voir `app_combat.rs`).
    pub(super) fn siege_war_participant(&mut self) -> bool {
        let Some(cd) = self.siege_my_castle() else { return false };
        let guild = self.guild.state.name().to_string();
        let at_war = self.siege.castle(&cd.name).is_some_and(|c| status::at_war(c.status.unwrap_or(0)));
        at_war && (self.siege.is_attacker(&cd.name, &guild) || self.siege.is_defender(&cd.name, &guild))
    }

    /// Pour les taxes (voir `app_tax.rs`) : fiche du château où se tient le joueur (`FUN_1402123e0`) — nom,
    /// indice dans SiegeWar.txt, propriétaire connu — et nom de la fiche 0 (Rensphere, château de l'ArchLord).
    pub(super) fn siege_tax_castles(&mut self) -> (Option<(String, usize, Option<String>)>, Option<String>) {
        let assets = self.assets.clone();
        let first = self.siege.data(&assets).castles.iter().find(|c| c.index == 0).map(|c| c.name.clone());
        let here = self.siege_my_castle().map(|c| {
            let owner = self.siege.castle(&c.name).and_then(|k| k.owner.clone()).filter(|o| !o.is_empty());
            (c.name, c.index, owner)
        });
        (here, first)
    }

    /// Joueur et résurrection de siège, pour la passe egui.
    pub(super) fn siege_me(&mut self) -> SiegeMe {
        let Some(p) = self.player.as_ref() else { return SiegeMe::default() };
        let name = p.name.clone();
        let g = &self.guild.state;
        let guild = g.name().to_string();
        let mut me = SiegeMe {
            master: g.info.as_ref().is_some_and(|i| i.master.eq_ignore_ascii_case(&name)),
            members: g.members.len(),
            guild_created: g.info.as_ref().map_or(0, |i| i.created_at),
            dead: self.combat.player_dead,
            name,
            guild,
            ..Default::default()
        };
        // `FUN_14021db60` : château en guerre ; Rensphere hors statut 7 ; ailleurs camp (assaillant) ou tour de
        // résurrection (défenseur) debout.
        if me.dead {
            if let Some(cd) = self.siege_my_castle() {
                if let Some(c) = self.siege.castle(&cd.name) {
                    let st = c.status.unwrap_or(0);
                    let (att, def) = (self.siege.is_attacker(&cd.name, &me.guild), self.siege.is_defender(&cd.name, &me.guild));
                    let ok = status::at_war(st)
                        && if cd.index == 0 {
                            st != status::ARCHLORD && (att || def)
                        } else {
                            (att && c.attacker_res == Some(true)) || (def && c.defender_res == Some(true))
                        };
                    if ok {
                        me.resurrect = Some(cd.name.clone());
                        me.resurrect_direct = att || cd.index == 0;
                    }
                }
            }
        }
        me
    }

    /// Clic sur un personnage (`FUN_140713570`) : trône → carving, catapulte / camp → fenêtre. `true` s'il est pris.
    pub(super) fn siege_click(&mut self, cid: i32) -> bool {
        let Some((tid, pos)) = self.entities.iter().find(|e| e.cid == cid).map(|e| (e.tid, e.position)) else { return false };
        let assets = self.assets.clone();
        let Some(kind) = self.siege.data(&assets).types.get(&tid).copied() else { return false };
        if !matches!(kind, object::THRONE | object::ATTACK_RES_TOWER | object::CATAPULT) {
            return false;
        }
        let Some(cd) = self.siege_castle_at(pos.x, pos.z) else { return false };
        let me = self.siege_me();
        let Some(c) = self.siege.castle(&cd.name).cloned() else { return false };
        let st = c.status.unwrap_or(0);
        let (att, def) = (self.siege.is_attacker(&cd.name, &me.guild), self.siege.is_defender(&cd.name, &me.guild));
        let command = if kind == object::THRONE {
            // `FUN_14021e680` : chef d'une guilde assaillante ou défenseuse, vivant ; châteaux 1-4 au statut 5,
            // Rensphere au statut 3 sans carve guild ; première gravure par un assaillant non propriétaire, ensuite
            // par une autre guilde que la dernière.
            let carve = c.carve_guild.clone().unwrap_or_default();
            let phase = if cd.index == 0 { st == status::BATTLE && carve.is_empty() } else { st == status::EYES_OPEN };
            let owner = c.guild.clone().unwrap_or_default();
            let who = if carve.is_empty() { att && !owner.eq_ignore_ascii_case(&me.guild) } else { !carve.eq_ignore_ascii_case(&me.guild) };
            if !(me.master && !me.dead && (att || def) && phase && who) {
                return false;
            }
            SiegeCommand::Carve(cid)
        } else {
            if !att {
                return false;
            }
            SiegeCommand::OpenObject(cid)
        };
        self.addons.set_target(Some(cid));
        if let Some(net) = &self.net {
            net.send(Command::Siege(command));
        }
        true
    }

    /// Paquets du siège et du tableau des guerres.
    pub(super) fn on_siege_event(&mut self, ev: SiegeEvent) {
        let assets = self.assets.clone();
        self.siege.data(&assets);
        match ev {
            SiegeEvent::NpcGranted { npc } => {
                let pos = self.entities.iter().find(|e| e.cid == npc).map(|e| e.position);
                let castle = pos.and_then(|p| self.siege_castle_at(p.x, p.z)).or_else(|| self.siege_my_castle());
                let Some(castle) = castle else {
                    log::warn!("siège : château du PNJ {npc} introuvable");
                    return;
                };
                self.siege.close_all();
                self.siege.castle = castle.name;
                self.siege.npc_name = self.entities.iter().find(|e| e.cid == npc).map(|e| e.name.clone()).unwrap_or_default();
                self.siege.show_main = true;
            }
            SiegeEvent::Castle(c) => {
                let key = c.name.to_lowercase();
                let old = self.siege.castles.get(&key).and_then(|c| c.status);
                self.siege.castles.entry(key).or_insert_with(|| Castle { name: c.name.clone(), ..Default::default() }).merge(&c);
                // Statut 6 : la fenêtre de carving se ferme (`FUN_140713300`) ; statut 1 : listes vidées.
                match c.status {
                    Some(status::FINISHED) => self.siege.carving = false,
                    Some(status::DECLARED) if old != Some(status::DECLARED) => {
                        self.siege.attackers.remove(&c.name.to_lowercase());
                        self.siege.defenders.remove(&c.name.to_lowercase());
                    }
                    _ => {}
                }
            }
            SiegeEvent::Result { code, .. } => {
                match code {
                    result::CARVE_START => self.siege.carving = true,
                    result::CARVE_CANCEL | result::CARVE_END => self.siege.carving = false,
                    _ => {}
                }
                if let Some((boxed, key)) = result::message(code) {
                    if boxed {
                        self.siege.event_box(&assets, key);
                    } else {
                        let t = msg_text(&mut self.hud, &assets, key);
                        self.ui.push_chat_in(CHAT_MASK, colored(&t));
                        self.siege.push_center(t);
                    }
                }
            }
            SiegeEvent::AttackApplicants { page, .. } => self.siege.attack_page = page,
            SiegeEvent::DefenseApplicants { page, .. } => {
                self.siege.defense_page = page;
                self.siege.selected = None;
            }
            SiegeEvent::Attackers { castle, guilds } => {
                self.siege.attackers.insert(castle.to_lowercase(), guilds);
            }
            SiegeEvent::Defenders { castle, guilds } => {
                self.siege.defenders.insert(castle.to_lowercase(), guilds);
            }
            SiegeEvent::ObjectReply { op, flag, target, info } => {
                // Seule l'op 0x13 avec 0x0E ouvre la fenêtre de l'objet (`FUN_14062a7d0`).
                if op == siege::op::OBJECT_REPLY && flag == result::OPEN_OBJECT_UI {
                    let tid = self.entities.iter().find(|e| e.cid == target).map(|e| e.tid).unwrap_or(0);
                    let kind = self.siege.data(&assets).types.get(&tid).copied().unwrap_or(object::CATAPULT);
                    let info = info.or_else(|| self.siege.object.as_ref().filter(|o| o.0 == target).map(|o| o.2.clone())).unwrap_or(ObjectInfo { items: [-1; 8], ..Default::default() });
                    self.siege.object = Some((target, kind, info, Instant::now()));
                }
            }
            SiegeEvent::ItemLoaded { .. } => {}
            SiegeEvent::ObjectInfo { target, info } => {
                if let Some(o) = self.siege.object.as_mut().filter(|o| target == 0 || o.0 == target) {
                    o.2 = info;
                    o.3 = Instant::now();
                }
            }
            SiegeEvent::Message(m) => self.on_siege_message(m),
            SiegeEvent::Countdown { kind, seconds } => {
                let key = if kind == 1 { "SiegeWarMsgStartTime" } else { "SiegeWarMsgEndTime" };
                let t = format!("{} {}:{:02}", msg_text(&mut self.hud, &assets, key), seconds / 60, seconds % 60);
                self.ui.push_chat_in(CHAT_MASK, colored(&t));
                self.siege.push_center(t);
            }
            SiegeEvent::WarInfo(w) => self.on_war_info(w),
        }
    }

    fn on_siege_message(&mut self, m: siege::StatusMsg) {
        let assets = self.assets.clone();
        let mut t = |k: &str| msg_text(&mut self.hud, &assets, k);
        let fill = super::pvp_glue::fill;
        let mut lines = Vec::new();
        match m.code {
            msg::START => {
                lines.push(fill(&t("SiegeWarStatusStart"), &[m.castle.clone()]));
                self.audio.play_effect("sound/ui/SiegeWar_Start.mp3");
            }
            msg::OBJECT_DESTROYED => {
                let what = object::name_key(m.object_type as i32).map(&mut t).unwrap_or_default();
                lines.push(fill(&t("SiegeWarStatusDestroyObject"), &[what]));
            }
            msg::THRONE_RELEASED => lines.push(t("SiegeWarStatusReleaseThrone")),
            msg::EYE_OPENED => lines.push(t(if m.side == 0 { "SiegeWarStatusActiveArchonLeftEye" } else { "SiegeWarStatusActiveArchonRightEye" })),
            msg::EYE_CLOSED => lines.push(t("SiegeWarStatusFailedActiveArchonEye")),
            msg::CARVE_START => lines.push(fill(&t("SiegeWarStatusStartCarveGuild"), &[m.guild.clone()])),
            msg::CARVE_CANCEL => lines.push(fill(&t("SiegeWarStatusCancelCarveGuild"), &[m.guild.clone()])),
            msg::CARVE_DONE => {
                lines.push(fill(&t("SiegeWarStatusNewCarveGuild"), &[m.guild.clone()]));
                // Rensphere : le combat est suspendu jusqu'à la fin.
                let rensphere = self.siege.data.as_ref().and_then(|d| d.castles.iter().find(|c| c.name.eq_ignore_ascii_case(&m.castle))).is_some_and(|c| c.index == 0);
                if rensphere {
                    lines.push(t("SiegeWarStatusStopBattleUntilEnd"));
                }
            }
            msg::TERMINATE => {
                lines.push(t("SiegeWarStatusTerminate"));
                self.siege.carving = false;
                self.audio.play_effect("sound/ui/SiegeWar_Close.mp3");
            }
            msg::CAMP_ACTIVE_LEFT => lines.push(t("SiegeWarStatusActiveLeftAttackResTower")),
            msg::CAMP_ACTIVE_RIGHT => lines.push(t("SiegeWarStatusActiveRightAttackResTower")),
            msg::CAMP_DISABLED_LEFT => lines.push(fill(&t("SiegeWarStatusDisableLeftAttackResTower"), &[m.castle.clone()])),
            msg::CAMP_DISABLED_RIGHT => lines.push(fill(&t("SiegeWarStatusDisableRightAttackResTower"), &[m.castle.clone()])),
            msg::REPAIR_LEFT | msg::REPAIR_RIGHT => {
                let side = t(if m.code == msg::REPAIR_LEFT { "SiegeWarMsgLeft" } else { "SiegeWarMsgRight" });
                let what = t(if m.object_type as i32 == object::CATAPULT { "SiegeWarMsgCatapult" } else { "SiegeWarMsgAtkResTower" });
                lines.push(fill(&t("SiegeWarMsgStartRepair"), &[side, what]));
            }
            _ => {}
        }
        for l in lines.into_iter().filter(|l| !l.trim().is_empty()) {
            self.ui.push_chat_in(CHAT_MASK, colored(&l));
            self.siege.push_center(l);
        }
    }

    fn on_war_info(&mut self, w: WarInfoEvent) {
        let board = &mut self.siege.board;
        match w {
            WarInfoEvent::Step { war, step, .. } => {
                // Un Battleground n'écrase pas un tableau de Shrine en cours.
                if war == war::BATTLEGROUND && board.as_ref().is_some_and(|b| b.war == war::SHRINE) {
                    return;
                }
                let keep = board.as_ref().filter(|b| b.war == war).cloned().unwrap_or_default();
                *board = Some(WarBoard { war, step, center: 1, ..keep });
            }
            WarInfoEvent::Open { .. } => {}
            WarInfoEvent::Counter { value, .. } => {
                if let Some(b) = board {
                    b.counter = Some(value);
                }
            }
            WarInfoEvent::CenterMessage { index, .. } => {
                if let Some(b) = board {
                    b.center = index as usize;
                }
            }
            WarInfoEvent::Score { race, win, lose, .. } => {
                if let Some(b) = board {
                    b.scores[race as usize] = Some((win, lose));
                }
            }
            WarInfoEvent::Clear { .. } => *board = None,
            WarInfoEvent::Args { war, args } => {
                let b = board.get_or_insert_with(|| WarBoard { war, ..Default::default() });
                b.args = args;
            }
        }
    }

    /// Remises à zéro.
    pub(super) fn siege_observe(&mut self, ev: &WorldEvent) {
        if matches!(ev, WorldEvent::Disconnected(_) | WorldEvent::Connected) {
            let data = self.siege.data.take();
            self.siege = SiegeGlue { data, ..Default::default() };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_filetime() {
        // 2026-10-31 (samedi) 20 h.
        let ft = filetime_of(2026, 10, 31, 20);
        assert_eq!(filetime_parts(ft), (2026, 10, 31, 20, 0, 6));
        // Dernier siège le samedi 24 → base samedi 31 ; un dimanche recule d'un jour.
        let last = filetime_of(2026, 10, 24, 21);
        assert_eq!(filetime_parts(declare_base(last, 7, 0)).2, 31);
        let sunday = filetime_of(2026, 10, 25, 21);
        assert_eq!(filetime_parts(declare_base(sunday, 7, 0)).2, 31);
        // Jamais de siège : samedi de la semaine de « maintenant + 7 − 7 j ».
        let now = filetime_of(2026, 10, 28, 15);
        assert_eq!(filetime_parts(declare_base(0, 7, now)).2, 31);
        // Créneau : lendemain à 22 h.
        assert_eq!(filetime_parts(declare_date(ft, 1, 2)), (2026, 11, 1, 22, 0, 0));
        let c = Castle { name: "Inflame".into(), next: Some(ft), status: Some(1), ..Default::default() };
        assert_eq!(war_time_text(&c, "Siege war time", "R", "C"), "-Siege war time\n 10/31/2026 20:00(GST) R");
        assert_eq!(strip_tags("<Siege Commander> Garrison"), "Garrison");
    }

    #[test]
    fn donnees_du_siege() {
        let castles = parse_castles("Index\tCastle\tRegion\tSWPlayingTime\tArchonEyeOpenTime\tStampingTime\tNextSWDeclaring(Hours)\tSelectGuildForSW(Hours)\tDuration4nextSW(Days)\n1\tInflame\ta/b\t3600000\t300000\t180000\t144\t143\t7\n", |n| (n == "a").then_some(80));
        assert_eq!(castles[0], CastleData { index: 1, name: "Inflame".into(), regions: vec![80], playing_ms: 3_600_000, stamping_ms: 180_000, declaring_hours: 144, duration_days: 7 });
        let castles = parse_castles("h\n4\tDrayger\t?/?\t1\t2\t3\t4\t5\t6\n", |_| None);
        assert_eq!(castles[0].regions, vec![131, 132]);
        let types = parse_siege_types("TID\tName\tSiegeWarCharType\n1312\tSealed Throne\t6\n1\tArcher\t\n");
        assert_eq!((types.get(&1312), types.get(&1)), (Some(&6), None));
        let steps = parse_war_steps("TYPE\tSTEP\tSTEP MESSAGE\tC1\tC2\tC3\nSiegeWar\t9\tSiege War ended<\\n>\t%s won\tfailed\t\n");
        assert_eq!(steps[&(0, 9)].1[1], "failed");
        assert_eq!(steps[&(0, 9)].0, "Siege War ended\n");
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : châteaux, objets, étapes et fenêtres d'Archonia.
    #[test]
    fn donnees_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let mut glue = SiegeGlue::default();
        let d = glue.data(&assets);
        assert_eq!(d.castles.len(), 5);
        assert_eq!(d.castles[1].regions, vec![80, 76]);
        assert_eq!(d.types.get(&1312), Some(&6));
        assert_eq!(d.types.get(&1258), Some(&10));
        assert!(d.steps.contains_key(&(war::SIEGE, 9)));
        assert!(d.events.contains_key("SiegeWarRequireCastleMaster"));
        assert!(super::super::npc_glue::sysstr(&assets, 529).is_some_and(|t| !t.is_empty()));
        let ui = assets.table("ini", "UI_1024X768.ini").unwrap();
        for (w, c) in [("UI_SiegeWarMain", "CONTROL_EDIT_WAR_TIME"), ("UI_SiegeWarSetTIme", "CONTROL_TimeCombo"), ("UI_SiegeWarApplDefense", "CONTROL_23908"), ("UI_SiegeWarApplAttack", "CTRL_PAGE_09"), ("UI_SiegeWarSelectDefense", "CONTROL_21546"), ("UI_SiegeWarCatapult", "CONTROL_964"), ("UI_SiegeWarAttackRes", "CONTROL_26506"), ("UI_ResurrectSiege", "CONTROL_26467"), ("UI_CarveMessage", "CONTROL_14556"), ("UI_SiegeWarRepair", "CONTROL_27046"), ("UI_WarInfo", "Center_Message_Edit"), ("UI_Resurrection", "CONTROL_26364")] {
            let win = crate::hud::UiWindow::parse(&ui, None, w).unwrap_or_else(|| panic!("{w}"));
            assert!(win.control(c).is_some(), "{w} {c}");
        }
    }
}
