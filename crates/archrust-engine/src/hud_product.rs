//! Fenêtres de production d'Archonia (AgcmUIProduct, AgcmUIProductSkill), d'après `UI_1024X768.ini` :
//!
//! * `UI_Compose` (cuisine, alchimie) : titre `CTL_ST_TITLE` (`COMPOSE_DISP_TITLE`), invite
//!   `CTL_EDIT_BASE` (`COMPOSE_DISP_SEL_EDIT`), liste des recettes connues `CTL_LIST_SEL` (lignes de
//!   20 px, barre `Refinery_List_Bar.png` de `CTL_BTN_SEL` sous la ligne choisie ou survolée), détail
//!   `CTL_EDIT_SUB` (`COMPOSE_DISP_MAKE_EDIT`), matériaux en icônes dans `CTL_GRID_SRC`
//!   (`COMPOSE_UD_MAKE_GRID`, 4 × 2 cases de 50 px), boutons `CTL_BTN_OK` (actif selon
//!   `COMPOSE_ACTIVE_COMPOSE_BTN`), `CTL_BTN_CLOSE` et la croix ;
//! * `UI_PDSkillBuy` (PNJ de production) : grille de deux colonnes d'éléments de 208 × 56
//!   (`PDSkillBuy_Listitem.png`) : icône (`CTL_GRID_SKILL`), nom (`PDSKBuyName`), prix (`PDSKBuyPrice`),
//!   voile `CTL_ST_DISABLE` (`PDSkillBuy_Btn_Disable.png`, ou `…_Learned.png` pour une connaissance
//!   acquise), cadre de sélection `CONTROL_12311` ; or porté `CTL_ST_MONEY`, catégorie `CTL_ST_TITLE2`,
//!   boutons « Apprendre » (`PDSKActiveBuyBtn`) et « Fermer » ;
//! * `UI_PDSkill` (compétences de production, touche J) : mêmes éléments (`PDSkill_Listitem.png`), nom,
//!   « niveau, expérience % » (`PDSKLvExp`) et barre `CTL_BAR_SKILL` (`PDSkill_Bar.png`,
//!   `PDSKLvExpDegree` / `PDSKLvExpDegreeMax`).
//!
//! Les demandes reviennent au moteur en [`ComposeUi`] et [`PdSkillUi`] (voir `app_product.rs`).

use super::convert_ui::edit_box_aligned;
use super::items_ui::icon as item_icon;
use super::npc_ui::{button, frame, window};
use super::portal_ui::{control_text, state_button, static_text};
use super::skills_ui::SkillDrag;
use super::*;

/// Fenêtres de production, chargées avec le HUD (textes fixes d'`ini_fr`).
pub(super) const PRODUCT_WINDOWS: [&str; 3] = ["UI_Compose", "UI_PDSkillBuy", "UI_PDSkill"];
/// Lignes visibles de la liste des recettes (rangées de 20 px sur 375).
const COMPOSE_ROWS: usize = 18;

/// Ce que montre `UI_Compose`.
pub struct ComposeView<'a> {
    pub title: &'a str,
    /// Invite de `CTL_EDIT_BASE` (`sysstr` 31 ou 33).
    pub prompt: &'a str,
    /// Recettes connues de la catégorie : nom de l'objet produit et couleur (0xFFAEA59C si elle n'est pas
    /// réalisable).
    pub rows: &'a [(String, Color32)],
    pub selected: Option<usize>,
    pub first_row: usize,
    /// Détail de la recette choisie (`COMPOSE_DISP_MAKE_EDIT`).
    pub detail: &'a str,
    /// Cases de `COMPOSE_UD_MAKE_GRID` : TID et quantité.
    pub grid: &'a [(i32, i32)],
    /// Libellés des boutons (`sysstr` 36 et 37).
    pub ok: &'a str,
    pub cancel: &'a str,
    pub can_compose: bool,
}

/// Demandes d'`UI_Compose`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeUi {
    Close,
    Select(usize),
    Compose,
    /// Défilement (lignes, négatif vers le haut).
    Scroll(i32),
}

