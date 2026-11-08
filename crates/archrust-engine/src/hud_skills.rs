//! Compétences dans les fenêtres d'Archonia : livre des compétences (`UI_Skill` et ses onglets Combat, Buff,
//! Class, Passive, AL ; `UI_ScionArchlordSkill` du Dragon Scion), instructeur (`UI_SkillBuy`, onglets Combat à
//! Passive ; Combo et Heroic ouvrent les arbres de `hud_mastery.rs`) et barre de raccourcis de `UI_CharInfo` (grilles `Quick_Belt_0(L)`/`(R)` et cases
//! des potions de vie et de mana).
//!
//! Glisser une compétence du livre, ou un objet du sac, sur une case de la barre l'y range ; glisser
//! une case hors de la barre la vide. Les deux curseurs `PointScrollBar` posés sur les jauges de PV et de
//! PM (`CONTROL_9745`, `CONTROL_24256`, piste de 350 sur 370) règlent le seuil de prise automatique des
//! potions des cases 1 et 2 (`ChangeHPPointScrollBarValue` d'Archonia : position × 100, en %). Les demandes reviennent au moteur en [`SkillUi`] (voir
//! `app_skills.rs`).

use super::items_ui::{icon as item_icon, ItemDrag};
use super::mastery_ui::{draw_tabs, SkillTab};
use super::*;

/// Barres de raccourcis en plus d'Archonia (`UI_QuickBelt_1` à `_4`, 520 × 52, fond
/// `CharInfo_Base3.png`) : la flèche haute de la barre (`CONTROL_20969`, `QuickBeltLayerUp`,
/// FUN_14060df50) en affiche une de plus, la basse (`CONTROL_9156`) cache la dernière.
pub(super) const QUICKBELT_WINDOWS: [&str; 4] = ["UI_QuickBelt_1", "UI_QuickBelt_2", "UI_QuickBelt_3", "UI_QuickBelt_4"];
/// Nombre de barres en plus.
pub const EXTRA_BELTS: usize = QUICKBELT_WINDOWS.len();
/// Cases : potions (2), barre principale (10), puis 10 par barre en plus.
pub const QUICK_SLOTS: usize = 12 + 10 * EXTRA_BELTS;

/// Raccourci d'une case de la barre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    Skill(i32),
    Item(i32),
}

/// Compétence saisie dans le livre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkillDrag {
    pub tid: i32,
}

/// Case de la barre saisie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotDrag {
    pub slot: usize,
}

/// Demande des fenêtres de compétences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillUi {
    CloseBook,
    CloseTrainer,
    Cast(i32),
    SelectTrainer(i32),
    Learn(i32),
    Upgrade(i32),
    /// Bouton d'achat de l'instructeur : TID, compétence pas encore apprise.
    TrainerBuy(i32, bool),
    SlotSet { slot: usize, shortcut: Shortcut },
    /// Objet lâché sur une case : contrôlé par le moteur (`QB_Add_Invalid_Item`) avant d'être posé.
    SlotItem { slot: usize, iid: i32, tid: i32 },
    SlotSwap(usize, usize),
    SlotClear(usize),
    SlotUse(usize),
    /// Seuil de prise automatique de la potion de vie (`hp`) ou de mana, en %.
    AutoGauge { hp: bool, value: u8 },
    /// Flèches de la barre : une barre en plus (+1) ou en moins (-1).
    Belts(i8),
    /// Onglet du livre, ou de l'instructeur, choisi (change de fenêtre pour les arbres).
    Tab(SkillTab),
    TrainerTab(SkillTab),
    /// Case d'un arbre de maîtrise choisie.
    TreeSelect(i32),
    /// « Apprendre » / « Améliorer » d'un arbre de l'instructeur (confirmation d'Archonia, puis envoi).
    TreeLearn(i32),
    /// Compétence choisie dans la liste du livre (`Skill_SelectGrid`).
    BookSelect(i32),
    /// « Reset Selected » (`Skill_Rollback`, `Skill_*_Skill_RollBack`) sur la compétence choisie (TID) : Return Orb,
    /// voir `app_consumables.rs`.
    Rollback(i32),
}

