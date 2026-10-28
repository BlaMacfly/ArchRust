//! Combats de Shrine de l'épisode 8 (AgpmShrineBattle 0x62, AgcmShrine d'Archonia) et annonces du Battleground
//! (0x58, AgcmBattleGround).
//!
//! * **Messages** (op 7, `FUN_140725c00`) : seuls les `MessageTid` 6, 9, 10, 11, 12, 16, 20 et 22 de
//!   `ini/shrinemessage.xml` s'affichent, avec à leur suite 5 (après 6), 14 et 15 (après 9 et 11, sauf pour le
//!   graveur lui-même), 26 (après 20), 23 et 27 (après 22). Arguments : nom du Shrine, nom de race de
//!   `RaceCode` (champ race), nom (champ 0x24). 14 / 15 reçoivent la race du joueur (sysstr 652-654, 773) et le
//!   nom du graveur. `MessageShowType` 0 discussion et centre, 1 centre, 2 discussion ; `MessageShowTarget`
//!   1 dans un Shrine, 2 hors Shrine ; `MessageShowTargetRace` 1 race du graveur, 2 autres races (choix
//!   d'ArchRust : le filtre d'Archonia, `FUN_140729d80`, passe par des drapeaux du lecteur XML non relevés).
//! * **Ouverture** (op 10, `FUN_140725170`) : `Shrine_Ready_to_Start_Msg` au centre 5 s, bouton de déplacement
//!   actif pour ce Shrine, message 3 la première fois.
//! * **Fenêtre** `UI_Shrine_info` (bouton `CONTROL_MINIMAP_SHRINE`) : demande op 0 à l'ouverture, icônes et liste
//!   (voir `hud_shrine.rs`), clic d'une icône permise (`FUN_140725a20`) : refus `Teleport_TransformChar` si
//!   métamorphosé, sinon `Shrine_Join_Question` puis op 9.
//! * **Jauge** `UI_SHRINE_HP` : op 3 (≠ 0 affichée), op 1 état 2 (gravure) l'affiche, états 0, 1, 3, 6 et op 6
//!   (drapeau 0, état 2 : gravure annulée, message 13) la masquent ; texte `Shrine_Gauge` (`FUN_140725780`) :
//!   race du graveur (+0x88 par `RaceCode`), nom du graveur (CID +0xB8), état (`Shrine_Status_Imprint` /
//!   `_Occupy`), +0xA8 / (+0x70 en gravure, +0x78 sinon) en secondes.
//! * **Noms** (`FUN_140825a20`) : joueur dans une région de Shrine (236-283), autre joueur de race différente →
//!   couleur `SH_HUMAN/ORC/MOONELF/DRAGONSCION_NAME` de color.ini `[SHRINE]` ; même race → couleur par défaut
//!   (pas de couleur de meurtrier).
//! * **Battleground** (`FUN_14062ec70`) : type 2 sysstr 780 / 782, type 4 sysstr 781 / 783 (vert, 15 s), type 5
//!   texte du paquet au centre (sa couleur, 15 s) ; les autres types vont à une fenêtre d'AgcmUISiegeWar
//!   qu'ArchRust n'a pas : discussion.
//!
//! Non repris : effets 0xFF / 0xF7 sur le graveur, icônes de la minicarte (`ShrineRender`, TID 5185 / 2231),
//! sons, boutique des pièces (`AgcmUIShrineShop`).

