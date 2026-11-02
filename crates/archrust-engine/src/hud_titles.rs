//! Fenêtres des titres d'Archonia (AgcmTitle, `UI_1024X768.ini`) :
//!
//! * `UI_TitleAll` (« Title Settings », `UiDialog_Title_BG_Big.png`, 697 × 475) : catégories
//!   (`List_TitleCategory`, lignes `ListItem_TitleCategory` de 20 px), titres de la catégorie (`List_TitleList`,
//!   15 par page : `FUN_1405fad30` divise par 15), pages `Button_ListPage1…5` par groupes de cinq, flèches
//!   `Button_ListPrev` / `Next`, « n / total » dans `Edit_TitlePage`, nom (`Edit_TitleName`), conditions
//!   (`Edit_TitleInfo`, « %s</n>(%d/%d) » pour un objectif, `FUN_1405faa20`), bonus (`Edit_TitleEffect`), boutons
//!   `Button_SetTargetTitle` / `Button_ActivateTitle` / `Button_CompleteTitle` (libellés et états de
//!   `FUN_1405f9e40`), croix `Button_CloseDialog` ;
//! * `UI_TitleCurrent` (`UiDialog_Title_BG_Medium.png`, 517 × 475) : titres possédés (`List_TitleList`, 10 lignes),
//!   mêmes zones de texte, `Button_Close` et croix ;
//! * `UI_TitleMini` (`UiDialog_Title_BG_BigSmall.png`, 267 × 473) : objectifs en cours, légende « Title
//!   Prerequisite » et bouton « Give Up » écrits en dur par `FUN_1405f3660`, `Button_Close` et croix.
//!
//! Les noms des lignes prennent la couleur `FontColor` de leur état (`titlesetting.xml`).

use super::npc_ui::{frame, window};
use super::portal_ui::{control_text, state_button};
use super::*;

pub(super) const TITLE_WINDOWS: [&str; 3] = ["UI_TitleAll", "UI_TitleCurrent", "UI_TitleMini"];

/// Titres par page d'`UI_TitleAll` (`FUN_1405fad30` : × 0,066 666 67).
pub const TITLE_PAGE_ROWS: usize = 15;
/// Lignes d'`UI_TitleCurrent` et d'`UI_TitleMini` (`List=10`).
pub const TITLE_SHORT_ROWS: usize = 10;
/// Lignes de la liste des catégories (`List=17`).
pub const TITLE_CATEGORY_ROWS: usize = 17;

/// Les trois fenêtres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleWindow {
    All,
    Current,
    Mini,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleUi {
    Close(TitleWindow),
    Category(usize),
    /// Ligne cliquée (indice dans la page ou la liste).
    Row(TitleWindow, usize),
    /// Page choisie (à partir de 1).
    Page(usize),
    PrevPage,
    NextPage,
    SetTarget,
    Activate,
    Complete,
    Surrender,
}

/// Ligne d'une liste : texte et couleur de son état.
#[derive(Debug, Clone, PartialEq)]
pub struct TitleRow {
    pub text: String,
    pub color: Color32,
}

/// Nom, conditions et bonus du titre choisi.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TitleDetail {
    pub name: String,
    pub info: String,
    pub effect: String,
}

pub struct TitleAllView<'a> {
    pub categories: &'a [String],
    pub category: Option<usize>,
    pub rows: &'a [TitleRow],
    pub selected: Option<usize>,
    /// Page courante (à partir de 1) et nombre de pages (0 sans catégorie).
    pub page: usize,
    pub pages: usize,
    pub detail: Option<&'a TitleDetail>,
    /// Libellés et activité de `Button_SetTargetTitle`, `Button_ActivateTitle`, `Button_CompleteTitle`.
    pub buttons: [(String, bool); 3],
}

pub struct TitleListView<'a> {
    pub rows: &'a [TitleRow],
    pub selected: Option<usize>,
    pub detail: Option<&'a TitleDetail>,
    /// `UI_TitleMini` : bouton « Give Up » actif.
    pub surrender: bool,
}

/// Texte d'Archonia (`</n>` : retour à la ligne, codes `<H…>`) dans une zone de texte.
fn title_text(painter: &egui::Painter, r: Rect, text: &str, font: f32) {
    super::convert_ui::edit_box_aligned(painter, r, &text.replace("</n>", "\n"), font, egui::Align::Min);
}

