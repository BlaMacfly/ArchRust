//! Titre d'ArchLord (AgpmArchlord 0x55, AgcmArchlord, AgcmUIArchlord d'Archonia).
//!
//! * **Nom** : l'op 0 mémorise le nom de l'ArchLord (vide ou plus de 48 caractères : ignoré), l'op 1 l'efface.
//!   `AgpmCharacter::IsArchlord` (`FUN_14017f9d0`, rappel `FUN_1402949b0`, `FUN_1402944c0`) compare le nom d'un
//!   personnage à ce nom ([`ArchlordGlue::is`]).
//! * **Annonces** (`FUN_1405e76b0` → `FUN_1405e7a20`) : `sysstr` 630 / 631 au centre de l'écran, sur
//!   `RegionInfo.png`, à 19 % de la hauteur ; étape (op 5, `FUN_1405e7460`) : 642 à 645 au centre **et** en message
//!   système ; gardes (ops 2, 3, 4 : 638-641, 650) et messages (op 6 : 646-649, 656) en message système.
//! * **Événement de fonction 36** : bouton `sysstr` 672 « Archlord » du dialogue (`FUN_1406ca6a0`) → op 7 seule
//!   (aucun objet du client ne porte cette fonction ; pas de fenêtre ArchLord dans Archonia).
//! * **Droits de l'ArchLord** gérés par ArchRust : compétences de `skill_type` 9 réservées à l'ArchLord
//!   (`FUN_1402fa5c0`, voir `app_skills.rs`) ; potions d'usage 12 permises métamorphosé ou à cheval
//!   (`FUN_14053d0b0`, voir `app_ride.rs`). Les effets des compétences 472-476, 478, 522, 523 viennent du serveur
//!   (météo 0x30, monture Alcadia 1295 par 0x4D, gardes royaux en invocations 0x4F).
//!
//! Les exceptions de l'ArchLord sous métamorphose (icônes d'annulation absentes, PNJ, passeports, compétences)
//! sont dans `app_morph.rs`. Archonia ne colore pas le nom de l'ArchLord et n'affiche pas de titre : rien d'ajouté
//! aux panneaux de nom.

use super::App;
use crate::assets::Assets;
use crate::hud::HudState;
use archrust_netclient::archlord::{message_sysstr, step_sysstr, ArchlordEvent, MAX_NAME};
use archrust_netclient::WorldEvent;
use std::time::{Duration, Instant};

/// Durée d'une annonce du centre de l'écran, et de son fondu final.
const ANNOUNCE_TIME: Duration = Duration::from_secs(5);
const FADE: f32 = 1.0;

#[derive(Default)]
pub struct ArchlordGlue {
    /// Nom de l'ArchLord courant (vide : aucun).
    name: String,
    /// Étape de la guerre de l'ArchLord (op 5).
    pub step: i32,
    /// Annonces du centre de l'écran (texte, fin).
    announces: Vec<(String, Instant)>,
}

impl ArchlordGlue {
    /// `IsArchlord` : le personnage `name` est l'ArchLord (comparaison exacte, `FUN_1402944c0`).
    pub fn is(&self, name: &str) -> bool {
        !self.name.is_empty() && self.name == name
    }

    /// Nom reçu par l'op 0 : retenu s'il n'est pas vide et ne dépasse pas 48 caractères.
    pub fn set(&mut self, name: &str) -> bool {
        if name.is_empty() || name.chars().count() > MAX_NAME {
            return false;
        }
        self.name = name.to_string();
        true
    }

    fn announce(&mut self, text: String) {
        if !text.trim().is_empty() {
            // Une seule annonce à la fois dans Archonia : la nouvelle remplace l'ancienne.
            self.announces.clear();
            self.announces.push((text, Instant::now() + ANNOUNCE_TIME));
        }
    }
}

fn sys(assets: &Assets, id: u32) -> String {
    super::npc_glue::sysstr(assets, id).unwrap_or_default()
}

/// Annonce du centre de l'écran (passe egui).
pub fn panel(ctx: &egui::Context, glue: &mut ArchlordGlue, hud: &mut HudState, assets: &Assets) {
    let now = Instant::now();
    glue.announces.retain(|(_, until)| now < *until);
    if let Some((text, until)) = glue.announces.first() {
        let left = until.saturating_duration_since(now).as_secs_f32();
        crate::hud::draw_announce(ctx, hud, assets, text, (left / FADE).min(1.0));
        ctx.request_repaint();
    }
}

impl App {
    /// Le joueur est l'ArchLord.
    pub(super) fn archlord_is_me(&self) -> bool {
        self.player.as_ref().is_some_and(|p| self.archlord.is(&p.name))
    }

    /// Message système (`FUN_1405e7620` …) : à droite de l'écran et dans la discussion.
    fn archlord_system(&mut self, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        self.hud.push_system(text);
        self.addon_chat_received("CHAT_MSG_SYSTEM", text, "");
    }

    /// Paquets d'AgpmArchlord.
    pub(super) fn on_archlord_event(&mut self, ev: ArchlordEvent) {
        let assets = self.assets.clone();
        let fill = super::pvp_glue::fill;
        match ev {
            ArchlordEvent::Set(name) => {
                if self.archlord.set(&name) {
                    log::info!("ArchLord : {name}");
                    self.archlord.announce(fill(&sys(&assets, 630), &[name]));
                }
            }
            ArchlordEvent::Cancel(name) => {
                let name = if name.is_empty() { self.archlord.name.clone() } else { name };
                self.archlord.name.clear();
                log::info!("ArchLord : fin du règne de {name}");
                self.archlord.announce(fill(&sys(&assets, 631), &[name]));
            }
            ArchlordEvent::Guard { add, guard, .. } => {
                let id = match (add, guard.is_empty()) {
                    (true, false) => 638,
                    (true, true) => 639,
                    (false, false) => 640,
                    (false, true) => 641,
                };
                self.archlord_system(&sys(&assets, id));
            }
            ArchlordEvent::Guards { current, max } => {
                let t = fill(&sys(&assets, 650), &[current.to_string(), max.to_string()]);
                self.archlord_system(&t);
            }
            ArchlordEvent::Step(s) => {
                self.archlord.step = s;
                if let Some(id) = step_sysstr(s) {
                    let t = sys(&assets, id);
                    self.archlord.announce(t.clone());
                    self.archlord_system(&t);
                }
            }
            ArchlordEvent::Message(m) => {
                if let Some(id) = message_sysstr(m) {
                    self.archlord_system(&sys(&assets, id));
                }
            }
        }
    }

    /// Droits du joueur ArchLord repris par les autres modules.
    pub(super) fn archlord_frame(&mut self) {
        self.ride.archlord = self.archlord_is_me();
    }

    /// Remises à zéro.
    pub(super) fn archlord_observe(&mut self, ev: &WorldEvent) {
        if matches!(ev, WorldEvent::Disconnected(_) | WorldEvent::Connected) {
            self.archlord = ArchlordGlue::default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nom_de_l_archlord() {
        let mut g = ArchlordGlue::default();
        assert!(!g.is(""));
        assert!(!g.set(""));
        assert!(!g.set(&"x".repeat(49)));
        assert!(g.set("Bob"));
        assert!(g.is("Bob") && !g.is("bob") && !g.is("Bobby"));
        g.announce("[Bob] is the Archlord.".into());
        g.announce("Begin the Archlord Battle".into());
        assert_eq!(g.announces.len(), 1);
        assert_eq!(g.name, "Bob");
    }
}
