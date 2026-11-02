//! Fenêtres des maîtrises d'Archonia : arbres à cases fixes des compétences supérieures
//! (`UI_SkillHighLevel`, `UI_SkillHighLevelUp`), héroïques (`UI_HeroicSkill`, `UI_HeroicSkill_Upgrade`) et
//! du Dragon Scion (`UI_SkillDragonScion_*`, `UI_SkillUpDragonScion_*`), plus les onglets communs à ces
//! fenêtres, au livre (`UI_Skill`, `UI_ScionArchlordSkill`) et à l'instructeur (`UI_SkillBuy`).
//!
//! Chaque case est une grille (`CtrlType=8`) dont le nom finit par son numéro (`SkillHighLevelGrid0`,
//! `SkillHeroicUserData_3`, `SkillScionUserData_12`…) ; le niveau connu s'écrit dans le texte dont l'affichage
//! nomme la même case (`SkillPointHighLevel0`, `Skill_Heroic_Display_3`…). Une case apprise porte le cadre
//! `Skill_ActiveBox.png`, une case non apprise le voile `Skill_DisableBox2.png` (contrôles `…_ACTIVE_n` et
//! `…_DISABLE_n`, statut 0/1 d'Archonia, FUN_1405dfc60). Les demandes reviennent au moteur en [`SkillUi`].

use super::skills_ui::{frame_window, skill_icon, x_button, SkillDrag, SkillUi};
use super::*;

/// Fenêtres chargées par le HUD.
pub(super) const MASTERY_WINDOWS: [&str; 16] = [
    "UI_SkillHighLevel",
    "UI_SkillHighLevelUp",
    "UI_HeroicSkill",
    "UI_HeroicSkill_Upgrade",
    "UI_SkillDragonScion_Scion",
    "UI_SkillDragonScion_Sla",
    "UI_SkillDragonScion_Obiter",
    "UI_SkillDragonScion_Sum",
    "UI_SkillDragonScion_Pas",
    "UI_SkillDragonScion_Heroic",
    "UI_SkillUpDragonScion_Scion",
    "UI_SkillUpDragonScion_Sla",
    "UI_SkillUpDragonScion_Obiter",
    "UI_SkillUpDragonScion_Sum",
    "UI_SkillUpDragonScion_Pas",
    "UI_SkillUpDragonScion_Heroic",
];

/// Fenêtre de la liste des compétences d'ArchLord du Dragon Scion (dessinée comme le livre).
pub(super) const SCION_ARCHLORD_WINDOW: &str = "UI_ScionArchlordSkill";

/// Onglets des fenêtres de compétences (libellés `CtrlDisplay` des boutons d'onglet).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SkillTab {
    #[default]
    Combat,
    Buff,
    Class,
    Passive,
    /// « Combo » : compétences supérieures (`SkillSetMasteryHighLevel`).
    Combo,
    Heroic,
    /// Compétences de l'ArchLord (`SkillSetMastery4`).
    Al,
    Scion,
    Slayer,
    Orbiter,
    Summoner,
    ScionPassive,
    ScionHeroic,
    ScionAl,
}

impl SkillTab {
    /// Catégorie de `SKILLMASTERY.txt` des onglets à liste (0 Battle… 3 Passive).
    pub fn category(self) -> Option<usize> {
        match self {
            SkillTab::Combat => Some(0),
            SkillTab::Buff => Some(1),
            SkillTab::Class => Some(2),
            SkillTab::Passive => Some(3),
            _ => None,
        }
    }

    pub fn is_scion(self) -> bool {
        matches!(self, SkillTab::Scion | SkillTab::Slayer | SkillTab::Orbiter | SkillTab::Summoner | SkillTab::ScionPassive | SkillTab::ScionHeroic | SkillTab::ScionAl)
    }

    /// Onglet d'un bouton d'après son libellé ; « Passive », « Heroic » et « AL » diffèrent dans les fenêtres du
    /// Scion.
    pub fn from_label(label: &str, scion: bool) -> Option<SkillTab> {
        Some(match (label.trim(), scion) {
            ("Combat", _) => SkillTab::Combat,
            ("Buff", _) => SkillTab::Buff,
            ("Class", _) => SkillTab::Class,
            ("Passive", false) => SkillTab::Passive,
            ("Passive", true) => SkillTab::ScionPassive,
            ("Combo", _) => SkillTab::Combo,
            ("Heroic", false) => SkillTab::Heroic,
            ("Heroic", true) => SkillTab::ScionHeroic,
            ("AL", false) => SkillTab::Al,
            ("AL", true) => SkillTab::ScionAl,
            ("Scion", _) => SkillTab::Scion,
            ("Slayer", _) => SkillTab::Slayer,
            ("Orbiter", _) => SkillTab::Orbiter,
            ("Summoner", _) => SkillTab::Summoner,
            _ => return None,
        })
    }

