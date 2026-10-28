//! Fenêtre des boss d'Archonia (AgpmBoss 0x64, AgcmUIBoss) : `UI_Boss`, compteur de dégâts `UI_BossDPS`, boutons
//! `UI_BossDPSButton` et `UI_BossWindowMiniBtn` (voir `hud_boss.rs`).
//!
//! * Boss d'`ini/UIBoss.xml` (`<Boss TID Category Icon>`, `Name`, `SpawnPlace`, `SpawnPeriod`), filtrés par le
//!   masque des catégories (bit catégorie − 1 ; 1 ChaoticFrontier, 2 Asylum, 3 Battlesquare, 4 Shinewood,
//!   5 HeavensHell, 6 Firmament, 7 Purgatory), gardé dans `BossFilter.ini` (`[BossFilter]` `Mask=%d`, 0x7F par
//!   défaut ; Archonia l'écrit dans `INI\`, ArchRust dans `saved-variables`).
//! * Statut (`FUN_140675b20`) : boss sans état reçu « Not specified. » (0xFFBEB47D), vivant « Alive »
//!   (0xFF04E000), prochaine apparition à venir « Next event in HH:MM:SS » (0xFFFFBF00, heure du serveur
//!   d'AgpmTimer champ 2), sinon « Next Spawn: HH:MM » (orange) ou « Last Death At: HH:MM » (0xFFBEB47D).
//! * Clic sur un boss : op 4 et page de classement remise à 0 ; flèches : op 6 (page bornée à [0, pages]).
//! * Compteur de dégâts (op 7, `FUN_1406765f0` / `FUN_140677c90`) : fenêtre ouverte à chaque envoi sauf si le
//!   joueur l'a fermée, bouton `UI_BossDPSButton` pour la rouvrir, tout effacé 10 s après le dernier envoi.
//! * « Server Time: jj/mm/aaaa hh:mm:ss » = champ 2 + champ 3 + écoulé, en UTC (`_gmtime64`).

use super::App;
use crate::assets::Assets;
use crate::hud::{BossDetail, BossEntry, BossUi, BossView, DpsRow, HudState};
use crate::net::NetLink;
use crate::zones::GameClock;
use archrust_netclient::boss::{BossCommand, BossDamage, BossDrops, BossEvent, BossRanking, BossSpawn};
use archrust_netclient::Command;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

/// Effacement du compteur de dégâts après le dernier op 7.
const DAMAGE_TIMEOUT: Duration = Duration::from_secs(10);
/// Masque des filtres par défaut (7 catégories).
const DEFAULT_MASK: u32 = 0x7F;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BossDef {
    pub tid: i32,
    pub category: u32,
    pub icon: u32,
    pub name: String,
    pub place: String,
    pub period: String,
}

/// Catégorie d'UIBoss.xml (`FUN_140677fa0`).
fn category(name: &str) -> u32 {
    match name.trim() {
        "ChaoticFrontier" => 1,
        "Asylum" => 2,
        "Battlesquare" => 3,
        "Shinewood" => 4,
        "HeavensHell" => 5,
        "Firmament" => 6,
        "Purgatory" => 7,
        _ => 0,
    }
}

fn attr(s: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let i = s.find(&key)? + key.len();
    Some(s[i..].split('"').next()?.to_string())
}

/// Lit `UIBoss.xml` (32 boss au plus).
pub fn parse_bosses(xml: &str) -> Vec<BossDef> {
    xml.split("<Boss ")
        .skip(1)
        .filter_map(|b| {
            let b = b.split("</Boss>").next().unwrap_or("");
            let head = b.split('>').next().unwrap_or("");
            let text = |tag: &str| b.find(&format!("<{tag}")).and_then(|i| attr(&b[i..], "Text")).unwrap_or_default();
            Some(BossDef {
                tid: attr(head, "TID")?.trim().parse().ok()?,
                category: attr(head, "Category").map_or(0, |c| category(&c)),
                icon: attr(head, "Icon").and_then(|i| i.trim().parse().ok()).unwrap_or(0),
                name: text("Name"),
                place: text("SpawnPlace"),
                period: text("SpawnPeriod"),
            })
        })
        .take(archrust_netclient::boss::MAX_BOSSES)
        .collect()
}

