//! Caméra d'Archonia en jeu (AgcmCamera2, AgcuCamMode) : réglages de `ini/CamSetting.xml`, touches
//! de caméra, modes, hauteur visée par TID et collision avec le sol et les objets du décor. Règles
//! et adresses dans [`crate::camera`].
//!
//! **Touches** (FUN_14085ee60, entrées `HotkeyFlag=1` de l'EventMap sans action, retrouvées par leur
//! défaut comme FUN_14054d990, donc modifiables dans `UI_Hotkey`) : flèches « Camera Left / Right /
//! Up / Down » (DIK 203, 205, 200, 208), pavé − (74) et + (78) pour reculer et avancer, F10 « Sky
//! Camera » (mode 1), F11 « Angle Camera » (mode 2), F12 « Far Camera » (mode 0). Les touches tenues
//! agissent à chaque image selon le temps écoulé. Le bouton caméra de la minicarte
//! (`Minimap_CameraView_Click`) passe au mode suivant (0 → 1 → 2 → 0).
//!
//! **Écart** : à cheval, Archonia vise la monture (FUN_1406a6d90) ; ArchRust garde le cavalier.
//!
//! **Minicarte** (AgcmUIMinimap) : `MiniMap_ToggleUI` (Maj+M, entrée `MenuBtnMiniMap`) ferme ou ouvre la fenêtre
//! `MiniMap` ; dans les 21 régions de `Type` à bit `0x200` elle ne s'ouvre pas et se ferme en y entrant
//! (`MiniMap_ForcedClose`) ; en sortir carte fermée affiche `MiniMap_YouCanOpenUI`.

use super::hotkey_glue::{dik_of, kind_of, Key};
use super::App;
use crate::camera::CamSetting;
use winit::keyboard::KeyCode;

/// Défauts des touches tenues : gauche, droite, haut, bas, pavé −, pavé +.
/// Bit « sans minicarte » du `Type` de RegionTemplate.ini (`+0x7d & 2`, FUN_140701930 et FUN_140701440).
const REGION_NO_MINIMAP: i64 = 0x200;
const HELD_KEYS: [u8; 6] = [203, 205, 200, 208, 74, 78];
/// Défauts des touches de mode et mode choisi (FUN_14085ee60) : F10 → 1, F11 → 2, F12 → 0.
const MODE_KEYS: [(u8, usize); 3] = [(68, 1), (87, 2), (88, 0)];

#[derive(Default)]
pub struct CameraGlue {
    setting: Option<CamSetting>,
    /// Touches tenues (ordre de [`HELD_KEYS`]).
    held: [bool; 6],
    /// Bornes du fichier appliquées à la caméra de jeu.
    ready: bool,
    /// TID dont `Height` / `Face` sont appliqués.
    heights_for: Option<i32>,
    /// Cible déplacée au bouton du milieu (commande d'ArchRust) : la caméra ne suit plus jusqu'au
    /// prochain pas du joueur.
    pub panned: bool,
    /// Le joueur a bougé depuis l'image précédente.
    pub moved: bool,
}

impl App {
    fn camera_setting(&mut self) -> &CamSetting {
        let assets = &self.assets;
        self.cam.setting.get_or_insert_with(|| {
            let text = super::npc_glue::client_text_in(assets, "ini", "CamSetting.xml");
            if text.trim().is_empty() {
                log::warn!("caméra : ini/CamSetting.xml introuvable, valeurs d'Archonia par défaut");
                CamSetting::default()
            } else {
                let s = CamSetting::parse(&text);
                log::info!("caméra : CamSetting.xml, {} réglage(s) par TID", s.specific.len());
                s
            }
        })
    }

    /// Caméra de jeu remise à neuf (sortie de la scène de connexion) : bornes relues à l'image suivante.
    pub(super) fn camera_reset(&mut self) {
        self.cam.ready = false;
        self.cam.heights_for = None;
        self.cam.held = [false; 6];
    }

    /// Mode demandé (touches, bouton de la minicarte).
    pub(super) fn camera_set_mode(&mut self, mode: usize) {
        let setting = self.camera_setting().clone();
        self.camera.set_mode(&setting, mode);
    }