    /// Fenêtre du livre (touche K) pour cet onglet.
    pub fn book_window(self) -> &'static str {
        match self {
            SkillTab::Combat | SkillTab::Buff | SkillTab::Class | SkillTab::Passive | SkillTab::Al => "UI_Skill",
            SkillTab::Combo => "UI_SkillHighLevel",
            SkillTab::Heroic => "UI_HeroicSkill",
            SkillTab::Scion => "UI_SkillDragonScion_Scion",
            SkillTab::Slayer => "UI_SkillDragonScion_Sla",
            SkillTab::Orbiter => "UI_SkillDragonScion_Obiter",
            SkillTab::Summoner => "UI_SkillDragonScion_Sum",
            SkillTab::ScionPassive => "UI_SkillDragonScion_Pas",
            SkillTab::ScionHeroic => "UI_SkillDragonScion_Heroic",
            SkillTab::ScionAl => SCION_ARCHLORD_WINDOW,
        }
    }

    /// Fenêtre de l'instructeur pour cet onglet (`None` : pas de fenêtre d'instructeur, ex. AL).
    pub fn trainer_window(self) -> Option<&'static str> {
        Some(match self {
            SkillTab::Combat | SkillTab::Buff | SkillTab::Class | SkillTab::Passive => "UI_SkillBuy",
            SkillTab::Combo => "UI_SkillHighLevelUp",
            SkillTab::Heroic => "UI_HeroicSkill_Upgrade",
            SkillTab::Scion => "UI_SkillUpDragonScion_Scion",
            SkillTab::Slayer => "UI_SkillUpDragonScion_Sla",
            SkillTab::Orbiter => "UI_SkillUpDragonScion_Obiter",
            SkillTab::Summoner => "UI_SkillUpDragonScion_Sum",
            SkillTab::ScionPassive => "UI_SkillUpDragonScion_Pas",
            SkillTab::ScionHeroic => "UI_SkillUpDragonScion_Heroic",
            SkillTab::Al | SkillTab::ScionAl => return None,
        })
    }

    /// Arbre à cases fixes (et non liste) : supérieur, héroïque, Dragon Scion.
    pub fn is_tree(self) -> bool {
        !matches!(self, SkillTab::Combat | SkillTab::Buff | SkillTab::Class | SkillTab::Passive | SkillTab::Al | SkillTab::ScionAl)
    }

    /// Arbre héroïque (points héroïques au lieu des points de compétence).
    pub fn heroic(self) -> bool {
        matches!(self, SkillTab::Heroic | SkillTab::ScionHeroic)
    }

    /// (race, classe) du gabarit dont l'arbre du Dragon Scion est montré (Scion 4/3, Slayer 4/1, Orbiter
    /// 4/2, Summoner 4/4).
    pub fn scion_form(self) -> Option<(i32, i32)> {
        match self {
            SkillTab::Scion => Some((4, 3)),
            SkillTab::Slayer => Some((4, 1)),
            SkillTab::Orbiter => Some((4, 2)),
            SkillTab::Summoner => Some((4, 4)),
            _ => None,
        }
    }
}

/// Numéro final d'un nom (`SkillHeroicUserData_12` → 12).
fn trailing_index(name: &str) -> Option<usize> {
    let digits: String = name.chars().rev().take_while(|c| c.is_ascii_digit()).collect::<Vec<_>>().into_iter().rev().collect();
    digits.parse().ok()
}

/// Case d'un arbre : grille dont le nom finit par un numéro (les listes `SkillListGrid`… n'en ont pas).
pub(super) fn tree_slot(c: &UiControl) -> Option<usize> {
    (c.kind == 8).then_some(())?;
    trailing_index(c.grid_name.as_deref()?)
}