/// Statut d'un boss et sa couleur ARGB (`FUN_140675b20`, textes préparés par `FUN_1406768d0`).
pub fn spawn_status(spawn: Option<&BossSpawn>, server_now: u64) -> (String, u32) {
    let Some(s) = spawn else { return ("Not specified.".into(), 0xFFBE_B47D) };
    if s.alive {
        return ("Alive".into(), 0xFF04_E000);
    }
    if s.next > 1 && server_now < s.next {
        let d = s.next - server_now;
        return (format!("Next event in {:02}:{:02}:{:02}", d / 3600, d / 60 % 60, d % 60), 0xFFFF_BF00);
    }
    let label = if s.next != 0 { "Next Spawn: " } else { "Last Death At: " };
    let text = format!("{label}{:02}:{:02}", s.hour, s.minute);
    let color = if text.starts_with('N') { 0xFFFF_BF00 } else { 0xFFBE_B47D };
    (text, color)
}

/// Date civile (UTC) d'un instant Unix : (année, mois, jour, heure, minute, seconde).
pub fn civil(t: i64) -> (i64, u32, u32, u32, u32, u32) {
    let (days, secs) = (t.div_euclid(86_400), t.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d, (secs / 3600) as u32, (secs / 60 % 60) as u32, (secs % 60) as u32)
}

/// « Server Time: jj/mm/aaaa hh:mm:ss » (`FUN_140677c90`).
pub fn server_time_text(display: i64) -> String {
    let (y, mo, d, h, mi, s) = civil(display);
    format!("Server Time: {d:02}/{mo:02}/{y} {h:02}:{mi:02}:{s:02}")
}

/// Dégâts « %.1fB/M/K ».
pub fn damage_text(d: i64) -> String {
    let f = d as f64;
    match d.abs() {
        n if n >= 1_000_000_000 => format!("{:.1}B", f / 1e9),
        n if n >= 1_000_000 => format!("{:.1}M", f / 1e6),
        n if n >= 1_000 => format!("{:.1}K", f / 1e3),
        _ => d.to_string(),
    }
}

/// Icône de classe d'après le TID du modèle (0 à 9, textures `party_face_*.png`).
pub fn class_icon(tid: i32) -> usize {
    match tid {
        96 => 0,
        1 => 1,
        6 => 2,
        4 => 3,
        8 => 4,
        3 => 5,
        377 => 6,
        460 => 7,
        9 => 8,
        _ => 9,
    }
}

fn load_mask(dir: &Path) -> u32 {
    std::fs::read_to_string(dir.join("BossFilter.ini"))
        .ok()
        .and_then(|t| t.lines().find_map(|l| l.trim().strip_prefix("Mask=").and_then(|v| v.trim().parse::<u32>().ok())))
        .unwrap_or(DEFAULT_MASK)
}

fn save_mask(dir: &Path, mask: u32) {
    let _ = std::fs::create_dir_all(dir);
    if let Err(e) = std::fs::write(dir.join("BossFilter.ini"), format!("[BossFilter]\nMask={mask}\n")) {
        log::warn!("BossFilter.ini : {e}");
    }
}

#[derive(Default)]
pub struct BossGlue {
    defs: Option<Vec<BossDef>>,
    spawns: HashMap<i32, BossSpawn>,
    pub show: bool,
    mask: Option<u32>,
    first: usize,
    selected: Option<i32>,
    drops: Option<BossDrops>,
    ranking: Option<BossRanking>,
    page: i8,
    damage: Option<(BossDamage, Instant)>,
    dps_open: bool,
    /// Le joueur a fermé le compteur : il ne se rouvre plus tout seul jusqu'à l'effacement.
    dps_closed: bool,
}

impl BossGlue {
    fn defs(&mut self, assets: &Assets) -> &[BossDef] {
        self.defs.get_or_insert_with(|| parse_bosses(&super::npc_glue::client_text_in(assets, "ini", "UIBoss.xml")))
    }

    /// Sortie du monde : états reçus oubliés (XML, filtres et fenêtre gardés).
    pub(super) fn reset(&mut self) {
        self.spawns.clear();
        self.selected = None;
        self.drops = None;
        self.ranking = None;
        self.damage = None;
        self.dps_open = false;
        self.dps_closed = false;
    }

    fn name_of(&mut self, assets: &Assets, tid: i32) -> String {
        self.defs(assets).iter().find(|d| d.tid == tid).map(|d| d.name.clone()).unwrap_or_default()
    }
}

