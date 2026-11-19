//! Boutique Cash côté moteur. Protocole : `archrust_netclient::cash_mall` ; fenêtres d'Archonia : `hud_cashmall.rs`.
//!
//! Comportement relevé dans Archonia (AgcmUICashMall, sha256 e9c7609b…) :
//! * ouverture (`FUN_140718f40`, entrée `MenuBtnRoniMall` du menu Chantra) : op 7 (version connue) puis op 1 de
//!   l'onglet courant ; la croix referme ;
//! * op 2 : onglets et version gardés ; op 3 : produits de l'onglet, table remplacée (`FUN_1407194f0`), grille
//!   redessinée si l'onglet est le courant ;
//! * onglet choisi : op 1 ; douze produits par page dans l'ordre de l'onglet ;
//! * « Buy » d'une case : `UI_CashMallBuy` (prix, Cash utilisé, reste) ; « Purchase » (`FUN_1407176f0`) : refus local
//!   si mort (`NotCommitOnDead`), sinon op 4 avec la version ;
//! * op 5 (`FUN_1407191b0`) : message de l'événement `CashMallBuyResult*` (`{{…}}` : nom de l'objet) ; code 4 : op 1
//!   de l'onglet courant ;
//! * solde : Cash du personnage (champ 10 d'AGPMCHARACTER), « KeepCash : %lld » (`FUN_140719f70`).

use super::App;
use crate::assets::Assets;
use crate::hud::{CashBuyView, CashCell, CashMallUi, CashMallView, HudState, CASHMALL_PER_PAGE};
use crate::net::NetLink;
use crate::scene::item_name_for_tid;
use archrust_netclient::cash_mall::{code, CashMallCommand, CashMallEvent, MallProduct, MallTab};
use archrust_netclient::Command;
use std::collections::HashMap;

#[derive(Default)]
pub struct CashMallGlue {
    pub open: bool,
    version: u8,
    tabs: Vec<MallTab>,
    tab: usize,
    page: usize,
    /// Produits du dernier op 3, par identifiant.
    products: HashMap<i32, MallProduct>,
    /// Achat en attente de confirmation : produit et fenêtre.
    buy: Option<(i32, CashBuyView)>,
    /// Produit du dernier achat envoyé, pour le message de réussite.
    bought: String,
    /// Boîte d'information (résultat d'un achat, refus).
    alert: Option<String>,
    keep_cash: Option<String>,
}

fn send(net: Option<&NetLink>, cmd: CashMallCommand) {
    if let Some(net) = net {
        net.send(Command::CashMall(cmd));
    }
}

/// `{{…}}` remplacé par `value` (messages de l'`[EventMap]`).
fn fill_braces(text: &str, value: &str) -> String {
    match (text.find("{{"), text.find("}}")) {
        (Some(a), Some(b)) if b > a => format!("{}{}{}", &text[..a], value, &text[b + 2..]),
        _ => text.to_string(),
    }
}

/// Produits d'une page de l'onglet, dans l'ordre de l'onglet (op 2), parmi ceux reçus (op 3).
pub fn page_products<'a>(tab: Option<&MallTab>, products: &'a HashMap<i32, MallProduct>, page: usize) -> Vec<&'a MallProduct> {
    let Some(tab) = tab else { return Vec::new() };
    tab.products.iter().filter_map(|id| products.get(id)).skip(page * CASHMALL_PER_PAGE).take(CASHMALL_PER_PAGE).collect()
}

/// Pages d'un onglet (au moins une).
pub fn page_count(tab: Option<&MallTab>, products: &HashMap<i32, MallProduct>) -> usize {
    let n = tab.map_or(0, |t| t.products.iter().filter(|id| products.contains_key(id)).count());
    n.div_ceil(CASHMALL_PER_PAGE).max(1)
}

impl CashMallGlue {
    /// Ouvre la boutique (op 7, op 1) ou la ferme.
    pub fn toggle(&mut self, net: Option<&NetLink>) {
        if self.open {
            self.open = false;
            self.buy = None;
            return;
        }
        self.open = true;
        send(net, CashMallCommand::Open { version: self.version, tab: self.tab as i32 });
    }
}

