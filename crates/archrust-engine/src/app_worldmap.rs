//! Carte du monde (touche M, `Hotkey=0:50` d'Archonia) et téléportation des MJ par clic sur les
//! cartes. Fenêtre : `hud_worldmap.rs`.
//!
//! * Cartes : `ini/worldmap.ini`, une section par carte (`GroupID`, `ParentGroupID`, `XStart…ZEnd`,
//!   `RegionTexture`, `UIActiveArea*`). À l'ouverture, la plus petite carte de zone ou de région qui
//!   contient le joueur, sinon le continent.
//! * MJ : le niveau vient du paquet AGPMADMIN op 0 de l'entrée en jeu (voir
//!   `archrust_netclient::admin`). Un clic gauche sur une carte de zone ou de région, ou sur la
//!   minicarte, envoie la commande MJ `/move x,z` d'ArchCore par la discussion. Un joueur sans
//!   niveau MJ n'a rien de plus (comme Archonia, qui n'a pas cette fonction).

use super::App;
use crate::assets::Assets;
use crate::hud::{HudState, WorldMapUi, WorldMapView};
use archrust_netclient::Command;
use glam::Vec3;

/// Une carte de `worldmap.ini`.
#[derive(Debug, Clone)]
struct MapEntry {
    group: String,
    parent: String,
    bounds: [f32; 4],
    texture: String,
    area: [f32; 4],
}

#[derive(Default)]
pub struct WorldMapGlue {
    pub open: bool,
    maps: Option<Vec<MapEntry>>,
    current: Option<usize>,
    /// Niveau MJ du joueur (0 : joueur).
    pub gm_level: i32,
    /// Demandes de la fenêtre et de la minicarte, traitées à l'image suivante.
    pending: Vec<WorldMapUi>,
}

fn load_maps(assets: &Assets) -> Vec<MapEntry> {
    let Ok(t) = assets.table("ini", "worldmap.ini") else {
        log::warn!("carte du monde : ini/worldmap.ini illisible");
        return Vec::new();
    };
    let num = |s: &archrust_data::ini::IniSection, k: &str| s.get(k).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(0.0);
    let maps: Vec<MapEntry> = t
        .sections
        .iter()
        .filter(|s| s.name.parse::<i32>().is_ok())
        .filter_map(|s| {
            let texture = s.get("RegionTexture")?.trim().to_string();
            (!texture.is_empty()).then(|| MapEntry {
                group: s.get("GroupID").unwrap_or("").trim().to_string(),
                parent: s.get("ParentGroupID").unwrap_or("").trim().to_string(),
                bounds: [num(s, "XStart"), num(s, "ZStart"), num(s, "XEnd"), num(s, "ZEnd")],
                texture,
                area: [num(s, "UIActiveAreaSX"), num(s, "UIActiveAreaSY"), num(s, "UIActiveAreaEX"), num(s, "UIActiveAreaEY")],
            })
        })
        .collect();
    log::info!("carte du monde : {} carte(s)", maps.len());
    maps
}

/// Carte à ouvrir : la plus petite qui a une parente et contient le point, sinon la première sans parente.
fn map_for(maps: &[MapEntry], at: Option<Vec3>) -> Option<usize> {
    let inside = |m: &MapEntry, p: Vec3| p.x >= m.bounds[0].min(m.bounds[2]) && p.x <= m.bounds[0].max(m.bounds[2]) && p.z >= m.bounds[1].min(m.bounds[3]) && p.z <= m.bounds[1].max(m.bounds[3]);
    let size = |m: &MapEntry| ((m.bounds[2] - m.bounds[0]) * (m.bounds[3] - m.bounds[1])).abs();
    at.and_then(|p| maps.iter().enumerate().filter(|(_, m)| !m.parent.is_empty() && inside(m, p)).min_by(|a, b| size(a.1).total_cmp(&size(b.1))).map(|(i, _)| i))
        .or_else(|| maps.iter().position(|m| m.parent.is_empty()))
}