use super::App;
use crate::assets::Assets;
use crate::hud::{HudState, ShrineIcon, ShrineInfoView, ShrineMapEntry, ShrineRow, ShrineUi, SHRINE_LIST_ROWS};
use crate::net::NetLink;
use archrust_netclient::battleground::{kind, BattlegroundEvent};
use archrust_netclient::shrine::{state, Shrine, ShrineCommand, ShrineEvent};
use archrust_netclient::world::WorldEvent;
use archrust_netclient::Command;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Régions des Shrines (`FUN_140275cc0` : `région − 236 < 48`).
pub const SHRINE_REGIONS: std::ops::RangeInclusive<u32> = 236..=283;
/// Icônes de la carte (`C_Shrine_icon_1…8`).
const MAP_ICONS: usize = 8;
/// Durée des messages du centre (Shrine) et des annonces du Battleground.
const CENTER_TIME: Duration = Duration::from_secs(5);
const BATTLEGROUND_TIME: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq)]
pub struct ShrineMessage {
    pub show_type: i32,
    pub target: i32,
    pub target_race: i32,
    pub content: String,
    /// Couleur ARGB.
    pub color: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShrineData {
    pub messages: HashMap<i32, ShrineMessage>,
    /// Race (1-4) → nom et couleur ARGB (`RaceCode`).
    pub races: HashMap<i32, (String, u32)>,
    /// `MessagePos` et `HPUIPos` (px 1024 × 768 depuis le haut).
    pub message_top: f32,
    pub gauge_top: f32,
}

fn tag<'a>(block: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let start = block.find(&open)? + open.len();
    let end = block[start..].find(&format!("</{name}>"))? + start;
    Some(block[start..end].trim())
}

fn color(block: &str) -> u32 {
    let Some(i) = block.find("<Color") else { return 0xFFFF_FFFF };
    let c = &block[i..block[i..].find("/>").map_or(block.len(), |e| i + e)];
    let comp = |k: &str| {
        let key = format!("{k}=\"");
        c.find(&key).and_then(|p| c[p + key.len()..].split('"').next()).and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(255).min(255)
    };
    0xFF00_0000 | comp("r") << 16 | comp("g") << 8 | comp("b")
}

/// Lit `shrinemessage.xml` (`RaceCode`, `MessageCode`, `MessagePos`, `HPUIPos`).
pub fn parse_shrine_messages(xml: &str) -> ShrineData {
    let mut d = ShrineData { message_top: 150.0, gauge_top: 50.0, ..Default::default() };
    for race in xml.split("<Race>").skip(1) {
        let race = race.split("</Race>").next().unwrap_or("");
        if let Some(tid) = tag(race, "RaceTid").and_then(|t| t.parse().ok()) {
            d.races.insert(tid, (tag(race, "RaceName").unwrap_or("").to_string(), color(race)));
        }
    }
    for m in xml.split("<Message>").skip(1) {
        let m = m.split("</Message>").next().unwrap_or("");
        let n = |k: &str| tag(m, k).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
        if let Some(tid) = tag(m, "MessageTid").and_then(|t| t.parse().ok()) {
            d.messages.insert(tid, ShrineMessage { show_type: n("MessageShowType"), target: n("MessageShowTarget"), target_race: n("MessageShowTargetRace"), content: tag(m, "MessageContent").unwrap_or("").to_string(), color: color(m) });
        }
    }
    let offset = |k: &str| xml.find(&format!("<{k}>")).and_then(|i| tag(&xml[i..], "Offset")).and_then(|v| v.parse::<f32>().ok());
    d.message_top = offset("MessagePos").unwrap_or(d.message_top);
    d.gauge_top = offset("HPUIPos").unwrap_or(d.gauge_top);
    d
}

/// Le message s'adresse-t-il au joueur (dans un Shrine ou non, race du graveur ou non) ?
pub fn message_targets(m: &ShrineMessage, in_shrine: bool, my_race: Option<i32>, race: Option<i32>) -> bool {
    let place = match m.target {
        1 => in_shrine,
        2 => !in_shrine,
        _ => true,
    };
    let kin = match (m.target_race, my_race, race) {
        (1, Some(a), Some(b)) => a == b,
        (2, Some(a), Some(b)) => a != b,
        _ => true,
    };
    place && kin
}

