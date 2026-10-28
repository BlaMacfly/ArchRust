//! Fenêtres de la maison des ventes d'Archonia (AgcmUIAuction, AgcmUIAuctionSale, boîte de ventes
//! d'AgcmUIItem), d'après `UI_1024X768.ini` :
//!
//! * `UI_Auction` (1197 × 465, `ac_base_version2.png`) : onglets de tri `CONTROL_AC_*Tab`, liste des
//!   catégories `CONTROL_AC_Tree` (20 lignes de 19 px : `CONTROL_AC_Tree_Plus_Button` / `_Minus_Button`,
//!   barre `AC_CategoryBar.png`, bouton `CONTROL_4940`, défilement `CONTROL_19102`), recherche
//!   `CONTROL_AC_SearchTID`, planche `CONTROL_AC_BoardList` (15 lignes de 19 px, colonnes
//!   `CONTROL_AC_*Button`, barre `AC_BoardBar.png` au survol), objet choisi `CONTROL_AC_BoardItemTemplateGrid`,
//!   nom `CONTROL_AC_TIDNameStatic`, pages `CONTROL_AC_Page1…10` et flèches, `CONTROL_3992` (page) ;
//! * `UI_SalesBox2` (348 × 300, `SalesBoard_Base.png`) : 4 lignes de 50 px (`CONTROL_SalesBox2_List_Data` :
//!   case, nom, prix or, CC, bouton `SalesBoxOkCancel_*.png` « Cancel » / « Confirm »), défilement,
//!   compteurs « For Sale » / « Sold » ;
//! * `UI_AuctionCreateSale` (485 × 179) : prix or et CC, frais, case du parchemin Marvel, « Confirm » / « Cancel ».
//!
//! Les demandes reviennent au moteur en [`AuctionUi`] (voir `app_auction.rs`).

use super::items_ui::icon as item_icon;
use super::npc_ui::{frame, window};
use super::portal_ui::{control_text, state_button, static_text};
use super::*;

/// Fenêtres de la maison des ventes, chargées avec le HUD.
pub(super) const AUCTION_WINDOWS: [&str; 3] = ["UI_Auction", "UI_SalesBox2", "UI_AuctionCreateSale"];
/// Lignes visibles de la planche, de la liste des catégories et de la boîte de ventes.
pub const AUCTION_BOARD_ROWS: usize = 15;
pub const AUCTION_TREE_ROWS: usize = 20;
pub const SALES_BOX_ROWS: usize = 4;
const BOARD_ROW_H: f32 = 19.0;
const TREE_ROW_H: f32 = 19.0;
const SALES_ROW_H: f32 = 50.0;

/// Colonnes de tri de la planche (`AC_SortBy*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuctionSort {
    Name,
    Gold,
    GoldPerUnit,
    Cc,
    CcPerUnit,
    Count,
    Seller,
}

/// Ligne de la liste des catégories : libellé, ouvrable (catégorie), choisie.
pub struct TreeLine {
    pub label: String,
    pub branch: bool,
    pub selected: bool,
}

/// Ligne de la planche, textes déjà mis en forme.
pub struct BoardLine {
    pub name: String,
    pub gold: String,
    pub gold_each: String,
    pub cc: String,
    pub cc_each: String,
    pub count: String,
    pub seller: String,
}

pub struct AuctionView<'a> {
    /// Nom de la catégorie ou de l'objet choisi (`Auction_BoardCategoryName`).
    pub title: &'a str,
    /// TID de l'objet choisi (case `CONTROL_AC_BoardItemTemplateGrid`).
    pub item_tid: Option<i32>,
    pub tree: &'a [TreeLine],
    pub tree_first: usize,
    /// Niveau affiché (0 catégories, 1 sous-catégories, 2 objets) : bouton « − » au-delà de 0.
    pub level: u8,
    /// Lignes de la page affichée.
    pub board: &'a [BoardLine],
    pub search: &'a mut String,
    /// Page courante (0…), nombre de pages.
    pub page: usize,
    pub pages: usize,
}