/// Fenêtre (passe egui) : les demandes attendent `App::worldmap_frame`.
pub fn panel(ctx: &egui::Context, glue: &mut WorldMapGlue, player: Option<Vec3>, hud: &mut HudState, assets: &Assets) {
    let gm = glue.gm_level > 0;
    // Minicarte : clic d'un MJ = téléportation.
    if gm {
        if let Some([x, z]) = player.and_then(|p| crate::hud::minimap_click(ctx, hud, assets, p)) {
            glue.pending.push(WorldMapUi::Teleport { x, z });
        }
    }
    if !glue.open {
        return;
    }
    let maps = glue.maps.get_or_insert_with(|| load_maps(assets));
    let Some(m) = glue.current.and_then(|i| maps.get(i)).cloned() else { return };
    let view = WorldMapView { name: &m.group, texture: &m.texture, area: m.area, bounds: m.bounds, continent: m.parent.is_empty(), player, gm };
    let mut out = Vec::new();
    crate::hud::draw_world_map(ctx, hud, assets, &view, &mut out);
    glue.pending.extend(out);
}

impl App {
    /// Niveau MJ reçu (AGPMADMIN op 0).
    pub(super) fn on_admin_level(&mut self, a: archrust_netclient::admin::AdminLevel) {
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        if me == 0 || a.cid == me {
            self.worldmap.gm_level = a.level;
            log::info!("niveau MJ : {}", a.level);
        }
    }

    /// Touche M : ouvre la carte de la région du joueur, ou ferme la carte.
    pub(super) fn worldmap_toggle(&mut self) {
        let assets = self.assets.clone();
        let glue = &mut self.worldmap;
        glue.open = !glue.open;
        if glue.open {
            let maps = glue.maps.get_or_insert_with(|| load_maps(&assets));
            glue.current = map_for(maps, self.player.as_ref().map(|p| p.position));
        }
    }

    /// Demandes de la carte et de la minicarte.
    pub(super) fn worldmap_frame(&mut self) {
        for r in std::mem::take(&mut self.worldmap.pending) {
            let glue = &mut self.worldmap;
            let Some(maps) = glue.maps.as_ref() else {
                if let WorldMapUi::Teleport { x, z } = r {
                    self.gm_move(x, z);
                }
                continue;
            };
            match r {
                WorldMapUi::Close => glue.open = false,
                WorldMapUi::Zone(name) => {
                    if let Some(i) = maps.iter().position(|m| m.group.eq_ignore_ascii_case(&name)) {
                        glue.current = Some(i);
                    }
                }
                WorldMapUi::Up => {
                    let parent = glue.current.and_then(|i| maps.get(i)).map(|m| m.parent.clone()).unwrap_or_default();
                    if let Some(i) = (!parent.is_empty()).then(|| maps.iter().position(|m| m.group.eq_ignore_ascii_case(&parent))).flatten() {
                        glue.current = Some(i);
                    }
                }
                WorldMapUi::Teleport { x, z } => self.gm_move(x, z),
            }
        }
    }

    /// Commande MJ `/move x,z` d'ArchCore (le serveur pose le personnage au sol du point).
    fn gm_move(&mut self, x: f32, z: f32) {
        if self.worldmap.gm_level <= 0 {
            return;
        }
        let cmd = format!("/move {x:.0},{z:.0}");
        log::info!("MJ : {cmd}");
        if let Some(net) = &self.net {
            net.send(Command::Say(cmd.clone()));
        }
        self.ui.push_chat(format!("[MJ] {cmd}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carte_du_joueur() {
        let m = |group: &str, parent: &str, b: [f32; 4]| MapEntry { group: group.into(), parent: parent.into(), bounds: b, texture: "t.png".into(), area: [0.0; 4] };
        let maps = vec![
            m("Chantra Continent", "", [-409_677.0, -206_464.0, 409_741.0, 204_455.0]),
            m("Human Zone", "Chantra Continent", [-396_600.0, -197_200.0, -102_200.0, 200.0]),
            m("Tullan", "Human Zone", [-431_557.0, 33_615.0, -276_111.0, 150_200.0]),
        ];
        assert_eq!(map_for(&maps, Some(Vec3::new(-358_935.0, 0.0, 78_802.0))), Some(2));
        assert_eq!(map_for(&maps, Some(Vec3::new(-200_000.0, 0.0, -100_000.0))), Some(1));
        assert_eq!(map_for(&maps, None), Some(0));
    }
}