/// Texte du niveau d'une case : `CtrlDisplay=2:1:SkillPointHighLevel3 …`, `2:1:Skill_Scion_Display_3 …`.
pub(super) fn level_slot(c: &UiControl) -> Option<usize> {
    let token = c.display.as_deref()?.strip_prefix("2:1:")?.split_whitespace().next()?;
    (token.contains("HighLevel") || token.contains("_Display_")).then_some(())?;
    trailing_index(token)
}

/// Onglets d'une fenêtre (boutons dont le libellé est un onglet, posés sur la rangée des onglets) : dessinés
/// avec `INV_TAB_SEL.png` pour l'onglet courant ; rend l'onglet cliqué.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_tabs(ctx: &egui::Context, ui: &mut egui::Ui, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, layout: &Layout, origin: Pos2, w: &UiWindow, scion: bool, current: SkillTab, allowed: &dyn Fn(SkillTab) -> bool) -> Option<SkillTab> {
    let font = (11.0 * layout.scale).max(9.0);
    let mut clicked = None;
    for c in w.controls.iter().filter(|c| c.kind == 3 && (60.0..100.0).contains(&c.pos[1])) {
        let Some(label) = c.display.as_deref().and_then(|d| d.strip_prefix("1:")) else { continue };
        let Some(tab) = SkillTab::from_label(label, scion) else { continue };
        let r = layout.rect(origin, c.pos, c.size);
        let selected = tab == current;
        let tex = c.textures.iter().find(|(_, n)| n.to_ascii_lowercase().contains("_sel") == selected).map(|(_, n)| n.clone());
        if let Some(t) = tex {
            control_image(ctx, hud, assets, painter, r, c.size, &t, 1.0);
        }
        let enabled = allowed(tab);
        let resp = ui.interact(r, egui::Id::new(("skill-tab", &w.name, label)), if enabled && !selected { egui::Sense::click() } else { egui::Sense::hover() });
        let color = if selected { Color32::from_rgb(255, 230, 160) } else if !enabled { Color32::GRAY } else if resp.hovered() { Color32::WHITE } else { Color32::from_rgb(220, 210, 190) };
        shadow_text(painter, r.center(), Align2::CENTER_CENTER, c.fixed_label(), font, color);
        if resp.clicked() {
            clicked = Some(tab);
        }
    }
    clicked
}

/// Case d'un arbre de maîtrise.
#[derive(Debug, Clone, Default)]
pub struct TreeSlot {
    pub tid: i32,
    pub name: String,
    /// Niveau connu (0 : pas apprise) et niveau maximal.
    pub level: i32,
    pub max_level: i32,
    /// Fenêtre de l'instructeur : la prochaine étape est possible (niveau, points, conditions, argent).
    pub possible: bool,
    /// Part du délai de relance restante (livre seulement).
    pub cooldown: f32,
    pub tip: Vec<(String, Color32)>,
}

pub struct TreeView<'a> {
    pub tab: SkillTab,
    /// Fenêtre de l'instructeur (`UI_SkillHighLevelUp`…) plutôt que du livre.
    pub trainer: bool,
    /// Nom de la classe (texte `2:0:主_Character …` du titre).
    pub title: String,
    /// Cases dans l'ordre de l'arbre (`None` : case vide).
    pub slots: &'a [Option<TreeSlot>],
    /// Points de compétence, ou points héroïques dans un arbre héroïque.
    pub points: Option<i32>,
    pub selected: Option<i32>,
    /// Onglets ouvrables (ex. Heroic : arbre héroïque connu pour la classe).
    pub allowed: &'a dyn Fn(SkillTab) -> bool,
}