    /// Bouton caméra de la minicarte : mode suivant.
    pub(super) fn camera_next_mode(&mut self) {
        let next = self.camera.next_mode();
        self.camera_set_mode(next);
    }

    /// Touche de caméra d'Archonia : `true` si elle a servi.
    pub(super) fn camera_key(&mut self, code: KeyCode, down: bool) -> bool {
        if self.phase != super::Phase::InWorld {
            return false;
        }
        let Some(dik) = dik_of(code) else { return false };
        let kind = kind_of(self.addon_glue.modifiers);
        let assets = self.assets.clone();
        for (i, default) in HELD_KEYS.iter().enumerate() {
            let k = self.hotkey.current_key(&assets, Key { kind: 0, code: *default });
            // Relâchement : quel que soit le modificateur, la touche ne reste pas tenue.
            if k.code == dik && (!down || k.kind == kind) {
                self.cam.held[i] = down;
                return true;
            }
        }
        if !down {
            return false;
        }
        for (default, mode) in MODE_KEYS {
            if self.hotkey.current_key(&assets, Key { kind: 0, code: default }) == (Key { kind, code: dik }) {
                self.camera_set_mode(mode);
                return true;
            }
        }
        false
    }

    /// Repères de la minicarte : PNJ vus à portée du raster (237 px à 0,0037695 px par unité, soit
    /// ~31 400 unités de côté), avec leur type de quête (voir `app_npc.rs`), et objets à quêtes qui
    /// proposent ou reprennent une quête (voir `app_quest_object.rs`).
    pub(super) fn minimap_icons(&mut self) -> Vec<crate::hud::MinimapIcon> {
        let Some(me) = self.player.as_ref().map(|p| p.position) else { return Vec::new() };
        const REACH: f32 = 237.0 / 2.0 / 0.003_769_531_1;
        let near: Vec<(i32, glam::Vec3)> = self.entities.iter().filter(|e| !e.is_player && (e.position.x - me.x).abs() < REACH && (e.position.z - me.z).abs() < REACH).map(|e| (e.cid, e.position)).collect();
        let mut icons: Vec<crate::hud::MinimapIcon> = near.into_iter().filter_map(|(cid, pos)| self.npc_minimap_kind(cid).map(|kind| crate::hud::MinimapIcon { kind, pos })).collect();
        icons.extend(self.quest_object_minimap_icons(me, REACH));
        icons
    }

    /// Région sans minicarte : bit `0x200` (`disable_minimap`, `+0x7d & 2`) du `Type` de RegionTemplate.ini.
    fn minimap_disabled_in(&mut self, region: Option<u16>) -> bool {
        let assets = self.assets.clone();
        region.and_then(|r| self.consumables.region_flags(&assets, r)).is_some_and(|t| t & REGION_NO_MINIMAP != 0)
    }

    /// Message `Action=4:3` de l'[EventMap] dans la discussion.
    fn minimap_message(&mut self, event: &str) {
        let text = super::portal_glue::ui_events(&self.assets).get(event).cloned().unwrap_or_default();
        self.hud.push_system(&text);
    }

    /// `MiniMap_ToggleUI` (Maj+M, `MenuBtnMiniMap` ; FUN_140701930) : ouverte → fermée ; fermée → ouverte, sauf dans
    /// une région sans minicarte (`MiniMap_ForcedClose`).
    pub(super) fn minimap_toggle(&mut self) {
        if !crate::hud::minimap_closed(&self.hud) {
            crate::hud::set_minimap_closed(&mut self.hud, true);
            return;
        }
        let region = self.zone_region.as_ref().map(|z| z.region);
        if self.minimap_disabled_in(region) {
            self.minimap_message("MiniMap_ForcedClose");
            return;
        }
        crate::hud::set_minimap_closed(&mut self.hud, false);
    }

