//! Retour à la sélection des personnages côté moteur (« Select Character » d'`UI_EXIT`). Protocole et
//! enchaînement réseau : `archrust_netclient::return_login`, `WorldClient::run_until`,
//! `LoginClient::return_to_selection` et `net.rs`.
//!
//! Comportement d'Archonia (sha256 e9c7609b…) :
//! * clic (`EXIT_ReturnLoginCharacterSelect`, FUN_1405a65d0 d'AgcmUILogin) : moins de 5 s depuis la dernière demande
//!   (+0x1970) ou depuis le dernier combat (+0x1968) → `CannotUseByStatus` ; dans une arène ou en combat →
//!   `BattleNotUse` (messages système d'`uimessage.txt`) ; sinon la demande est datée, les fenêtres se ferment
//!   (AgcmUIMain, quêtes, guilde, discussion, objets, carte, dialogue des PNJ, personnage) et l'op 1 part ;
//! * op 2 (refus) : rappel 0 d'AgcmReturnToLogin, qu'aucun module n'écoute — rien d'affiché ;
//! * op 5 (FUN_140686900) : fenêtres fermées, personnages, compétences et objets oubliés, effets de nature
//!   arrêtés, écran d'attente de connexion, puis reconnexion au serveur de connexion ;
//! * op 7 (FUN_1406868b0 → FUN_1405a6910 d'AgcmUILogin) : effets, caméra, nature, éclairage et fenêtres remis
//!   à zéro, scène de sélection rouverte, liste des personnages redemandée.
//!
//! Ici, l'op 5 remet l'état du jeu à zéro comme une déconnexion (mêmes remises à zéro, entités, effets,
//! AddOns quittés) sans couper la liaison ; la scène de sélection existante revient avec la nouvelle liste.
//! « Dernier combat » (+0x1968, horodatage dont l'écriture n'a pas été retrouvée) : ici le dernier coup donné ou
//! reçu par le joueur (0x41) ; « en combat » : mode combat d'AgpdCharacter (+0xb38, FUN_14017aaa0), 10 s après le
//! dernier coup ou la dernière compétence (`app_portal.rs`). L'arène n'est pas suivie par ArchRust.

use super::{App, Phase};
use archrust_netclient::return_login::ReturnLoginEvent;
use archrust_netclient::{Command, WorldEvent};
use std::time::{Duration, Instant};

/// Délai entre deux demandes et après un combat (5000 ms de FUN_1405a65d0).
pub const DELAY: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct ReturnLoginGlue {
    /// Dernière demande envoyée (AgcmUILogin+0x1970).
    last_request: Option<Instant>,
    /// Dernier coup donné ou reçu par le joueur (AgcmUILogin+0x1968).
    last_combat: Option<Instant>,
}

/// Contrôles de FUN_1405a65d0 avant l'envoi : clé du message d'`uimessage.txt` en cas de refus.
pub fn refusal(now: Instant, last_request: Option<Instant>, last_combat: Option<Instant>, in_combat: bool) -> Option<&'static str> {
    let recent = |t: Option<Instant>| t.is_some_and(|t| now < t + DELAY);
    if recent(last_request) || recent(last_combat) {
        Some("CannotUseByStatus")
    } else if in_combat {
        Some("BattleNotUse")
    } else {
        None
    }
}

impl App {
    /// Coups du joueur, pour le délai de 5 s après un combat.
    pub(super) fn return_login_observe(&mut self, ev: &WorldEvent) {
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        if let WorldEvent::CharacterAction { cid, target_cid, .. } = ev {
            if me != 0 && (*cid == me || *target_cid == me) && cid != target_cid {
                self.return_login.last_combat = Some(Instant::now());
            }
        }
    }