/// Fenêtre d'un arbre de maîtrise (livre ou instructeur).
pub fn draw_tree(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: TreeView, out: &mut Vec<SkillUi>) {
    let name = if view.trainer { view.tab.trainer_window() } else { Some(view.tab.book_window()) };
    let Some(w) = name.and_then(|n| hud.windows(assets).iter().find(|w| w.name == n).cloned()) else { return };
    let active = w.controls.iter().flat_map(|c| c.textures.iter()).find(|(_, n)| n.eq_ignore_ascii_case("Skill_ActiveBox.png")).map(|(_, n)| n.clone());
    let disable = w.controls.iter().flat_map(|c| c.textures.iter()).find(|(_, n)| n.eq_ignore_ascii_case("Skill_DisableBox2.png")).map(|(_, n)| n.clone());
    let selected = view.selected.and_then(|tid| view.slots.iter().flatten().find(|s| s.tid == tid));
    let close = if view.trainer { SkillUi::CloseTrainer } else { SkillUi::CloseBook };
    frame_window(ctx, hud, assets, &w, &format!("archonia-{}", w.name), |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let scion = view.tab.is_scion();
        if let Some(tab) = draw_tabs(ctx, ui, hud, assets, painter, layout, origin, &w, scion, view.tab, view.allowed) {
            out.push(if view.trainer { SkillUi::TrainerTab(tab) } else { SkillUi::Tab(tab) });
        }
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            let display = c.display.as_deref().unwrap_or("");
            match c.kind {
                // Fond de l'arbre du Scion (`Skill_Tree_DragonScion_*.png` de la liste).
                10 => {
                    if let Some((_, t)) = c.textures.first() {
                        control_image(ctx, hud, assets, painter, r, c.size, &t.clone(), 1.0);
                    }
                }
                1 | 8 => {
                    if display.starts_with("1:") {
                        // Alignement de `CtrlFont` (17 : centré).
                        let (pos, align) = if c.font.is_some_and(|f| f.1 == 17) { (r.center(), Align2::CENTER_CENTER) } else { (r.left_center(), Align2::LEFT_CENTER) };
                        shadow_text(painter, pos, align, c.fixed_label(), font, Color32::from_rgb(235, 225, 200));
                    } else if display.contains("Skillpoint") || display.contains("SkillPoint SkillPoint") {
                        let text = view.points.map_or(String::new(), |p| p.to_string());
                        shadow_text(painter, r.center(), Align2::CENTER_CENTER, &text, font, Color32::from_rgb(255, 230, 160));
                    } else if display.starts_with("2:0:") {
                        shadow_text(painter, r.center(), Align2::CENTER_CENTER, &view.title, font, Color32::from_rgb(255, 230, 160));
                    } else if let Some(k) = level_slot(c) {
                        if let Some(Some(slot)) = view.slots.get(k) {
                            shadow_text(painter, r.center(), Align2::CENTER_CENTER, &slot.level.to_string(), font, Color32::from_rgb(255, 230, 160));
                        }
                    }
                }
                3 if (60.0..100.0).contains(&c.pos[1]) && SkillTab::from_label(display.strip_prefix("1:").unwrap_or(""), scion).is_some() => {}
                3 => {
                    let action = c.action.as_deref().unwrap_or("");
                    if c.size == [16.0, 16.0] && c.pos[1] < 10.0 {
                        if !c.name.is_empty() && x_button(ctx, ui, hud, assets, painter, r, c, &format!("tree-x-{}", w.name)) {
                            out.push(close);
                        }
                    } else if display == "1:Close" {
                        if super::npc_ui::button(ctx, ui, hud, assets, painter, r, c, c.fixed_label(), font, true, ("tree-close", &w.name)).clicked() {
                            out.push(close);
                        }
                    } else if action.ends_with("_Upgrade") || display == "1:Learn" || action == "Skill_Upgrade" {
                        // Apprendre ou améliorer la case choisie (instructeur). Libellé : texte fixe du bouton
                        // (« Learn ») ou « SkillBuyButton » (FUN_1405d8030 : uimessage `SkillBuy`, `SkillUpgrade` si
                        // la compétence choisie est connue).
                        let enabled = selected.is_some_and(|e| e.possible && e.level < e.max_level);
                        let label = if !c.fixed_label().is_empty() {
                            c.fixed_label().to_string()
                        } else if display.contains("SkillBuyButton") {
                            crate::hud::ui_message(hud, assets, if selected.is_some_and(|e| e.level > 0) { "SkillUpgrade" } else { "SkillBuy" })
                        } else {
                            String::new()
                        };
                        if super::npc_ui::button(ctx, ui, hud, assets, painter, r, c, &label, font, enabled, ("tree-learn", &w.name)).clicked() {
                            if let Some(e) = selected {
                                out.push(SkillUi::TreeLearn(e.tid));
                            }
                        }
                    } else if display == "1:Reset Selected" || display == "1:Reset" {
                        // Remise à zéro d'une compétence (`Skill_Rollback`) et `UI_CharacterLoadouts` : ajouts
                        // d'Archonia, laissés inactifs.
                        super::npc_ui::button(ctx, ui, hud, assets, painter, r, c, c.fixed_label(), font, false, ("tree-reset", &w.name));
                    }
                }
                _ => {}
            }
        }
        let dragging = egui::DragAndDrop::has_any_payload(ctx);
        for c in &w.controls {
            let Some(k) = tree_slot(c) else { continue };
            let Some(Some(slot)) = view.slots.get(k) else { continue };
            let cell = layout.rect(origin, c.pos, c.size);
            let frame = cell.expand(s);
            if slot.level > 0 || (view.trainer && slot.possible) {
                if let Some(t) = &active {
                    control_image(ctx, hud, assets, painter, frame, [52.0, 52.0], t, 1.0);
                }
            }
            skill_icon(ctx, hud, assets, painter, cell, slot.tid, Color32::WHITE);
            if slot.level == 0 && !(view.trainer && slot.possible) {
                if let Some(t) = &disable {
                    control_image(ctx, hud, assets, painter, frame, [52.0, 52.0], t, 1.0);
                }
            }
            super::skills_ui::cooldown_veil(painter, cell, slot.cooldown, 0);
            let resp = ui.interact(cell, egui::Id::new(("tree-slot", &w.name, k)), if view.trainer { egui::Sense::click() } else { egui::Sense::click_and_drag() });
            if resp.clicked() {
                out.push(SkillUi::TreeSelect(slot.tid));
            }
            if !view.trainer && slot.level > 0 {
                if resp.dragged() {
                    resp.dnd_set_drag_payload(SkillDrag { tid: slot.tid });
                }
                if resp.secondary_clicked() || resp.double_clicked() {
                    out.push(SkillUi::Cast(slot.tid));
                }
            }
            if Some(slot.tid) == view.selected {
                painter.rect_stroke(frame, 2.0, egui::Stroke::new(1.5_f32, Color32::from_rgb(255, 210, 110)), egui::StrokeKind::Inside);
            } else if resp.hovered() {
                painter.rect_stroke(frame, 2.0, egui::Stroke::new(1.0_f32, Color32::from_rgb(120, 200, 255)), egui::StrokeKind::Inside);
            }
            if resp.hovered() && !dragging && !slot.tip.is_empty() {
                super::item_tooltip(ctx, &slot.tip, "");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cases_et_niveaux() {
        let grid = |n: &str| UiControl { kind: 8, grid_name: Some(n.into()), ..Default::default() };
        assert_eq!(tree_slot(&grid("SkillHighLevelGrid10")), Some(10));
        assert_eq!(tree_slot(&grid("SkillScionUserData_17")), Some(17));
        assert_eq!(tree_slot(&grid("SkillListGrid")), None);
        let text = |d: &str| UiControl { kind: 1, display: Some(d.into()), ..Default::default() };
        assert_eq!(level_slot(&text("2:1:SkillPointHighLevel4 SkillPointHighLevel4")), Some(4));
        assert_eq!(level_slot(&text("2:1:Skill_Heroic_Display_9 SkillHeroic_5_1")), Some(9));
        assert_eq!(level_slot(&text("2:1:SkillListGrid SkillName")), None);
    }

    /// Fenêtres du fichier d'interface du client (copie déchiffrée d'ArchCore) : cases placées dans la fenêtre,
    /// celles du Scion relatives à la liste (18, 98).
    #[test]
    fn fenetres_du_client() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../ArchCore/data/client-ini/UI_1024X768.ini");
        let Ok(raw) = std::fs::read(path) else { return };
        let ui = IniTable::parse(&String::from_utf8_lossy(&raw));
        let count = |name: &str| UiWindow::parse(&ui, None, name).map(|w| w.controls.iter().filter_map(tree_slot).max().map_or(0, |m| m + 1));
        assert_eq!(MASTERY_WINDOWS.map(count), [11, 11, 10, 10, 18, 11, 12, 12, 15, 10, 18, 11, 12, 12, 15, 10].map(Some));
        let w = UiWindow::parse(&ui, None, "UI_SkillDragonScion_Scion").unwrap();
        let first = w.controls.iter().find(|c| tree_slot(c) == Some(0)).unwrap();
        assert_eq!(first.pos, [18.0 + 183.0, 98.0 + 13.0]);
        assert!(UiWindow::parse(&ui, None, SCION_ARCHLORD_WINDOW).is_some());
    }
}