/// Demandes de `UI_Auction`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuctionUi {
    Close,
    Sort(AuctionSort),
    /// Ligne de la liste des catégories (indice absolu) : `AC_SelectCategory`.
    Tree(usize),
    /// `AC_BackToParent`.
    TreeBack,
    TreeScroll(i32),
    /// Entrée dans la recherche : complétion et saut dans la liste.
    Search,
    /// Ligne de la page (`AC_BuyItem`).
    Buy(usize),
    Prev,
    Next,
    Page(usize),
}

/// Vente de la boîte, textes déjà mis en forme.
pub struct SalesLine {
    pub tid: i32,
    pub count: i32,
    pub name: String,
    pub gold: String,
    pub cc: String,
    /// Libellé du bouton (366 « Cancel » en vente, 365 « Confirm » vendue).
    pub button: String,
}

pub struct SalesBoxView<'a> {
    pub rows: &'a [SalesLine],
    pub first: usize,
    /// « For Sale: n », « Sold: n ».
    pub on_sale: &'a str,
    pub sold: &'a str,
}

/// Demandes de `UI_SalesBox2`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SalesBoxUi {
    Close,
    Scroll(i32),
    /// Bouton de la ligne : annuler (en vente) ou encaisser (vendue).
    Button(usize),
    /// Objet du sac lâché sur la liste (`AC_MoveToSalesBox`).
    Drop(ItemDrag),
}

pub struct CreateSaleView<'a> {
    pub gold: &'a mut String,
    pub cc: &'a mut String,
    /// Frais affichés (« Gold fee:___%s », « CC fee:___%s »).
    pub fee_gold: &'a str,
    pub fee_cc: &'a str,
    /// Parchemin Marvel posé (TID).
    pub marvel: Option<i32>,
}

/// Demandes de `UI_AuctionCreateSale`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateSaleUi {
    Confirm,
    Cancel,
    Marvel(ItemDrag),
    RemoveMarvel,
}

/// Saisie d'un nombre (chiffres seulement, `limit` caractères) dans un contrôle d'édition d'Archonia.
fn digits_edit(ui: &mut egui::Ui, r: Rect, id: &str, text: &mut String, limit: usize, font: f32) {
    let edit = egui::TextEdit::singleline(text)
        .id(egui::Id::new(("archonia-auction-edit", id)))
        .frame(false)
        .char_limit(limit)
        .horizontal_align(egui::Align::Center)
        .font(FontId::proportional(font))
        .text_color(Color32::WHITE);
    ui.put(r, edit);
    text.retain(|c| c.is_ascii_digit());
}