/// Icône d'un Shrine sur la carte (`FUN_140729620`).
pub fn map_icon(s: &Shrine, move_on: bool) -> ShrineIcon {
    if move_on {
        return ShrineIcon::Enable;
    }
    match s.state {
        state::OPEN => ShrineIcon::Enable,
        state::CARVE | state::CARVED | state::OCCUPY => ShrineIcon::Battle,
        0 | 5 | 6 if (1..=4).contains(&s.race) => ShrineIcon::Race(s.race),
        _ => ShrineIcon::Disable,
    }
}

/// Ordre de la liste (`FUN_14072a4a0`) : Shrines en cours d'abord, puis fermés ou terminés, chacun trié sur le mot
/// bas de +0x90 plus l'indice.
pub fn list_order(shrines: &[Shrine]) -> Vec<usize> {
    let key = |i: usize| (shrines[i].run[0] as i32).wrapping_add(i as i32);
    let (mut active, mut done): (Vec<usize>, Vec<usize>) = (0..shrines.len()).filter(|&i| shrines[i].id > 0).partition(|&i| !matches!(shrines[i].state, 0 | 6));
    active.sort_by_key(|&i| key(i));
    done.sort_by_key(|&i| key(i));
    active.extend(done);
    active
}

#[derive(Default)]
pub struct ShrineGlue {
    data: Option<ShrineData>,
    /// Table des Shrines (10 entrées, ordre du serveur).
    shrines: Vec<Shrine>,
    /// Bouton de déplacement actif (op 10, `+0x20e8`) et message 3 déjà montré (`+0x20ec`), par Shrine.
    move_on: HashSet<i32>,
    announced: HashSet<i32>,
    pub show_info: bool,
    page: usize,
    /// Shrine de la jauge (`+0x2064`) et jauge affichée.
    gauge_shrine: Option<i32>,
    gauge_on: bool,
    /// Graveur (`+0x2368`), son nom (`+0x22f0`), le nom de sa race (`+0x22e8`) et le texte d'état (`+0x22f8`).
    carver: Option<i32>,
    carver_name: String,
    carver_race: String,
    status_text: String,
    /// Question `Shrine_Join_Question` en cours (Shrine, texte).
    join_ask: Option<(i32, String)>,
    /// Messages du centre (texte, couleur ARGB, fin) et annonce du Battleground.
    center: Vec<(String, u32, Instant)>,
    battleground: Option<(String, u32, Instant)>,
    /// Victoires / défaites du joueur (op 11) et de sa race (op 12).
    pub my_score: (i32, i32),
    pub race_score: (i32, i32),
    transformed: bool,
    /// Race par TID de modèle (charactertemplate.ini) et couleurs `SH_*_NAME` (format des polices 0xAABBGGRR).
    races: HashMap<i32, Option<i32>>,
    colors: Option<[u32; 4]>,
}

impl ShrineGlue {
    fn data(&mut self, assets: &Assets) -> &ShrineData {
        self.data.get_or_insert_with(|| parse_shrine_messages(&super::npc_glue::client_text_in(assets, "ini", "shrinemessage.xml")))
    }

    fn by_id(&self, id: i32) -> Option<&Shrine> {
        self.shrines.iter().find(|s| s.id == id && id != 0)
    }

    fn race_of_tid(&mut self, assets: &Assets, tid: i32) -> Option<i32> {
        *self.races.entry(tid).or_insert_with(|| {
            let t = assets.table("ini", "charactertemplate.ini").ok()?;
            t.section(&tid.to_string())?.get("Race")?.trim().parse().ok()
        })
    }

    fn push_center(&mut self, text: String, argb: u32) {
        if text.is_empty() {
            return;
        }
        self.center.push((text, argb, Instant::now() + CENTER_TIME));
        let excess = self.center.len().saturating_sub(5);
        self.center.drain(..excess);
    }
}