    /// « Select Character » d'`UI_EXIT` : contrôles, fenêtres fermées, puis AgpmReturnToLogin op 1.
    pub(super) fn return_login_request(&mut self) {
        let Some(net) = &self.net else { return };
        let glue = &self.return_login;
        if let Some(key) = refusal(Instant::now(), glue.last_request, glue.last_combat, self.portal_combat_mode()) {
            log::info!("retour à la sélection refusé par le client : {key}");
            let assets = self.assets.clone();
            let text = crate::hud::ui_message(&mut self.hud, &assets, key);
            if !text.is_empty() {
                self.hud.push_system(&text);
            }
            return;
        }
        net.send(Command::ReturnToLogin);
        self.return_login.last_request = Some(Instant::now());
        log::info!("retour à la sélection des personnages demandé (0x2F op 1)");
        self.return_login_close_windows();
    }

    /// Réponse du serveur (voir `archrust_netclient::return_login`).
    pub(super) fn return_login_event(&mut self, ev: ReturnLoginEvent) {
        match ev {
            // Archonia n'affiche rien (aucun module n'écoute le rappel de l'op 2).
            ReturnLoginEvent::Refused => log::info!("retour à la sélection refusé par le serveur"),
            ReturnLoginEvent::Leaving => self.return_login_leave(),
        }
    }

    /// Fenêtres du jeu fermées, comme AgcmUIMain et les modules d'interface au clic et à l'op 5.
    fn return_login_close_windows(&mut self) {
        self.hotkey.exit = false;
        // Une fenêtre à la fois pour certains modules : jusqu'à ce qu'il n'y en ait plus (borné).
        for _ in 0..16 {
            let mut closed = self.npc.close_windows();
            closed |= self.product.close_windows();
            closed |= self.buddy.close_windows();
            closed |= self.mail.close_windows();
            closed |= self.auction.close_windows();
            closed |= self.itemset.close_windows();
            closed |= self.help.close_windows();
            if !closed {
                break;
            }
        }
        for open in [
            &mut self.ui.show_inventory,
            &mut self.item_ui.show_character,
            &mut self.skills.show_book,
            &mut self.worldmap.open,
            &mut self.product.show_skills,
            &mut self.ui.show_options,
        ] {
            *open = false;
        }
    }

    /// Op 5 : le personnage a quitté le monde. Remises à zéro d'une déconnexion (observateurs des systèmes,
    /// groupe, guilde, amis, courrier, échange, ventes, entités, effets, AddOns), puis personnage, objets,
    /// améliorations, animations et fenêtres oubliés ; la liaison reste ouverte et l'écran revient à la
    /// sélection des personnages dès que la liste arrive.
    fn return_login_leave(&mut self) {
        log::info!("retour à la sélection des personnages : le personnage quitte le monde");
        let net = self.net.take();
        self.on_world_event(WorldEvent::Disconnected("retour à la sélection des personnages".into()));
        self.net = net;
        self.return_login_close_windows();
        self.player = None;
        self.player_animator = None;
        self.animators.clear();
        self.entities.clear();
        self.items.clear();
        self.hud.buffs.clear();
        self.hud.stats = Default::default();
        self.walking = false;
        self.exec_queue.clear();
        self.exec_at = None;
        self.return_login = ReturnLoginGlue::default();
        // Écran d'attente puis scène de sélection (nouvelle liste des personnages).
        self.ui.characters.clear();
        self.ui.selected_character = None;
        self.ui.creation = None;
        self.leave_login_scene();
        self.choosing_character = true;
        self.phase = Phase::Connecting;
        self.ui.status = "retour à la sélection des personnages…".into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controles_avant_l_envoi() {
        let now = Instant::now();
        let ago = |s: u64| now.checked_sub(Duration::from_secs(s));
        assert_eq!(refusal(now, None, None, false), None);
        assert_eq!(refusal(now, ago(2), None, false), Some("CannotUseByStatus"), "demande récente");
        assert_eq!(refusal(now, None, ago(4), false), Some("CannotUseByStatus"), "combat récent");
        assert_eq!(refusal(now, ago(6), ago(6), false), None, "plus de 5 s");
        assert_eq!(refusal(now, ago(6), None, true), Some("BattleNotUse"), "mode combat");
    }
}
