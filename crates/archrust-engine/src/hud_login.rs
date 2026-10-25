//! Écran de sélection des personnages d'Archonia (`UI_LG_CHARSELECT`, 1024 × 768) et
//! confirmation de suppression (`UI_ConfirmDeleteChar`), à la place des fenêtres egui.
//!
//! `UI_LG_CHARSELECT` est dessiné sur la scène 3D de sélection. L'écran 1024 × 768 est agrandi
//! sans déformation et centré, et le voile `LG_Base_1024.png` couvre toute la fenêtre. On y trouve :
//! - **Cadres de personnage** : `CTL_BTN_CHARINFO01…03` (`LG_CharInfo_Base_*`, 170 × 120), centre,
//!   gauche puis droite, avec nom, niveau (`LoginSlotCharLevel`) et XP.
//! - **Retour / Quitter** : en bas à gauche, sur `LG_2Button_Base.png`.
//! - **Créer / Supprimer** : en bas à droite, sur `LG_2Button_Base.png`.
//! - **« Jouer »** (`LoginConnect`) : bouton `CONTROL_8352` sur `LB_XXLargeButton_Base.png`.
//!
//! Écart : les boutons de compensation (`CTL_BTN_COMPEN*`) et les lignes guilde et hors-la-loi ne
//! sont pas affichés, ArchCore n'envoyant pas ces données.

use super::dialogs_ui::ui_message;
use super::npc_ui::{button, window};
use super::*;

/// Demande de l'écran de sélection.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectUi {
    Select(usize),
    Enter,
    Create,
    Delete,
    Back,
    Quit,
    /// Réponse à la confirmation de suppression.
    ConfirmDelete(bool),
}

/// Personnage d'un emplacement.
#[derive(Debug, Clone)]
pub struct CharSlotView {
    pub name: String,
    pub level: i32,
}

/// Placement d'un écran 1024 × 768 : agrandi sans déformation, centré.
struct Screen1024 {
    origin: Pos2,
    scale: f32,
}

impl Screen1024 {
    fn new(screen: Rect) -> Screen1024 {
        let scale = (screen.width() / 1024.0).min(screen.height() / 768.0);
        let origin = screen.center() - Vec2::new(1024.0, 768.0) * scale / 2.0;
        Screen1024 { origin, scale }
    }
    fn rect(&self, pos: [f32; 2], size: [f32; 2]) -> Rect {
        Rect::from_min_size(self.origin + Vec2::new(pos[0], pos[1]) * self.scale, Vec2::new(size[0], size[1]) * self.scale)
    }
}

/// Emplacement de chaque cadre : 01 au centre, 02 à gauche, 03 à droite.
const SLOTS: [(&str, &str, &str); 3] = [
    ("CTL_BTN_CHARINFO01", "CTL_ST_NAME01", "CTL_ST_LEVEL01"),
    ("CTL_BTN_CHARINFO02", "CTL_ST_NAME02", "CTL_ST_LEVEL02"),
    ("CTL_BTN_CHARINFO03", "CTL_ST_NAME03", "CTL_ST_LEVEL03"),
];

