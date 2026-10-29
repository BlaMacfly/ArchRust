//! Packs déposés en cours de jeu : `assets/textures/`, `assets/sounds/` et `patches/` sont surveillés
//! toutes les deux secondes (empreinte des noms, tailles et dates, voir `Assets::pack_signature`).
//! Un changement n'est appliqué qu'une fois le dossier stable d'un examen à l'autre (une copie en
//! cours n'est pas lue à moitié). La pile des packs est alors relue avec les préférences du joueur :
//! * les textures déjà chargées que les packs fournissent (avant ou après) sont renvoyées au GPU ; les
//!   modèles les retrouvent par leur nom à chaque image, sans être reconstruits ;
//! * les secteurs du sol, dont les textures sont liées à la construction, sont reconstruits ;
//! * les images d'interface et la minicarte sont relues à leur prochain affichage ;
//! * les sons sont relus à leur prochaine lecture.

use super::App;
use std::time::{Duration, Instant};

/// Intervalle entre deux examens des dossiers.
const CHECK_EVERY: Duration = Duration::from_secs(2);

#[derive(Default)]
pub(super) struct AssetsWatch {
    next_check: Option<Instant>,
    /// Empreinte appliquée.
    signature: Option<u64>,
    /// Empreinte nouvelle vue au dernier examen, appliquée si elle n'a pas bougé au suivant.
    pending: Option<u64>,
}

impl App {
    /// Recharge les packs si un dossier surveillé a changé et ne bouge plus.
    pub(super) fn watch_packs(&mut self) {
        let now = Instant::now();
        if self.assets_watch.next_check.is_some_and(|t| now < t) {
            return;
        }
        self.assets_watch.next_check = Some(now + CHECK_EVERY);
        let signature = self.assets.pack_signature();
        let watch = &mut self.assets_watch;
        match watch.signature {
            None => {
                watch.signature = Some(signature);
                return;
            }
            Some(applied) if applied == signature => {
                watch.pending = None;
                return;
            }
            _ if watch.pending != Some(signature) => {
                watch.pending = Some(signature);
                return;
            }
            _ => {}
        }
        watch.signature = Some(signature);
        watch.pending = None;
        let touched = self.assets.reload_packs(&self.packs.settings());
        self.packs = self.assets.pack_stack();
        let refreshed = self.refresh_textures(&touched);
        if !touched.is_empty() {
            self.terrain.rebuild_meshes();
        }
        self.hud.forget_images();
        let packs = self.packs.patches.iter().filter(|p| p.enabled && !p.official).count();
        log::info!("packs rechargés : {packs} pack(s) des joueurs, {refreshed} texture(s) renvoyée(s) au GPU");
        self.ui.push_chat(format!("[système] packs rechargés : {packs} pack(s) actif(s), {refreshed} texture(s) mise(s) à jour"));
    }

    /// Renvoie au GPU les textures déjà chargées dont le nom correspond à un motif `touched`
    /// (fournies par un pack avant ou après le rechargement). Rend leur nombre.
    fn refresh_textures(&mut self, touched: &[String]) -> usize {
        let Some(gpu) = self.gpu.as_mut() else { return 0 };
        let names: Vec<String> = gpu
            .renderer
            .textures
            .keys()
            .filter(|name| touched.iter().any(|pattern| archrust_patch::manifest::glob_match(pattern, name)))
            .cloned()
            .collect();
        for name in &names {
            if let Ok(img) = self.assets.texture(name) {
                gpu.renderer.upload_texture(name, img.width, img.height, &img.data);
            }
        }
        names.len()
    }
}