    /// Changement de région (FUN_140701440) : entrée dans une région sans minicarte, carte ouverte → fermée et
    /// `MiniMap_ForcedClose` ; sortie d'une telle région, carte fermée → `MiniMap_YouCanOpenUI` (la carte reste fermée).
    fn minimap_region_frame(&mut self) {
        let region = self.zone_region.as_ref().map(|z| z.region);
        let old = crate::hud::swap_minimap_region(&mut self.hud, region);
        if old == region || region.is_none() {
            return;
        }
        let closed = crate::hud::minimap_closed(&self.hud);
        if self.minimap_disabled_in(region) {
            if !closed {
                crate::hud::set_minimap_closed(&mut self.hud, true);
                self.minimap_message("MiniMap_ForcedClose");
            }
        } else if closed && self.minimap_disabled_in(old) {
            self.minimap_message("MiniMap_YouCanOpenUI");
        }
    }

    /// Image en jeu : bornes du fichier, hauteurs du TID du joueur, touches tenues, glissement entre
    /// modes, suivi du joueur et collision avec le sol et les objets.
    pub(super) fn camera_frame(&mut self, dt: f32) {
        // Boutons de la minicarte de l'image précédente (caméra, carte du monde).
        for r in std::mem::take(&mut self.hud.minimap_requests) {
            match r {
                crate::hud::MinimapUi::CameraView => self.camera_next_mode(),
                crate::hud::MinimapUi::WorldMap => self.worldmap_toggle(),
            }
        }
        self.minimap_region_frame();
        let Some((feet, tid)) = self.player.as_ref().map(|p| (p.position, p.tid)) else { return };
        if self.phase != super::Phase::InWorld {
            return;
        }
        if !self.cam.ready {
            let setting = self.camera_setting().clone();
            let mode = self.camera.mode;
            self.camera.apply_mode(&setting, mode);
            self.cam.ready = true;
        }
        if self.cam.heights_for != Some(tid) {
            let (h, f) = self.camera_setting().heights(tid);
            self.camera.look_height = h;
            self.camera.face_height = f;
            self.cam.heights_for = Some(tid);
        }
        let ms = dt * 1000.0;
        let [left, right, up, down, back, near] = self.cam.held;
        let turn = (left as i32 - right as i32) as f32;
        let tilt = (up as i32 - down as i32) as f32;
        if turn != 0.0 || tilt != 0.0 {
            // Même effet qu'un glissement de souris de `ms` pixels (FUN_1406a9960, FUN_1406a9830).
            self.camera.rotate(-turn * ms, tilt * ms);
        }
        let zoom = (near as i32 - back as i32) as f32;
        if zoom != 0.0 {
            self.camera.zoom_units(zoom * ms * crate::camera::ZOOM_PER_UNIT);
        }
        self.camera.step_transition(ms);
        if self.cam.moved {
            self.cam.panned = false;
            self.cam.moved = false;
        }
        if !self.cam.panned {
            self.camera.follow(feet);
        }
        let (terrain, objects, assets) = (&self.terrain, &self.objects, &self.assets);
        self.camera.collide(dt, |x, z| terrain.height_at(x, z), |from, to| objects.segment_hit(assets, from, to));
    }
}

#[cfg(test)]
mod tests {
    use crate::assets::Assets;
    use crate::camera::CamSetting;

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : `CamSetting.xml`, textures des ombres, du
    /// repère de destination, des bulles et des barres, fond masqué de la minicarte.
    #[test]
    fn donnees_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let s = CamSetting::parse(&super::super::npc_glue::client_text_in(&assets, "ini", "CamSetting.xml"));
        assert_eq!((s.modes[0].distance, s.modes[0].distance_min, s.modes[0].distance_max), (600.0, 96.0, 2750.0));
        assert_eq!((s.modes[0].pitch_min, s.modes[0].pitch_max, s.modes[2].pitch_min), (-45.0, 55.0, 45.0));
        assert_eq!(s.heights(96), (150.0, 145.0));
        assert!(s.specific.len() > 40, "{}", s.specific.len());
        assert!(assets.texture_found("ShadowCircle.bmp"));
        assert!(assets.texture_found("target.png"));
        for f in ["TB_lefttop.tif", "TB_back.tif", "TB_tailtobottom.tif", "HP_Blank.png", "HP_Bar.png", "MP_Bar.png", "Main_Minimap_Base_hide.png", "Main_Minimap_Pos_NPC0.png", "Main_Minimap_Pos_NPC1.png", "Main_Minimap_Pos_NPC2.png"] {
            assert!(assets.image("texture/ui", f).is_ok(), "{f}");
        }
    }
}