/// Écran de sélection ; `delete_confirm` affiche la confirmation de suppression par-dessus.
#[allow(clippy::too_many_arguments)]
pub fn draw_char_select(
    ctx: &egui::Context,
    hud: &mut HudState,
    assets: &Assets,
    slots: &[CharSlotView],
    selected: Option<usize>,
    busy: bool,
    status: &str,
    delete_confirm: bool,
    out: &mut Vec<SelectUi>,
) {
    let Some(w) = window(hud, assets, "UI_LG_CHARSELECT") else { return };
    let screen = ctx.screen_rect();
    let lay = Screen1024::new(screen);
    let s = lay.scale;
    let font = (11.0 * s).max(9.0);
    let play = ui_message(hud, assets, "LoginConnect");
    let level_word = ui_message(hud, assets, "LoginSlotCharLevel");
    egui::Area::new(egui::Id::new("archonia-char-select"))
        .fixed_pos(screen.min)
        .order(egui::Order::Background)
        .show(ctx, |ui| {
            ui.set_min_size(screen.size());
            let painter = ui.painter().clone();
            // Voile de l'écran de connexion, étiré sur toute la fenêtre.
            if let Some((_, tex)) = w.textures.first() {
                if let Some((id, _)) = hud.image(ctx, assets, tex) {
                    painter.image(id, screen, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                }
            }
            for c in &w.controls {
                let r = lay.rect(c.pos, c.size);
                let fixed = c.display.as_deref().and_then(|d| d.strip_prefix("1:"));
                match c.name.as_str() {
                    // Socles des boutons.
                    "CONTROL_5110" | "CONTROL_8984" | "CONTROL_28025" => {
                        if let Some((_, tex)) = c.textures.first() {
                            control_image(ctx, hud, assets, &painter, r, c.size, &tex.clone(), 1.0);
                        }
                    }
                    "CONTROL_8442" | "CONTROL_14053" | "CONTROL_2880" | "CONTROL_26112" | "CONTROL_8352" => {
                        let (label, enabled, req) = match c.name.as_str() {
                            "CONTROL_8442" => (fixed.unwrap_or(""), !busy, SelectUi::Back),
                            "CONTROL_14053" => (fixed.unwrap_or(""), true, SelectUi::Quit),
                            "CONTROL_2880" => (fixed.unwrap_or(""), !busy && slots.len() < 3, SelectUi::Create),
                            "CONTROL_26112" => (fixed.unwrap_or(""), !busy && selected.is_some_and(|i| i < slots.len()), SelectUi::Delete),
                            _ => (play.as_str(), !busy && selected.is_some_and(|i| i < slots.len()), SelectUi::Enter),
                        };
                        if button(ctx, ui, hud, assets, &painter, r, c, label, font, enabled, ("char-select", &c.name)).clicked() {
                            out.push(req);
                        }
                    }
                    _ => {}
                }
            }
            for (i, (frame, name_ctl, level_ctl)) in SLOTS.iter().enumerate() {
                let Some(slot) = slots.get(i) else { continue };
                let Some(c) = w.control(frame) else { continue };
                let r = lay.rect(c.pos, c.size);
                let resp = ui.interact(r, egui::Id::new(("char-slot", i)), egui::Sense::click());
                let pick = |suffix: &str| c.textures.iter().find(|(_, n)| n.to_ascii_lowercase().ends_with(suffix)).map(|(_, n)| n.clone());
                let tex = if selected == Some(i) {
                    pick("c.png")
                } else if resp.hovered() {
                    pick("b.png")
                } else {
                    pick("a.png")
                };
                if let Some(tex) = tex.or_else(|| c.textures.first().map(|(_, n)| n.clone())) {
                    control_image(ctx, hud, assets, &painter, r, c.size, &tex, 1.0);
                }
                if resp.clicked() && !busy {
                    out.push(SelectUi::Select(i));
                }
                // Textes posés relativement au cadre (CtrlParent) : nom puis niveau.
                for (ctl, text, color) in [
                    (*name_ctl, slot.name.clone(), Color32::from_rgb(255, 230, 160)),
                    (*level_ctl, format!("{level_word} {}", slot.level), Color32::WHITE),
                ] {
                    if let Some(t) = w.control(ctl) {
                        let tr = lay.rect(t.pos, t.size);
                        shadow_text(&painter, tr.center(), Align2::CENTER_CENTER, &text, font, color);
                    }
                }
            }
            if !status.is_empty() {
                let at = lay.origin + Vec2::new(512.0, 540.0) * s;
                shadow_text(&painter, at, Align2::CENTER_CENTER, status, font * 1.1, Color32::from_rgb(255, 210, 120));
            }
        });
    if delete_confirm {
        if let Some(answer) = draw_delete_confirm(ctx, hud, assets) {
            out.push(SelectUi::ConfirmDelete(answer));
        }
    }
}

/// `UI_ConfirmDeleteChar` (310 × 197, `LG_MessageBox_Base.png`) : titre et question fixes.
fn draw_delete_confirm(ctx: &egui::Context, hud: &mut HudState, assets: &Assets) -> Option<bool> {
    let w = window(hud, assets, "UI_ConfirmDeleteChar")?;
    let screen = ctx.screen_rect();
    let lay = Screen1024::new(screen);
    let s = lay.scale;
    let size = Vec2::new(w.size[0], w.size[1]) * s;
    let font = (11.0 * s).max(9.0);
    let mut answer = None;
    egui::Area::new(egui::Id::new("archonia-delete-char"))
        .fixed_pos(screen.center() - size / 2.0)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let painter = ui.painter().clone();
            if let Some((_, tex)) = w.textures.first() {
                control_image(ctx, hud, assets, &painter, rect, w.size, tex, 1.0);
            }
            for c in &w.controls {
                let r = Rect::from_min_size(rect.min + Vec2::new(c.pos[0], c.pos[1]) * s, Vec2::new(c.size[0], c.size[1]) * s);
                let fixed = c.display.as_deref().and_then(|d| d.strip_prefix("1:")).unwrap_or("");
                if c.kind == 3 {
                    if button(ctx, ui, hud, assets, &painter, r, c, fixed, font, true, ("delete-char", &c.name)).clicked() {
                        answer = Some(c.name == "CONTROL_31064");
                    }
                } else if !fixed.is_empty() {
                    shadow_text(&painter, r.center(), Align2::CENTER_CENTER, fixed, font, Color32::WHITE);
                }
            }
        });
    if answer.is_none() {
        ctx.input(|i| {
            if i.key_pressed(egui::Key::Escape) {
                answer = Some(false);
            }
        });
    }
    answer
}

