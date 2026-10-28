//! Fenêtres des guerres de siège d'Archonia (AgcmUISiegeWar, `UI_1024X768.ini`) :
//!
//! * `UI_SiegeWarMain` (`SiegeWarMain.png`) : titre `SiegeWarNPCName` (`CONTROL_30695`), texte `SiegeWarMainText`
//!   (`CONTROL_EDIT_DESC`), date du siège (`CONTROL_EDIT_WAR_TIME`), boutons « Signup for Defending-side »
//!   (`SiegeWarOpenDefenseApplicationUI`) et « Signup for Siege-side » (`SiegeWarOpenAttackApplicationUI`) ;
//! * `UI_SiegeWarSetTIme` : aide et échéance (`CONTROL_AttackHelpEdit`), date (`CONTROL_AttackSiegeWarTimeEdit`),
//!   listes `CONTROL_DateCombo` (2 jours) et `CONTROL_TimeCombo` (20, 21, 22 h), « Confirm siege war time » ;
//! * `UI_SiegeWarApplDefense` : conditions, guildes défenseuses, « Apply for Defence », « Select Guilds »,
//!   « Go Back », « Close » ;
//! * `UI_SiegeWarApplAttack` / `UI_SiegeWarSelectDefense` (`SiegeWarList.png`) : 15 lignes (guilde, chef, points),
//!   pages `CTRL_PAGE_01…09` / `CTRL_DEF_01…09` et flèches, « Register » / « Add » ;
//! * `UI_SiegeWarCatapult` / `UI_SiegeWarAttackRes` : description, grille 2 × 4, « Repair » et « Activate » ;
//! * `UI_SiegeWarRepair` (barre de réparation), `UI_CarveMessage` (« Stamping »), `UI_ResurrectSiege` (« Inside
//!   castle » / « Outside castle ») et `UI_WarInfo` (tableau des guerres : temps, étape, message central, scores).

use super::items_ui::{icon, ItemDrag};
use super::npc_ui::{button, window};
use super::portal_ui::{argb, control_text, state_button, static_text};
use super::*;

pub(super) const SIEGE_WINDOWS: [&str; 11] = [
    "UI_SiegeWarMain",
    "UI_SiegeWarSetTIme",
    "UI_SiegeWarApplDefense",
    "UI_SiegeWarApplAttack",
    "UI_SiegeWarSelectDefense",
    "UI_SiegeWarCatapult",
    "UI_SiegeWarAttackRes",
    "UI_SiegeWarRepair",
    "UI_CarveMessage",
    "UI_ResurrectSiege",
    "UI_WarInfo",
];

/// Lignes des listes de guildes (`List=15`).
pub const SIEGE_LIST_ROWS: usize = 15;

/// Clic dans une fenêtre de siège : contrôle cliqué (`CtrlName`), ligne de liste, case de grille.
#[derive(Debug, Clone, PartialEq)]
pub enum SiegeUi {
    Clicked(String),
    /// Ligne choisie (liste des défenseurs).
    Row(usize),
    /// Choix d'une liste déroulante (contrôle, indice).
    Combo(String, usize),
    /// Objet lâché dans la grille, objet retiré (clic droit) d'une case.
    Drop(ItemDrag),
    Unload(usize),
}

/// Ligne d'une liste de guildes.
#[derive(Debug, Clone, Default)]
pub struct SiegeGuildRow {
    pub name: String,
    pub master: String,
    pub points: String,
    /// Grisée (liste des défenseurs : `Status2=1 Disable`).
    pub disabled: bool,
}

/// Contenu variable d'une fenêtre de siège.
#[derive(Default)]
pub struct SiegeView<'a> {
    /// Textes des contrôles (`CtrlName` → texte).
    pub texts: Vec<(&'a str, String)>,
    pub rows: Vec<SiegeGuildRow>,
    pub selected: Option<usize>,
    /// Page affichée (1-based) et nombre de pages.
    pub page: usize,
    pub pages: usize,
    /// Listes déroulantes : contrôle, choix, choisi.
    pub combos: Vec<(&'a str, Vec<String>, usize)>,
    /// TID des 8 cases de la grille (-1 vide).
    pub grid: Option<[i32; 8]>,
    /// Boutons inactifs.
    pub disabled: Vec<&'a str>,
    /// Barre (`CtrlType 9`) : remplissage 0 à 1.
    pub bar: Option<f32>,
    /// Textes qui acceptent le clic (rendu en `SiegeUi::Clicked`).
    pub clickable: Vec<&'a str>,
}

impl SiegeView<'_> {
    fn text(&self, name: &str) -> Option<&str> {
        self.texts.iter().find(|(n, _)| *n == name).map(|(_, t)| t.as_str())
    }
}