/// Lignes d'une liste : fond `Texture=1` du contrôle `List_*` sous la ligne choisie, clic.
#[allow(clippy::too_many_arguments)]
fn list_rows(ctx: &egui::Context, ui: &mut egui::Ui, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, layout: &Layout, origin: Pos2, list: &UiControl, item: Option<&UiControl>, rows: &[TitleRow], selected: Option<usize>, max: usize, font: f32, id: &str, mut clicked: impl FnMut(usize)) {
    let (pos, size) = item.map_or((list.pos, [list.size[0], 20.0]), |i| (i.pos, i.size));
    for (i, row) in rows.iter().take(max).enumerate() {
        let r = layout.rect(origin, [pos[0], pos[1] + i as f32 * size[1]], size);
        let resp = ui.interact(r, egui::Id::new((id, i)), egui::Sense::click());
        if selected == Some(i) || resp.hovered() {
            if let Some((_, t)) = list.textures.first() {
                control_image(ctx, hud, assets, painter, r, size, t, 1.0);
            }
        }
        let c = item.unwrap_or(list);
        control_text(painter, r, c, &row.text, font, Some(row.color));
        if resp.clicked() {
            clicked(i);
        }
    }
}

pub fn draw_title_all(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &TitleAllView, out: &mut Vec<TitleUi>) {
    let Some(w) = window(hud, assets, "UI_TitleAll") else { return };
    frame(ctx, hud, assets, &w, "archonia-title-all", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let categories: Vec<TitleRow> = view.categories.iter().map(|c| TitleRow { text: c.clone(), color: Color32::WHITE }).collect();
        // Premier bouton du groupe de cinq pages (`FUN_1405fad30`).
        let first = view.page.saturating_sub(match view.page % 5 { 0 => 5, n => n }) + 1;
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match c.name.as_str() {
                "StaticText_TitleCategory" | "StaticText_TitleName" | "StaticText_TitleInfo" | "StaticTexT_TitleEffect" | "StaticText_TitleAllCaption" | "StaticText_TitleRequire" => control_text(painter, r, c, c.fixed_label(), font, None),
                "List_TitleCategory" => list_rows(ctx, ui, hud, assets, painter, layout, origin, c, w.control("ListItem_TitleCategory"), &categories, view.category, TITLE_CATEGORY_ROWS, font, "title-category", |i| out.push(TitleUi::Category(i))),
                "List_TitleList" => list_rows(ctx, ui, hud, assets, painter, layout, origin, c, w.control("ListItem_TItle"), view.rows, view.selected, TITLE_PAGE_ROWS, font, "title-all-row", |i| out.push(TitleUi::Row(TitleWindow::All, i))),
                "Button_CategoryUp" | "Button_CategoryDown" => {
                    state_button(ctx, ui, hud, assets, painter, r, c, "", font, false, ("title-category-arrow", c.name.as_str()));
                }
                "Button_ListPrev" | "Button_ListNext" => {
                    let next = c.name == "Button_ListNext";
                    let enabled = if next { view.page < view.pages } else { view.page > 1 };
                    if state_button(ctx, ui, hud, assets, painter, r, c, "", font, enabled, ("title-page-arrow", next)).clicked() {
                        out.push(if next { TitleUi::NextPage } else { TitleUi::PrevPage });
                    }
                }
                name if name.starts_with("Button_ListPage") => {
                    let Some(k) = name.strip_prefix("Button_ListPage").and_then(|n| n.parse::<usize>().ok()) else { continue };
                    let page = first + k - 1;
                    if view.pages == 0 || page > view.pages {
                        continue;
                    }
                    let resp = ui.interact(r, egui::Id::new(("title-page", k)), egui::Sense::click());
                    control_text(painter, r, c, &page.to_string(), font, None);
                    if resp.clicked() {
                        out.push(TitleUi::Page(page));
                    }
                }
                "Edit_TitlePage" => {
                    if view.pages > 0 {
                        control_text(painter, r, c, &format!("{} / {}", view.page, view.pages), font, None);
                    }
                }
                "Edit_TitleName" => {
                    if let Some(d) = view.detail {
                        control_text(painter, r, c, &d.name, font, None);
                    }
                }
                "Edit_TitleInfo" | "Edit_TitleEffect" => {
                    if let Some(d) = view.detail {
                        title_text(painter, r, if c.name == "Edit_TitleInfo" { &d.info } else { &d.effect }, font);
                    }
                }
                "Button_SetTargetTitle" | "Button_ActivateTitle" | "Button_CompleteTitle" => {
                    let (i, ev) = match c.name.as_str() {
                        "Button_SetTargetTitle" => (0, TitleUi::SetTarget),
                        "Button_ActivateTitle" => (1, TitleUi::Activate),
                        _ => (2, TitleUi::Complete),
                    };
                    let (label, enabled) = &view.buttons[i];
                    let label = if label.is_empty() { c.fixed_label() } else { label.as_str() };
                    if state_button(ctx, ui, hud, assets, painter, r, c, label, font, *enabled, ("title-all-button", i)).clicked() {
                        out.push(ev);
                    }
                }
                "Button_CloseDialog" => {
                    if state_button(ctx, ui, hud, assets, painter, r, c, "", font, true, "title-all-x").clicked() {
                        out.push(TitleUi::Close(TitleWindow::All));
                    }
                }
                _ => {}
            }
        }
    });
}