/// Fenêtres et jauge (passe egui).
pub fn panel(ctx: &egui::Context, glue: &mut ShrineGlue, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>) {
    if crate::hud::minimap_shrine_button(ctx, hud, assets) {
        glue.show_info = !glue.show_info;
        glue.page = 0;
        // `Func_OpenShrineInfo` : liste demandée à chaque ouverture.
        if let (true, Some(net)) = (glue.show_info, net) {
            net.send(Command::Shrine(ShrineCommand::RequestList));
        }
    }
    let (message_top, gauge_top) = {
        let d = glue.data(assets);
        (d.message_top, d.gauge_top)
    };
    if glue.show_info {
        let map: Vec<ShrineMapEntry> = (0..MAP_ICONS)
            .map(|i| {
                let s = glue.shrines.get(i).cloned().unwrap_or_default();
                let move_on = glue.move_on.contains(&s.id);
                let count = glue.shrines.iter().filter(|s| s.id > 0).count();
                ShrineMapEntry { name: s.name.clone(), icon: map_icon(&s, move_on), clickable: s.id > 0 && i < count && (move_on || s.entry_window()) }
            })
            .collect();
        let order = list_order(&glue.shrines);
        let pages = order.len().div_ceil(SHRINE_LIST_ROWS).max(1);
        glue.page = glue.page.min(pages - 1);
        let mut status = |s: &Shrine| {
            let key = match s.state {
                state::OPEN => "Shrine_Status_Open",
                state::CARVE | state::CARVED => "Shrine_Status_Imprint",
                state::OCCUPY => "Shrine_Status_Occupy",
                _ => "Shrine_Status_Close",
            };
            crate::hud::ui_message(hud, assets, key)
        };
        let rows: Vec<ShrineRow> = order
            .iter()
            .skip(glue.page * SHRINE_LIST_ROWS)
            .take(SHRINE_LIST_ROWS)
            .map(|&i| {
                let s = &glue.shrines[i];
                let icon = map_icon(s, glue.move_on.contains(&s.id));
                ShrineRow {
                    open: matches!(icon, ShrineIcon::Battle | ShrineIcon::Enable),
                    name: s.name.clone(),
                    bar: match s.state {
                        state::CARVE | state::CARVED | state::OCCUPY => 0,
                        state::OPEN => 1,
                        _ => 2,
                    },
                    bar_text: status(s),
                    race: s.race,
                    pc_name: s.holder.clone(),
                    remain: String::new(),
                }
            })
            .collect();
        let hint = crate::hud::ui_message(hud, assets, "Shrine_Move_Button_Click");
        let view = ShrineInfoView { map: &map, rows: &rows, page: format!("{}/{}", glue.page + 1, pages), move_hint: &hint };
        let mut out = Vec::new();
        crate::hud::draw_shrine_info(ctx, hud, assets, &view, &mut out);
        for a in out {
            match a {
                ShrineUi::Close => glue.show_info = false,
                ShrineUi::PrevPage => glue.page = glue.page.saturating_sub(1),
                ShrineUi::NextPage => glue.page = (glue.page + 1).min(pages - 1),
                ShrineUi::Icon(i) => {
                    let Some(s) = glue.shrines.get(i).cloned() else { continue };
                    if glue.transformed {
                        let t = crate::hud::ui_message(hud, assets, "Teleport_TransformChar");
                        glue.push_center(t, 0xFFFF_FFFF);
                    } else if matches!(s.state, 1..=4) || glue.move_on.contains(&s.id) {
                        let q = super::pvp_glue::fill(&crate::hud::ui_message(hud, assets, "Shrine_Join_Question"), &[s.name.clone()]);
                        glue.join_ask = Some((s.id, q));
                    }
                }
            }
        }
    }
    if let Some((id, text)) = glue.join_ask.clone() {
        if let Some(yes) = crate::hud::confirm_box(ctx, hud, assets, "shrine-join", &text) {
            glue.join_ask = None;
            if let (true, Some(net)) = (yes, net) {
                net.send(Command::Shrine(ShrineCommand::Join(id)));
            }
        }
    }
    // Jauge de gravure ou d'occupation.
    if glue.gauge_on {
        if let Some(s) = glue.gauge_shrine.and_then(|id| glue.by_id(id)).cloned() {
            let max = if s.state == state::CARVE { s.times[3] } else { s.times[4] };
            let cur = s.run[3];
            let ratio = if max > 0 { cur as f32 / max as f32 } else { 0.0 };
            let args = [glue.carver_race.clone(), glue.carver_name.clone(), glue.status_text.clone(), (cur / 1000).to_string(), (max / 1000).to_string()];
            let text = super::pvp_glue::fill(&crate::hud::ui_message(hud, assets, "Shrine_Gauge"), &args);
            crate::hud::draw_shrine_hp(ctx, hud, assets, &text, ratio, gauge_top);
        }
    }
    // Messages du centre et annonce du Battleground.
    let now = Instant::now();
    glue.center.retain(|(_, _, until)| now < *until);
    glue.battleground = glue.battleground.take().filter(|(_, _, until)| now < *until);
    if !glue.center.is_empty() || glue.battleground.is_some() {
        let lines: Vec<(String, u32, f32)> = glue.center.iter().enumerate().map(|(i, (t, c, _))| (t.clone(), c & 0xFF_FFFF, message_top + i as f32 * 20.0)).collect();
        crate::hud::draw_battle_texts(ctx, &lines, glue.battleground.as_ref().map(|(t, c, _)| (t.as_str(), *c)));
    }
}

