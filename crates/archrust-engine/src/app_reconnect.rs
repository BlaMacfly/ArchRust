//! Reconnexion automatique côté moteur. Protocole et tentatives : `archrust_netclient::reconnect` et
//! `WorldClient::recover` ; fenêtre : `UI_ReconnectHold` (`hud_portal.rs`).
//!
//! Comportement d'AgcmReconnect (Archonia, sha256 e9c7609b…) :
//! * connexion au monde perdue, personnage en jeu (FUN_14063c0c0) : `ReconnectingServerShow` ouvre
//!   `UI_ReconnectHold` ; le jeu reste affiché ;
//! * toutes les 5 s, la notice « Attempting to reconnect to the game server... » (AgcmUINotice,
//!   FUN_14063be40 ; phrase écrite en dur dans l'exe), affichée ici avec les messages système ;
//! * reprise acceptée (FUN_14063bd10) ou 60 s écoulées : `ReconnectingServerHide` ferme la fenêtre ;
//!   l'échec mène à la déconnexion (`DisconnectedByServer` : « Failed reconnecting to the server. »).

use super::App;
use std::time::{Duration, Instant};

/// Notice d'AgcmReconnect, en anglais dans Archonia comme ici.
const NOTICE: &str = "Attempting to reconnect to the game server...";
/// Intervalle de la notice (5000 ms).
const NOTICE_EVERY: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct ReconnectGlue {
    /// Reconnexion en cours depuis cet instant.
    since: Option<Instant>,
    /// Fenêtre `UI_ReconnectHold` ouverte (la croix la ferme, les tentatives continuent).
    window: bool,
    last_notice: Option<Instant>,
}

impl ReconnectGlue {
    /// Vrai pendant une reconnexion.
    pub fn active(&self) -> bool {
        self.since.is_some()
    }
}

/// Fenêtre et notice de la reconnexion.
pub fn panel(ctx: &egui::Context, glue: &mut ReconnectGlue, hud: &mut crate::hud::HudState, assets: &crate::assets::Assets) {
    if glue.since.is_none() {
        return;
    }
    if glue.last_notice.is_none_or(|t| t.elapsed() >= NOTICE_EVERY) {
        glue.last_notice = Some(Instant::now());
        hud.push_system(NOTICE);
    }
    if glue.window && crate::hud::draw_reconnect_hold(ctx, hud, assets) {
        glue.window = false;
    }
    ctx.request_repaint_after(Duration::from_millis(500));
}

impl App {
    /// `Reconnecting` / `Reconnected` du fil réseau ; une déconnexion termine la reconnexion.
    pub(super) fn reconnect_event(&mut self, reconnecting: bool) {
        if reconnecting {
            log::warn!("reconnexion au serveur de jeu…");
            self.reconnect = ReconnectGlue { since: Some(Instant::now()), window: true, last_notice: None };
        } else {
            let waited = self.reconnect.since.map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.0);
            log::info!("reconnecté au serveur de jeu après {waited:.1} s");
            self.reconnect = ReconnectGlue::default();
        }
    }

    /// `/coupure` : coupe la connexion au monde comme une panne du réseau, pour essayer la reconnexion
    /// (outil de pilotage, comme `/capture`).
    pub(super) fn reconnect_slash(&mut self, cmd: &str) -> bool {
        if cmd != "/coupure" {
            return false;
        }
        match &self.net {
            Some(net) => {
                net.send(archrust_netclient::Command::CutConnection);
                self.ui.push_chat("[système] connexion au monde coupée (essai de la reconnexion).");
            }
            None => self.ui.push_chat("[système] pas de connexion au monde."),
        }
        true
    }

    /// Déconnexion reçue : si une reconnexion était en cours, elle a échoué (`DisconnectedByServer`).
    pub(super) fn reconnect_failed(&mut self) -> Option<String> {
        if !self.reconnect.active() {
            return None;
        }
        self.reconnect = ReconnectGlue::default();
        let text = super::portal_glue::ui_event_text(&self.assets, "DisconnectedByServer");
        (!text.is_empty()).then_some(text)
    }
}
