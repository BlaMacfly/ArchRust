//! Boutique Cash d'Archonia, d'après `UI_1024X768.ini` (module AgcmUICashMall) :
//!
//! * `UI_CashMall` (1024 × 652, `CashMall_Base.png`) : croix `CONTROL_30581` (`CashMallOpenUI`) ; douze onglets
//!   `CONTROL_24674`… (77 × 37, `CtrlUserData2=n:0:CashMallCurrentTab`, nom de l'onglet `CashMallDisplayTabName`) ;
//!   grille `CONTROL_14249` (960 × 489, `List=3:0:4:0` : 3 lignes × 4 colonnes, cases de 240 × 163 = taille / nombre)
//!   dont la case `CONTROL_30863` (`cashmall_item_box.png`) porte l'icône `CONTROL_17524` (13, 40), le nom
//!   `CONTROL_3875` (nom du modèle, 21 caractères puis « ... » au-delà de 24, `FUN_1407185a0`), « - Count: » et le
//!   nombre `CONTROL_COUNT_VALUE`, « - Price: » et le prix `CONTROL_9440`, le bouton « Buy » `CONTROL_6866`
//!   (`CashMallBuyProductConfirm`) ; badges `CONTROL_27927` en (96, 88), même pas de grille : base
//!   `CONTROL_StatusBase` selon le genre du produit (`Status2=n:…:texture`) ; pages `CONTROL_19538` / `CONTROL_11465`
//!   (flèches), `CONTROL_Page_1` à `_11`, page maxi `CONTROL_17215` ; solde `CONTROL_Cash` « KeepCash : %lld »
//!   (`FUN_140719f70`).
//! * `UI_CashMallBuy` (408 × 217) : produit `CONTROL_21305` (`CashMallDetailList`), prix `CONTROL_19668`, Cash utilisé
//!   `CONTROL_4512` (rouge quand il manque, `CashMallNotEnoughMall`), reste `CONTROL_24128`, « Purchase »
//!   `CONTROL_26217` (`CashMallBuyProduct`), « Cancel » `CONTROL_18323` et la croix `CONTROL_6519`.
//!
//! Écartés (ajouts d'Archonia) : « Buy Credits » `CONTROL_31140` (`CashMallCashCharge`, achat en argent réel),
//! recherche `CTL_CASHMALL_SEARCH`, offre du jour `CONTROL_DailyDealList`, soldes `CONTROL_Cash2` / `CONTROL_Cash3`.

use super::items_ui::icon;
use super::npc_ui::{button, frame, window};
use super::portal_ui::{control_text, state_button};
use super::*;

/// Fenêtres de la boutique, chargées avec le HUD (libellés du dossier de langue).
pub(super) const CASHMALL_WINDOWS: [&str; 2] = ["UI_CashMall", "UI_CashMallBuy"];
/// Produits par page (`CashMallListItemGrid`, 0xc éléments) et disposition de la grille.
pub const PER_PAGE: usize = 12;
const COLUMNS: usize = 4;
const ROWS: usize = 3;
/// Boutons de page (`CONTROL_Page_1` à `_11`).
pub const PAGE_BUTTONS: usize = 11;
/// Onglets (`CONTROL_24674`… , `CashMallCurrentTab` 0 à 11).
const TAB_BUTTONS: [&str; 12] = ["CONTROL_24674", "CONTROL_26144", "CONTROL_19039", "CONTROL_16853", "CONTROL_21554", "CONTROL_28856", "CONTROL_21567", "CONTROL_7709", "CONTROL_11830", "CONTROL_14065", "CONTROL_2133", "CONTROL_4901"];

/// Une case de la grille.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CashCell {
    pub product: i32,
    pub tid: i32,
    pub name: String,
    pub count: i32,
    pub price: i64,
    /// Genre (statut de `CONTROL_StatusBase` : 0 normal… 6 TPack).
    pub kind: i32,
}

/// Achat en attente de confirmation (`UI_CashMallBuy`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CashBuyView {
    pub detail: String,
    pub price: i64,
    pub cash: i64,
}

/// Ce que montre `UI_CashMall`.
pub struct CashMallView<'a> {
    pub tabs: &'a [String],
    pub tab: usize,
    /// Page courante (0) et nombre de pages.
    pub page: usize,
    pub pages: usize,
    pub cells: &'a [CashCell],
    pub cash: i64,
    /// Libellé du solde (`KeepCash` d'uimessage).
    pub cash_label: &'a str,
    pub buy: Option<&'a CashBuyView>,
}