fn colored(text: &str, argb: u32) -> String {
    format!("|cff{:06x}{text}|r", argb & 0xFF_FFFF)
}

impl App {
    fn shrine_in_region(&self) -> bool {
        self.zone_region.as_ref().is_some_and(|z| SHRINE_REGIONS.contains(&(z.region as u32)))
    }

    fn shrine_my_race(&mut self) -> Option<i32> {
        let tid = self.player.as_ref()?.tid;
        let assets = self.assets.clone();
        self.shrine.race_of_tid(&assets, tid)
    }

    /// Affiche le message `tid` de shrinemessage.xml (`FUN_140729d80`) ; `race` : race du graveur, s'il y en a un.
    fn shrine_message(&mut self, tid: i32, args: &[String], race: Option<i32>) {
        let assets = self.assets.clone();
        let Some(m) = self.shrine.data(&assets).messages.get(&tid).cloned() else { return };
        let text = super::pvp_glue::fill(&m.content, args);
        let (in_shrine, my_race) = (self.shrine_in_region(), self.shrine_my_race());
        if text.trim().is_empty() || !message_targets(&m, in_shrine, my_race, race) {
            return;
        }
        if matches!(m.show_type, 0 | 1) {
            self.shrine.push_center(text.clone(), m.color);
        }
        if matches!(m.show_type, 0 | 2) {
            self.ui.push_chat(colored(&text, m.color));
        }
    }

