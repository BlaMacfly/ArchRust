//! Gacha (« Fortune Gate ») côté moteur. Protocole : `archrust_netclient::gacha` ; fenêtre d'Archonia :
//! `hud_gacha.rs`.
//!
//! Comportement relevé dans Archonia (AgcmUIEventGacha et AcUIGacha, sha256 e9c7609b…) :
//! * clic sur un socle du décor (`Function=38` des `obj*.ini`) : AGPMEVENTGACHA op 0 (`FUN_14065e4c0`), sauf pendant
//!   un tirage ;
//! * op 1 (`FUN_1405ff970`) : codes 0 à 4, `UI_Gacha` ouverte (`Gacha_OpenUI`), coût et bande gardés ; 5 et 6 :
//!   message seul. Ignorée pendant un tirage ;
//! * texte du coût (`FUN_1405ff580`) : objet demandé → `sysstr` 812 « You need %s %d. », ou 734 « You need %d %s
//!   and\n%d Gold. » avec de l'or ; sinon 735 (or) puis 811 (charisme) ;
//! * bande (`FUN_140880a60`) : au repos, `0,065` pixel par milliseconde ; « Start » (`FUN_1405ff890`) envoie l'op 2 ;
//!   l'op 3 réussie lance l'arrêt sur l'objet tiré : 2000 ms, 7 tours, `départ + (1 − cos πt)/2 × (cible − départ)`
//!   avec la cible `(7 N − 2 + i) × 50` (`FUN_140881500`) ; à chaque objet qui passe au centre, son nom
//!   (`Gacha_ItemName`) et `SOUND\UI\U_LB_A1.wav` ou `U_LB_A2.wav` ; à l'arrêt, `sysstr` 736 « You obtained %s item. »
//!   (`FUN_1405ffd40`) ;
//! * refus (`FUN_1405ffe00`) : `sysstr` 729, 730, 739, 731, 732, 733 (codes 1 à 6), boîte d'information centrée ;
//! * « Start » et « Close » sont sans effet pendant le tirage.
//!
//! Écart : Archonia pose aussi le drapeau 0x400 du personnage pendant le tirage (sens non relevé) ; ArchRust ne bloque
//! que les nouvelles demandes.

use super::App;
use crate::assets::Assets;
use crate::hud::{GachaUi, GachaView, HudState, GACHA_CELL as CELL};
use crate::net::NetLink;
use crate::objects::{tile_gachas, Placement};
use crate::scene::item_name_for_tid;
use crate::world::{sector_of, tile_of};
use archrust_netclient::gacha::{code, GachaCommand, GachaCost, GachaEvent};
use archrust_netclient::Command;
use glam::Vec3;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Défilement au repos, en pixels par milliseconde (`0.065000005`).
const IDLE_SPEED: f32 = 0.065;
/// Tours de la bande avant l'arrêt (`DAT_140f469e8`).
const LAPS: usize = 7;
/// Durée de l'arrêt (`DAT_140f469ec`).
const ROLL: Duration = Duration::from_millis(2000);
/// Sons du passage d'un objet au centre.
const TICKS: [&str; 2] = ["sound/ui/U_LB_A1.wav", "sound/ui/U_LB_A2.wav"];

/// Remplace dans l'ordre les `%d` (et `%lld`) par `ints`, les `%s` par `strings` : Archonia choisit l'ordre des
/// paramètres d'après la place de `%s` et `%d` dans la phrase, ce qui revient à les prendre chacun dans l'ordre.
pub fn printf(format: &str, ints: &[i64], strings: &[&str]) -> String {
    let (mut ints, mut strings) = (ints.iter(), strings.iter());
    let mut out = String::new();
    let mut rest = format;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 1..];
        let (n, kind) = match tail {
            t if t.starts_with("lld") => (3, 'd'),
            t if t.starts_with('d') => (1, 'd'),
            t if t.starts_with('s') => (1, 's'),
            t if t.starts_with('%') => (1, '%'),
            _ => (0, '?'),
        };
        match kind {
            'd' => out.push_str(&ints.next().map(|v| v.to_string()).unwrap_or_default()),
            's' => out.push_str(strings.next().copied().unwrap_or_default()),
            '%' => out.push('%'),
            _ => out.push('%'),
        }
        rest = &tail[n..];
    }
    out.push_str(rest);
    out.replace("\\n", "\n")
}