/// Texte multiligne d'un `Edit` (`<\n>` et `\n` en retour à la ligne), coupé à la largeur du contrôle.
fn edit_text(painter: &egui::Painter, r: Rect, text: &str, font: f32) {
    let text = text.replace("<\\n>", "\n");
    let galley = painter.layout(text, FontId::proportional(font), Color32::WHITE, r.width().max(10.0));
    let clip = painter.with_clip_rect(r);
    clip.galley(r.min, galley, Color32::WHITE);
}

/// Liste déroulante d'Archonia (`Common_Combo_Medium_a/b.png`) : rend le choix fait.
fn combo(ctx: &egui::Context, ui: &mut egui::Ui, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, r: Rect, c: &UiControl, choices: &[String], chosen: usize, font: f32) -> Option<usize> {
    let resp = ui.interact(r, egui::Id::new(("siege-combo", &c.name)), egui::Sense::click());
    let tex = if resp.hovered() { c.textures.get(1).or(c.textures.first()) } else { c.textures.first() };
    if let Some((_, t)) = tex.cloned() {
        control_image(ctx, hud, assets, painter, r, c.size, &t, 1.0);
    }
    if let Some(t) = choices.get(chosen) {
        shadow_text(painter, r.left_center() + Vec2::new(6.0, 0.0), Align2::LEFT_CENTER, t, font, Color32::WHITE);
    }
    let popup = egui::Id::new(("siege-combo-open", &c.name));
    let mut open = ui.data(|d| d.get_temp::<bool>(popup)).unwrap_or(false);
    if resp.clicked() {
        open = !open;
    }
    let mut picked = None;
    if open {
        let row_h = r.height();
        let list = Rect::from_min_size(r.left_bottom(), Vec2::new(r.width(), row_h * choices.len() as f32));
        let fg = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new(("siege-combo-list", &c.name))));
        fg.rect_filled(list, 2.0, Color32::from_black_alpha(230));
        for (i, t) in choices.iter().enumerate() {
            let rr = Rect::from_min_size(list.min + Vec2::new(0.0, row_h * i as f32), Vec2::new(r.width(), row_h));
            let item = ui.interact(rr, egui::Id::new(("siege-combo-item", &c.name, i)), egui::Sense::click());
            if item.hovered() {
                fg.rect_filled(rr, 0.0, Color32::from_rgb(90, 70, 40));
            }
            shadow_text(&fg, rr.left_center() + Vec2::new(6.0, 0.0), Align2::LEFT_CENTER, t, font, Color32::WHITE);
            if item.clicked() {
                picked = Some(i);
                open = false;
            }
        }
    }
    ui.data_mut(|d| d.insert_temp(popup, open));
    picked
}