/// Fenêtres des boss (passe egui).
#[allow(clippy::too_many_arguments)]
pub fn panel(ctx: &egui::Context, glue: &mut BossGlue, clock: &GameClock, saved_dir: &Path, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>) {
    let now = Instant::now();
    // Compteur effacé 10 s après le dernier envoi.
    if glue.damage.as_ref().is_some_and(|(_, at)| now.duration_since(*at) > DAMAGE_TIMEOUT) {
        glue.damage = None;
        glue.dps_open = false;
        glue.dps_closed = false;
    }
    if crate::hud::draw_boss_button(ctx, hud, assets, "UI_BossWindowMiniBtn") {
        glue.show = !glue.show;
    }
    let mask = *glue.mask.get_or_insert_with(|| load_mask(saved_dir));
    let mut out = Vec::new();
    if glue.show {
        let server_now = clock.server_unix_at(now).unwrap_or(0);
        let defs: Vec<BossDef> = glue.defs(assets).iter().filter(|d| d.category == 0 || mask & (1 << (d.category - 1)) != 0).cloned().collect();
        let entries: Vec<BossEntry> = defs
            .iter()
            .map(|d| {
                let (status, status_color) = spawn_status(glue.spawns.get(&d.tid), server_now);
                BossEntry { tid: d.tid, name: d.name.clone(), icon: d.icon, status, status_color }
            })
            .collect();
        let detail = glue.selected.and_then(|t| glue.defs(assets).iter().find(|d| d.tid == t).cloned()).map(|d| BossDetail { icon: d.icon, name: d.name, place: d.place, period: d.period });
        let drops: Vec<(i32, i32)> = glue.drops.as_ref().filter(|d| Some(d.boss) == glue.selected).map(|d| d.items.clone()).unwrap_or_default();
        let r = glue.ranking.clone().unwrap_or_default();
        let ranks: Vec<(String, String, String)> = r.rows.iter().map(|x| (x.rank.to_string(), x.name.clone(), x.kills.to_string())).collect();
        let me = if r.my_name.is_empty() { Default::default() } else { (r.my_rank.to_string(), r.my_name.clone(), r.my_kills.to_string()) };
        let page = if glue.ranking.is_some() { format!("{}/{}", r.page as i32 + 1, (r.pages as i32).max(1)) } else { String::new() };
        let server_time = clock.server_display_at(now).map(server_time_text).unwrap_or_default();
        let view = BossView { entries: &entries, first: &mut glue.first, selected: glue.selected, filters: mask, detail, drops: &drops, ranks: &ranks, me, page, server_time };
        crate::hud::draw_boss(ctx, hud, assets, view, &mut out);
    }
    if let Some((d, _)) = glue.damage.clone() {
        if crate::hud::draw_boss_button(ctx, hud, assets, "UI_BossDPSButton") {
            out.push(BossUi::OpenDps);
        }
        if glue.dps_open {
            let title = format!("Damage Meter - {}", glue.name_of(assets, d.boss));
            let rows: Vec<DpsRow> = d.rows.iter().map(|r| DpsRow { class: class_icon(r.tid), name: r.name.clone(), guild: r.guild.clone(), damage: damage_text(r.damage) }).collect();
            crate::hud::draw_boss_dps(ctx, hud, assets, &title, &rows, &mut out);
        }
    }
    for a in out {
        match a {
            BossUi::Close => glue.show = false,
            BossUi::Filter(bit) => {
                let m = mask ^ (1 << bit);
                glue.mask = Some(m);
                save_mask(saved_dir, m);
            }
            BossUi::Select(tid) => {
                glue.selected = Some(tid);
                glue.page = 0;
                glue.ranking = None;
                if let Some(net) = net {
                    net.send(Command::Boss(BossCommand::Select(tid)));
                }
            }
            BossUi::RankPage { next } => {
                let Some(tid) = glue.selected else { continue };
                let pages = glue.ranking.as_ref().map_or(0, |r| r.pages);
                let page = if next { (glue.page + 1).min(pages.max(0)) } else { (glue.page - 1).max(0) };
                if page != glue.page {
                    glue.page = page;
                    if let Some(net) = net {
                        net.send(Command::Boss(BossCommand::RankPage { tid, page }));
                    }
                }
            }
            BossUi::CloseDps => {
                glue.dps_open = false;
                glue.dps_closed = true;
            }
            BossUi::OpenDps => {
                glue.dps_open = true;
                glue.dps_closed = false;
            }
        }
    }
}