/// `UI_CashMall`, `UI_CashMallBuy` et la boîte d'information.
pub fn panel(ctx: &egui::Context, glue: &mut CashMallGlue, hud: &mut HudState, assets: &Assets, net: Option<&NetLink>, dead: bool) {
    if glue.open {
        let tab = glue.tabs.get(glue.tab);
        let pages = page_count(tab, &glue.products);
        glue.page = glue.page.min(pages - 1);
        let cells: Vec<CashCell> = page_products(tab, &glue.products, glue.page)
            .into_iter()
            .map(|p| CashCell { product: p.id, tid: p.tid, name: item_name_for_tid(assets, p.tid).unwrap_or_else(|| p.description.clone()), count: p.count, price: p.price, kind: p.kind })
            .collect();
        let names: Vec<String> = glue.tabs.iter().map(|t| t.name.clone()).collect();
        let cash = hud.stats.cash.unwrap_or(0);
        let label = glue.keep_cash.get_or_insert_with(|| super::npc_glue::client_text(assets, "uimessage.txt").lines().filter_map(|l| l.split_once('\t')).find(|(k, _)| k.trim() == "KeepCash").map(|(_, v)| v.trim().to_string()).unwrap_or_else(|| "Cash".into())).clone();
        let mut out = Vec::new();
        crate::hud::draw_cash_mall(ctx, hud, assets, CashMallView { tabs: &names, tab: glue.tab, page: glue.page, pages, cells: &cells, cash, cash_label: &label, buy: glue.buy.as_ref().map(|(_, b)| b) }, &mut out);
        for r in out {
            match r {
                CashMallUi::Close => glue.toggle(net),
                CashMallUi::Tab(i) if i < glue.tabs.len() => {
                    glue.tab = i;
                    glue.page = 0;
                    send(net, CashMallCommand::Products(i as i32));
                }
                CashMallUi::Tab(_) => {}
                CashMallUi::Page(p) => glue.page = p.min(pages - 1),
                CashMallUi::Choose(id) => {
                    if let Some(p) = glue.products.get(&id) {
                        let name = item_name_for_tid(assets, p.tid).unwrap_or_else(|| p.description.clone());
                        glue.buy = Some((id, CashBuyView { detail: format!("{name} × {}", p.count.max(1)), price: p.price, cash }));
                    }
                }
                CashMallUi::Purchase => {
                    if let Some((id, view)) = glue.buy.take() {
                        glue.bought = view.detail;
                        if dead {
                            glue.alert = Some(super::portal_glue::ui_event_text(assets, "NotCommitOnDead")).filter(|t| !t.is_empty());
                        } else {
                            log::info!("boutique : achat du produit {id} (version {})", glue.version);
                            send(net, CashMallCommand::Buy { product: id, version: glue.version });
                        }
                    }
                }
                CashMallUi::CancelBuy => glue.buy = None,
            }
        }
    }
    if let Some(text) = glue.alert.clone() {
        if crate::hud::info_box(ctx, hud, assets, &text) {
            glue.alert = None;
        }
    }
}

impl App {
    /// Réponses du serveur.
    pub(super) fn on_cash_mall_event(&mut self, ev: CashMallEvent) {
        let assets = self.assets.clone();
        let glue = &mut self.cash_mall;
        match ev {
            CashMallEvent::Tabs { version, tabs } => {
                log::info!("boutique : {} onglet(s), version {version}", tabs.len());
                glue.version = version;
                glue.tabs = tabs;
                glue.tab = glue.tab.min(glue.tabs.len().saturating_sub(1));
            }
            CashMallEvent::Products { version, tab, products } => {
                glue.version = version;
                glue.products = products.into_iter().map(|p| (p.id, p)).collect();
                if tab as usize != glue.tab {
                    log::info!("boutique : produits de l'onglet {tab} reçus, onglet courant {}", glue.tab);
                }
            }
            CashMallEvent::BuyResult(c) => {
                let item = glue.bought.clone();
                let text = code::event(c).map(|e| super::portal_glue::ui_event_text(&assets, e)).unwrap_or_default();
                log::info!("boutique : résultat de l'achat {c}");
                glue.alert = (!text.is_empty()).then(|| fill_braces(&text, &item));
                if c == code::NEED_NEW_ITEM_LIST {
                    send(self.net.as_ref(), CashMallCommand::Products(glue.tab as i32));
                }
            }
        }
    }

    /// Commandes (essais sans souris, `--exec`) : `/boutiquecash` ou `/cashmall` (ouvre ou ferme),
    /// `/boutiquecash onglet <n>`, `/boutiquecash acheter <produit>`. `/boutique` reste celle des PNJ (`app_npc.rs`).
    pub(super) fn cash_mall_slash(&mut self, cmd: &str, arg: &str) -> bool {
        if !matches!(cmd, "/boutiquecash" | "/cashmall") {
            return false;
        }
        let mut words = arg.split_whitespace();
        match (words.next(), words.next().and_then(|v| v.parse::<i32>().ok())) {
            (Some("onglet"), Some(n)) => {
                self.cash_mall.tab = n.max(0) as usize;
                self.cash_mall.page = 0;
                send(self.net.as_ref(), CashMallCommand::Products(n));
            }
            (Some("acheter"), Some(id)) => send(self.net.as_ref(), CashMallCommand::Buy { product: id, version: self.cash_mall.version }),
            _ => self.cash_mall.toggle(self.net.as_ref()),
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_de_la_grille() {
        let tab = MallTab { name: "Bundles".into(), products: (1..=30).collect() };
        let products: HashMap<i32, MallProduct> = (1..=30).filter(|i| *i != 5).map(|id| (id, MallProduct { id, tid: 100 + id, ..Default::default() })).collect();
        assert_eq!(page_count(Some(&tab), &products), 3);
        let first = page_products(Some(&tab), &products, 0);
        assert_eq!(first.len(), 12);
        assert_eq!((first[3].id, first[4].id), (4, 6), "produit absent de l'op 3 sauté");
        assert_eq!(page_products(Some(&tab), &products, 2).len(), 5);
        assert_eq!(page_count(None, &products), 1);
        assert_eq!(fill_braces("Item purchase is complete. {{CashMallBuyProduct.CashMallBuyMessage}}", "Orb × 5"), "Item purchase is complete. Orb × 5");
    }
}