/// Fenêtre de siège `name` : fond, textes fixes, boutons, contenu de `view`. Rend les clics.
pub fn draw_siege_window(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, name: &str, view: &SiegeView, out: &mut Vec<SiegeUi>) {
    let Some(w) = window(hud, assets, name) else { return };
    let dragging = egui::DragAndDrop::payload::<ItemDrag>(ctx).map(|p| *p);
    super::npc_ui::frame(ctx, hud, assets, &w, &format!("archonia-siege-{name}"), |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        // Rangée de liste (`CtrlType 11`) : sa hauteur et la position des colonnes qui en dépendent.
        let row = w.controls.iter().find(|c| c.kind == 11);
        let row_h = row.map_or(19.0, |c| c.size[1]);
        let row_top = row.map(|c| c.pos[1]);
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            if let Some(t) = view.text(&c.name) {
                if c.kind == 2 {
                    edit_text(painter, r.shrink(2.0 * s), t, font);
                } else {
                    control_text(painter, r, c, t, font, None);
                }
                if view.clickable.contains(&c.name.as_str()) {
                    let resp = ui.interact(r, egui::Id::new(("siege-text", name, &c.name)), egui::Sense::click());
                    if resp.hovered() {
                        painter.rect_stroke(r, 2.0, egui::Stroke::new(1.0_f32, Color32::from_rgb(200, 170, 110)), egui::StrokeKind::Inside);
                    }
                    if resp.clicked() {
                        out.push(SiegeUi::Clicked(c.name.clone()));
                    }
                }
                continue;
            }
            if let Some((_, choices, chosen)) = view.combos.iter().find(|(n, _, _)| *n == c.name) {
                if let Some(i) = combo(ctx, ui, hud, assets, painter, r, c, choices, *chosen, font) {
                    out.push(SiegeUi::Combo(c.name.clone(), i));
                }
                continue;
            }
            let display = c.display.as_deref().unwrap_or("");
            // Colonnes des lignes : `CtrlDisplay=2:1:…GuildList …GuildName|MasterName|Point`.
            if display.contains("GuildList") && row_top.is_some() {
                for (i, g) in view.rows.iter().take(SIEGE_LIST_ROWS).enumerate() {
                    let rr = layout.rect(origin, [c.pos[0], c.pos[1] + i as f32 * row_h], c.size);
                    let text = if display.ends_with("GuildName") {
                        &g.name
                    } else if display.ends_with("MasterName") {
                        &g.master
                    } else {
                        &g.points
                    };
                    let color = if g.disabled { c.status_colors.get(1).map(|&x| argb(x)) } else { None };
                    control_text(painter, rr, c, text, font, color);
                }
                continue;
            }
            // Ligne choisie (`SiegeWarDefenseGuildListSelect`, `SiegeWarSelectedList.png`).
            if c.action.as_deref() == Some("SiegeWarDefenseGuildListSelect") {
                for i in 0..view.rows.len().min(SIEGE_LIST_ROWS) {
                    let rr = layout.rect(origin, [c.pos[0], c.pos[1] + i as f32 * row_h], c.size);
                    let resp = ui.interact(rr, egui::Id::new(("siege-row", name, i)), egui::Sense::click());
                    if view.selected == Some(i) || resp.hovered() {
                        if let Some((_, t)) = c.textures.first() {
                            control_image(ctx, hud, assets, painter, rr, c.size, t, if view.selected == Some(i) { 1.0 } else { 0.5 });
                        }
                    }
                    if resp.clicked() {
                        out.push(SiegeUi::Row(i));
                    }
                }
                continue;
            }
            match c.kind {
                // Grille de l'objet de siège (2 × 4 cases de 50 px).
                8 => {
                    let Some(grid) = view.grid else { continue };
                    let [gx, gy, cw, ch] = c.grid.unwrap_or([2.0, 2.0, 50.0, 50.0]);
                    for (cell, tid) in grid.iter().enumerate() {
                        let (row, col) = ((cell / 4) as f32, (cell % 4) as f32);
                        let rr = layout.rect(origin, [c.pos[0] + col * (cw + gx), c.pos[1] + row * (ch + gy)], [cw, ch]);
                        let resp = ui.interact(rr, egui::Id::new(("siege-cell", name, cell)), egui::Sense::click());
                        if *tid > 0 {
                            icon(ctx, hud, assets, painter, rr, Some(*tid), Color32::WHITE);
                            if resp.secondary_clicked() {
                                out.push(SiegeUi::Unload(cell));
                            }
                        }
                        if let Some(drag) = resp.dnd_release_payload::<ItemDrag>() {
                            if !drag.worn && !drag.bank {
                                out.push(SiegeUi::Drop(*drag));
                            }
                        }
                        let target = dragging.is_some() && resp.contains_pointer();
                        if resp.hovered() || target {
                            let color = if target { Color32::from_rgb(255, 210, 110) } else { Color32::from_rgb(120, 200, 255) };
                            painter.rect_stroke(rr, 2.0, egui::Stroke::new(1.0_f32, color), egui::StrokeKind::Inside);
                        }
                    }
                }
                // Barre (réparation).
                9 => {
                    if let (Some(ratio), Some((_, t))) = (view.bar, c.textures.first()) {
                        control_image(ctx, hud, assets, painter, r, c.size, t, ratio.clamp(0.0, 1.0));
                    }
                }
                3 => {
                    // Numéros de page : `CtrlDisplay=2:0:…CurrentPage …PageNumber`, couleur `Status2` (choisie / non).
                    if display.contains("PageNumber") {
                        let n: usize = c.name.rsplit('_').next().and_then(|x| x.parse().ok()).unwrap_or(0);
                        let first = view.page.saturating_sub(1) / 9 * 9;
                        let page = first + n;
                        if n == 0 || page > view.pages {
                            continue;
                        }
                        let resp = ui.interact(r, egui::Id::new(("siege-page", name, n)), egui::Sense::click());
                        let color = c.status_colors.get(usize::from(page == view.page)).map(|&x| argb(x));
                        control_text(painter, r, c, &page.to_string(), font, color);
                        if resp.clicked() {
                            out.push(SiegeUi::Clicked(format!("PAGE:{page}")));
                        }
                        continue;
                    }
                    let enabled = !view.disabled.contains(&c.name.as_str());
                    let label = static_text(c);
                    let resp = if c.button_states.is_some_and(|b| b[0] != 1) {
                        state_button(ctx, ui, hud, assets, painter, r, c, label, font, enabled, ("siege-btn", name, &c.name))
                    } else {
                        button(ctx, ui, hud, assets, painter, r, c, label, font * 0.95, enabled, ("siege-btn", name, &c.name))
                    };
                    if resp.clicked() {
                        out.push(SiegeUi::Clicked(c.name.clone()));
                    }
                }
                1 | 2 => {
                    // Surlignage de ligne (`SiegeWarSelectedList.png`) : dessiné avec la ligne choisie.
                    if let Some((_, tex)) = c.textures.first().filter(|(_, t)| !t.eq_ignore_ascii_case("SiegeWarSelectedList.png")) {
                        control_image(ctx, hud, assets, painter, r, c.size, tex, 1.0);
                    }
                    let t = static_text(c);
                    if !t.is_empty() {
                        control_text(painter, r, c, t, font, None);
                    }
                }
                _ => {}
            }
        }
    });
}