/// Demande de l'écran de création.
#[derive(Debug, Clone, PartialEq)]
pub enum CreateUi {
    /// (TID du modèle, nom, visage, coiffure).
    Submit(i32, String, i32, i32),
    Cancel,
}

/// Boutons de classe d'`UI_LG_CHARCREATE` par race : (contrôle, nom de classe du modèle).
const CLASS_BUTTONS: [(&str, &[(&str, &str)]); 4] = [
    ("Human", &[("CONTROL_BTNCLASS_KNIGHT", "knight"), ("CONTROL_BTNCLASS_ARCHER", "archer"), ("CONTROL_BTNCLASS_MAGE", "mage")]),
    ("Orc", &[("CONTROL_BTNCLASS_BERSERKER", "berserker"), ("CONTROL_BTNCLASS_HUNTER", "hunter"), ("CONTROL_BTNCLASS_SOCERER", "sorcerer")]),
    ("MoonElf", &[("CONTROL_BTNCLASS_SWASHBUCKLER", "swashbuckler"), ("CONTROL_BTNCLASS_RANGER", "ranger"), ("CONTROL_BTNCLASS_ELEMENTALIST", "elementalist")]),
    ("DragonScion", &[("CONTROL_BTNCLASS_SCION", "scion")]),
];

const RACE_BUTTONS: [(&str, &str); 4] = [
    ("CONTROL_BTNRACE_HUMAN", "Human"),
    ("CONTROL_BTNRACE_ORC", "Orc"),
    ("CONTROL_BTNRACE_MOONELF", "MoonElf"),
    ("CONTROL_BTNRACE_DRAGONSCION", "DragonScion"),
];

