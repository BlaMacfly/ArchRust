//! Colle des ombres projetées des objets sur le sol (AgcmShadow2 d'Archonia, voir `shadow` pour les
//! relevés) : à chaque image, secteur courant du joueur (ou de la caméra) et option `OP_Shadow` ;
//! quand `shadow::Schedule` le demande, liste des objets qui projettent (`WorldObjects::shadow_casters`)
//! envoyée au rendu (`Renderer::shadow_request`) avec la caméra de lumière du secteur.
//!
//! CHOIX d'ArchRust : une nouvelle prise est aussi demandée quand le nombre de secteurs d'objets
//! chargés autour du secteur change (ArchRust les charge un par image ; Archonia attendrait la prise
//! suivante, 120 s plus tard).

use super::App;
use crate::shadow::{self, Schedule, ShadowView};
use crate::shadow_render::ShadowRequest;
use std::time::Instant;

#[derive(Default)]
pub(super) struct ShadowGlue {
    schedule: Schedule,
    last: Option<Instant>,
    /// Secteurs d'objets chargés autour du secteur courant à la dernière vérification.
    loaded: usize,
    on: bool,
}

impl App {
    /// Ombres projetées de l'image (après les décalques).
    pub(super) fn shadow_frame(&mut self) {
        let now = Instant::now();
        let dt_ms = self.shadow2.last.map_or(0, |l| now.duration_since(l).as_millis() as u64);
        self.shadow2.last = Some(now);
        let on = self.phase == super::Phase::InWorld && shadow::enabled(self.options.video.shadow) && self.assets.has_client();
        let Some(gpu) = self.gpu.as_mut() else { return };
        if !on {
            if self.shadow2.on {
                gpu.renderer.shadow_uv = None;
                gpu.renderer.shadow_request = None;
                self.shadow2.schedule.reset();
                self.shadow2.loaded = 0;
                self.shadow2.on = false;
            }
            return;
        }
        let center = self.player.as_ref().map(|p| p.position).unwrap_or(self.camera.target);
        let sector = crate::world::sector_of(center.x, center.z);
        let loaded = self.objects.loaded_sectors_near(sector, shadow::SECTOR_RANGE);
        if loaded != self.shadow2.loaded {
            self.shadow2.loaded = loaded;
            self.shadow2.schedule.request();
        }
        if !self.shadow2.schedule.tick(sector, dt_ms) {
            return;
        }
        let view = ShadowView::for_sector(sector.0, sector.1);
        let items = self.objects.shadow_casters(&self.assets, sector, shadow::SECTOR_RANGE);
        log::debug!("ombres projetées : secteur {sector:?}, {} pièce(s) d'objets", items.len());
        gpu.renderer.shadow_request = Some(ShadowRequest { clip: view.clip_matrix(), items });
        gpu.renderer.shadow_uv = Some(view.uv_matrix());
        self.shadow2.on = true;
    }
}