/// Planche de la maison des ventes.
pub fn draw_auction(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: AuctionView, out: &mut Vec<AuctionUi>) {
    let Some(w) = window(hud, assets, "UI_Auction") else { return };
    let AuctionView { title, item_tid, tree, tree_first, level, board, search, page, pages } = view;
    frame(ctx, hud, assets, &w, "archonia-auction", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        if let Some(t) = w.control("CONTROL_AC_Tree") {
            if ui.rect_contains_pointer(layout.rect(origin, t.pos, t.size)) {
                let wheel = ui.input(|i| i.raw_scroll_delta.y);
                if wheel.abs() > 0.5 {
                    out.push(AuctionUi::TreeScroll(if wheel > 0.0 { -1 } else { 1 }));
                }
                if ui.input(|i| i.pointer.secondary_clicked()) && level > 0 {
                    out.push(AuctionUi::TreeBack);
                }
            }
        }
        let first_page = page / 10 * 10;
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            let name = c.name.as_str();
            // Lignes de la liste des catégories (enfants de `CONTROL_AC_Tree_Data`).
            if matches!(name, "CONTROL_AC_Tree_Plus_Button" | "CONTROL_AC_Tree_Minus_Button" | "CONTROL_9334" | "CONTROL_4940") {
                for (k, line) in tree.iter().enumerate().skip(tree_first).take(AUCTION_TREE_ROWS) {
                    let rr = layout.rect(origin, [c.pos[0], c.pos[1] + (k - tree_first) as f32 * TREE_ROW_H], c.size);
                    match name {
                        "CONTROL_AC_Tree_Plus_Button" if line.branch => {
                            if state_button(ctx, ui, hud, assets, painter, rr, c, "", font, true, ("ac-tree-plus", k)).clicked() {
                                out.push(AuctionUi::Tree(k));
                            }
                        }
                        // « − » : retour au niveau supérieur (`AC_BackToParent`, sans contrôle dans l'ini).
                        "CONTROL_AC_Tree_Minus_Button" if level > 0 && k == tree_first => {
                            if state_button(ctx, ui, hud, assets, painter, rr, c, "", font, true, ("ac-tree-minus", k)).clicked() {
                                out.push(AuctionUi::TreeBack);
                            }
                        }
                        "CONTROL_9334" if line.selected => {
                            if let Some((_, t)) = c.textures.first() {
                                control_image(ctx, hud, assets, painter, rr, c.size, t, 1.0);
                            }
                        }
                        "CONTROL_4940" => {
                            let resp = ui.interact(rr, egui::Id::new(("ac-tree-row", k)), egui::Sense::click());
                            if resp.hovered() {
                                if let Some((_, t)) = c.textures.first() {
                                    control_image(ctx, hud, assets, painter, rr, c.size, t, 1.0);
                                }
                            }
                            let tr = Rect::from_min_max(rr.min + Vec2::new(4.0 * s, 0.0), rr.max);
                            control_text(painter, tr, c, &line.label, font, None);
                            if resp.clicked() {
                                out.push(AuctionUi::Tree(k));
                            }
                        }
                        _ => {}
                    }
                }
                continue;
            }
            // Colonnes de la planche (enfants de `CONTROL_AC_BoardBase`).
            if name.starts_with("CONTROL_AC_") && name.ends_with("Button") && c.display.as_deref().is_some_and(|d| d.contains("AuctionBoard")) || name == "CONTROL_AC_BoardSelectBar" {
                for (k, line) in board.iter().enumerate().take(AUCTION_BOARD_ROWS) {
                    let rr = layout.rect(origin, [c.pos[0], c.pos[1] + k as f32 * BOARD_ROW_H], c.size);
                    let text = match name {
                        "CONTROL_AC_ItemNameButton" => &line.name,
                        "CONTROL_AC_PriceButton" => &line.gold,
                        "CONTROL_AC_GoldPerUnitButton" => &line.gold_each,
                        "CONTROL_AC_CCButton" => &line.cc,
                        "CONTROL_AC_CCPerUnitButton" => &line.cc_each,
                        "CONTROL_AC_QuantityButton" => &line.count,
                        "CONTROL_AC_SellerNameButton" => &line.seller,
                        _ => {
                            // Barre de sélection : texture au survol, achat au clic (`AC_BuyItem`).
                            let resp = ui.interact(rr, egui::Id::new(("ac-board-row", k)), egui::Sense::click());
                            if resp.hovered() {
                                if let Some((_, t)) = c.textures.first() {
                                    control_image(ctx, hud, assets, painter, rr, c.size, t, 1.0);
                                }
                            }
                            if resp.clicked() {
                                out.push(AuctionUi::Buy(k));
                            }
                            continue;
                        }
                    };
                    control_text(painter, rr, c, text, font, None);
                }
                continue;
            }
            if let Some(n) = name.strip_prefix("CONTROL_AC_Page").and_then(|n| n.parse::<usize>().ok()) {
                let p = first_page + n - 1;
                if p < pages {
                    let current = p == page;
                    if current {
                        if let Some((_, t)) = c.textures.first() {
                            control_image(ctx, hud, assets, painter, r, c.size, t, 1.0);
                        }
                    }
                    let resp = ui.interact(r, egui::Id::new(("ac-page", n)), egui::Sense::click());
                    let color = c.status_colors.first().map_or(Color32::WHITE, |&v| super::portal_ui::argb(v));
                    control_text(painter, r, c, &(p + 1).to_string(), font * 0.95, Some(if current { Color32::from_rgb(255, 220, 120) } else { color }));
                    if resp.clicked() && !current {
                        out.push(AuctionUi::Page(p));
                    }
                }
                continue;
            }
            match (c.kind, name) {
                (3, "CONTROL_AC_CloseButton" | "CONTROL_2002") => {
                    let label = if name == "CONTROL_2002" { "" } else { static_text(c) };
                    if state_button(ctx, ui, hud, assets, painter, r, c, label, font, true, ("ac-close", name)).clicked() {
                        out.push(AuctionUi::Close);
                    }
                }
                (3, "CONTROL_AC_PrevPageButton" | "CONTROL_AC_NextPageButton") => {
                    let prev = name == "CONTROL_AC_PrevPageButton";
                    let enabled = if prev { page > 0 } else { page + 1 < pages };
                    if state_button(ctx, ui, hud, assets, painter, r, c, "", font, enabled, ("ac-arrow", name)).clicked() {
                        out.push(if prev { AuctionUi::Prev } else { AuctionUi::Next });
                    }
                }
                (3, "CONTROL_1226" | "CONTROL_1501") => {
                    let up = name == "CONTROL_1226";
                    let enabled = if up { tree_first > 0 } else { tree_first + AUCTION_TREE_ROWS < tree.len() };
                    if state_button(ctx, ui, hud, assets, painter, r, c, "", font, enabled, ("ac-tree-scroll", name)).clicked() {
                        out.push(AuctionUi::TreeScroll(if up { -1 } else { 1 }));
                    }
                }
                (15, "CONTROL_6622") => {
                    // Curseur entre les deux flèches (barre de 385 px).
                    let span = tree.len().saturating_sub(AUCTION_TREE_ROWS).max(1) as f32;
                    let travel = (385.0 - 32.0 - c.size[1]).max(0.0);
                    let y = (tree_first as f32 / span).clamp(0.0, 1.0) * travel;
                    if let Some((_, t)) = c.textures.first() {
                        control_image(ctx, hud, assets, painter, layout.rect(origin, [c.pos[0], c.pos[1] + y], c.size), c.size, t, 1.0);
                    }
                }
                (3, n) if n.ends_with("Tab") => {
                    let sort = match n {
                        "CONTROL_AC_ItemNameTab" => AuctionSort::Name,
                        "CONTROL_AC_PriceTab" => AuctionSort::Gold,
                        "CONTROL_AC_GoldPerUnitTab" => AuctionSort::GoldPerUnit,
                        "CONTROL_AC_CCTab" => AuctionSort::Cc,
                        "CONTROL_AC_CCPerUnitTab" => AuctionSort::CcPerUnit,
                        "CONTROL_AC_QuantityTab" => AuctionSort::Count,
                        _ => AuctionSort::Seller,
                    };
                    if state_button(ctx, ui, hud, assets, painter, r, c, static_text(c), font, true, ("ac-tab", n)).clicked() {
                        out.push(AuctionUi::Sort(sort));
                    }
                }
                (1, "CONTROL_AC_TIDNameStatic") => control_text(painter, r, c, title, font, None),
                (1, "CONTROL_3992") => {
                    let info = if pages > 0 { format!("{}/{}", page + 1, pages) } else { String::new() };
                    control_text(painter, r, c, &info, font * 0.95, None);
                }
                (1, "CONTROL_4715") => control_text(painter, r, c, static_text(c), font * 1.1, None),
                (1, "CONTROL_AC_SearchTID_Base") => {
                    let rr = Rect::from_min_size(r.min, Vec2::new(46.0, 18.0) * s);
                    control_text(painter, rr, c, static_text(c), font, Some(Color32::WHITE));
                }
                (2, "CONTROL_AC_SearchTID") => {
                    if let Some((_, t)) = c.textures.first() {
                        control_image(ctx, hud, assets, painter, r, c.size, t, 1.0);
                    }
                    let edit = egui::TextEdit::singleline(&mut *search)
                        .id(egui::Id::new("archonia-auction-search"))
                        .frame(false)
                        .char_limit(80)
                        .font(FontId::proportional(font))
                        .text_color(Color32::WHITE);
                    let resp = ui.put(r.shrink2(Vec2::new(3.0, 0.0)), edit);
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        out.push(AuctionUi::Search);
                    }
                }
                (8, "CONTROL_AC_BoardItemTemplateGrid") => {
                    if let Some(tid) = item_tid {
                        item_icon(ctx, hud, assets, painter, r, Some(tid), Color32::WHITE);
                    }
                }
                _ => {}
            }
        }
    });
}