/// Élément de `UI_PDSkillBuy` : compétence (icône de compétence) ou recette (icône de l'objet produit).
#[derive(Debug, Clone, PartialEq)]
pub struct PdBuyEntry {
    pub skill_tid: Option<i32>,
    pub item_tid: Option<i32>,
    pub name: String,
    pub price: String,
    /// État d'Archonia (`FUN_14063ea50`) : 0 possible, 2 déjà acquis, autre : impossible.
    pub state: i32,
}

pub struct PdBuyView<'a> {
    /// Catégorie (`PDSKBuyTitle` : `sysstr` 27, 28, 29).
    pub title: &'a str,
    pub entries: &'a [PdBuyEntry],
    pub selected: Option<usize>,
    pub first: &'a mut usize,
    pub money: i64,
}

/// Élément de `UI_PDSkill`.
#[derive(Debug, Clone, PartialEq)]
pub struct PdSkillEntry {
    pub tid: i32,
    pub name: String,
    /// `PDSKLvExp` : « niveau, expérience % ».
    pub level_exp: String,
    /// `PDSKLvExpDegree` / `PDSKLvExpDegreeMax`.
    pub fraction: f32,
}

/// Demandes d'`UI_PDSkillBuy` et d'`UI_PDSkill`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdSkillUi {
    CloseBuy,
    SelectBuy(usize),
    Buy,
    CloseSkills,
    /// Clic droit ou double clic sur une compétence : la lancer.
    Cast(i32),
}

fn skill_icon(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, cell: Rect, tid: i32) {
    match hud.skill_icon(assets, tid).and_then(|n| texture_icon(ctx, hud, assets, &n)) {
        Some(t) => {
            painter.image(t, cell.shrink(1.0), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            painter.rect_filled(cell.shrink(3.0), 3.0, Color32::from_rgb(40, 60, 90));
        }
    }
}

/// Fenêtre de fabrication.
pub fn draw_compose(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: ComposeView, out: &mut Vec<ComposeUi>) {
    let Some(w) = window(hud, assets, "UI_Compose") else { return };
    let bar = w.control("CTL_BTN_SEL").and_then(|c| c.textures.first()).map(|(_, t)| t.clone());
    frame(ctx, hud, assets, &w, "archonia-compose", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match (c.kind, c.name.as_str()) {
                (1, "CTL_ST_TITLE") => control_text(painter, r, c, view.title, font, None),
                (2, "CTL_EDIT_BASE") => edit_box_aligned(painter, r, view.prompt, font, egui::Align::Center),
                (2, "CTL_EDIT_SUB") => edit_box_aligned(painter, r, view.detail, font, egui::Align::Center),
                (8, "CTL_GRID_SRC") => {
                    let [gx, gy, cw, ch] = c.grid.unwrap_or([2.0, 2.0, 50.0, 50.0]);
                    let cols = ((c.size[0] + gx) / (cw + gx)).floor().max(1.0) as usize;
                    let rows = ((c.size[1] + gy) / (ch + gy)).floor().max(1.0) as usize;
                    for (i, (tid, count)) in view.grid.iter().enumerate().take(cols * rows) {
                        let (row, col) = ((i / cols) as f32, (i % cols) as f32);
                        let rr = layout.rect(origin, [c.pos[0] + col * (cw + gx), c.pos[1] + row * (ch + gy)], [cw, ch]);
                        item_icon(ctx, hud, assets, painter, rr, Some(*tid), Color32::WHITE);
                        if *count > 1 {
                            shadow_text(painter, rr.right_bottom() - Vec2::new(3.0, 2.0) * s, Align2::RIGHT_BOTTOM, &count.to_string(), font, Color32::WHITE);
                        }
                    }
                }
                (10, "CTL_LIST_SEL") => {
                    // Lignes de 20 px ; la colonne de droite (15 px) est la barre de défilement.
                    let list = Rect::from_min_size(r.min, Vec2::new(r.width() - 17.0 * s, r.height()));
                    let resp = ui.interact(list, egui::Id::new("compose-list"), egui::Sense::hover());
                    let wheel = ui.input(|i| i.raw_scroll_delta.y);
                    if resp.hovered() && wheel.abs() > 0.5 {
                        out.push(ComposeUi::Scroll(if wheel > 0.0 { -3 } else { 3 }));
                    }
                    for (k, (name, color)) in view.rows.iter().enumerate().skip(view.first_row).take(COMPOSE_ROWS) {
                        let row = (k - view.first_row) as f32;
                        let rr = Rect::from_min_size(list.min + Vec2::new(0.0, row * 20.0 * s), Vec2::new(180.0 * s, 20.0 * s));
                        let line = ui.interact(rr, egui::Id::new(("compose-row", k)), egui::Sense::click());
                        if view.selected == Some(k) || line.hovered() {
                            if let Some(t) = &bar {
                                control_image(ctx, hud, assets, painter, rr, [180.0, 20.0], t, 1.0);
                            }
                        }
                        shadow_text(painter, rr.center(), Align2::CENTER_CENTER, name, font, *color);
                        if line.clicked() {
                            out.push(ComposeUi::Select(k));
                        }
                    }
                }
                (3, "CTL_BTN_SEL") => {}
                (3, name) => {
                    let (label, action, enabled) = match name {
                        "CTL_BTN_OK" => (view.ok, Some(ComposeUi::Compose), view.can_compose),
                        "CTL_BTN_CLOSE" => (view.cancel, Some(ComposeUi::Close), true),
                        "CTL_BTN_SYSCLOSE" => ("", Some(ComposeUi::Close), true),
                        "CTL_PGUP" => ("", Some(ComposeUi::Scroll(-(COMPOSE_ROWS as i32))), view.first_row > 0),
                        "CTL_PGDN" => ("", Some(ComposeUi::Scroll(COMPOSE_ROWS as i32)), view.first_row + COMPOSE_ROWS < view.rows.len()),
                        _ => ("", None, true),
                    };
                    if button(ctx, ui, hud, assets, painter, r, c, label, font, enabled, ("compose-btn", name)).clicked() {
                        out.extend(action);
                    }
                }
                (15, _) => scroll_thumb(ctx, hud, assets, painter, layout, r, c, view.first_row, view.rows.len(), COMPOSE_ROWS, 330.0),
                _ => {}
            }
        }
    });
}