impl App {
    /// Paquet AgpmBoss.
    pub(super) fn on_boss_event(&mut self, ev: BossEvent) {
        match ev {
            // Op 1 : la table est vidée puis remplie (clé = TID).
            BossEvent::List(list) => self.boss.spawns = list.into_iter().map(|s| (s.tid, s)).collect(),
            BossEvent::Drops(d) => self.boss.drops = Some(d),
            BossEvent::Ranking(r) => {
                self.boss.page = r.page;
                self.boss.ranking = Some(r);
            }
            BossEvent::Damage(d) => {
                self.boss.damage = Some((d, Instant::now()));
                if !self.boss.dps_closed {
                    self.boss.dps_open = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boss_d_archonia() {
        let xml = "<UIBoss>\n\t<Boss TID=\"2073\" Category=\"Battlesquare\" Icon=\"14\">\n\t\t<Name Text=\"Basilis (Lv.30)\"/>\n\t\t<SpawnPlace Text=\"Battlesquare First Floor\"/>\n\t\t<SpawnPeriod Text=\"Spawn Period: 30-45 minutes\"/>\n\t</Boss>\n\t<Boss TID=\"5794\" Category=\"Firmament\" Icon=\"1\">\n\t\t<Name Text=\"X\"/>\n\t</Boss>\n</UIBoss>";
        let b = parse_bosses(xml);
        assert_eq!(b.len(), 2);
        assert_eq!((b[0].tid, b[0].category, b[0].icon, b[0].name.as_str(), b[0].place.as_str()), (2073, 3, 14, "Basilis (Lv.30)", "Battlesquare First Floor"));
        assert_eq!((b[1].category, b[1].period.as_str()), (6, ""));
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : UIBoss.xml et fenêtres d'Archonia.
    #[test]
    fn donnees_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let b = parse_bosses(&super::super::npc_glue::client_text_in(&assets, "ini", "UIBoss.xml"));
        assert!(b.len() > 10 && b.iter().all(|d| (1..=7).contains(&d.category)));
        assert_eq!((b[0].tid, b[0].icon, b[0].category), (2073, 14, 3));
        let ui = assets.table("ini", "UI_1024X768.ini").unwrap();
        let w = crate::hud::UiWindow::parse(&ui, None, "UI_Boss").unwrap();
        for c in ["Boss_Filter_PG_Btn", "Boss_Tab_Button_ListItem", "Boss_Icon_ListItem", "Boss_Spawn_ListItem", "Boss_Drop_Grid", "Boss_Rank_PlayerName", "Boss_ServerTime_Edit"] {
            assert!(w.control(c).is_some(), "{c}");
        }
        // Lignes de la liste posées dans `Boss_Tab_List` (12, 60).
        assert_eq!(w.control("Boss_Tab_Button_ListItem").unwrap().pos, [12.0, 60.0]);
        for n in ["UI_BossDPS", "UI_BossDPSButton", "UI_BossWindowMiniBtn"] {
            assert!(crate::hud::UiWindow::parse(&ui, None, n).is_some(), "{n}");
        }
    }

    #[test]
    fn statuts() {
        assert_eq!(spawn_status(None, 0).0, "Not specified.");
        let mut s = BossSpawn { tid: 1, next: 10_000, hour: 9, minute: 5, alive: true };
        assert_eq!(spawn_status(Some(&s), 0), ("Alive".into(), 0xFF04_E000));
        s.alive = false;
        assert_eq!(spawn_status(Some(&s), 10_000 - 3_725), ("Next event in 01:02:05".into(), 0xFFFF_BF00));
        assert_eq!(spawn_status(Some(&s), 10_001), ("Next Spawn: 09:05".into(), 0xFFFF_BF00));
        s.next = 0;
        assert_eq!(spawn_status(Some(&s), 10_001), ("Last Death At: 09:05".into(), 0xFFBE_B47D));
    }

    #[test]
    fn heure_degats_classes() {
        assert_eq!(server_time_text(0), "Server Time: 01/01/1970 00:00:00");
        assert_eq!(server_time_text(1_793_000_000), "Server Time: 26/10/2026 07:33:20");
        assert_eq!(civil(951_782_400), (2000, 2, 29, 0, 0, 0));
        assert_eq!([damage_text(999), damage_text(1_500), damage_text(2_340_000), damage_text(3_100_000_000)], ["999", "1.5K", "2.3M", "3.1B"]);
        assert_eq!([class_icon(96), class_icon(9), class_icon(1722)], [0, 8, 9]);
    }
}