/// Boîte des ventes du joueur.
pub fn draw_sales_box(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &SalesBoxView, out: &mut Vec<SalesBoxUi>) {
    let Some(w) = window(hud, assets, "UI_SalesBox2") else { return };
    frame(ctx, hud, assets, &w, "archonia-salesbox2", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let dragging = egui::DragAndDrop::payload::<ItemDrag>(ctx);
        if let Some(list) = w.control("CONTROL_SalesBox2_List") {
            let lr = layout.rect(origin, list.pos, list.size);
            let resp = ui.interact(lr, egui::Id::new("ac-salesbox-list"), egui::Sense::hover());
            if let Some(drag) = resp.dnd_release_payload::<ItemDrag>() {
                out.push(SalesBoxUi::Drop(*drag));
            }
            if dragging.is_some() && resp.contains_pointer() {
                painter.rect_stroke(lr, 2.0, egui::Stroke::new(1.0_f32, Color32::from_rgb(255, 210, 110)), egui::StrokeKind::Inside);
            }
            if ui.rect_contains_pointer(lr) {
                let wheel = ui.input(|i| i.raw_scroll_delta.y);
                if wheel.abs() > 0.5 {
                    out.push(SalesBoxUi::Scroll(if wheel > 0.0 { -1 } else { 1 }));
                }
            }
        }
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            let name = c.name.as_str();
            if matches!(name, "CONTROL_SalesBox2_List_Grid" | "CONTROL_531" | "CONTROL_32732" | "CONTROL_4130" | "CONTROL_SalesBox2_OkCancel_Button") {
                for (k, row) in view.rows.iter().enumerate().skip(view.first).take(SALES_BOX_ROWS) {
                    let rr = layout.rect(origin, [c.pos[0], c.pos[1] + (k - view.first) as f32 * SALES_ROW_H], c.size);
                    match name {
                        "CONTROL_SalesBox2_List_Grid" => {
                            item_icon(ctx, hud, assets, painter, rr, Some(row.tid), Color32::WHITE);
                            if row.count > 1 {
                                shadow_text(painter, rr.right_bottom() - Vec2::new(3.0, 2.0) * s, Align2::RIGHT_BOTTOM, &row.count.to_string(), font, Color32::WHITE);
                            }
                        }
                        "CONTROL_531" => control_text(painter, rr, c, &row.name, font, None),
                        "CONTROL_32732" => control_text(painter, rr, c, &row.gold, font, Some(Color32::from_rgb(255, 220, 120))),
                        "CONTROL_4130" => control_text(painter, rr, c, &row.cc, font, None),
                        _ => {
                            if state_button(ctx, ui, hud, assets, painter, rr, c, &row.button, font * 0.9, true, ("ac-salesbox-btn", k)).clicked() {
                                out.push(SalesBoxUi::Button(k));
                            }
                        }
                    }
                }
                continue;
            }
            match (c.kind, name) {
                (3, "CONTROL_SaleBox2_Close_Button" | "CONTROL_SalesBox2_ExitButton") => {
                    let label = if name == "CONTROL_SalesBox2_ExitButton" { "" } else { static_text(c) };
                    if state_button(ctx, ui, hud, assets, painter, r, c, label, font, true, ("ac-salesbox-close", name)).clicked() {
                        out.push(SalesBoxUi::Close);
                    }
                }
                (1, "CONTROL_SalesBox2_Static") => control_text(painter, r, c, static_text(c), font, None),
                (1, "CONTROL_SalesBox2_SalesOn_Caption") => control_text(painter, r, c, view.on_sale, font, None),
                (1, "CONTROL_SalesBox2_SalesComplete_Caption") => control_text(painter, r, c, view.sold, font, None),
                (15, "CONTROL_SalesBox2_ScrollButton") => {
                    let span = view.rows.len().saturating_sub(SALES_BOX_ROWS).max(1) as f32;
                    let travel = (202.0 - c.size[1]).max(0.0);
                    let y = (view.first as f32 / span).clamp(0.0, 1.0) * travel;
                    if let Some((_, t)) = c.textures.first() {
                        control_image(ctx, hud, assets, painter, layout.rect(origin, [c.pos[0], c.pos[1] + y], c.size), c.size, t, 1.0);
                    }
                }
                _ => {}
            }
        }
    });
}