/// Curseur de défilement placé selon la première ligne affichée (`Scroll=…:début:course`).
#[allow(clippy::too_many_arguments)]
fn scroll_thumb(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, layout: &Layout, r: Rect, c: &UiControl, first: usize, count: usize, page: usize, track: f32) {
    let span = count.saturating_sub(page).max(1) as f32;
    let y = (first as f32 / span).clamp(0.0, 1.0) * (track - c.size[1]).max(0.0);
    let rr = Rect::from_min_size(r.min + Vec2::new(0.0, y * layout.scale), r.size());
    if let Some((_, tex)) = c.textures.first() {
        control_image(ctx, hud, assets, painter, rr, c.size, tex, 1.0);
    }
}

/// Cases d'une liste de deux colonnes (`List=6:0:2:0`) : indice de l'élément et son rectangle.
fn list_cells(ui: &egui::Ui, layout: &Layout, origin: Pos2, list: &UiControl, item: &UiControl, count: usize, first: &mut usize) -> Vec<(usize, Rect)> {
    let list_rect = layout.rect(origin, list.pos, list.size);
    let rows = ((list.size[1] / item.size[1]).floor() as usize).max(1);
    let per_page = rows * 2;
    if ui.rect_contains_pointer(list_rect) {
        let wheel = ui.input(|i| i.raw_scroll_delta.y);
        if wheel < 0.0 && *first + per_page < count {
            *first += 2;
        } else if wheel > 0.0 {
            *first = first.saturating_sub(2);
        }
    }
    *first = (*first).min(count.saturating_sub(1) / 2 * 2);
    (*first..count.min(*first + per_page))
        .map(|i| {
            let k = i - *first;
            let (col, row) = (k % 2, k / 2);
            (i, layout.rect(origin, [list.pos[0] + col as f32 * item.size[0], list.pos[1] + row as f32 * item.size[1]], item.size))
        })
        .collect()
}

/// Rectangle d'un contrôle enfant de l'élément de liste, posé dans la case `cell`.
fn child(layout: &Layout, cell: Rect, item: &UiControl, c: &UiControl) -> Rect {
    layout.rect(cell.min, [c.pos[0] - item.pos[0], c.pos[1] - item.pos[1]], c.size)
}

