//! Portails côté moteur. Protocole : `archrust_netclient::teleport` ; fenêtres : `hud_portal.rs`.
//!
//! Comportement relevé dans Archonia (AgcmUIEventTeleport, sha256 e9c7609b…) :
//! * clic sur l'objet du décor qui porte le portail (`Function=8`, `PointName` dans `obj*.ini`) :
//!   demande op 4 ; l'accord (op 5) ouvre la liste (FUN_1406ea050) ; en mode combat (10 s après le
//!   dernier coup donné ou reçu, FUN_14016ea90) la demande n'est pas envoyée et `Teleport_CombatMode`
//!   s'affiche dans la discussion, métamorphosé `Teleport_TransformChar` (AgcmEventTeleport
//!   FUN_1406ac370, messages FUN_1406e9a00 / FUN_1406e9a40) ;
//! * destinations : les points des groupes visés (`TargetGroup`) par le point de départ, dans l'ordre
//!   des groupes puis de `TeleportPoint.ini`, sans doublon ni le départ, 20 au plus (FUN_1402e71e0) ;
//!   un départ en zone dangereuse (`RegionType=1`) ne propose que « Retour » (`Teleport_ReturnOnly`,
//!   op 7) ;
//! * noms : `RegionGlossary.txt` du dossier de langue (FUN_14048d970), le nom coréen sinon ;
//! * prix : `TeleportFee.txt` (« départ-arrivée », FUN_1402e9a00), affiché « %d %s » avec `Money_Name` ;
//!   plus la taxe de la région du joueur (FUN_1402e79a0, voir `app_tax.rs`) ;
//! * statut d'une ligne : `NoMoney` si le prix dépasse l'or porté, `Normal` sinon ; `Disable` pour un
//!   point désactivé par le serveur (champ 5, qu'ArchCore n'envoie pas) — ArchRust y met aussi les
//!   points dont `UseType` n'admet pas la race du joueur, qu'ArchCore refuse ;
//! * choix (FUN_1406e9ec0) : or insuffisant → boîte `TELEPORT_NotEnoughMOney` (`Action=4:0`) ;
//!   arrivée en zone de combat entre joueurs → boîte de confirmation `TELEPORT_ConfirmPvPZone`
//!   (`Action=4:1`), qui envoie l'op 0 si elle est acceptée (FUN_1406e9a80) ; sinon op 0 avec le nom
//!   du point ; la fenêtre se ferme ;
//! * départ depuis une zone de combat entre joueurs : seule ligne « Retour » (op 7), qui ramène au
//!   point de départ du dernier trajet ;
//! * la liste se ferme si le joueur s'éloigne de plus de 149 unités (FUN_1406ea670) ;
//! * départ (op 1) → `UI_LG_Loading` jusqu'à l'op 8, ou l'op 2 s'il est annulé (FUN_1406e9e50,
//!   FUN_1406e9df0).

use super::App;
use crate::assets::Assets;
use crate::hud::{HudState, PortalUi, TeleportRow, TeleportView};
use crate::net::NetLink;
use crate::objects::{tile_portals, PortalObject};
use crate::ui::UiState;
use crate::world::{sector_of, tile_of};
use archrust_data::ini::{decode_text, encode_text, CodePage};
use archrust_netclient::skills::SkillEvent;
use archrust_netclient::teleport::{TeleportCommand, TeleportEvent};
use archrust_netclient::WorldEvent;
use archrust_netclient::Command;
use glam::Vec3;
use std::collections::{HashMap, HashSet};

/// Déplacement au-delà duquel la liste se ferme (FUN_1406ea670 : `0x95 < distance`).
const CLOSE_MOVE: f32 = 149.0;
/// Destinations au plus (tableau de 20 d'AgcmUIEventTeleport).
const MAX_TARGETS: usize = 20;
/// Durée du mode combat après le dernier coup (`+0xb48 + 10000` de FUN_14016ea90).
const COMBAT_MODE: std::time::Duration = std::time::Duration::from_secs(10);