/// Fixation du prix d'une mise en vente.
pub fn draw_create_sale(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: CreateSaleView, out: &mut Vec<CreateSaleUi>) {
    let Some(w) = window(hud, assets, "UI_AuctionCreateSale") else { return };
    let CreateSaleView { gold, cc, fee_gold, fee_cc, marvel } = view;
    frame(ctx, hud, assets, &w, "archonia-auction-create-sale", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let dragging = egui::DragAndDrop::payload::<ItemDrag>(ctx);
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match (c.kind, c.name.as_str()) {
                (3, name) => {
                    let (action, label) = match name {
                        "CONTROL_29441" => (CreateSaleUi::Confirm, static_text(c)),
                        "CONTROL_14440" => (CreateSaleUi::Cancel, ""),
                        _ => (CreateSaleUi::Cancel, static_text(c)),
                    };
                    if state_button(ctx, ui, hud, assets, painter, r, c, label, font, true, ("ac-create-btn", name)).clicked() {
                        out.push(action);
                    }
                }
                // Le libellé « GOLD: » / « CC: » occupe le début du champ : saisie à sa droite.
                (2, "CONTROL_PriceGold") => digits_edit(ui, Rect::from_min_max(r.min + Vec2::new(42.0 * s, 0.0), r.max), "gold", gold, 13, font),
                (2, "CONTROL_PriceCC") => digits_edit(ui, Rect::from_min_max(r.min + Vec2::new(42.0 * s, 0.0), r.max), "cc", cc, 13, font),
                (2, "CONTROL_AuctionSaleFeeGold") => super::convert_ui::edit_box(painter, r, fee_gold, font),
                (2, "CONTROL_AuctionSaleFeeCC") => super::convert_ui::edit_box(painter, r, fee_cc, font),
                (2, _) => super::convert_ui::edit_box_aligned(painter, r, static_text(c), font, egui::Align::Min),
                (1, _) => control_text(painter, r, c, static_text(c), font, None),
                (8, "CONTROL_MarvelScrollAuction") => {
                    let resp = ui.interact(r, egui::Id::new("ac-marvel-grid"), egui::Sense::click());
                    if let Some(tid) = marvel {
                        item_icon(ctx, hud, assets, painter, r, Some(tid), Color32::WHITE);
                    }
                    if resp.secondary_clicked() && marvel.is_some() {
                        out.push(CreateSaleUi::RemoveMarvel);
                    }
                    if let Some(drag) = resp.dnd_release_payload::<ItemDrag>() {
                        out.push(CreateSaleUi::Marvel(*drag));
                    }
                    let target = dragging.is_some() && resp.contains_pointer();
                    if resp.hovered() || target {
                        let color = if target { Color32::from_rgb(255, 210, 110) } else { Color32::from_rgb(120, 200, 255) };
                        painter.rect_stroke(r, 2.0, egui::Stroke::new(1.0_f32, color), egui::StrokeKind::Inside);
                    }
                }
                _ => {}
            }
        }
    });
}