/// Texte du coût (`FUN_1405ff580`), `sys` donnant les phrases de `sysstr`.
pub fn notice(sys: &dyn Fn(u32) -> String, cost: &GachaCost, item_name: &str) -> String {
    if cost.item_tid != 0 && cost.item_count != 0 {
        if cost.gold == 0 {
            return printf(&sys(812), &[cost.item_count as i64], &[item_name]);
        }
        return printf(&sys(734), &[cost.item_count as i64, cost.gold as i64], &[item_name]);
    }
    let mut out = String::new();
    if cost.gold != 0 {
        out.push_str(&printf(&sys(735), &[cost.gold as i64], &[]));
    }
    if cost.charisma != 0 {
        out.push_str(&printf(&sys(811), &[cost.charisma as i64], &[]));
    }
    out
}

/// État de la bande (AcUIGacha `+0x4df0` : 0 repos, 1 arrêt en cours, 2 arrêtée).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Spin {
    Idle { since: Instant },
    Rolling { from: f32, to: f32, started: Instant, tid: i32 },
    Done { at: f32, tid: i32 },
}

/// Cible de l'arrêt sur le rang `index` d'une bande de `len` objets.
pub fn roll_target(len: usize, index: usize) -> f32 {
    ((LAPS * len) as f32 - 2.0 + index as f32) * CELL
}

/// Position pendant l'arrêt, `t` de 0 à 1.
pub fn rolling_offset(from: f32, to: f32, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    from + (1.0 - (t * std::f32::consts::PI).cos()) / 2.0 * (to - from)
}

/// Rang de l'objet au centre (`((p / 50) mod N + 2) mod N`).
pub fn centered(len: usize, offset: f32) -> Option<usize> {
    (len > 0).then(|| ((offset.max(0.0) / CELL) as usize % len + 2) % len)
}

impl Spin {
    fn offset(&self, now: Instant) -> f32 {
        match *self {
            Spin::Idle { since } => now.saturating_duration_since(since).as_millis() as f32 * IDLE_SPEED,
            Spin::Rolling { from, to, started, .. } => rolling_offset(from, to, now.saturating_duration_since(started).as_secs_f32() / ROLL.as_secs_f32()),
            Spin::Done { at, .. } => at,
        }
    }
}

/// Fenêtre ouverte sur un socle.
struct Open {
    object: u32,
    band: Vec<i32>,
    notice: String,
    spin: Spin,
    /// Objet au centre (`Gacha_ItemName`) et son rang.
    center: Option<usize>,
    item_name: String,
}

#[derive(Default)]
pub struct GachaGlue {
    /// Tuiles déjà lues et socles qu'elles portent.
    tiles: HashSet<u32>,
    sockets: Vec<(u32, Placement)>,
    open: Option<Open>,
    /// Boîte d'information (refus, objet obtenu).
    alert: Option<String>,
    sysstr: HashMap<u32, String>,
    /// Son à jouer à l'image suivante.
    sound: Option<&'static str>,
}

impl GachaGlue {
    fn sys(&mut self, assets: &Assets, id: u32) -> String {
        self.sysstr.entry(id).or_insert_with(|| super::npc_glue::sysstr(assets, id).unwrap_or_default()).clone()
    }

    fn rolling(&self) -> bool {
        matches!(self.open.as_ref().map(|o| o.spin), Some(Spin::Rolling { .. }))
    }

    /// Bouton « Start » : op 2, sauf pendant l'arrêt.
    fn roll(&mut self, net: Option<&NetLink>) {
        if self.rolling() {
            return;
        }
        if let Some(o) = &self.open {
            send(net, GachaCommand::Roll(o.object));
        }
    }

    /// Bouton « Close » et croix, sauf pendant l'arrêt (`FUN_1405ff820`).
    fn close(&mut self) {
        if !self.rolling() {
            self.open = None;
        }
    }
}

fn send(net: Option<&NetLink>, cmd: GachaCommand) {
    if let Some(net) = net {
        net.send(Command::Gacha(cmd));
    }
}

/// Fenêtre `UI_Gacha` et boîte d'information.
pub fn panel(ctx: &egui::Context, glue: &mut GachaGlue, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>) {
    if let Some(o) = &glue.open {
        let offset = o.spin.offset(Instant::now());
        let won = match o.spin {
            Spin::Done { tid, .. } => Some(tid),
            _ => None,
        };
        let rolling = glue.rolling();
        let mut out = Vec::new();
        crate::hud::draw_gacha(ctx, hud, assets, GachaView { band: &o.band, offset, item_name: &o.item_name, notice: &o.notice, won, rolling }, &mut out);
        for r in out {
            match r {
                GachaUi::Roll => glue.roll(net),
                GachaUi::Close => glue.close(),
            }
        }
        if glue.open.as_ref().is_some_and(|o| !matches!(o.spin, Spin::Done { .. })) {
            ctx.request_repaint();
        }
    }
    if let Some(text) = glue.alert.clone() {
        if crate::hud::info_box(ctx, hud, assets, &text) {
            glue.alert = None;
        }
    }
}