/// Point de `TeleportPoint.ini`.
#[derive(Debug, Clone, Default)]
struct Point {
    groups: Vec<String>,
    targets: Vec<String>,
    use_type: i64,
    region_type: i64,
}

/// Données des portails lues dans le client.
#[derive(Default)]
struct PortalData {
    points: HashMap<String, Point>,
    /// Points dans l'ordre du fichier.
    order: Vec<String>,
    fees: HashMap<(String, String), i64>,
    /// Nom affiché par nom coréen (`RegionGlossary.txt`).
    glossary: HashMap<String, String>,
    /// Textes de `uimessage.txt`.
    messages: HashMap<String, String>,
    /// Texte des messages `Action=4:…` des événements d'interface.
    events: HashMap<String, String>,
}

impl PortalData {
    fn name(&self, point: &str) -> String {
        self.glossary.get(point).cloned().unwrap_or_else(|| point.to_string())
    }
    fn message(&self, key: &str) -> String {
        self.messages.get(key).cloned().unwrap_or_default()
    }
    /// Prix d'un trajet, taxe de la région du joueur comprise.
    fn fee(&self, from: &str, to: &str) -> i64 {
        super::tax_glue::taxed(self.fees.get(&(from.to_string(), to.to_string())).copied().unwrap_or(0))
    }
    /// Destinations du point `from`, dans l'ordre d'Archonia.
    fn targets(&self, from: &str) -> Vec<String> {
        let Some(src) = self.points.get(from) else { return Vec::new() };
        let mut out: Vec<String> = Vec::new();
        for g in &src.targets {
            for name in &self.order {
                if name != from && !out.contains(name) && self.points[name].groups.contains(g) {
                    out.push(name.clone());
                }
            }
        }
        out.truncate(MAX_TARGETS);
        out
    }
}

