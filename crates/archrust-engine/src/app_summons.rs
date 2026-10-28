//! Invocations côté moteur. Protocole et registre : `archrust_netclient::summons` ; fenêtres d'Archonia :
//! `hud_summons.rs`.
//!
//! Comportement relevé dans Archonia (sha256 e9c7609b…) :
//! * l'op 0 n'est retenue que si le maître et la créature sont en vue et que la créature porte l'état spécial de son
//!   type (AGPMCHARACTER champ 17, reçu juste avant) ; limites et emplacements « forcés » : voir `Registry` ;
//! * la créature invoquée s'affiche comme un monstre (vue 0x42, coups 0x41), sous le nom envoyé par le serveur : le
//!   client n'y ajoute pas celui du maître (aucune chaîne ni appel trouvés) ; sa fiche de cible est l'ordinaire ;
//! * cibler une de ses invocations (liste 0, `FUN_140292810`) ouvre `UI_TargetSummonsMenu`
//!   (`TargetUserSummonsMenuOpen`, `FUN_140500020`) ; changer de cible ferme les deux menus ; le bouton
//!   (`ToggleTargetSummonsMenu`, `FUN_1404fa380`) ouvre ou ferme `UI_TargetSummonsMenu2` si la cible porte l'état
//!   0x800 ou 0x1000 ; un bouton de propension ferme le menu et envoie l'op 2 si la cible porte 0x800
//!   (`FUN_1404f9db0`…) ; l'étiquette ne change qu'à la réception de l'op 2 (rappel n° 2, `FUN_1404f9150`) ;
//! * une invocation du joueur est « amie » (contour 1, `FUN_1404bb6b0`) : un clic la cible sans l'attaquer ; celle
//!   d'un autre ne s'attaque que si son maître le peut (règle du serveur, `summons::may_attack` d'ArchCore) ;
//! * un familier (type 8) n'est pas sélectionnable (`FUN_1404e1d40`, `FUN_140825a20`) ;
//! * messages système 10 à 13 et 15 à 17 (`systemmsgformat.xml`) : `TameFailure*`, `FixedFailureMaxCount`,
//!   `SummonsFailureMaxCount`, `SummonsLevelLimitRegion` d'uimessage, couleur 255, 204, 0.
//!
//! Les familiers sortent et rentrent par leur objet (`Type` 1 d'`ItemTemplateAll.ini`, utilisé d'un clic droit comme
//! une potion) : le serveur renvoie le familier quand l'objet resservi le tient déjà.

use super::App;
use crate::assets::Assets;
use crate::hud::{HudState, SummonsUi};
use crate::net::NetLink;
use archrust_netclient::summons::{status, Applied, Registry, SummonsCommand, SummonsEvent};
use archrust_netclient::{Command, WorldEvent};
use std::collections::HashMap;

/// Codes de message système des invocations (`systemmsgformat.xml`).
const SYSTEM_CODES: [i32; 7] = [10, 11, 12, 13, 15, 16, 17];
/// Couleur de ces messages (`ColorR/G/B` 255, 204, 0).
const SYSTEM_COLOR: (u8, u8, u8) = (255, 204, 0);

#[derive(Default)]
pub struct SummonsGlue {
    pub book: Registry,
    /// État spécial connu de chaque personnage (AGPMCHARACTER champ 17).
    status: HashMap<i32, u64>,
    /// Cible pour laquelle les menus sont ouverts, et `UI_TargetSummonsMenu2` ouvert.
    target: Option<i32>,
    menu2: bool,
    /// Code de message système → clé d'uimessage (`systemmsgformat.xml`), lu au premier besoin.
    codes: Option<HashMap<i32, String>>,
}

impl SummonsGlue {
    fn status_of(&self, cid: i32) -> u64 {
        self.status.get(&cid).copied().unwrap_or(0)
    }
}

/// `Code` → `UiMessage` des balises `<Msg>` de `systemmsgformat.xml`.
fn system_codes(xml: &str) -> HashMap<i32, String> {
    let attr = |tag: &str, name: &str| {
        let at = tag.find(&format!("{name}=\""))? + name.len() + 2;
        tag[at..].split('"').next().map(str::to_string)
    };
    xml.split("<Msg ")
        .skip(1)
        .filter_map(|tag| {
            let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
            Some((attr(tag, "Code")?.trim().parse().ok()?, attr(tag, "UiMessage")?))
        })
        .collect()
}

impl App {
    /// Relevé avant `on_world_event` : remise à zéro à chaque connexion, personnages sortis de la vue.
    pub(super) fn summons_observe(&mut self, ev: &WorldEvent) {
        match ev {
            WorldEvent::Connected | WorldEvent::Disconnected(_) => {
                let codes = self.summons.codes.take();
                self.summons = SummonsGlue { codes, ..Default::default() };
            }
            WorldEvent::CharacterRemoved { cid, .. } => {
                // Rappel n° 4 d'AgpmCharacter : l'attache du personnage retiré s'efface.
                self.summons.book.forget(*cid);
                self.summons.status.remove(cid);
            }
            _ => {}
        }
    }