/// Bouton aux textures `_Normal` / `_Over` / `_Push` / `_Disable` (boutons de race, de classe,
/// flèches de coiffure et de visage) ; `selected` garde la texture « Push ».
#[allow(clippy::too_many_arguments)]
fn state_button(ctx: &egui::Context, ui: &mut egui::Ui, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, r: Rect, c: &UiControl, enabled: bool, selected: bool, id: impl std::hash::Hash) -> egui::Response {
    let resp = ui.interact(r, egui::Id::new(id), if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let pick = |suffix: &str| c.textures.iter().find(|(_, n)| n.to_ascii_lowercase().ends_with(suffix)).map(|(_, n)| n.clone());
    let tex = if !enabled {
        pick("_disable.png").or_else(|| pick("_normal.png"))
    } else if selected || resp.is_pointer_button_down_on() {
        pick("_push.png")
    } else if resp.hovered() {
        pick("_over.png")
    } else {
        pick("_normal.png")
    };
    if let Some(t) = tex.or_else(|| c.textures.first().map(|(_, n)| n.clone())) {
        let tint = if enabled { Color32::WHITE } else { Color32::from_gray(110) };
        if let Some((id, orig)) = hud.image(ctx, assets, &t) {
            let uv = Pos2::new((c.size[0] / orig[0]).min(1.0), (c.size[1] / orig[1]).min(1.0));
            painter.image(id, r, Rect::from_min_max(Pos2::ZERO, uv), tint);
        }
    }
    resp
}

/// Écran de création d'Archonia (`UI_LG_CHARCREATE`) :
/// - **Panneau** `CreateCharacter_BG.png` (en 680, 100) : boutons de race et de classe (images
///   `CreateCharacter_Btn*`, seuls ceux de la race choisie sont visibles), coiffure et visage
///   précédents / suivants, nom, descriptions (`RaceDesc<Race>`,
///   `CharDesc<Race><Knight|Archer|Wizard|Scion>` de `uimessage.txt`).
/// - **Rotation** : flèches `LG_LeftArrow` / `LG_RightArrow`, tant qu'elles sont enfoncées
///   (`LG_RotateLeftStart` … `LG_RotateStop`).
/// - **Zoom** : bouton `LoginZoomIn` / `LoginZoomOut`.
/// - **Retour** et **Créer**.
pub fn draw_char_create(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, c: &mut crate::ui::CreationUi, busy: bool, status: &str, out: &mut Vec<CreateUi>) {
    let Some(w) = window(hud, assets, "UI_LG_CHARCREATE") else { return };
    let screen = ctx.screen_rect();
    let lay = Screen1024::new(screen);
    let s = lay.scale;
    let font = (11.0 * s).max(9.0);
    let gold = Color32::from_rgb(255, 230, 160);
    let race = c.race;
    let template = c.current().cloned();
    let class_lower = template.as_ref().map(|t| t.class_name.to_ascii_lowercase()).unwrap_or_default();
    let desc_kind = match crate::login_settings::class_kind(&class_lower) {
        "Knight" => "Knight",
        "Ranger" => "Archer",
        "Mage" => "Wizard",
        _ => "Scion",
    };
    let race_title = ui_message(hud, assets, race);
    let class_title = {
        let key = template.as_ref().map(|t| if class_lower == "swashbuckler" { "SwashBuckler".to_string() } else { t.class_name.clone() }).unwrap_or_default();
        let m = ui_message(hud, assets, &key);
        if m.is_empty() {
            key
        } else {
            m
        }
    };
    let race_desc = ui_message(hud, assets, &format!("RaceDesc{race}"));
    // Clés de uimessage.txt : nom de classe pour les orcs (`CharDescOrcSorcerer`), type générique
    // pour les autres races (`CharDescMoonElfWizard`, `CharDescDragonScionScion`).
    let class_desc = {
        let by_name = template.as_ref().map(|t| ui_message(hud, assets, &format!("CharDesc{race}{}", t.class_name))).unwrap_or_default();
        if by_name.is_empty() {
            ui_message(hud, assets, &format!("CharDesc{race}{desc_kind}"))
        } else {
            by_name
        }
    };
    let head_word = ui_message(hud, assets, "HeadType1").trim_end_matches('1').trim().to_string();
    let face_word = ui_message(hud, assets, "FaceType1").trim_end_matches('1').trim().to_string();
    let zoom_label = ui_message(hud, assets, if c.zoom { "LoginZoomOut" } else { "LoginZoomIn" });
    let (faces, hairs) = template.as_ref().map(|t| (t.face_count, t.hair_count)).unwrap_or((0, 0));
    let class_controls: Vec<(&str, &str)> = CLASS_BUTTONS.iter().find(|(r, _)| *r == race).map(|(_, l)| l.to_vec()).unwrap_or_default();
    let dt = ctx.input(|i| i.stable_dt).min(0.1);
    egui::Area::new(egui::Id::new("archonia-char-create"))
        .fixed_pos(screen.min)
        .order(egui::Order::Background)
        .show(ctx, |ui| {
            ui.set_min_size(screen.size());
            let painter = ui.painter().clone();
            if let Some((_, tex)) = w.textures.first() {
                if let Some((id, _)) = hud.image(ctx, assets, tex) {
                    painter.image(id, screen, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                }
            }
            for ctl in &w.controls {
                let r = lay.rect(ctl.pos, ctl.size);
                let fixed = ctl.display.as_deref().and_then(|d| d.strip_prefix("1:"));
                let name = ctl.name.as_str();
                match name {
                    "CONTROL_20458" => {
                        if button(ctx, ui, hud, assets, &painter, r, ctl, fixed.unwrap_or(""), font, !busy, ("create", name)).clicked() {
                            out.push(CreateUi::Cancel);
                        }
                    }
                    "CONTROL_22619" => {
                        let nm = c.name.trim().to_string();
                        let ok = !busy && !nm.is_empty() && template.is_some();
                        if button(ctx, ui, hud, assets, &painter, r, ctl, fixed.unwrap_or(""), font, ok, ("create", name)).clicked() {
                            if let Some(t) = &template {
                                out.push(CreateUi::Submit(t.tid, nm, c.face, c.hair));
                            }
                        }
                    }
                    "CONTROL_23334" => {
                        if button(ctx, ui, hud, assets, &painter, r, ctl, &zoom_label, font, true, ("create", name)).clicked() {
                            c.zoom = !c.zoom;
                        }
                    }
                    "CONTROL_25891" | "CONTROL_26957" => {
                        let resp = button(ctx, ui, hud, assets, &painter, r, ctl, "", font, true, ("create", name));
                        if resp.is_pointer_button_down_on() {
                            c.rotate += if name == "CONTROL_26957" { 120.0 } else { -120.0 } * dt;
                        }
                    }
                    "CT_LG_CHARCREATE_EDIT" => {
                        let edit = egui::TextEdit::singleline(&mut c.name)
                            .id(egui::Id::new("archonia-create-name"))
                            .frame(false)
                            .char_limit(16)
                            .font(FontId::proportional(font))
                            .text_color(Color32::WHITE);
                        ui.put(r, edit);
                    }
                    "CONTROL_EDIT_DESCRACE" | "CONTROL_EDIT_DESCCLASS" => {
                        let text = if name == "CONTROL_EDIT_DESCRACE" { &race_desc } else { &class_desc };
                        let galley = painter.layout(text.clone(), FontId::proportional(font * 0.95), Color32::from_rgb(230, 222, 200), r.width());
                        painter.with_clip_rect(r).galley(r.min, galley, Color32::WHITE);
                    }
                    "CONTROL_15506" => shadow_text(&painter, r.left_center(), Align2::LEFT_CENTER, &race_title, font, gold),
                    "CONTROL_7486" => shadow_text(&painter, r.left_center(), Align2::LEFT_CENTER, &class_title, font, gold),
                    "CONTROL_17924" => shadow_text(&painter, r.center(), Align2::CENTER_CENTER, &format!("{head_word} {}", c.hair + 1), font, Color32::WHITE),
                    "CONTROL_17938" => shadow_text(&painter, r.center(), Align2::CENTER_CENTER, &format!("{face_word} {}", c.face + 1), font, Color32::WHITE),
                    "CONTROL_22410" | "CONTROL_13401" | "CONTROL_30906" | "CONTROL_7381" => {
                        let hair = matches!(name, "CONTROL_22410" | "CONTROL_13401");
                        let count = if hair { hairs } else { faces };
                        let step = if matches!(name, "CONTROL_22410" | "CONTROL_30906") { -1 } else { 1 };
                        if state_button(ctx, ui, hud, assets, &painter, r, ctl, count > 1, false, ("create", name)).clicked() {
                            let v = if hair { &mut c.hair } else { &mut c.face };
                            *v = (*v + step).rem_euclid(count.max(1));
                        }
                    }
                    _ if name.starts_with("CONTROL_BTNRACE_") => {
                        let Some((_, r_key)) = RACE_BUTTONS.iter().find(|(n, _)| *n == name) else { continue };
                        let available = c.templates.iter().any(|t| t.race == *r_key);
                        if state_button(ctx, ui, hud, assets, &painter, r, ctl, available && !busy, race == *r_key, ("create", name)).clicked() {
                            c.select_race(r_key);
                        }
                    }
                    _ if name.starts_with("CONTROL_BTNCLASS_") => {
                        let Some((_, cls)) = class_controls.iter().find(|(n, _)| *n == name) else { continue };
                        let index = c.templates.iter().position(|t| t.race == race && t.class_name.eq_ignore_ascii_case(cls));
                        let sel = index.is_some() && index == Some(c.template);
                        if state_button(ctx, ui, hud, assets, &painter, r, ctl, index.is_some() && !busy, sel, ("create", name)).clicked() {
                            if let Some(i) = index {
                                c.template = i;
                                c.face = 0;
                                c.hair = 0;
                            }
                        }
                    }
                    _ => {
                        if let Some((_, tex)) = ctl.textures.first() {
                            control_image(ctx, hud, assets, &painter, r, ctl.size, &tex.clone(), 1.0);
                        }
                        if let Some(text) = fixed.filter(|t| !t.is_empty()) {
                            let title = name == "CONTROL_27650";
                            let align = if title { Align2::CENTER_CENTER } else { Align2::LEFT_CENTER };
                            let at = if title { r.center() } else { r.left_center() };
                            shadow_text(&painter, at, align, text, font, gold);
                        }
                    }
                }
            }
            if !status.is_empty() {
                let at = lay.origin + Vec2::new(512.0, 690.0) * s;
                shadow_text(&painter, at, Align2::CENTER_CENTER, status, font * 1.1, Color32::from_rgb(255, 210, 120));
            }
        });
}