impl App {
    /// Socles des tuiles autour du joueur ; défilement de la bande, nom de l'objet au centre, fin de l'arrêt.
    pub(super) fn gacha_window_frame(&mut self) {
        if let Some(me) = self.player.as_ref().map(|p| p.position) {
            let (sx, sy) = sector_of(me.x, me.z);
            for dy in [-16, 0, 16] {
                for dx in [-16, 0, 16] {
                    let tile = tile_of(sx + dx, sy + dy);
                    if self.gacha.tiles.insert(tile) {
                        self.gacha.sockets.extend(tile_gachas(&self.assets, tile));
                    }
                }
            }
        }
        let now = Instant::now();
        let assets = self.assets.clone();
        let glue = &mut self.gacha;
        let mut obtained = None;
        if let Some(o) = glue.open.as_mut() {
            if let Spin::Rolling { to, started, tid, .. } = o.spin {
                if now.saturating_duration_since(started) >= ROLL {
                    o.spin = Spin::Done { at: to, tid };
                    obtained = Some(tid);
                }
            }
            let center = centered(o.band.len(), o.spin.offset(now));
            if center != o.center {
                o.center = center;
                o.item_name = center.and_then(|k| item_name_for_tid(&assets, o.band[k])).unwrap_or_default();
                // A1 ou A2 selon la parité de l'horloge, comme Archonia (`GetTickCount64() & 1`).
                let tick = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
                glue.sound = Some(TICKS[(tick & 1) as usize]);
            }
        }
        if let Some(tid) = obtained {
            let name = item_name_for_tid(&assets, tid).unwrap_or_default();
            let text = printf(&glue.sys(&assets, 736), &[], &[&name]);
            log::info!("gacha : {name} ({tid}) obtenu");
            glue.alert = Some(text);
        }
        if let Some(sound) = self.gacha.sound.take() {
            self.audio.play_effect(sound);
        }
    }