/// Compétence du livre.
pub struct BookEntry {
    pub tid: i32,
    pub name: String,
    pub level: i32,
    /// Part du délai de relance restante (0 : prête) et secondes restantes.
    pub cooldown: f32,
    pub left_s: u32,
    /// Infobulle (lignes colorées, voir `app_skills.rs`).
    pub tip: Vec<(String, Color32)>,
}

pub struct BookView<'a> {
    pub entries: &'a [BookEntry],
    pub points: Option<i32>,
    pub first: &'a mut usize,
    /// Onglet courant (Combat… AL, ou AL du Scion : `UI_ScionArchlordSkill`) et onglets ouvrables.
    pub tab: SkillTab,
    pub allowed: &'a dyn Fn(SkillTab) -> bool,
    /// Nom de la classe (texte `2:0:主_Character …`).
    pub title: String,
    /// Compétence choisie (`Skill_SelectGrid`, TID).
    pub selected: Option<i32>,
    /// « Reset Selected » actif (`Skill_ActiveRollbackButton`, FUN_1405da000).
    pub rollback: bool,
}

/// Compétence enseignée par un instructeur.
pub struct TrainerEntry {
    pub tid: i32,
    pub name: String,
    /// Niveau connu (0 : pas apprise) et niveau maximal.
    pub level: i32,
    pub max_level: i32,
    /// Niveau du personnage exigé et prix de la prochaine étape (apprentissage ou amélioration).
    pub require_level: i32,
    pub cost: i64,
    /// Le personnage remplit les conditions (niveau, points, argent).
    pub possible: bool,
    /// Infobulle de la prochaine étape (voir `app_skills.rs`).
    pub tip: Vec<(String, Color32)>,
}

pub struct TrainerView<'a> {
    pub title: String,
    pub entries: &'a [TrainerEntry],
    pub selected: Option<i32>,
    pub points: Option<i32>,
    pub first: &'a mut usize,
    pub tab: SkillTab,
    pub allowed: &'a dyn Fn(SkillTab) -> bool,
}

/// Case de la barre : raccourci, nombre d'objets, part de relance restante, infobulle.
#[derive(Debug, Clone, Default)]
pub struct SlotView {
    pub shortcut: Option<Shortcut>,
    pub count: Option<i32>,
    pub cooldown: f32,
    pub left_s: u32,
    pub tip: Vec<(String, Color32)>,
}

fn shortcut_icon(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, cell: Rect, s: Shortcut, tint: Color32) {
    match s {
        Shortcut::Item(tid) => item_icon(ctx, hud, assets, painter, cell, Some(tid), tint),
        Shortcut::Skill(tid) => skill_icon(ctx, hud, assets, painter, cell, tid, tint),
    }
}

pub(super) fn skill_icon(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, cell: Rect, tid: i32, tint: Color32) {
    match hud.skill_icon(assets, tid).and_then(|n| texture_icon(ctx, hud, assets, &n)) {
        Some(t) => {
            painter.image(t, cell.shrink(1.0), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), tint);
        }
        None => {
            painter.rect_filled(cell.shrink(3.0), 3.0, Color32::from_rgb(40, 60, 90));
        }
    }
}

/// Voile de relance : la part restante du délai assombrit l'icône depuis le haut, avec les
/// secondes restantes au centre.
pub(super) fn cooldown_veil(painter: &egui::Painter, cell: Rect, part: f32, left_s: u32) {
    if part > 0.0 {
        let h = cell.height() * part.clamp(0.0, 1.0);
        painter.rect_filled(Rect::from_min_size(cell.min, Vec2::new(cell.width(), h)), 0.0, Color32::from_black_alpha(200));
        painter.hline(cell.x_range(), cell.min.y + h, egui::Stroke::new(1.0_f32, Color32::from_rgb(255, 210, 110)));
        if left_s > 0 {
            let text = if left_s >= 60 { format!("{}m", left_s.div_ceil(60)) } else { left_s.to_string() };
            shadow_text(painter, cell.center(), Align2::CENTER_CENTER, &text, (cell.height() * 0.32).max(9.0), Color32::from_rgb(255, 235, 170));
        }
    }
}

/// Grille de deux colonnes d'éléments de 208 × 56 dans `list`, avec défilement à la molette.
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
            (i, layout.rect(origin, [list.pos[0] + col as f32 * (item.size[0] + 2.0), list.pos[1] + row as f32 * item.size[1]], item.size))
        })
        .collect()
}