/// Fenêtre `UI_CarveMessage` (« Stamping »), au bas de l'écran, sans fond.
pub fn draw_carve_message(ctx: &egui::Context, hud: &mut HudState, assets: &Assets) {
    let Some(w) = window(hud, assets, "UI_CarveMessage") else { return };
    let layout = Layout::new(ctx.screen_rect());
    let origin = layout.window_origin(&w);
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("archonia-carve-message")));
    for c in &w.controls {
        let r = layout.rect(origin, c.pos, c.size);
        control_text(&painter, r, c, static_text(c), (12.0 * layout.scale).max(9.0), None);
    }
}

/// Bouton de résurrection de siège de `UI_Resurrection` (`CONTROL_26364`, `CharResurrectionSiege`), libellé du
/// château (`SiegeResurrection`, `FUN_1404f8ac0`), placé comme la fenêtre de `draw_resurrection_with` (centrée).
/// Rend `true` au clic.
pub fn draw_resurrection_siege(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, label: &str) -> bool {
    let Some(w) = hud.windows(assets).iter().find(|w| w.name == "UI_Resurrection").cloned() else { return false };
    let Some(c) = w.control("CONTROL_26364").cloned() else { return false };
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale * 1.25;
    let origin = layout.screen.center() - Vec2::new(w.size[0], w.size[1]) * s / 2.0 - Vec2::new(0.0, 80.0 * layout.scale);
    let r = Rect::from_min_size(origin + Vec2::new(c.pos[0], c.pos[1]) * s, Vec2::new(c.size[0], c.size[1]) * s);
    let mut clicked = false;
    egui::Area::new(egui::Id::new("archrust-resurrect-siege")).fixed_pos(r.min).order(egui::Order::Foreground).show(ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(r.size(), egui::Sense::hover());
        let painter = ui.painter().clone();
        clicked = button(ctx, ui, hud, assets, &painter, rect, &c, label, (11.0 * s).max(9.0), true, "archrust-resurrect-siege-btn").clicked();
    });
    clicked
}