    /// Paquet AgpmShrineBattle.
    pub(super) fn on_shrine_event(&mut self, ev: ShrineEvent) {
        let assets = self.assets.clone();
        self.shrine.data(&assets);
        match ev {
            ShrineEvent::List(list) => self.shrine.shrines = list,
            ShrineEvent::Update(s) => {
                if let Some(e) = self.shrine.shrines.iter_mut().find(|e| e.id == s.id && s.id != 0) {
                    *e = s;
                }
            }
            ShrineEvent::State { id, shrine } => {
                let Some(e) = self.shrine.shrines.iter_mut().find(|e| e.id == id && id != 0) else { return };
                *e = shrine.clone();
                self.shrine.gauge_shrine = Some(shrine.id);
                if let Some(name) = self.shrine.data.as_ref().and_then(|d| d.races.get(&shrine.carver_race)).map(|r| r.0.clone()) {
                    self.shrine.carver_race = name;
                }
                if let Some(c) = self.entities.iter().chain(self.player.iter()).find(|c| c.cid as u32 == shrine.carver && shrine.carver != 0) {
                    self.shrine.carver_name = c.name.clone();
                    self.shrine.carver = Some(c.cid);
                }
                match shrine.state {
                    0 | state::OPEN | state::CARVED | state::FINISHED => self.shrine.gauge_on = false,
                    state::CARVE => {
                        self.shrine.status_text = crate::hud::ui_message(&mut self.hud, &assets, "Shrine_Status_Imprint");
                        self.shrine.gauge_on = true;
                    }
                    state::OCCUPY => self.shrine.status_text = crate::hud::ui_message(&mut self.hud, &assets, "Shrine_Status_Occupy"),
                    _ => {}
                }
            }
            ShrineEvent::Gauge { value, .. } => self.shrine.gauge_on = value != 0,
            ShrineEvent::Player { flag, state, .. } => {
                // Gravure annulée (drapeau 0, état 2) : jauge masquée, message 13.
                if flag == 0 && state == 2 {
                    self.shrine.gauge_on = false;
                    self.shrine_message(13, &[], None);
                }
            }
            ShrineEvent::Opening { id, minutes } => {
                let Some(s) = self.shrine.by_id(id).cloned() else { return };
                self.shrine.move_on.insert(id);
                let t = super::pvp_glue::fill(&crate::hud::ui_message(&mut self.hud, &assets, "Shrine_Ready_to_Start_Msg"), &[s.name.clone(), minutes.to_string()]);
                self.shrine.push_center(t, 0xFFFF_FFFF);
                if self.shrine.announced.insert(id) {
                    self.shrine_message(3, &[], None);
                }
            }
            ShrineEvent::Score { race: false, win, lose } => self.shrine.my_score = (win, lose),
            ShrineEvent::Score { race: true, win, lose } => self.shrine.race_score = (win, lose),
            ShrineEvent::Message { id, tid, race, name } => self.on_shrine_message(id, tid, race, name),
        }
    }

    fn on_shrine_message(&mut self, id: i32, tid: i32, race: i32, name: String) {
        let Some(shrine) = self.shrine.by_id(id).map(|s| s.name.clone()) else { return };
        if !self.shrine.data.as_ref().is_some_and(|d| d.messages.contains_key(&tid)) {
            return;
        }
        let race_name = self.shrine.data.as_ref().and_then(|d| d.races.get(&race)).map(|r| r.0.clone());
        match tid {
            6 => {
                self.shrine.move_on.remove(&id);
                self.shrine_message(6, &[shrine], None);
                self.shrine_message(5, &[], None);
            }
            9 | 11 => {
                let me = self.player.as_ref().map_or(0, |p| p.cid);
                // Graveur d'après le nom du paquet (`+0x2368`).
                if let Some(c) = self.player.iter().chain(self.entities.iter()).find(|c| !name.is_empty() && c.name.eq_ignore_ascii_case(&name)) {
                    self.shrine.carver = Some(c.cid);
                }
                // « [race du joueur] must protect / kill the carver, [graveur] » (`FUN_140727640` 14 et 15).
                if self.shrine.carver.is_some_and(|c| c != me) {
                    let assets = self.assets.clone();
                    let key = |r: i32| [652u32, 653, 654, 773].get(usize::try_from(r).ok()?.checked_sub(1)?).copied();
                    let mine = self.shrine_my_race().and_then(key).and_then(|k| super::npc_glue::sysstr(&assets, k)).unwrap_or_default();
                    for t in [14, 15] {
                        self.shrine_message(t, &[mine.clone(), name.clone()], Some(race));
                    }
                }
                if let Some(r) = race_name {
                    self.shrine_message(tid, &[shrine, r, name], Some(race));
                }
            }
            10 | 12 | 16 => {
                if let Some(r) = race_name {
                    self.shrine_message(tid, &[shrine, r, name], Some(race));
                }
            }
            20 => {
                self.shrine.move_on.remove(&id);
                self.shrine_message(20, &[shrine], None);
                self.shrine_message(26, &[], None);
            }
            22 => {
                if let Some(r) = race_name {
                    self.shrine_message(22, &[r], None);
                    self.shrine_message(23, &[], None);
                    self.shrine_message(27, &[], None);
                }
            }
            _ => {}
        }
    }