/// Boutons de page et curseur d'une liste à deux colonnes.
#[allow(clippy::too_many_arguments)]
fn list_scroll(ctx: &egui::Context, ui: &mut egui::Ui, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, layout: &Layout, origin: Pos2, w: &UiWindow, count: usize, per_page: usize, first: &mut usize, id: &str) {
    for c in &w.controls {
        let r = layout.rect(origin, c.pos, c.size);
        match c.name.as_str() {
            "CTL_PGUP" => {
                if state_button(ctx, ui, hud, assets, painter, r, c, "", 10.0, *first > 0, (id, "up")).clicked() {
                    *first = first.saturating_sub(per_page);
                }
            }
            "CTL_PGDN" => {
                if state_button(ctx, ui, hud, assets, painter, r, c, "", 10.0, *first + per_page < count, (id, "down")).clicked() {
                    *first = (*first + per_page).min(count.saturating_sub(1) / 2 * 2);
                }
            }
            "CTL_SCROLLBTN" => scroll_thumb(ctx, hud, assets, painter, layout, r, c, *first, count, per_page, 268.0 + c.size[1]),
            _ => {}
        }
    }
}

/// Fenêtre du PNJ de production : compétences puis recettes de sa catégorie.
pub fn draw_pdskill_buy(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: PdBuyView, out: &mut Vec<PdSkillUi>) {
    let Some(w) = window(hud, assets, "UI_PDSkillBuy") else { return };
    let ctl = |n: &str| w.control(n).cloned();
    let (Some(list), Some(item)) = (ctl("CTL_LIST"), ctl("CTL_LIST_ITEM")) else { return };
    let back = item.textures.first().map(|(_, t)| t.clone());
    let [name_c, price_c, veil_c, active_c] = ["CTL_ST_SKILLNAME", "CTL_ST_SKILLDESC", "CTL_ST_DISABLE", "CONTROL_12311"].map(|n| ctl(n));
    let grid_c = ctl("CTL_GRID_SKILL");
    let per_page = ((list.size[1] / item.size[1]).floor() as usize).max(1) * 2;
    let can_buy = view.selected.and_then(|i| view.entries.get(i)).is_some_and(|e| e.state == 0);
    frame(ctx, hud, assets, &w, "archonia-pdskill-buy", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match c.name.as_str() {
                "CTL_ST_TITLE" => control_text(painter, r, c, static_text(c), font, None),
                "CTL_ST_TITLE2" => control_text(painter, r, c, view.title, font, None),
                "CTL_ST_MONEY" => control_text(painter, r, c, &view.money.to_string(), font, None),
                "CTL_BTN_SYSCLOSE" | "CTL_BTN_CLOSE" | "CTL_BTN_BUY" => {
                    let buy = c.name == "CTL_BTN_BUY";
                    let label = if c.name == "CTL_BTN_SYSCLOSE" { "" } else { static_text(c) };
                    if state_button(ctx, ui, hud, assets, painter, r, c, label, font, !buy || can_buy, ("pdskill-buy", c.name.as_str())).clicked() {
                        out.push(if buy { PdSkillUi::Buy } else { PdSkillUi::CloseBuy });
                    }
                }
                _ => {}
            }
        }
        let mut first = *view.first;
        for (i, cell) in list_cells(ui, layout, origin, &list, &item, view.entries.len(), &mut first) {
            let e = &view.entries[i];
            if let Some(t) = &back {
                control_image(ctx, hud, assets, painter, cell, item.size, t, 1.0);
            }
            if let Some(g) = &grid_c {
                let gr = child(layout, cell, &item, g);
                match (e.skill_tid, e.item_tid) {
                    (Some(tid), _) => skill_icon(ctx, hud, assets, painter, gr, tid),
                    (None, tid) => item_icon(ctx, hud, assets, painter, gr, tid, Color32::WHITE),
                }
            }
            if let Some(c) = &name_c {
                control_text(painter, child(layout, cell, &item, c), c, &e.name, font, None);
            }
            if let Some(c) = &price_c {
                control_text(painter, child(layout, cell, &item, c), c, &e.price, font * 0.95, None);
            }
            // `PDSK_EVENT_BUY_LEARNED_ITEM` : texture 2 ; `…_DISABLE_ITEM` : texture 1.
            if let Some(c) = veil_c.as_ref().filter(|_| e.state != 0) {
                let index = if e.state == 2 { 2 } else { 1 };
                if let Some((_, t)) = c.textures.iter().find(|(k, _)| *k == index) {
                    control_image(ctx, hud, assets, painter, child(layout, cell, &item, c), c.size, t, 1.0);
                }
            }
            if let Some(c) = active_c.as_ref().filter(|_| view.selected == Some(i)) {
                if let Some((_, t)) = c.textures.first() {
                    control_image(ctx, hud, assets, painter, child(layout, cell, &item, c), c.size, t, 1.0);
                }
            }
            let resp = ui.interact(cell, egui::Id::new(("pdskill-buy-item", i)), egui::Sense::click());
            if resp.clicked() {
                out.push(PdSkillUi::SelectBuy(i));
            }
        }
        list_scroll(ctx, ui, hud, assets, painter, layout, origin, &w, view.entries.len(), per_page, &mut first, "pdskill-buy-page");
        *view.first = first;
    });
}