    pub(super) fn on_summons_event(&mut self, ev: SummonsEvent) {
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        if let SummonsEvent::SpecialStatus { cid, status } = ev {
            self.summons.status.insert(cid, status);
            return;
        }
        let known = |cid: i32| cid == me || self.entities.iter().any(|e| e.cid == cid);
        let glue = &mut self.summons;
        let statuses = &glue.status;
        let applied = glue.book.apply(&ev, me, known, |cid| statuses.get(&cid).copied().unwrap_or(0));
        match (&ev, applied) {
            (SummonsEvent::Add(s), Applied::Added) => {
                log::info!("invocation {} (TID {}, type {}) de {} ajoutée", s.cid, s.tid, s.kind, s.owner);
            }
            (SummonsEvent::Add(s), Applied::Refused) => {
                log::warn!("invocation {} de {} refusée (limite, créature inconnue ou état spécial {:#x})", s.cid, s.owner, glue.status_of(s.cid));
            }
            (SummonsEvent::Remove { cid, .. }, Applied::Removed) => {
                // Les bits « invoqué » de la créature s'effacent.
                if let Some(s) = glue.status.get_mut(cid) {
                    *s &= !(status::SUMMONER | status::DONT_MOVE | status::TAME);
                }
                if glue.target == Some(*cid) {
                    glue.menu2 = false;
                }
            }
            (_, Applied::Propensity) => log::debug!("propension d'invocation reçue : {ev:?}"),
            (_, Applied::Limit) => log::debug!("limite d'invocations : {ev:?}"),
            _ => {}
        }
    }

    /// Message système d'invocation (codes de `systemmsgformat.xml`) ; `false` pour un autre code.
    pub(super) fn summons_system_code(&mut self, code: i32) -> bool {
        if !SYSTEM_CODES.contains(&code) {
            return false;
        }
        let assets = self.assets.clone();
        let codes = self.summons.codes.get_or_insert_with(|| system_codes(&super::npc_glue::client_text(&assets, "systemmsgformat.xml")));
        let Some(key) = codes.get(&code).cloned() else { return false };
        let text = crate::hud::ui_message(&mut self.hud, &assets, &key);
        if text.is_empty() {
            return false;
        }
        self.hud.push_system(&text);
        let (r, g, b) = SYSTEM_COLOR;
        self.ui.push_chat_in(0x10, format!("|cff{r:02x}{g:02x}{b:02x}{text}|r"));
        true
    }

    /// Règle d'attaque d'une créature invoquée : `None` si ce n'en est pas une ; jamais la sienne ni un familier ;
    /// celle d'un autre si son maître peut être attaqué.
    pub(super) fn summons_attackable(&self, cid: i32) -> Option<bool> {
        let owner = self.summons.book.owner_of(cid)?;
        let me = self.player.as_ref().map_or(0, |p| p.cid);
        if owner == me || self.summons.book.is_pet(cid) {
            return Some(false);
        }
        Some(self.guild.state.is_enemy(owner) || self.pvp_target(owner))
    }

    /// Personnage sélectionnable (clic, Tab) : pas un familier.
    pub(super) fn summons_selectable(&self, cid: i32) -> bool {
        !self.summons.book.is_pet(cid)
    }
}

/// Menus d'une invocation ciblée (dans la passe egui). `target` : cible du joueur.
pub fn panel(ctx: &egui::Context, glue: &mut SummonsGlue, target: Option<i32>, me: i32, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>) {
    // Nouvelle cible : les menus se ferment (`FUN_140500020`).
    if glue.target != target {
        glue.target = target;
        glue.menu2 = false;
    }
    let Some(cid) = target.filter(|c| glue.book.is_summons_of(me, *c)) else {
        glue.menu2 = false;
        return;
    };
    // Étiquette `SummonsPropensityUD` (`FUN_1404f8b20`).
    let key = match glue.book.entry(me, cid).map(|e| e.propensity) {
        Some(0) => "SummonsPropensityAttack",
        Some(1) => "SummonsPropensityDefense",
        Some(2) => "SummonsPropensityShadow",
        _ => "",
    };
    let label = if key.is_empty() { String::new() } else { crate::hud::ui_message(hud, assets, key) };
    let mut out = Vec::new();
    crate::hud::draw_summons_menu(ctx, hud, assets, &label, glue.menu2, &mut out);
    let st = glue.status_of(cid);
    for r in out {
        match r {
            SummonsUi::Toggle => {
                if st & (status::SUMMONER | status::TAME) != 0 {
                    glue.menu2 = !glue.menu2;
                }
            }
            SummonsUi::Propensity(p) => {
                glue.menu2 = false;
                if st & status::SUMMONER != 0 {
                    if let Some(net) = net {
                        net.send(Command::Summons(SummonsCommand::SetPropensity { cid, propensity: p }));
                    }
                }
            }
            SummonsUi::Close => glue.menu2 = false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_des_messages() {
        let xml = r#"<SystemMsgFormat>
  <Msg Code="15" UiMessage="FixedFailureMaxCount" Type="0" ColorR="255" ColorG="204" ColorB="0">AGPMSYSTEMMESSAGE_CODE_FIXED_FAILURE_MAX_COUNT
  </Msg>
  <Msg Code="16" UiMessage="SummonsFailureMaxCount" Type="0" ColorR="255" ColorG="204" ColorB="0">X
  </Msg>"#;
        let m = system_codes(xml);
        assert_eq!(m.get(&16).map(String::as_str), Some("SummonsFailureMaxCount"));
        assert_eq!(m.get(&15).map(String::as_str), Some("FixedFailureMaxCount"));
        assert_eq!(m.len(), 2);
    }
}