pub fn draw_title_current(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &TitleListView, out: &mut Vec<TitleUi>) {
    let Some(w) = window(hud, assets, "UI_TitleCurrent") else { return };
    frame(ctx, hud, assets, &w, "archonia-title-current", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match c.name.as_str() {
                "StaticText_TitleTargetName" | "StaticText_TitleInfo" | "StaticTexT_TitleEffect" | "StaticText_TitleCurrentCaption" | "StaticText_TitleRequire" => control_text(painter, r, c, c.fixed_label(), font, None),
                "List_TitleList" => list_rows(ctx, ui, hud, assets, painter, layout, origin, c, w.control("ListItem_TItle"), view.rows, view.selected, TITLE_SHORT_ROWS, font, "title-current-row", |i| out.push(TitleUi::Row(TitleWindow::Current, i))),
                "Edit_TitleName" => {
                    if let Some(d) = view.detail {
                        control_text(painter, r, c, &d.name, font, None);
                    }
                }
                "Edit_TitleInfo" | "Edit_TitleEffect" => {
                    if let Some(d) = view.detail {
                        title_text(painter, r, if c.name == "Edit_TitleInfo" { &d.info } else { &d.effect }, font);
                    }
                }
                "Button_Close" | "Button_CloseDialog" => {
                    let label = if c.name == "Button_Close" { c.fixed_label() } else { "" };
                    if state_button(ctx, ui, hud, assets, painter, r, c, label, font, true, ("title-current-close", c.name.as_str())).clicked() {
                        out.push(TitleUi::Close(TitleWindow::Current));
                    }
                }
                _ => {}
            }
        }
    });
}

/// Légende et bouton d'`UI_TitleMini`, écrits en dur par Archonia (`FUN_1405f3660`, `s_Title_Prerequisite`,
/// `s_Give_Up_`).
pub const MINI_CAPTION: &str = "Title Prerequisite";
pub const MINI_SURRENDER: &str = "Give Up";

pub fn draw_title_mini(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &TitleListView, out: &mut Vec<TitleUi>) {
    let Some(w) = window(hud, assets, "UI_TitleMini") else { return };
    frame(ctx, hud, assets, &w, "archonia-title-mini", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match c.name.as_str() {
                "StaticText_TitleMiniCaption" => control_text(painter, r, c, MINI_CAPTION, font, None),
                "List_TitleList" => list_rows(ctx, ui, hud, assets, painter, layout, origin, c, w.control("ListItem_TItle"), view.rows, view.selected, TITLE_SHORT_ROWS, font, "title-mini-row", |i| out.push(TitleUi::Row(TitleWindow::Mini, i))),
                "Button_SurrenderTitle" => {
                    if state_button(ctx, ui, hud, assets, painter, r, c, MINI_SURRENDER, font, view.surrender, "title-mini-surrender").clicked() {
                        out.push(TitleUi::Surrender);
                    }
                }
                "Button_Close" | "Button_CloseDialog" => {
                    let label = if c.name == "Button_Close" { c.fixed_label() } else { "" };
                    if state_button(ctx, ui, hud, assets, painter, r, c, label, font, true, ("title-mini-close", c.name.as_str())).clicked() {
                        out.push(TitleUi::Close(TitleWindow::Mini));
                    }
                }
                _ => {}
            }
        }
    });
}