pub(super) fn frame_window<R>(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, w: &UiWindow, id: &str, add: impl FnOnce(&mut egui::Ui, &mut HudState, &Layout, Pos2, &egui::Painter) -> R) {
    let layout = Layout::new(ctx.screen_rect());
    let size = Vec2::new(w.size[0], w.size[1]) * layout.scale;
    // Sans fondu : chaque onglet à arbre est une autre fenêtre (`Action=1:2:0:1:UI_SkillBuy`, `1:1:0:1:UI_SkillHighLevelUp`
    // comme dans Archonia) ; le fondu d'apparition d'egui donnait l'impression d'un rechargement.
    egui::Area::new(egui::Id::new(id)).default_pos(layout.window_origin(w)).movable(true).constrain(true).fade_in(false).order(egui::Order::Middle).show(ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter().clone();
        match w.textures.first() {
            Some((_, tex)) if hud.image(ctx, assets, tex).is_some() => window_image(ctx, hud, assets, &painter, rect, w.size, tex),
            _ => {
                painter.rect_filled(rect, 4.0, Color32::from_black_alpha(220));
            }
        }
        add(ui, hud, &layout, rect.min, &painter);
    });
}

pub(super) fn x_button(ctx: &egui::Context, ui: &mut egui::Ui, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, r: Rect, c: &UiControl, id: &str) -> bool {
    let resp = ui.interact(r, egui::Id::new(id), egui::Sense::click());
    if let Some(tex) = button_texture(c).map(str::to_string) {
        control_image(ctx, hud, assets, painter, r, c.size, &tex, 1.0);
    }
    resp.clicked()
}