/// Demandes des fenêtres de la boutique.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CashMallUi {
    Close,
    Tab(usize),
    Page(usize),
    /// Bouton « Buy » d'une case : `UI_CashMallBuy` sur ce produit.
    Choose(i32),
    Purchase,
    CancelBuy,
}

/// Nom d'un produit (`FUN_1407185a0` : au-delà de 24 caractères, les 21 premiers et « ... »).
pub fn product_name(name: &str) -> String {
    if name.chars().count() > 24 {
        format!("{}...", name.chars().take(21).collect::<String>())
    } else {
        name.to_string()
    }
}

/// Coin d'une case de la grille `list` (taille / colonnes, taille / lignes), rang `k` de la page.
pub fn cell_origin(list: &UiControl, k: usize) -> [f32; 2] {
    let (w, h) = (list.size[0] / COLUMNS as f32, list.size[1] / ROWS as f32);
    [list.pos[0] + (k % COLUMNS) as f32 * w, list.pos[1] + (k / COLUMNS) as f32 * h]
}

/// Fenêtre de la boutique et, au-dessus, la confirmation d'achat.
pub fn draw_cash_mall(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: CashMallView, out: &mut Vec<CashMallUi>) {
    let Some(w) = window(hud, assets, "UI_CashMall") else { return };
    frame(ctx, hud, assets, &w, "archonia-cash-mall", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let at = |c: &UiControl, base: [f32; 2]| layout.rect(origin, [base[0] + c.pos[0], base[1] + c.pos[1]], c.size);
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match c.name.as_str() {
                "CONTROL_30581" => {
                    if state_button(ctx, ui, hud, assets, painter, r, c, "", font, true, "cashmall-x").clicked() {
                        out.push(CashMallUi::Close);
                    }
                }
                name if TAB_BUTTONS.contains(&name) => {
                    let i = TAB_BUTTONS.iter().position(|n| *n == name).unwrap_or(0);
                    let Some(label) = view.tabs.get(i) else { continue };
                    if i == view.tab {
                        painter.rect_stroke(r.shrink(2.0 * s), 3.0, egui::Stroke::new(1.5 * s, Color32::from_rgb(230, 190, 90)), egui::StrokeKind::Inside);
                    }
                    if button(ctx, ui, hud, assets, painter, r, c, label, font, true, ("cashmall-tab", i)).clicked() {
                        out.push(CashMallUi::Tab(i));
                    }
                }
                "CONTROL_14249" => {
                    let (cellc, iconc, buyc, namec) = (w.control("CONTROL_30863"), w.control("CONTROL_17524"), w.control("CONTROL_6866"), w.control("CONTROL_3875"));
                    let labels = [w.control("CONTROL_PRICE_LABEL"), w.control("CONTROL_PRICE_LABEL_NEW")];
                    let values = [w.control("CONTROL_COUNT_VALUE"), w.control("CONTROL_9440")];
                    let badges = w.control("CONTROL_27927");
                    for (k, cell) in view.cells.iter().take(PER_PAGE).enumerate() {
                        let base = cell_origin(c, k);
                        if let Some(bc) = cellc {
                            if let Some((_, tex)) = bc.textures.first() {
                                control_image(ctx, hud, assets, painter, at(bc, base), bc.size, tex, 1.0);
                            }
                        }
                        if let Some(ic) = iconc {
                            icon(ctx, hud, assets, painter, at(ic, base), Some(cell.tid), Color32::WHITE);
                        }
                        if let Some(nc) = namec {
                            control_text(painter, at(nc, base), nc, &product_name(&cell.name), font, None);
                        }
                        for lc in labels.iter().flatten() {
                            control_text(painter, at(lc, base), lc, lc.fixed_label(), font, None);
                        }
                        for (vc, text) in values.iter().zip([cell.count.max(1).to_string(), cell.price.to_string()]) {
                            if let Some(vc) = vc {
                                control_text(painter, at(vc, base), vc, &text, font, None);
                            }
                        }
                        // Badge du genre : texture de `Status2=genre:…:texture` (0 : aucune).
                        if let (Some(bl), Some(sb)) = (badges, w.control("CONTROL_StatusBase")) {
                            let tex = [0u32, 2, 1, 3, 4, 5, 6].get(cell.kind.max(0) as usize).copied().unwrap_or(0);
                            if let Some((_, t)) = sb.textures.iter().find(|(i, _)| tex != 0 && *i == tex) {
                                let bb = cell_origin(bl, k);
                                control_image(ctx, hud, assets, painter, at(sb, bb), sb.size, t, 1.0);
                            }
                        }
                        if let Some(bc) = buyc {
                            if button(ctx, ui, hud, assets, painter, at(bc, base), bc, bc.fixed_label(), font, view.buy.is_none(), ("cashmall-buy", k)).clicked() {
                                out.push(CashMallUi::Choose(cell.product));
                            }
                        }
                    }
                }
                "CONTROL_19538" | "CONTROL_11465" => {
                    let next = c.name == "CONTROL_11465";
                    let enabled = if next { view.page + 1 < view.pages } else { view.page > 0 };
                    if state_button(ctx, ui, hud, assets, painter, r, c, "", font, enabled, ("cashmall-page", next)).clicked() {
                        out.push(CashMallUi::Page(if next { view.page + 1 } else { view.page - 1 }));
                    }
                }
                name if name.starts_with("CONTROL_Page_") => {
                    let Some(n) = name.trim_start_matches("CONTROL_Page_").parse::<usize>().ok().filter(|n| (1..=PAGE_BUTTONS).contains(n)) else { continue };
                    if n > view.pages {
                        continue;
                    }
                    let color = if n == view.page + 1 { Color32::from_rgb(255, 210, 90) } else { Color32::WHITE };
                    let resp = ui.interact(r, egui::Id::new(("cashmall-goto", n)), egui::Sense::click());
                    shadow_text(painter, r.center(), Align2::CENTER_CENTER, &n.to_string(), font, color);
                    if resp.clicked() {
                        out.push(CashMallUi::Page(n - 1));
                    }
                }
                "CONTROL_17215" => control_text(painter, r, c, &format!("/ {}", view.pages.max(1)), font, None),
                "CONTROL_Cash" => {
                    control_text(painter, r, c, &format!("{} : {}", view.cash_label, view.cash), font, None);
                }
                _ => {}
            }
        }
    });
    if let Some(buy) = view.buy {
        draw_buy(ctx, hud, assets, buy, out);
    }
}