    /// Clic gauche sur un socle : demande au serveur (op 0). Rend `true` si un socle est visé.
    pub(super) fn gacha_left_click(&mut self) -> bool {
        let (Some(gpu), Some((cx, cy))) = (&self.gpu, self.last_cursor) else { return false };
        let (w, h) = (gpu.renderer.config.width as f32, gpu.renderer.config.height as f32);
        let ndc = Vec3::new((cx as f32 / w) * 2.0 - 1.0, 1.0 - (cy as f32 / h) * 2.0, 0.0);
        let inv = self.camera.view_proj(gpu.renderer.aspect()).inverse();
        let near = inv.project_point3(ndc);
        let dir = (inv.project_point3(Vec3::new(ndc.x, ndc.y, 1.0)) - near).normalize_or_zero();
        // Sphère englobante du modèle, sinon 150 unités autour du point posé.
        let hit = self
            .gacha
            .sockets
            .iter()
            .filter_map(|(id, p)| {
                let (center, radius) = self.objects.placed_bounds(p).unwrap_or((p.position + Vec3::new(0.0, 100.0, 0.0), 150.0));
                let t = (center - near).dot(dir);
                (t > 0.0 && (near + dir * t).distance(center) <= radius).then_some((*id, t))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id);
        let Some(object) = hit else { return false };
        if !self.gacha.rolling() {
            log::info!("gacha : demande au socle {object}");
            send(self.net.as_ref(), GachaCommand::Request(object));
        }
        true
    }

    /// Réponses du serveur.
    pub(super) fn on_gacha_event(&mut self, ev: GachaEvent) {
        let assets = self.assets.clone();
        let glue = &mut self.gacha;
        match ev {
            GachaEvent::Open { object, code: c, cost, band } => {
                if glue.rolling() {
                    return;
                }
                if !code::opens(c) {
                    let text = code::sysstr(c).map(|id| glue.sys(&assets, id)).unwrap_or_default();
                    log::info!("gacha : refus à l'ouverture ({c})");
                    glue.alert = (!text.is_empty()).then_some(text);
                    return;
                }
                let Some(object) = object else { return };
                let item = item_name_for_tid(&assets, cost.item_tid).unwrap_or_default();
                let sys = |id: u32| super::npc_glue::sysstr(&assets, id).unwrap_or_default();
                let notice = notice(&sys, &cost, &item);
                // Nom de départ : l'objet de rang 2 (`FUN_140881200`).
                let item_name = (!band.is_empty()).then(|| item_name_for_tid(&assets, band[2 % band.len()])).flatten().unwrap_or_default();
                log::info!("gacha : socle {object} ouvert (code {c}, {} objet(s), coût {cost:?})", band.len());
                glue.open = Some(Open { object, band, notice, spin: Spin::Idle { since: Instant::now() }, center: None, item_name });
            }
            GachaEvent::Result { code: c, tid } => {
                if c != code::SUCCESS {
                    let text = code::sysstr(c).map(|id| glue.sys(&assets, id)).unwrap_or_default();
                    log::info!("gacha : tirage refusé ({c})");
                    glue.alert = (!text.is_empty()).then_some(text);
                    return;
                }
                let now = Instant::now();
                let Some(o) = glue.open.as_mut() else { return };
                match o.band.iter().position(|t| *t == tid) {
                    Some(i) => {
                        let n = o.band.len();
                        let from = o.spin.offset(now) % (n as f32 * CELL);
                        o.spin = Spin::Rolling { from, to: roll_target(n, i), started: now, tid };
                    }
                    // Objet absent de la bande : fin aussitôt (`FUN_1405ffd40`).
                    None => {
                        let name = item_name_for_tid(&assets, tid).unwrap_or_default();
                        glue.alert = Some(printf(&glue.sys(&assets, 736), &[], &[&name]));
                    }
                }
            }
        }
    }

    /// Commandes (essais sans souris, `--exec`) : `/gacha` (socle le plus proche, comme un clic), `/gacha start`
    /// (bouton « Start »), `/gacha fermer`.
    pub(super) fn gacha_slash(&mut self, cmd: &str, arg: &str) -> bool {
        if cmd != "/gacha" {
            return false;
        }
        match arg.trim() {
            "start" => self.gacha.roll(self.net.as_ref()),
            "fermer" | "close" => self.gacha.close(),
            _ => {
                let me = self.player.as_ref().map_or(Vec3::ZERO, |p| p.position);
                match self.gacha.sockets.iter().map(|(id, p)| (*id, p.position.distance(me))).min_by(|a, b| a.1.total_cmp(&b.1)) {
                    Some((object, d)) => {
                        self.ui.push_chat(format!("[gacha] socle {object} à {d:.0} unités : demande envoyée."));
                        send(self.net.as_ref(), GachaCommand::Request(object));
                    }
                    None => self.ui.push_chat("[gacha] Aucun socle dans les tuiles proches."),
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textes_du_cout() {
        let sys = |id: u32| match id {
            734 => "You need %d %s and\\n%d Gold.".to_string(),
            735 => "You need %d Gold for this.".to_string(),
            736 => "You obtained %s item.".to_string(),
            811 => "You need %d Charisma Points.".to_string(),
            812 => "You need %s %d.".to_string(),
            _ => String::new(),
        };
        let pass = GachaCost { item_tid: 7436, item_count: 1, gold: 0, charisma: 0 };
        assert_eq!(notice(&sys, &pass, "Fortune Gate Pass"), "You need Fortune Gate Pass 1.");
        let both = GachaCost { gold: 500, ..pass.clone() };
        assert_eq!(notice(&sys, &both, "Pass"), "You need 1 Pass and\n500 Gold.");
        let charisma = GachaCost { charisma: 100, ..Default::default() };
        assert_eq!(notice(&sys, &charisma, ""), "You need 100 Charisma Points.");
        assert_eq!(printf("%lld Gold, %d%%", &[5, 7], &[]), "5 Gold, 7%");
        assert_eq!(printf(&sys(736), &[], &["Bow"]), "You obtained Bow item.");
    }

    #[test]
    fn bande_et_arret() {
        // Arrêt sur le rang 4 d'une bande de 10 : l'objet au centre est le 4, après 7 tours.
        let to = roll_target(10, 4);
        assert_eq!(to, 72.0 * CELL);
        assert_eq!(centered(10, to), Some(4));
        assert_eq!(centered(10, 0.0), Some(2), "au départ, le rang 2");
        assert_eq!(centered(0, 0.0), None);
        assert_eq!(rolling_offset(100.0, to, 0.0), 100.0);
        assert_eq!(rolling_offset(100.0, to, 1.0), to);
        let mid = rolling_offset(0.0, 1000.0, 0.5);
        assert!((mid - 500.0).abs() < 0.01, "{mid}");
        let start = Instant::now();
        let idle = Spin::Idle { since: start }.offset(start + Duration::from_millis(1000));
        assert!((idle - 65.0).abs() < 0.01, "{idle}");
    }
}