/// Fenêtre des compétences de production du joueur.
pub fn draw_pdskill(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, entries: &[PdSkillEntry], first: &mut usize, out: &mut Vec<PdSkillUi>) {
    let Some(w) = window(hud, assets, "UI_PDSkill") else { return };
    let ctl = |n: &str| w.control(n).cloned();
    let (Some(list), Some(item)) = (ctl("CTL_LIST"), ctl("CTL_LISTITEM")) else { return };
    let back = item.textures.first().map(|(_, t)| t.clone());
    let [name_c, exp_c, bar_c, grid_c] = ["CTL_ST_SKILLNAME", "CTL_ST_SKILLLVEXP", "CTL_BAR_SKILL", "CTL_GRID_SKILL"].map(|n| ctl(n));
    let per_page = ((list.size[1] / item.size[1]).floor() as usize).max(1) * 2;
    frame(ctx, hud, assets, &w, "archonia-pdskill", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match c.name.as_str() {
                "CTL_ST_TITLE1" | "CTL_ST_TITLE2" => control_text(painter, r, c, static_text(c), font, None),
                "CTL_BTN_SYSCLOSE" | "CTL_BTN_CLOSE" => {
                    let label = if c.name == "CTL_BTN_SYSCLOSE" { "" } else { static_text(c) };
                    if state_button(ctx, ui, hud, assets, painter, r, c, label, font, true, ("pdskill", c.name.as_str())).clicked() {
                        out.push(PdSkillUi::CloseSkills);
                    }
                }
                _ => {}
            }
        }
        let dragging = egui::DragAndDrop::has_any_payload(ctx);
        let mut shown = *first;
        for (i, cell) in list_cells(ui, layout, origin, &list, &item, entries.len(), &mut shown) {
            let e = &entries[i];
            if let Some(t) = &back {
                control_image(ctx, hud, assets, painter, cell, item.size, t, 1.0);
            }
            if let Some(g) = &grid_c {
                let gr = child(layout, cell, &item, g);
                skill_icon(ctx, hud, assets, painter, gr, e.tid);
                // La case de l'icône se glisse vers la barre de raccourcis, comme le livre des compétences.
                let resp = ui.interact(gr, egui::Id::new(("pdskill-icon", e.tid)), egui::Sense::click_and_drag());
                if resp.dragged() {
                    resp.dnd_set_drag_payload(SkillDrag { tid: e.tid });
                }
                if resp.secondary_clicked() || resp.double_clicked() {
                    out.push(PdSkillUi::Cast(e.tid));
                }
                if resp.hovered() && !dragging {
                    painter.rect_stroke(gr, 2.0, egui::Stroke::new(1.0_f32, Color32::from_rgb(120, 200, 255)), egui::StrokeKind::Inside);
                }
            }
            if let Some(c) = &name_c {
                control_text(painter, child(layout, cell, &item, c), c, &e.name, font, None);
            }
            if let Some(c) = &exp_c {
                control_text(painter, child(layout, cell, &item, c), c, &e.level_exp, font * 0.95, None);
            }
            if let Some(c) = &bar_c {
                if let Some((_, t)) = c.textures.first() {
                    control_image(ctx, hud, assets, painter, child(layout, cell, &item, c), c.size, t, e.fraction);
                }
            }
        }
        list_scroll(ctx, ui, hud, assets, painter, layout, origin, &w, entries.len(), per_page, &mut shown, "pdskill-page");
        *first = shown;
    });
}
