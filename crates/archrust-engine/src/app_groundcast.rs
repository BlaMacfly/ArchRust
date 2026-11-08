//! Ciblage au sol des compétences de zone, comme AgcmSkill et AgcmTargeting d'Archonia (relevés dans
//! `Archonia.exe`, sha256 e9c7609b…) :
//!
//! * **Compétences** : drapeau 0x4 de `+0xf0` du modèle (`FUN_1402fed30`), posé par le chargeur de
//!   `SKILL_SPEC.txt` (`FUN_14023d850`) pour `range` 7, 701 et 702. Données du client : Meteoric Shower,
//!   Narcolepsy, Imposed Fatigue (701), Concealment et ses deux variantes `ShadowIllusion` (702).
//! * **Première demande** (`FUN_14069a0c0`) : aucune vérification, rien n'est envoyé ; la compétence est
//!   gardée (`+0x1afc`) et `area_r` de son niveau (`+0x6148`, colonne × 100) part au rappel 0xd,
//!   qu'AgcmTargeting (`FUN_1404e5280`) change en repère au sol : état 4, largeur et hauteur = `area_r`
//!   (`FUN_1406f2080` : dimensions complètes), posé d'abord sous le joueur, blanc, sans échéance.
//! * **Texture** : `targeting.ini` `[Textures2]`, selon la largeur : sous 400 → 128, sous 700 → 256, sinon
//!   512 ; `ET_Alkadia_*` (indices 0 à 2) pour l'ArchLord (`FUN_1402944c0`), `ET_Selection_*` (3 à 5) pour
//!   les autres. Elle n'est pas proportionnelle au rayon : l'image est étirée.
//! * **Curseur** (`FUN_1404e41f0`, à chaque image) : le repère suit le point du sol sous le curseur.
//! * **Clic** : le bouton gauche enfoncé ne fait rien en état 4 (`FUN_1404e35d0`) ; relâché
//!   (`FUN_1404e4080`, emplacement relevé dans la table de la fenêtre `0x140d76698`, sens déduit) sur le
//!   sol, il retire le repère et relance `FUN_14069a0c0` par `FUN_140699f80` : la position passe à `+0x1948`,
//!   les contrôles habituels s'appliquent et le paquet part avec le joueur pour cible, sans attaque
//!   forcée, la position au champ 3 du mini SKILL_ACTION (`FUN_14033c440`, types 0 et 1 seulement, → `FUN_14033c2b0` :
//!   champs 0 type, 1 cible, 2 facteurs, 3 position, 4 attaque forcée de `+0x1a50`).
//! * **Annulation** : Échap (rappel 10 d'AgcmUIMain, `FUN_1404e2110`, déclencheur déduit) retire le repère
//!   et oublie la compétence. Le clic droit ne l'annule pas.
//! * **Une autre compétence pendant le ciblage** : la compétence gardée est oubliée, et la nouvelle part tout
//!   de suite — une compétence au sol avec la dernière position retenue. Le repère reste jusqu'au prochain
//!   clic, qui n'envoie alors rien (`FUN_140699f80` ne fait rien sans compétence gardée).
//! * Non repris : rappel 0x10 d'AgcmCharacter (`0x1404e2480`, remet l'état à 0), déclencheur non relevé.

use super::App;
use glam::Vec3;

/// Repère affiché (état 4 d'AgcmTargeting).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Marker {
    /// Largeur et hauteur (`area_r` du niveau).
    pub size: f32,
    pub texture: &'static str,
    pub at: Vec3,
}

#[derive(Debug, Default)]
pub struct GroundCastGlue {
    /// Compétence gardée (`AgcmSkill+0x1afc`).
    pub pending: Option<i32>,
    /// Repère au sol.
    pub marker: Option<Marker>,
    /// Dernière position retenue (`AgcmSkill+0x1948`).
    pub last_pos: [f32; 3],
}