/// `UI_CashMallBuy`.
fn draw_buy(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, buy: &CashBuyView, out: &mut Vec<CashMallUi>) {
    let Some(w) = window(hud, assets, "UI_CashMallBuy") else { return };
    // Confirmation au-dessus de la boutique (même ordre d'affichage).
    ctx.move_to_top(egui::LayerId::new(egui::Order::Middle, egui::Id::new("archonia-cash-mall-buy")));
    frame(ctx, hud, assets, &w, "archonia-cash-mall-buy", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let short = buy.cash < buy.price;
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match c.name.as_str() {
                "CONTROL_6519" | "CONTROL_18323" => {
                    if state_button(ctx, ui, hud, assets, painter, r, c, c.fixed_label(), font, true, ("cashmall-cancel", c.name.as_str())).clicked() {
                        out.push(CashMallUi::CancelBuy);
                    }
                }
                "CONTROL_26217" => {
                    if state_button(ctx, ui, hud, assets, painter, r, c, c.fixed_label(), font, true, "cashmall-purchase").clicked() {
                        out.push(CashMallUi::Purchase);
                    }
                }
                "CONTROL_21305" => control_text(painter, r, c, &buy.detail, font, None),
                "CONTROL_19668" => control_text(painter, r, c, &buy.price.to_string(), font, None),
                "CONTROL_4512" => control_text(painter, r, c, &buy.price.to_string(), font, short.then_some(Color32::from_rgb(255, 80, 80))),
                "CONTROL_24128" => control_text(painter, r, c, &(buy.cash - buy.price).to_string(), font, None),
                _ if c.kind == 1 => control_text(painter, r, c, c.fixed_label(), font, None),
                _ => {}
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms_et_cases() {
        assert_eq!(product_name("Return Orb"), "Return Orb");
        assert_eq!(product_name("Extra-Large Light Speed Potion"), "Extra-Large Light Spe...");
        let list = UiControl { name: "CONTROL_14249".into(), kind: 10, pos: [40.0, 92.0], size: [960.0, 489.0], ..Default::default() };
        assert_eq!(cell_origin(&list, 0), [40.0, 92.0]);
        assert_eq!(cell_origin(&list, 5), [280.0, 255.0]);
        assert_eq!(cell_origin(&list, 11), [760.0, 418.0]);
    }
}