    /// Annonce du Battleground (0x58).
    pub(super) fn on_battleground(&mut self, ev: BattlegroundEvent) {
        let assets = self.assets.clone();
        let sys = |id: u32| super::npc_glue::sysstr(&assets, id).unwrap_or_default();
        let until = Instant::now() + BATTLEGROUND_TIME;
        match ev.kind {
            kind::START => self.shrine.battleground = Some((sys(if ev.variant != 0 { 782 } else { 780 }), 0xFF00_FF00, until)),
            kind::END => self.shrine.battleground = Some((sys(if ev.variant != 0 { 783 } else { 781 }), 0xFF00_FF00, until)),
            kind::CENTER => self.shrine.battleground = Some((ev.text, ev.color, until)),
            // Fenêtre d'AgcmUISiegeWar (absente d'ArchRust) : dans la discussion.
            _ if !ev.text.is_empty() => self.ui.push_chat(colored(&ev.text, ev.color)),
            _ => {}
        }
    }

    /// Remises à zéro et métamorphose du joueur.
    pub(super) fn shrine_observe(&mut self, ev: &WorldEvent) {
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        match ev {
            WorldEvent::Transformed { cid, transformed } if *cid == me => self.shrine.transformed = *transformed,
            WorldEvent::Disconnected(_) | WorldEvent::Connected => {
                let data = self.shrine.data.take();
                let races = std::mem::take(&mut self.shrine.races);
                self.shrine = ShrineGlue { data, races, ..Default::default() };
                self.boss.reset();
            }
            _ => {}
        }
    }

