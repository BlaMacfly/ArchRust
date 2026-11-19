//! Renseignements sur un objet qu'on ne détient pas (AGPMITEM op 31 → op 32), pour les liens d'objet de la
//! discussion (voir `chat_links`).
//!
//! Archonia (sha256 e9c7609b…) : un clic sur un lien de genre 1 (FUN_140511500) cherche l'objet parmi ceux
//! déjà reçus (FUN_1407a5c20) et montre son infobulle (FUN_1407a5e50) ; sinon il envoie l'op 31 genre 8 avec
//! l'IID seul (FUN_1407a5f60 → FUN_140649610). L'op 32 passe au rappel 106 d'AgpmItem (FUN_1401c5fa0) :
//! l'interface des objets garde l'objet et montre son infobulle. ArchCore ne répond pas pour un objet disparu.
//!
//! Choix d'ArchRust : un objet de son propre sac montre son infobulle sans demande (sockets et pierres
//! compris) ; l'infobulle s'ouvre sous le lien et se ferme au clic suivant ou sur Échap. Un objet d'autrui
//! n'a que ce que porte l'op 32 (modèle, quantité, options) : ArchCore n'envoie pas ses sockets.

use super::App;
use crate::hud::HudState;
use archrust_netclient::items::{info_kind, ItemCommand, ItemInfo};
use archrust_netclient::world::ItemRecord;
use archrust_netclient::Command;
use egui::{Color32, Pos2};
use std::collections::HashMap;

#[derive(Default)]
pub struct ItemInfoGlue {
    /// Objets reçus par l'op 32, comme la liste de FUN_1407a5c20.
    known: HashMap<i32, ItemInfo>,
    /// Lien cliqué qui attend l'op 32 : IID et position du lien.
    waiting: Option<(i32, Pos2)>,
    /// Infobulle montrée : position et lignes.
    shown: Option<(Pos2, Vec<(String, Color32)>)>,
}

impl ItemInfoGlue {
    /// Déconnexion : plus rien de connu.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Objet de l'op 32 sous la forme d'un objet de l'inventaire (sans statut, place ni propriétaire).
fn record_of(info: &ItemInfo) -> ItemRecord {
    ItemRecord {
        operation: archrust_netclient::items::op::INFO,
        id: info.iid,
        template_id: Some(info.tid),
        owner_cid: None,
        count: info.count,
        status: None,
        grid: None,
    }
}

/// Infobulle ouverte par un lien : fermée au clic suivant (hors du lien qui la rouvre) ou sur Échap.
pub fn panel(ctx: &egui::Context, glue: &mut ItemInfoGlue, _hud: &mut HudState) {
    if glue.shown.is_some() && ctx.input(|i| i.pointer.any_pressed() || i.key_pressed(egui::Key::Escape)) {
        glue.shown = None;
    }
    if let Some((at, lines)) = &glue.shown {
        crate::hud::item_tooltip_at(ctx, *at, lines, "");
    }
}

impl App {
    /// Clic sur un lien d'objet de la discussion (FUN_140511500, genre 1).
    pub(super) fn on_item_link(&mut self, iid: i32, at: Pos2) {
        if self.items.contains_key(&iid) || self.item_info.known.contains_key(&iid) {
            self.show_item_info(iid, at);
            return;
        }
        if let Some(net) = &self.net {
            self.item_info.waiting = Some((iid, at));
            net.send(Command::Item(ItemCommand::RequestInfo { iid, tid: 0, kind: info_kind::CHAT_LINK }));
        }
    }

    /// AGPMITEM op 32 : objet gardé (options comprises) et infobulle du lien qui l'attendait.
    pub(super) fn on_item_info(&mut self, info: ItemInfo) {
        // Genre 4 (modèle seul) : ArchRust n'a pas de bouton d'objet qui le demande.
        if info.iid == 0 {
            return;
        }
        let iid = info.iid;
        self.convert.set_options(iid, info.options.clone());
        self.item_info.known.insert(iid, info);
        if let Some((_, at)) = self.item_info.waiting.take_if(|(w, _)| *w == iid) {
            self.show_item_info(iid, at);
        }
    }

    fn show_item_info(&mut self, iid: i32, at: Pos2) {
        let lines = match (self.hud.item_tips.get(&iid), self.item_info.known.get(&iid)) {
            (Some(tip), _) if self.items.contains_key(&iid) => tip.clone(),
            (_, Some(info)) => {
                let record = record_of(info);
                self.item_tooltip_lines(&record)
            }
            (Some(tip), None) => tip.clone(),
            (None, None) => return,
        };
        self.item_info.shown = Some((at, lines));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objet_de_l_op_32() {
        let info = ItemInfo { iid: 77, tid: 1234, count: Some(3), options: vec![12], kind: info_kind::CHAT_LINK };
        let r = record_of(&info);
        assert_eq!((r.id, r.template_id, r.count, r.status, r.grid), (77, Some(1234), Some(3), None, None));
    }
}