/// Texture du repère (`FUN_1404e5280`, `targeting.ini` `[Textures2]`).
pub fn selection_texture(size: f32, archlord: bool) -> &'static str {
    let rank = if size < 400.0 {
        0
    } else if size < 700.0 {
        1
    } else {
        2
    };
    // Noms sans `.tif` : les textures du client se cherchent sans lui (`Assets::texture`).
    match (archlord, rank) {
        (true, 0) => "ET_Alkadia_128",
        (true, 1) => "ET_Alkadia_256",
        (true, _) => "ET_Alkadia_512",
        (false, 0) => "ET_Selection_128",
        (false, 1) => "ET_Selection_256",
        (false, _) => "ET_Selection_512",
    }
}

/// Texture de repère de zone (pour la retirer des lots de l'image).
pub fn is_selection_texture(name: &str) -> bool {
    name.starts_with("ET_Selection_") || name.starts_with("ET_Alkadia_")
}

impl App {
    /// Première demande d'une compétence au sol : garde la compétence et pose le repère sous le joueur.
    /// Largeur nulle : pas de repère (`FUN_1404e5280` refuse), la compétence reste gardée comme dans Archonia.
    pub(super) fn groundcast_begin(&mut self, tid: i32, size: f32) {
        self.ground_cast.pending = Some(tid);
        let Some(at) = self.player.as_ref().map(|p| p.position) else { return };
        if size > 0.0 && self.ground_cast.marker.is_none() {
            let texture = selection_texture(size, self.archlord_is_me());
            self.ground_cast.marker = Some(Marker { size, texture, at });
        }
    }

    /// Repère affiché : le clic gauche enfoncé ne fait alors rien d'autre (`FUN_1404e35d0`).
    pub(super) fn groundcast_active(&self) -> bool {
        self.ground_cast.marker.is_some()
    }

    /// Le repère suit le sol sous le curseur (`FUN_1404e41f0`).
    pub(super) fn groundcast_frame(&mut self) {
        if self.ground_cast.marker.is_none() {
            return;
        }
        if self.phase != super::Phase::InWorld {
            self.ground_cast = GroundCastGlue::default();
            return;
        }
        if let Some(p) = self.cursor_ground_point() {
            if let Some(m) = self.ground_cast.marker.as_mut() {
                m.at = p;
            }
        }
    }

    /// Bouton gauche relâché (`FUN_1404e4080`) : sur le sol, le repère est retiré et la compétence gardée part.
    pub(super) fn groundcast_release(&mut self) {
        if self.ground_cast.marker.is_none() {
            return;
        }
        let Some(p) = self.cursor_ground_point() else { return };
        self.ground_cast.marker = None;
        if let Some(tid) = self.ground_cast.pending {
            self.skills_cast_at(tid, Some(p.to_array()));
            self.ground_cast.pending = None;
        }
    }

    /// Échap (`FUN_1404e2110`) : repère retiré, compétence oubliée. Rend `true` s'il y en avait un.
    pub(super) fn groundcast_escape(&mut self) -> bool {
        if self.ground_cast.marker.take().is_some() {
            self.ground_cast.pending = None;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_selon_la_largeur_et_l_archlord() {
        assert_eq!(selection_texture(399.0, false), "ET_Selection_128");
        assert_eq!(selection_texture(400.0, false), "ET_Selection_256");
        assert_eq!(selection_texture(699.0, false), "ET_Selection_256");
        assert_eq!(selection_texture(700.0, false), "ET_Selection_512");
        assert_eq!(selection_texture(2000.0, true), "ET_Alkadia_512");
        assert_eq!(selection_texture(100.0, true), "ET_Alkadia_128");
        assert!(is_selection_texture("ET_Alkadia_256") && is_selection_texture("ET_Selection_128") && !is_selection_texture("target.png"));
    }

    /// Avec les données du client : `targeting.ini` nomme bien ces textures, et elles existent.
    #[test]
    fn textures_du_client() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = crate::assets::Assets::new(Some(dir.into()), &[]);
        let ini = super::super::npc_glue::client_text_in(&assets, "ini", "targeting.ini");
        for (i, name) in ["ET_Alkadia_128", "ET_Alkadia_256", "ET_Alkadia_512", "ET_Selection_128", "ET_Selection_256", "ET_Selection_512"].iter().enumerate() {
            assert!(ini.contains(&format!("FN={i}:{name}.tif")), "{name}");
            assert!(assets.texture_found(name), "{name}");
        }
    }
}
