//! Carte du monde d'Archonia : fenêtre `UI_WORLD_MAP` (780 × 575, `UIPos` 100,50), ouverte par la
//! touche M (`Hotkey=0:50` « World Map », `WorldMap_Toggle`). Le contrôle de type 16 (`WorldMap`)
//! affiche l'image de la carte choisie dans `ini/worldmap.ini` (`RegionTexture`, pack `texture/ui`,
//! FUN_14068b8f0 : `FUN_14078e8a0(contrôle, carte + 0x104, "Texture\UI\\", 1)`, 1024 × 1024 ; le pack
//! `texture/worldmap`, sans les 92 images nommées, n'est lu que par FUN_140702c40, jamais appelée) : cette
//! image porte aussi le cadre de la fenêtre. La zone
//! `UIActiveAreaSX…EY` de l'image couvre le rectangle du monde `XStart…ZEnd` (x vers la droite,
//! z vers le bas).
//!
//! * carte du continent : boutons des zones (contrôles de type 21, textures `…_A` au repos et `…_B`
//!   au survol, `WorldMap_LeftClick`) qui ouvrent la carte de même `GroupID` ;
//! * clic droit (`WorldMap_RightClick`) : carte parente (`ParentGroupID`) ;
//! * repère du joueur (`Main_Minimap_Pos_PC.png`) sur les cartes de zone et de région ;
//! * MJ : clic gauche sur une carte de zone ou de région = téléportation à cet endroit (voir
//!   `app_worldmap.rs`).

use super::*;

/// Fenêtre à charger avec le reste de l'interface.
pub(super) const WORLDMAP_WINDOWS: [&str; 1] = ["UI_WORLD_MAP"];

/// Carte affichée.
pub struct WorldMapView<'a> {
    pub name: &'a str,
    /// Image de la carte (`RegionTexture`).
    pub texture: &'a str,
    /// `UIActiveAreaSX, SY, EX, EY` (px de l'image) et `XStart, ZStart, XEnd, ZEnd` (unités du monde).
    pub area: [f32; 4],
    pub bounds: [f32; 4],
    /// Carte du continent (sans parente) : boutons des zones, pas de repère ni de téléportation.
    pub continent: bool,
    pub player: Option<Vec3>,
    /// Le joueur est MJ : clic gauche = téléportation.
    pub gm: bool,
}

/// Demande de la carte du monde.
#[derive(Debug, Clone, PartialEq)]
pub enum WorldMapUi {
    Close,
    /// Bouton de zone de la carte du continent (`CtrlName` = `GroupID` de la carte à ouvrir).
    Zone(String),
    /// Clic droit : carte parente.
    Up,
    /// Clic gauche d'un MJ : point du monde visé.
    Teleport { x: f32, z: f32 },
}

/// Point de l'image (px) d'un point du monde.
pub fn world_to_map(area: [f32; 4], bounds: [f32; 4], x: f32, z: f32) -> [f32; 2] {
    let fx = (x - bounds[0]) / (bounds[2] - bounds[0]).max(1.0);
    let fz = (z - bounds[1]) / (bounds[3] - bounds[1]).max(1.0);
    [area[0] + fx * (area[2] - area[0]), area[1] + fz * (area[3] - area[1])]
}

/// Point du monde d'un point de l'image (px), inverse de [`world_to_map`].
pub fn map_to_world(area: [f32; 4], bounds: [f32; 4], px: f32, py: f32) -> [f32; 2] {
    let fx = (px - area[0]) / (area[2] - area[0]).max(1.0);
    let fz = (py - area[1]) / (area[3] - area[1]).max(1.0);
    [bounds[0] + fx * (bounds[2] - bounds[0]), bounds[1] + fz * (bounds[3] - bounds[1])]
}