    /// Couleur du nom d'un autre joueur pendant un combat de Shrine : `Some(Some(c))` race différente,
    /// `Some(None)` même race (couleur par défaut), `None` hors d'un Shrine.
    pub(super) fn shrine_name_color(&mut self, cid: i32) -> Option<Option<u32>> {
        if !self.shrine_in_region() {
            return None;
        }
        let tid = self.entities.iter().find(|e| e.cid == cid && e.is_player)?.tid;
        let assets = self.assets.clone();
        let (mine, theirs) = (self.shrine_my_race(), self.shrine.race_of_tid(&assets, tid));
        let (Some(mine), Some(theirs)) = (mine, theirs) else { return None };
        if mine == theirs {
            return Some(None);
        }
        let colors = *self.shrine.colors.get_or_insert_with(|| {
            let table = assets.table("ini", "color.ini").ok();
            let rgb = |k: &str, d: u32| table.as_ref().and_then(|t| t.sections.iter().find_map(|s| s.get(k))).and_then(|v| u32::from_str_radix(v.trim(), 16).ok()).unwrap_or(d);
            // color.ini est en 0xRRGGBB ; les polices attendent 0xAABBGGRR.
            let swap = |c: u32| 0xFF00_0000 | ((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF);
            [swap(rgb("SH_HUMAN_NAME", 0x00FF00)), swap(rgb("SH_ORC_NAME", 0xFF0000)), swap(rgb("SH_MOONELF_NAME", 0x0000FF)), swap(rgb("SH_DRAGONSCION_NAME", 0xFFFF00))]
        });
        Some(usize::try_from(theirs).ok().and_then(|r| r.checked_sub(1)).and_then(|i| colors.get(i).copied()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = "<ShrineMessage><RaceCode><Race><RaceTid>1</RaceTid><RaceName>Human</RaceName><Color r=\"0\" g=\"0\" b=\"255\" /></Race>\
        <Race><RaceTid>4</RaceTid><RaceName>DragonScion</RaceName><Color r=\"255\" g=\"0\" b=\"0\" /></Race></RaceCode><MessageCode>\
        <Message><MessageTid>14</MessageTid><MessageShowType>0</MessageShowType><MessageShowTarget>1</MessageShowTarget>\
        <MessageShowTargetRace>1</MessageShowTargetRace><MessageContent>[%s] must protect the carver, [%s].</MessageContent><Color r=\"255\" g=\"255\" b=\"0\" /></Message>\
        </MessageCode><MessagePos><Offset>150</Offset></MessagePos><HPUIPos><Offset>50</Offset></HPUIPos></ShrineMessage>";

    #[test]
    fn messages_des_shrines() {
        let d = parse_shrine_messages(XML);
        assert_eq!(d.races.get(&1), Some(&("Human".to_string(), 0xFF00_00FF)));
        assert_eq!(d.races.get(&4).map(|r| r.1), Some(0xFFFF_0000));
        let m = &d.messages[&14];
        assert_eq!((m.show_type, m.target, m.target_race, m.color), (0, 1, 1, 0xFFFF_FF00));
        assert_eq!(super::super::pvp_glue::fill(&m.content, &["Orc".into(), "Bob".into()]), "[Orc] must protect the carver, [Bob].");
        assert_eq!((d.message_top, d.gauge_top), (150.0, 50.0));
        // Dans un Shrine, même race que le graveur seulement.
        assert!(message_targets(m, true, Some(2), Some(2)));
        assert!(!message_targets(m, true, Some(1), Some(2)));
        assert!(!message_targets(m, false, Some(2), Some(2)));
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : shrinemessage.xml, fenêtres et contrôles d'Archonia.
    #[test]
    fn donnees_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let d = parse_shrine_messages(&super::super::npc_glue::client_text_in(&assets, "ini", "shrinemessage.xml"));
        assert_eq!((d.messages.len(), d.races.len()), (27, 4));
        assert_eq!(d.messages[&9].content, "[%s] of [%s] is carving on the door of [%s].");
        assert_eq!((d.messages[&15].target, d.messages[&15].target_race), (1, 2));
        let ui = assets.table("ini", "UI_1024X768.ini").unwrap();
        let info = crate::hud::UiWindow::parse(&ui, None, "UI_Shrine_info").unwrap();
        for c in ["C_Shrine_icon_1", "C_Shrine_icon_8", "C_Shrine_8", "C_LE_Status_bar", "C_LE_Status_Bar_Text", "C_Prev", "C_Next", "CONTROL_12082"] {
            assert!(info.control(c).is_some(), "{c}");
        }
        let hp = crate::hud::UiWindow::parse(&ui, None, "UI_SHRINE_HP").unwrap();
        assert!(hp.control("CTRL_Shrine_HP").is_some() && hp.control("Ctrl_HP_DP").is_some());
        let map = crate::hud::UiWindow::parse(&ui, None, "MiniMap").unwrap();
        assert!(map.control("CONTROL_MINIMAP_SHRINE").is_some());
    }

    #[test]
    fn icones_et_ordre_de_la_liste() {
        let s = |id: i32, st: i32, race: i32, key: i64| Shrine { id, state: st, race, run: [key, 0, 0, 0, 0], ..Default::default() };
        assert_eq!(map_icon(&s(1, 2, 0, 0), false), ShrineIcon::Battle);
        assert_eq!(map_icon(&s(1, 1, 0, 0), false), ShrineIcon::Enable);
        assert_eq!(map_icon(&s(1, 6, 3, 0), false), ShrineIcon::Race(3));
        assert_eq!(map_icon(&s(1, 0, 0, 0), false), ShrineIcon::Disable);
        assert_eq!(map_icon(&s(1, 0, 0, 0), true), ShrineIcon::Enable);
        let list = vec![s(1, 0, 0, 5), s(2, 4, 0, 9), s(0, 1, 0, 0), s(3, 1, 0, 1)];
        assert_eq!(list_order(&list), vec![3, 1, 0]);
    }
}