/// Lignes `clé<TAB>valeur` d'un texte du client.
fn key_values(text: &str) -> HashMap<String, String> {
    text.lines().filter_map(|l| l.split_once('\t')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect()
}

/// `RegionGlossary.txt` du dossier de langue : clé en CP949, nom traduit dans la page du dossier.
pub(super) fn load_glossary(assets: &Assets) -> HashMap<String, String> {
    for dir in crate::assets::text_dirs() {
        let text = super::npc_glue::client_text_in(assets, dir, "regionglossary.txt");
        if text.is_empty() {
            continue;
        }
        let cp = CodePage::for_dir(dir);
        return text
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .map(|(k, v)| (decode_text(&encode_text(k, cp), CodePage::Cp949).trim().to_string(), v.trim().to_string()))
            .collect();
    }
    HashMap::new()
}

/// Messages des événements d'interface : `EventName=…` suivi de `Action=4:type:-1:texte`
/// (`UI_1024X768.ini` du dossier de langue).
pub(super) fn ui_events(assets: &Assets) -> HashMap<String, String> {
    let mut events = HashMap::new();
    for dir in crate::assets::text_dirs() {
        let Ok(t) = assets.table(dir, "UI_1024X768.ini") else { continue };
        for s in &t.sections {
            let mut event: Option<&str> = None;
            for (k, v) in &s.entries {
                match k.as_str() {
                    "EventName" => event = Some(v.trim()),
                    "Action" => {
                        if let (Some(e), Some(text)) = (event, v.strip_prefix("4:").and_then(|r| r.splitn(3, ':').nth(2))) {
                            events.entry(e.to_string()).or_insert_with(|| text.trim().to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
        if !events.is_empty() {
            break;
        }
    }
    events
}

/// Message d'un événement d'interface (vide s'il n'en a pas).
pub(super) fn ui_event_text(assets: &Assets, event: &str) -> String {
    ui_events(assets).remove(event).unwrap_or_default()
}

/// Message d'un événement d'interface, les événements lus une fois dans `cache` (textes demandés à
/// chaque image : boîtes de confirmation).
pub(super) fn ui_event_cached(cache: &mut Option<HashMap<String, String>>, assets: &Assets, event: &str) -> String {
    cache.get_or_insert_with(|| ui_events(assets)).get(event).cloned().unwrap_or_default()
}

fn load_data(assets: &Assets) -> PortalData {
    let mut d = PortalData::default();
    if let Ok(t) = assets.table("ini", "TeleportPoint.ini") {
        for s in &t.sections {
            let list = |k: &str| s.get_all(k).map(|v| v.trim().to_string()).collect::<Vec<_>>();
            let point = Point { groups: list("Group"), targets: list("TargetGroup"), use_type: s.get_i64("UseType").unwrap_or(0), region_type: s.get_i64("RegionType").unwrap_or(0) };
            d.order.push(s.name.clone());
            d.points.insert(s.name.clone(), point);
        }
    }
    for line in super::npc_glue::client_text_in(assets, "ini", "TeleportFee.txt").lines().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        if let (Some(a), Some(b), Some(fee)) = (cols.first(), cols.get(1), cols.get(2).and_then(|f| f.trim().parse().ok())) {
            d.fees.insert((a.trim().to_string(), b.trim().to_string()), fee);
        }
    }
    d.glossary = load_glossary(assets);
    d.messages = key_values(&super::npc_glue::client_text(assets, "uimessage.txt"));
    d.events = ui_events(assets);
    log::info!("portails : {} point(s), {} prix, {} nom(s) traduits", d.points.len(), d.fees.len(), d.glossary.len());
    d
}

/// Bits de race de `UseType` (1 humain, 2 orc, 4 elfe de lune, 8 dragonien ; 0 toutes).
fn admits(use_type: i64, race: Option<usize>) -> bool {
    match race {
        Some(r) => use_type == 0 || use_type & (1 << r) != 0,
        None => true,
    }
}

/// Liste ouverte.
struct OpenList {
    object: u32,
    here: String,
    return_only: bool,
    /// Destination : point, prix, statut (0 normal, 1 or insuffisant, 2 indisponible).
    rows: Vec<(String, i64, usize)>,
    at: Vec3,
}

#[derive(Default)]
pub struct PortalGlue {
    data: Option<PortalData>,
    tiles: HashSet<u32>,
    portals: Vec<PortalObject>,
    open: Option<OpenList>,
    first: usize,
    /// Écran de chargement affiché (op 1 reçue, op 8 attendue).
    pub loading: bool,
    /// Mode combat du joueur jusqu'à cet instant (coup donné ou reçu, compétence sur une cible).
    combat_until: Option<std::time::Instant>,
    /// Joueur métamorphosé (AGPMCHARACTER champ 18).
    transformed: bool,
    /// Trajet vers une zone de combat entre joueurs en attente de confirmation : objet, point.
    confirm: Option<(u32, String)>,
    /// Boîte d'information ouverte (`TELEPORT_NotEnoughMOney`).
    notice: Option<String>,
}

impl PortalGlue {
    fn data(&mut self, assets: &Assets) -> &PortalData {
        self.data.get_or_insert_with(|| load_data(assets))
    }
}

fn send(net: Option<&NetLink>, cmd: TeleportCommand) {
    if let Some(net) = net {
        net.send(Command::Teleport(cmd));
    }
}

/// Fenêtres des portails et leurs demandes.
pub fn panel(ctx: &egui::Context, glue: &mut PortalGlue, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>, ui: &mut UiState) {
    if glue.loading {
        crate::hud::draw_loading(ctx, hud, assets);
    }
    if let Some(text) = glue.notice.clone() {
        if crate::hud::info_box(ctx, hud, assets, &text) {
            glue.notice = None;
        }
    }
    if let Some((object, point)) = glue.confirm.clone() {
        let text = glue.data(assets).events.get("TELEPORT_ConfirmPvPZone").cloned().unwrap_or_default();
        match crate::hud::confirm_box(ctx, hud, assets, ("portal-pvp", object), &text) {
            Some(true) => {
                glue.confirm = None;
                let data = glue.data(assets);
                log::info!("portail : départ vers {} (zone de combat entre joueurs)", data.name(&point));
                send(net, TeleportCommand::Go { object, point });
            }
            Some(false) => glue.confirm = None,
            None => {}
        }
    }
    let Some(open) = glue.open.as_ref() else { return };
    let data = glue.data.get_or_insert_with(|| load_data(assets));
    let money_name = data.message("Money_Name");
    let rows: Vec<TeleportRow> = if open.return_only {
        vec![TeleportRow { name: data.message("Teleport_ReturnOnly"), fee: String::new(), status: 0 }]
    } else {
        open.rows.iter().map(|(p, fee, status)| TeleportRow { name: data.name(p), fee: format!("{fee} {money_name}"), status: *status }).collect()
    };
    let here = data.name(&open.here);
    let mut out = Vec::new();
    crate::hud::draw_teleport(ctx, hud, assets, TeleportView { here: &here, rows: &rows, first: &mut glue.first }, &mut out);
    for r in out {
        match r {
            PortalUi::Choose(i) => choose(glue, i, hud, assets, net, ui),
            _ => glue.open = None,
        }
    }
}

/// Ligne `i` de la liste choisie (clic ou `/portail n`) : la fenêtre se ferme, puis départ si l'or
/// suffit et que l'arrivée n'est pas en zone dangereuse.
fn choose(glue: &mut PortalGlue, i: usize, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>, _ui: &mut UiState) {
    let Some(open) = glue.open.take() else { return };
    if open.return_only {
        send(net, TeleportCommand::Return(open.object));
        return;
    }
    let Some((point, fee, status)) = open.rows.get(i).cloned() else { return };
    if status == 2 {
        return;
    }
    let data = glue.data(assets);
    if fee > hud.stats.money.unwrap_or(0) {
        glue.notice = Some(data.events.get("TELEPORT_NotEnoughMOney").cloned().unwrap_or_default());
    } else if data.points.get(&point).is_some_and(|p| p.region_type == 1) {
        // Zone de combat entre joueurs : la boîte de confirmation décide du départ (`panel`).
        glue.confirm = Some((open.object, point));
    } else {
        log::info!("portail : départ vers {} ({fee})", data.name(&point));
        send(net, TeleportCommand::Go { object: open.object, point });
    }
}

impl App {
    /// Mode combat du joueur en cours (AgpdCharacter+0xb38, lu par FUN_14017aaa0) : aussi refusé par le retour à la
    /// sélection des personnages (`app_return_login.rs`).
    pub(super) fn portal_combat_mode(&self) -> bool {
        self.portal.combat_until.is_some_and(|t| std::time::Instant::now() < t)
    }

    /// Relevé des événements : mode combat (coups et compétences du joueur ou sur lui, FUN_14017aab0)
    /// et métamorphose, que les portails refusent.
    pub(super) fn portal_observe(&mut self, ev: &WorldEvent) {
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        let fight = match ev {
            WorldEvent::CharacterAction { cid, target_cid, .. } => *cid == me || *target_cid == me,
            WorldEvent::Skill(SkillEvent::Cast { caster, target, .. }) => (*caster == me && *target != me && *target != 0) || (*target == me && *caster != me),
            WorldEvent::Transformed { cid, transformed } if *cid == me => {
                self.portal.transformed = *transformed;
                false
            }
            WorldEvent::Connected | WorldEvent::Disconnected(_) => {
                self.portal.combat_until = None;
                self.portal.transformed = false;
                false
            }
            _ => false,
        };
        if fight && me != 0 {
            self.portal.combat_until = Some(std::time::Instant::now() + COMBAT_MODE);
        }
    }

    /// Demande au portail porté par `object` (op 4), sauf en mode combat ou métamorphosé : le message
    /// d'Archonia va alors dans la discussion (FUN_140515040, canal « SYSTEM »).
    fn portal_request(&mut self, object: u32) -> bool {
        let refusal = if self.portal.combat_until.is_some_and(|t| std::time::Instant::now() < t) {
            Some("Teleport_CombatMode")
        } else if self.portal.transformed {
            Some("Teleport_TransformChar")
        } else {
            None
        };
        if let Some(key) = refusal {
            let text = self.portal.data(&self.assets.clone()).message(key);
            log::info!("portail : demande refusée par le client ({key})");
            self.addon_chat_received("CHAT_MSG_SYSTEM", &text, "");
            return false;
        }
        send(self.net.as_ref(), TeleportCommand::Request(object));
        true
    }

    /// Portails des tuiles autour du joueur (lus une fois par tuile).
    pub(super) fn portal_frame(&mut self) {
        let Some(me) = self.player.as_ref().map(|p| p.position) else { return };
        let (sx, sy) = sector_of(me.x, me.z);
        for dy in [-16, 0, 16] {
            for dx in [-16, 0, 16] {
                let tile = tile_of(sx + dx, sy + dy);
                if self.portal.tiles.insert(tile) {
                    self.portal.portals.extend(tile_portals(&self.assets, tile));
                }
            }
        }
        if self.portal.open.as_ref().is_some_and(|o| Vec3::new(me.x - o.at.x, 0.0, me.z - o.at.z).length() > CLOSE_MOVE) {
            self.portal.open = None;
        }
    }

    /// Ferme la liste des destinations (`UI_Teleport`), par exemple au rappel de groupe accepté.
    pub(super) fn portal_close_list(&mut self) {
        self.portal.open = None;
    }

    /// Clic gauche sur un portail : demande au serveur (op 4). Rend `true` si un portail est visé.
    pub(super) fn portal_left_click(&mut self) -> bool {
        let (Some(gpu), Some((cx, cy))) = (&self.gpu, self.last_cursor) else { return false };
        let (w, h) = (gpu.renderer.config.width as f32, gpu.renderer.config.height as f32);
        let ndc = Vec3::new((cx as f32 / w) * 2.0 - 1.0, 1.0 - (cy as f32 / h) * 2.0, 0.0);
        let inv = self.camera.view_proj(gpu.renderer.aspect()).inverse();
        let near = inv.project_point3(ndc);
        let dir = (inv.project_point3(Vec3::new(ndc.x, ndc.y, 1.0)) - near).normalize_or_zero();
        // Portail le plus proche dont la sphère englobante coupe le rayon du curseur.
        let hit = self
            .portal
            .portals
            .iter()
            .filter_map(|p| {
                let (center, radius) = self.objects.placed_bounds(&p.placement)?;
                let t = (center - near).dot(dir);
                (t > 0.0 && (near + dir * t).distance(center) <= radius).then_some((p.id, t))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id);
        let Some(object) = hit else { return false };
        log::info!("portail : demande à l'objet {object}");
        self.portal_request(object);
        true
    }

    /// `/portail` (`/portal`) : demande au portail le plus proche, comme un clic sur l'objet ;
    /// `/portail n` : choisit la n-ième destination de la liste ouverte, comme un clic sur la ligne ;
    /// `/portail oui` / `/portail non` : répond à la confirmation d'une zone de combat entre joueurs.
    pub(super) fn portal_slash(&mut self, cmd: &str, arg: &str) -> bool {
        if !matches!(cmd, "/portail" | "/portal") {
            return false;
        }
        let arg = arg.trim();
        if let (Some((object, point)), true) = (self.portal.confirm.clone(), matches!(arg, "oui" | "non")) {
            self.portal.confirm = None;
            if arg == "oui" {
                send(self.net.as_ref(), TeleportCommand::Go { object, point });
            }
            return true;
        }
        if !arg.is_empty() {
            let assets = self.assets.clone();
            self.portal.data(&assets);
            let data = self.portal.data.as_ref().expect("données lues");
            let pick = self.portal.open.as_ref().and_then(|o| match arg.parse::<usize>() {
                Ok(n) => Some(n.saturating_sub(1)),
                Err(_) => o.rows.iter().position(|(p, _, _)| p == arg || data.name(p).eq_ignore_ascii_case(arg)),
            });
            match pick {
                Some(i) => choose(&mut self.portal, i, &mut self.hud, &assets, self.net.as_ref(), &mut self.ui),
                None => self.ui.push_chat("[portail] Destination absente de la liste ouverte."),
            }
            return true;
        }
        let me = self.player.as_ref().map_or(Vec3::ZERO, |p| p.position);
        let near = self.portal.portals.iter().map(|p| (p.id, p.point.clone(), p.placement.position.distance(me))).min_by(|a, b| a.2.total_cmp(&b.2));
        match near {
            Some((object, point, d)) => {
                let name = self.portal.data(&self.assets.clone()).name(&point);
                if self.portal_request(object) {
                    self.ui.push_chat(format!("[portail] {name} à {d:.0} unités : demande envoyée."));
                }
            }
            None => self.ui.push_chat("[portail] Aucun portail dans les tuiles proches."),
        }
        true
    }

    /// Réponses du portail.
    pub(super) fn on_teleport_event(&mut self, ev: TeleportEvent) {
        let me = self.player.as_ref().map(|p| p.cid);
        match ev {
            TeleportEvent::Granted { object } => {
                let Some(here) = self.portal.portals.iter().find(|p| p.id == object).map(|p| p.point.clone()) else {
                    log::warn!("portail : accord pour l'objet {object}, absent des tuiles lues");
                    return;
                };
                let race = self.player.as_ref().map(|p| crate::login_settings::template_identity(&self.assets, p.tid).0).and_then(|r| crate::login_settings::RACES.iter().position(|x| *x == r));
                let money = self.hud.stats.money.unwrap_or(0);
                let assets = self.assets.clone();
                let data = self.portal.data(&assets);
                let return_only = data.points.get(&here).is_some_and(|p| p.region_type == 1);
                let rows = data
                    .targets(&here)
                    .into_iter()
                    .map(|t| {
                        let fee = data.fee(&here, &t);
                        let status = if !admits(data.points[&t].use_type, race) { 2 } else if fee > money { 1 } else { 0 };
                        (t, fee, status)
                    })
                    .collect();
                let at = self.player.as_ref().map_or(Vec3::ZERO, |p| p.position);
                self.portal.open = Some(OpenList { object, here, return_only, rows, at });
                self.portal.first = 0;
            }
            TeleportEvent::Start { cid, position } if Some(cid) == me => {
                self.portal.open = None;
                self.portal.loading = true;
                if let (Some(p), Some(at)) = (&mut self.player, position) {
                    p.position = Vec3::from_array(at);
                    p.destination = p.position;
                }
                self.walking = false;
            }
            TeleportEvent::Loaded { cid } if Some(cid) == me => self.portal.loading = false,
            // Op 2 (rappel 4) : la fenêtre de téléport se ferme, le chargement aussi.
            TeleportEvent::Canceled { cid } if Some(cid) == me => {
                self.portal.loading = false;
                self.portal.open = None;
            }
            // Op 6 : demande ignorée, la liste ne s'ouvre pas (rien d'affiché).
            TeleportEvent::Ignored { cid } if Some(cid) == me => log::info!("portail : demande ignorée par le serveur"),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destinations_et_races() {
        let mut d = PortalData::default();
        let mut add = |name: &str, groups: &[&str], targets: &[&str], use_type: i64| {
            let s = |l: &[&str]| l.iter().map(|g| g.to_string()).collect();
            d.order.push(name.to_string());
            d.points.insert(name.to_string(), Point { groups: s(groups), targets: s(targets), use_type, region_type: 0 });
        };
        add("A", &["0.a"], &["0.a", "1.b"], 0);
        add("B", &["1.b"], &["1.b"], 4);
        add("C", &["0.a", "1.b"], &["0.a"], 0);
        assert_eq!(d.targets("A"), vec!["C".to_string(), "B".to_string()]);
        assert_eq!(d.targets("B"), vec!["C".to_string()]);
        assert!(admits(4, Some(2)) && !admits(4, Some(0)) && admits(0, Some(3)) && admits(4, None));
    }
}