/// Livre des compétences : icône, nom et niveau ; clic droit pour lancer, glisser vers la barre.
pub fn draw_skill_book(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: BookView, out: &mut Vec<SkillUi>) {
    let window = view.tab.book_window();
    let Some(w) = hud.windows(assets).iter().find(|w| w.name == window).cloned() else { return };
    let ctl = |n: &str| w.controls.iter().find(|c| c.name == n).cloned();
    let (Some(list), Some(item)) = (ctl("CONTROL_SkillList"), ctl("CONTROL_SkillListItem")) else { return };
    let back = item.textures.first().map(|(_, t)| t.clone());
    let box_tex = ctl("CONTROL_16100").and_then(|c| c.textures.first().map(|(_, t)| t.clone()));
    // Libellé fixe « Niveau » (`CONTROL_14935`) et niveau (`CONTROL_28200`, « SkillLevel » : « %d ») de chaque ligne.
    let level_label = ctl("CONTROL_14935").map(|c| c.fixed_label().to_string()).unwrap_or_default();
    frame_window(ctx, hud, assets, &w, &format!("archonia-skill-book-{window}"), |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        if let Some(tab) = draw_tabs(ctx, ui, hud, assets, painter, layout, origin, &w, view.tab.is_scion(), view.tab, view.allowed) {
            out.push(SkillUi::Tab(tab));
        }
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match c.name.as_str() {
                "CONTROL_2968" => shadow_text(painter, r.left_center(), Align2::LEFT_CENTER, c.fixed_label(), font * 1.1, Color32::from_rgb(255, 230, 160)),
                "CONTROL_21134" => {
                    if x_button(ctx, ui, hud, assets, painter, r, c, &format!("skill-book-x-{window}")) {
                        out.push(SkillUi::CloseBook);
                    }
                }
                "CONTROL_28776" => shadow_text(painter, r.left_center(), Align2::LEFT_CENTER, c.fixed_label(), font, Color32::from_rgb(235, 225, 200)),
                "CONTROL_12489" => shadow_text(painter, r.center(), Align2::CENTER_CENTER, &view.points.map_or(String::new(), |p| p.to_string()), font, Color32::from_rgb(255, 230, 160)),
                // Nom de la classe (`UI_Skill`) ou « AL » (`UI_ScionArchlordSkill`).
                "CONTROL_32752" => {
                    let text = c.display.as_deref().and_then(|d| d.strip_prefix("1:")).map_or(view.title.clone(), str::to_string);
                    shadow_text(painter, r.center(), Align2::CENTER_CENTER, &text, font, Color32::from_rgb(255, 230, 160));
                }
                "CONTROL_1134" => {
                    if super::npc_ui::button(ctx, ui, hud, assets, painter, r, c, c.fixed_label(), font, true, ("skill-book-close", window)).clicked() {
                        out.push(SkillUi::CloseBook);
                    }
                }
                // « Reset Selected » (`Skill_Rollback`), actif selon `Skill_ActiveRollbackButton`.
                "CONTROL_12212" => {
                    let enabled = view.rollback && view.selected.is_some();
                    if super::npc_ui::button(ctx, ui, hud, assets, painter, r, c, c.fixed_label(), font, enabled, ("skill-book-reset", window)).clicked() {
                        if let Some(tid) = view.selected.filter(|_| enabled) {
                            out.push(SkillUi::Rollback(tid));
                        }
                    }
                }
                _ => {}
            }
        }
        let dragging = egui::DragAndDrop::has_any_payload(ctx);
        for (i, cell) in list_cells(ui, layout, origin, &list, &item, view.entries.len(), view.first) {
            let e = &view.entries[i];
            if let Some(t) = &back {
                control_image(ctx, hud, assets, painter, cell, item.size, t, 1.0);
            }
            let icon_rect = Rect::from_min_size(cell.min + Vec2::new(3.0, 3.0) * s, Vec2::splat(50.0 * s));
            if let Some(t) = &box_tex {
                control_image(ctx, hud, assets, painter, Rect::from_min_size(cell.min + Vec2::splat(s), Vec2::splat(52.0 * s)), [52.0, 52.0], t, 1.0);
            }
            skill_icon(ctx, hud, assets, painter, icon_rect, e.tid, Color32::WHITE);
            cooldown_veil(painter, icon_rect, e.cooldown, e.left_s);
            shadow_text(painter, cell.min + Vec2::new(67.0, 10.0) * s, Align2::LEFT_TOP, &e.name, font, Color32::WHITE);
            shadow_text(painter, cell.min + Vec2::new(67.0, 30.0) * s, Align2::LEFT_TOP, &level_label, font, Color32::from_rgb(255, 230, 160));
            shadow_text(painter, cell.min + Vec2::new(103.0, 30.0) * s, Align2::LEFT_TOP, &e.level.to_string(), font, Color32::from_rgb(255, 230, 160));
            let resp = ui.interact(cell, egui::Id::new(("skill-book", e.tid)), egui::Sense::click_and_drag());
            if resp.dragged() {
                resp.dnd_set_drag_payload(SkillDrag { tid: e.tid });
            }
            if resp.secondary_clicked() || resp.double_clicked() {
                out.push(SkillUi::Cast(e.tid));
            } else if resp.clicked() {
                out.push(SkillUi::BookSelect(e.tid));
            }
            if Some(e.tid) == view.selected {
                painter.rect_stroke(cell, 2.0, egui::Stroke::new(1.5_f32, Color32::from_rgb(255, 210, 110)), egui::StrokeKind::Inside);
            }
            if resp.hovered() && !dragging {
                painter.rect_stroke(cell, 2.0, egui::Stroke::new(1.0_f32, Color32::from_rgb(120, 200, 255)), egui::StrokeKind::Inside);
                super::item_tooltip(ctx, &e.tip, "");
            }
        }
    });
}

