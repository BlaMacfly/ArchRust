//! Réglages des onglets GFX, Affichage et Fonction des options (`hud_options.rs`), appliqués au jeu :
//!
//! - **GFX** : `CharView` borne la distance de tracé des personnages, `MapView` celle des objets et
//!   les rayons de sol détaillé et grossier (`world::TerrainView`), `NaturalObj` l'herbe (voir
//!   [`crate::options::VideoOptions`]) ;
//! - **Affichage** : noms filtrés dans `app_boards.rs`, casque retiré du modèle du joueur
//!   (`app_items.rs`) et envoyé au serveur (AgpmUIStatus op 3, champ 8) ;
//! - **Fonction** : drapeaux de refus envoyés au serveur (AgpmCharacter op 0x11, champ 22) à l'entrée
//!   en jeu et à chaque changement, comme Archonia (`FUN_140706ef0`, `FUN_14070ea70`). En attendant
//!   qu'ArchCore les applique, ArchRust décline lui-même les invitations de groupe et de guilde reçues.

use super::App;
use crate::net::NetLink;
use crate::objects::WorldObjects;
use crate::options::ClientOptions;
use crate::world::Terrain;
use archrust_netclient::Command;

impl App {
    /// Après la fenêtre des options : applique ce qui a changé depuis `before` (fonction associée,
    /// appelée pendant que l'interface emprunte le GPU).
    pub(super) fn options_changed(now: &ClientOptions, before: &ClientOptions, terrain: &mut Terrain, objects: &mut WorldObjects, net: Option<&NetLink>, in_world: bool) {
        if now.video != before.video {
            Self::apply_video_options(now, terrain, objects);
        }
        let Some(net) = net.filter(|_| in_world) else { return };
        if now.display.helmet != before.display.helmet {
            net.send(Command::ViewHelmet(now.display.helmet));
        }
        if now.refuse.flags() != before.refuse.flags() {
            net.send(Command::OptionFlags(now.refuse.flags()));
        }
    }

    /// Rayon de sol et d'objets chargés d'après `MapView`.
    fn apply_video_options(options: &ClientOptions, terrain: &mut Terrain, objects: &mut WorldObjects) {
        let radius = options.video.sector_radius();
        let view = crate::world::TerrainView { map_view: options.video.map_view, natural_obj: options.video.natural_obj };
        if terrain.view != view {
            terrain.view = view;
            terrain.last_center = None;
        }
        objects.lod_factor = options.video.map_factor();
        if terrain.radius != radius || objects.radius != radius {
            terrain.radius = radius;
            objects.radius = radius;
            // Secteurs gardés recalculés au prochain passage.
            terrain.last_center = None;
        }
    }

    /// Entrée en jeu : drapeaux de refus et casque d'après `options.toml`, qui fait foi comme
    /// `COption.ini` pour Archonia.
    pub(super) fn options_enter_world(&mut self) {
        self.terrain.view = crate::world::TerrainView { map_view: self.options.video.map_view, natural_obj: self.options.video.natural_obj };
        self.objects.lod_factor = self.options.video.map_factor();
        let Some(net) = self.net.as_ref() else { return };
        net.send(Command::OptionFlags(self.options.refuse.flags()));
        net.send(Command::ViewHelmet(self.options.display.helmet));
    }
}