/// Dessine la carte du monde ; les demandes s'ajoutent à `out`.
pub fn draw_world_map(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &WorldMapView, out: &mut Vec<WorldMapUi>) {
    let Some(w) = super::npc_ui::window(hud, assets, "UI_WORLD_MAP") else { return };
    super::npc_ui::frame(ctx, hud, assets, &w, "archonia-world-map", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let map_rect = layout.rect(origin, [0.0, 0.0], w.size);
        // Image de la carte, au pixel près depuis le coin haut gauche (cadre compris).
        if let Some((tex, orig)) = hud.image(ctx, assets, view.texture) {
            let uv = Pos2::new((w.size[0] / orig[0]).min(1.0), (w.size[1] / orig[1]).min(1.0));
            let draw = Rect::from_min_size(map_rect.min, Vec2::new(map_rect.width() * (orig[0] / w.size[0]).min(1.0), map_rect.height() * (orig[1] / w.size[1]).min(1.0)));
            painter.image(tex, draw, Rect::from_min_max(Pos2::ZERO, uv), Color32::WHITE);
        }
        // Nom de la carte sur la plaque du haut de l'image.
        shadow_text(painter, origin + Vec2::new(397.0, 52.0) * s, Align2::CENTER_CENTER, view.name, (12.0 * s).max(10.0), Color32::from_rgb(255, 230, 160));
        // Surface de la carte : clic droit (carte parente), clic gauche d'un MJ (téléportation).
        let surface = ui.interact(map_rect, egui::Id::new("archonia-world-map-surface"), egui::Sense::click());
        if surface.secondary_clicked() {
            out.push(WorldMapUi::Up);
        }
        let mut on_button = false;
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match c.kind {
                // Boutons de zone de la carte du continent.
                21 if view.continent => {
                    let resp = ui.interact(r, egui::Id::new(("archonia-world-map-zone", c.name.as_str())), egui::Sense::click());
                    let pick = |i: u32| c.textures.iter().find(|(k, _)| *k == i).map(|(_, n)| n.clone());
                    let hovered = resp.hovered();
                    if let Some(t) = if hovered { pick(2) } else { pick(1) } {
                        control_image(ctx, hud, assets, painter, r, c.size, &t, 1.0);
                    }
                    if hovered {
                        shadow_text(painter, r.center(), Align2::CENTER_CENTER, &c.name, (12.0 * s).max(10.0), Color32::WHITE);
                    }
                    if resp.clicked() {
                        out.push(WorldMapUi::Zone(c.name.clone()));
                        on_button = true;
                    }
                    if resp.secondary_clicked() {
                        out.push(WorldMapUi::Up);
                    }
                }
                3 if c.name == "CONTROL_16087" => {
                    let resp = ui.interact(r, egui::Id::new("archonia-world-map-x"), egui::Sense::click());
                    if let Some(t) = button_texture(c).map(str::to_string) {
                        control_image(ctx, hud, assets, painter, r, c.size, &t, 1.0);
                    }
                    if resp.clicked() {
                        out.push(WorldMapUi::Close);
                        on_button = true;
                    }
                }
                _ => {}
            }
        }
        if view.continent {
            return;
        }
        // Repère du joueur.
        if let Some(p) = view.player {
            let inside = p.x >= view.bounds[0] && p.x <= view.bounds[2] && p.z >= view.bounds[1] && p.z <= view.bounds[3];
            if inside {
                let at = world_to_map(view.area, view.bounds, p.x, p.z);
                let center = origin + Vec2::new(at[0], at[1]) * s;
                if map_rect.contains(center) {
                    if let Some((tex, orig)) = hud.image(ctx, assets, "Main_Minimap_Pos_PC.png") {
                        painter.image(tex, Rect::from_center_size(center, Vec2::new(orig[0], orig[1]) * s), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    }
                }
            }
        }
        if view.gm && !on_button && surface.clicked() {
            if let Some(p) = surface.interact_pointer_pos() {
                let local = (p - origin) / s;
                let [x, z] = map_to_world(view.area, view.bounds, local.x, local.y);
                out.push(WorldMapUi::Teleport { x, z });
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carte_aller_retour() {
        // Tullan (`worldmap.ini` [9]) : 1,1 → 800,602 pour -431557,33615 → -276111,150200.
        let (area, bounds) = ([1.0, 1.0, 800.0, 602.0], [-431_557.0, 33_615.2, -276_111.0, 150_200.0]);
        let at = world_to_map(area, bounds, -358_935.0, 78_802.0);
        assert!((at[0] - 374.3).abs() < 1.0 && (at[1] - 234.0).abs() < 1.0, "{at:?}");
        let back = map_to_world(area, bounds, at[0], at[1]);
        assert!((back[0] + 358_935.0).abs() < 1.0 && (back[1] - 78_802.0).abs() < 1.0, "{back:?}");
    }
}