/// Instructeur : compétences de sa classe, niveau requis, prix ; « Apprendre » ou « Améliorer ».
pub fn draw_trainer(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: TrainerView, out: &mut Vec<SkillUi>) {
    let Some(w) = hud.windows(assets).iter().find(|w| w.name == "UI_SkillBuy").cloned() else { return };
    let ctl = |n: &str| w.controls.iter().find(|c| c.name == n).cloned();
    let (Some(list), Some(item)) = (ctl("CONTROL_22502"), ctl("CONTROL_10080")) else { return };
    let back = item.textures.first().map(|(_, t)| t.clone());
    let disabled = ctl("CONTROL_Disable").and_then(|c| c.textures.first().map(|(_, t)| t.clone()));
    let selected = view.entries.iter().find(|e| Some(e.tid) == view.selected);
    let (level_word, money_name) = (crate::hud::ui_message(hud, assets, "SkillLevel"), crate::hud::ui_message(hud, assets, "Money_Name"));
    frame_window(ctx, hud, assets, &w, "archonia-skill-trainer", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        // Onglets Combat… Passive (`SkillSetBuyMastery0` à `3`), Combo (`UI_SkillHighLevelUp`), Heroic
        // (`UI_HeroicSkill_Upgrade`).
        if let Some(tab) = draw_tabs(ctx, ui, hud, assets, painter, layout, origin, &w, false, view.tab, view.allowed) {
            out.push(SkillUi::TrainerTab(tab));
        }
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match c.name.as_str() {
                "CONTROL_1140" => shadow_text(painter, r.center(), Align2::CENTER_CENTER, &view.title, font, Color32::from_rgb(255, 230, 160)),
                "CONTROL_20244" => {
                    if x_button(ctx, ui, hud, assets, painter, r, c, "skill-trainer-x") {
                        out.push(SkillUi::CloseTrainer);
                    }
                }
                "CONTROL_24921" => shadow_text(painter, r.left_center(), Align2::LEFT_CENTER, c.fixed_label(), font, Color32::from_rgb(235, 225, 200)),
                "CONTROL_9296" => shadow_text(painter, r.center(), Align2::CENTER_CENTER, &view.points.map_or(String::new(), |p| p.to_string()), font, Color32::from_rgb(255, 230, 160)),
                // Bouton d'action : apprendre ou améliorer la compétence choisie. Libellé de « SkillBuyButton »
                // (FUN_1405d8030) : uimessage `SkillBuy`, `SkillUpgrade` si la compétence choisie est connue.
                "CONTROL_18446" => {
                    let (key, enabled) = match selected {
                        Some(e) if e.level == 0 => ("SkillBuy", e.possible),
                        Some(e) => ("SkillUpgrade", e.possible && e.level < e.max_level),
                        None => ("SkillBuy", false),
                    };
                    let label = crate::hud::ui_message(hud, assets, key);
                    let resp = super::npc_ui::button(ctx, ui, hud, assets, painter, r, c, &label, font, enabled, "skill-trainer-do");
                    if resp.clicked() {
                        if let Some(e) = selected {
                            // `Skill_Buy` (FUN_1405d7c00) : confirmation, d'achat ou d'amélioration selon la compétence.
                            out.push(SkillUi::TrainerBuy(e.tid, e.level == 0));
                        }
                    }
                }
                "CONTROL_17539" => {
                    if super::npc_ui::button(ctx, ui, hud, assets, painter, r, c, c.fixed_label(), font, true, "skill-trainer-close").clicked() {
                        out.push(SkillUi::CloseTrainer);
                    }
                }
                _ => {}
            }
        }
        for (i, cell) in list_cells(ui, layout, origin, &list, &item, view.entries.len(), view.first) {
            let e = &view.entries[i];
            if let Some(t) = &back {
                control_image(ctx, hud, assets, painter, cell, item.size, t, 1.0);
            }
            let icon_rect = Rect::from_min_size(cell.min + Vec2::new(3.0, 3.0) * s, Vec2::splat(50.0 * s));
            skill_icon(ctx, hud, assets, painter, icon_rect, e.tid, Color32::WHITE);
            // Nom (`CONTROL_5918`), « SkillLevel2 » (`CONTROL_4297`, « %s %d » : uimessage `SkillLevel`, niveau) et
            // « SkillPrice » (`CONTROL_24573`, « %d %s » : prix, `Money_Name`) ; FUN_1405d8200, FUN_1405d9cc0.
            // Compétence pas encore apprise : FUN_1405d8200 ne trouve pas la compétence du personnage
            // (FUN_1402f3f30 nul) et n'écrit rien.
            let state = if e.level > 0 { format!("{} {}", level_word, e.level) } else { String::new() };
            shadow_text(painter, cell.min + Vec2::new(62.0, 10.0) * s, Align2::LEFT_TOP, &e.name, font, Color32::WHITE);
            shadow_text(painter, cell.min + Vec2::new(62.0, 30.0) * s, Align2::LEFT_TOP, &state, font * 0.95, Color32::from_rgb(255, 230, 160));
            shadow_text(painter, cell.min + Vec2::new(185.0, 30.0) * s, Align2::RIGHT_TOP, &format!("{} {}", e.cost.max(0), money_name), font * 0.95, Color32::from_rgb(255, 220, 120));
            if !e.possible {
                if let Some(t) = &disabled {
                    control_image(ctx, hud, assets, painter, Rect::from_min_size(cell.min + Vec2::splat(2.0 * s), Vec2::new(196.0, 52.0) * s), [196.0, 52.0], t, 1.0);
                }
            }
            let resp = ui.interact(cell, egui::Id::new(("skill-trainer", e.tid)), egui::Sense::click());
            if resp.clicked() {
                out.push(SkillUi::SelectTrainer(e.tid));
            }
            if Some(e.tid) == view.selected {
                painter.rect_stroke(cell, 2.0, egui::Stroke::new(1.5_f32, Color32::from_rgb(255, 210, 110)), egui::StrokeKind::Inside);
            } else if resp.hovered() {
                painter.rect_stroke(cell, 2.0, egui::Stroke::new(1.0_f32, Color32::from_rgb(120, 200, 255)), egui::StrokeKind::Inside);
            }
            if resp.hovered() && !e.tip.is_empty() {
                super::item_tooltip(ctx, &e.tip, "");
            }
        }
    });
}

/// Course des curseurs de prise automatique (`Scroll=0:0:350` de `CONTROL_9745`).
const AUTO_TRACK: f32 = 350.0;

/// Cinq cases d'une grille de barre (`Grid=0:0:2:0:50:50`).
fn grid_cells(layout: Layout, origin: Pos2, c: &UiControl) -> impl Iterator<Item = Rect> + '_ {
    let [gx, _, cw, ch] = c.grid.unwrap_or([2.0, 0.0, 50.0, 50.0]);
    (0..5).map(move |k| layout.rect(origin, [c.pos[0] + k as f32 * (cw + gx), c.pos[1]], [cw, ch]))
}

/// Barre de raccourcis : potions et dix cases posées sur les grilles de `UI_CharInfo`, puis les
/// barres en plus (`UI_QuickBelt_n`) que `belts` demande. Archonia les centre en bas de l'écran,
/// empilées au-dessus de la barre : x = (largeur écran − 520) / 2, y = hauteur écran − 52 × n − 63
/// (FUN_140612b40, FUN_140612ca0).
pub fn draw_quickbar(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, slots: &[SlotView], belts: usize, auto: [u8; 2], out: &mut Vec<SkillUi>) {
    let Some(w) = hud.windows(assets).iter().find(|w| w.name == "UI_CharInfo").cloned() else { return };
    let layout = Layout::new(ctx.screen_rect());
    let origin = layout.window_origin(&w);
    let s = layout.scale;
    // Cases : potions (1, 2), puis 5 à gauche et 5 à droite.
    let mut cells: Vec<(usize, Rect)> = Vec::new();
    for name in ["CONTROL_20778", "CONTROL_28144"] {
        if let Some(c) = w.controls.iter().find(|c| c.name == name) {
            cells.push((cells.len(), layout.rect(origin, c.pos, c.size)));
        }
    }
    for name in ["CONTROL_GRID_LEFT_0", "CONTROL_GRID_RIGHT_0"] {
        if let Some(c) = w.controls.iter().find(|c| c.name == name) {
            for r in grid_cells(layout, origin, c) {
                cells.push((cells.len(), r));
            }
        }
    }
    if cells.len() != 12 {
        return;
    }
    let font = (10.0 * s).max(8.0);
    // Flèches de la barre et nombre de barres (`CONTROL_1735`, `QBelt_Layer`).
    for (name, delta) in [("CONTROL_20969", 1i8), ("CONTROL_9156", -1)] {
        let Some(c) = w.control(name) else { continue };
        let r = layout.rect(origin, c.pos, c.size);
        egui::Area::new(egui::Id::new(("archrust-quickbelt-arrow", name))).fixed_pos(r.min).order(egui::Order::Foreground).show(ctx, |ui| {
            let painter = ui.painter().clone();
            let enabled = if delta > 0 { belts < EXTRA_BELTS } else { belts > 0 };
            if super::npc_ui::button(ctx, ui, hud, assets, &painter, r, c, "", font, enabled, ("quickbelt-arrow", name)).clicked() && enabled {
                out.push(SkillUi::Belts(delta));
            }
        });
    }
    if let Some(c) = w.control("CONTROL_1735") {
        let r = layout.rect(origin, c.pos, c.size);
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("archrust-quickbelt-layer")));
        shadow_text(&painter, r.center(), Align2::CENTER_CENTER, &(belts + 1).to_string(), font, Color32::from_rgb(255, 230, 160));
    }
    // Barres en plus : fond, touches (« Shift+3 »…) et cases.
    let screen = layout.screen;
    for b in 1..=belts.min(EXTRA_BELTS) {
        let Some(bw) = hud.windows(assets).iter().find(|x| x.name == QUICKBELT_WINDOWS[b - 1]).cloned() else { continue };
        let bo = Pos2::new(screen.center().x - bw.size[0] * s / 2.0, screen.bottom() - (bw.size[1] * b as f32 + 63.0) * s);
        egui::Area::new(egui::Id::new(("archrust-quickbelt", b))).fixed_pos(bo).order(egui::Order::Middle).show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(bw.size[0], bw.size[1]) * s, egui::Sense::hover());
            let painter = ui.painter().clone();
            if let Some((_, tex)) = bw.textures.first() {
                window_image(ctx, hud, assets, &painter, rect, bw.size, tex);
            }
            for c in bw.controls.iter().filter(|c| c.kind == 1) {
                if let Some(label) = c.display.as_deref().and_then(|d| d.strip_prefix("1:")).filter(|l| !l.is_empty()) {
                    let r = layout.rect(bo, c.pos, c.size);
                    shadow_text(&painter, r.left_top() + Vec2::new(2.0, 1.0) * s, Align2::LEFT_TOP, label, font, Color32::from_rgb(230, 230, 230));
                }
            }
        });
        for (half, name) in [(0, format!("CONTROL_GRID_LEFT_{b}")), (5, format!("CONTROL_GRID_RIGHT_{b}"))] {
            if let Some(c) = bw.control(&name) {
                for (k, r) in grid_cells(layout, bo, c).enumerate() {
                    cells.push((12 + (b - 1) * 10 + half + k, r));
                }
            }
        }
    }
    let item_drag = egui::DragAndDrop::payload::<ItemDrag>(ctx).map(|d| *d);
    let skill_drag = egui::DragAndDrop::payload::<SkillDrag>(ctx).map(|d| *d);
    let slot_drag = egui::DragAndDrop::payload::<SlotDrag>(ctx).map(|d| *d);
    let dragging = item_drag.is_some() || skill_drag.is_some() || slot_drag.is_some();
    for &(slot, cell) in &cells {
        let cell = &cell;
        egui::Area::new(egui::Id::new(("archrust-quickbar", slot))).fixed_pos(cell.min).order(egui::Order::Foreground).show(ctx, |ui| {
            let (_, resp) = ui.allocate_exact_size(cell.size(), egui::Sense::click_and_drag());
            let painter = ui.painter();
            let v = slots.get(slot).cloned().unwrap_or_default();
            if resp.hovered() && !dragging && !v.tip.is_empty() {
                super::item_tooltip(ctx, &v.tip, "");
            }
            if let Some(sc) = v.shortcut {
                let lifted = slot_drag.is_some_and(|d| d.slot == slot);
                shortcut_icon(ctx, hud, assets, painter, *cell, sc, if lifted { Color32::from_white_alpha(70) } else { Color32::WHITE });
                cooldown_veil(painter, *cell, v.cooldown, v.left_s);
                if let Some(n) = v.count {
                    shadow_text(painter, cell.right_bottom() - Vec2::new(2.0, 1.0) * s, Align2::RIGHT_BOTTOM, &n.to_string(), font, Color32::WHITE);
                }
                if resp.dragged() {
                    resp.dnd_set_drag_payload(SlotDrag { slot });
                }
                if resp.clicked() {
                    out.push(SkillUi::SlotUse(slot));
                }
            }
            if let Some(d) = resp.dnd_release_payload::<SkillDrag>() {
                out.push(SkillUi::SlotSet { slot, shortcut: Shortcut::Skill(d.tid) });
            } else if let Some(d) = resp.dnd_release_payload::<ItemDrag>() {
                if !d.bank {
                    out.push(SkillUi::SlotItem { slot, iid: d.iid, tid: d.tid });
                }
            } else if let Some(d) = resp.dnd_release_payload::<SlotDrag>() {
                if d.slot != slot {
                    out.push(SkillUi::SlotSwap(d.slot, slot));
                }
            }
            let target = (item_drag.is_some() || skill_drag.is_some() || slot_drag.is_some()) && resp.contains_pointer();
            if target {
                painter.rect_stroke(*cell, 2.0, egui::Stroke::new(1.5_f32, Color32::from_rgb(255, 210, 110)), egui::StrokeKind::Inside);
            }
        });
    }
    // Curseurs de prise automatique des potions, sur les jauges de PV et de PM.
    for (k, (bar, thumb, tip)) in [("CONTROL_9745", "CONTROL_5672", "Use Life Potion Automatically"), ("CONTROL_24256", "CONTROL_3123", "Use Mana Potion Automatically")].into_iter().enumerate() {
        let (Some(b), Some(t)) = (w.controls.iter().find(|c| c.name == bar), w.controls.iter().find(|c| c.name == thumb)) else { continue };
        let track = layout.rect(origin, b.pos, b.size);
        let id = egui::Id::new(("archrust-auto-potion", k));
        let mut value = ctx.data(|d| d.get_temp::<f32>(id)).unwrap_or(auto[k] as f32);
        egui::Area::new(id).fixed_pos(track.min).order(egui::Order::Foreground).show(ctx, |ui| {
            let (_, resp) = ui.allocate_exact_size(track.size(), egui::Sense::click_and_drag());
            if resp.dragged() || resp.clicked() {
                if let Some(p) = resp.interact_pointer_pos() {
                    value = (((p.x - track.left()) / s - t.size[0] / 2.0) / AUTO_TRACK * 100.0).clamp(0.0, 100.0);
                    ctx.data_mut(|d| d.insert_temp(id, value));
                }
            }
            if resp.drag_stopped() || resp.clicked() {
                ctx.data_mut(|d| d.remove::<f32>(id));
                out.push(SkillUi::AutoGauge { hp: k == 0, value: value.round() as u8 });
            }
            let resp = resp.on_hover_text(tip);
            let thumb_rect = layout.rect(origin, [b.pos[0] + value / 100.0 * AUTO_TRACK, b.pos[1]], t.size);
            if let Some((_, tex)) = t.textures.first() {
                control_image(ctx, hud, assets, ui.painter(), thumb_rect, t.size, &tex.clone(), 1.0);
            }
            let _ = resp;
        });
    }
    // Case glissée hors de la barre : vidée.
    if ctx.input(|i| i.pointer.any_released()) {
        if let Some(d) = egui::DragAndDrop::take_payload::<SlotDrag>(ctx) {
            let over_bar = ctx.pointer_latest_pos().is_some_and(|p| cells.iter().any(|(_, c)| c.contains(p)));
            if !over_bar {
                out.push(SkillUi::SlotClear(d.slot));
            }
        }
    }
    draw_shortcut_drag(ctx, hud, assets, slots);
}

/// Icône de la compétence (livre) ou de la case de barre saisie, sous le curseur, comme l'objet saisi
/// (`draw_drag`) : Archonia la fait suivre la souris jusqu'à la barre.
fn draw_shortcut_drag(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, slots: &[SlotView]) {
    let Some(pos) = ctx.pointer_latest_pos() else { return };
    let shortcut = if let Some(d) = egui::DragAndDrop::payload::<SkillDrag>(ctx) {
        Some(Shortcut::Skill(d.tid))
    } else if let Some(d) = egui::DragAndDrop::payload::<SlotDrag>(ctx) {
        slots.get(d.slot).and_then(|v| v.shortcut)
    } else {
        None
    };
    let Some(sc) = shortcut else { return };
    let size = 44.0 * Layout::new(ctx.screen_rect()).scale.max(0.6);
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("archrust-skill-drag")));
    shortcut_icon(ctx, hud, assets, &painter, Rect::from_center_size(pos, Vec2::splat(size)), sc, Color32::from_white_alpha(220));
}
