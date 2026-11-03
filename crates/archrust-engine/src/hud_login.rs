//! Écrans de connexion d'Archonia, à la place des fenêtres egui, dans l'ordre du client :
//! saisie du compte (`UI_LG_IDPASSWORD`), choix du monde (`UI_SelectServer`), sélection des
//! personnages (`UI_LG_CHARSELECT`, 1024 × 768) avec confirmation de suppression
//! (`UI_ConfirmDeleteChar`), création (`UI_LG_CHARCREATE`).
//!
//! La saisie du compte par défaut est le panneau d'ArchRust (`ui::login_panel`) ; `UI_LG_IDPASSWORD`
//! le remplace avec l'option `archonia_login` (onglet Avancé). `UI_SelectServer` sert aux deux : toujours
//! après `UI_LG_IDPASSWORD`, après le panneau d'ArchRust si le serveur annonce plusieurs mondes.
//!
//! `UI_LG_IDPASSWORD` et `UI_SelectServer` existent en trois variantes (1024 × 768, 1280 × 1024,
//! 1600 × 1200) : la plus grande qui tient dans la fenêtre est agrandie sans déformation et centrée,
//! sur son fond `LG_2D_Back_*.png` recadré à toute la fenêtre.
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
        Self::fit(screen, [1024.0, 768.0])
    }
    /// Écran de taille `size` (variantes 1280 × 1024, 1600 × 1200) agrandi sans déformation, centré.
    fn fit(screen: Rect, size: [f32; 2]) -> Screen1024 {
        let scale = (screen.width() / size[0]).min(screen.height() / size[1]);
        let origin = screen.center() - Vec2::new(size[0], size[1]) * scale / 2.0;
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
    // État de la connexion tenu par ArchRust : pas affiché, Archonia n'a pas de ligne d'état ici.
    _status: &str,
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
pub fn draw_char_create(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, c: &mut crate::ui::CreationUi, busy: bool, _status: &str, out: &mut Vec<CreateUi>) {
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
        });
}

/// Fenêtres de connexion d'Archonia et leurs variantes de résolution (mêmes contrôles, suffixés pour
/// certains : `CT_LG_EDIT_ID_1280`…).
pub(super) const LOGIN_WINDOWS: [&str; 6] =
    ["UI_LG_IDPASSWORD", "UI_LG_IDPASSWORD_1280", "UI_LG_IDPASSWORD_1600", "UI_SelectServer", "UI_SelectServer_1280", "UI_SelectServer_1600"];

/// Demande des écrans de connexion.
#[derive(Debug, Clone, PartialEq)]
pub enum LoginUi {
    /// « Confirm » ou Entrée (`LG_IdPasswordOK`) : identifiant et mot de passe non vides.
    Confirm,
    /// « Finish » / « Exit » (`Quit_Event_Call`).
    Quit,
    /// Monde choisi dans la liste (`LG_ClickServerList`, premier clic).
    Select(String),
    /// « Connect » (`LG_SelectServer`), ou second clic sur le monde choisi.
    Connect(String),
    /// Branche d'un groupe ouverte ou fermée (`LG_ServerTreeButton`, clic sur l'en-tête).
    ToggleGroup(String),
}

/// Nom d'un contrôle sans le suffixe de sa variante (`CT_LG_EDIT_ID_1280` → `CT_LG_EDIT_ID`).
fn base_control(name: &str) -> &str {
    name.strip_suffix("_1280").or_else(|| name.strip_suffix("_1600")).unwrap_or(name)
}

/// Variante de la fenêtre `base` : la plus grande (1600 × 1200, 1280 × 1024) qui tient dans la fenêtre
/// du jeu, sinon 1024 × 768 (agrandie ensuite sans déformation).
fn login_variant(hud: &mut HudState, assets: &Assets, base: &str, screen: Rect) -> Option<UiWindow> {
    for suffix in ["_1600", "_1280"] {
        if let Some(w) = window(hud, assets, &format!("{base}{suffix}")).filter(|w| w.size[0] <= screen.width() && w.size[1] <= screen.height()) {
            return Some(w);
        }
    }
    window(hud, assets, base)
}

/// Fond plein écran de la fenêtre (`LG_2D_Back_*.png`) : l'image couvre la fenêtre du jeu sans
/// déformation, recadrée sur les bords.
fn login_back(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, screen: Rect, w: &UiWindow) {
    let Some((_, tex)) = w.textures.first() else { return };
    let Some((id, orig)) = hud.image(ctx, assets, tex) else { return };
    // Partie utile de l'image : la taille de la fenêtre (les fonds sont en puissances de deux).
    let used = [(w.size[0] / orig[0]).min(1.0), (w.size[1] / orig[1]).min(1.0)];
    let scale = (screen.width() / w.size[0]).max(screen.height() / w.size[1]);
    let vis = [screen.width() / scale / w.size[0], screen.height() / scale / w.size[1]];
    let u0 = (1.0 - vis[0]) / 2.0 * used[0];
    let v0 = (1.0 - vis[1]) / 2.0 * used[1];
    let uv = Rect::from_min_max(Pos2::new(u0, v0), Pos2::new(u0 + vis[0] * used[0], v0 + vis[1] * used[1]));
    painter.image(id, screen, uv, Color32::WHITE);
}

/// Couleur ARGB d'Archonia (`Status2`, `CtrlFont`).
fn argb(c: u32) -> Color32 {
    Color32::from_rgba_unmultiplied((c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8)
}

/// Écran de saisie du compte d'Archonia (`UI_LG_IDPASSWORD`) :
/// - **Cadre** `CT_LG_MAIN` (`LG_IDPass_Main.png`) au centre, titre « Login », consigne, libellés
///   « User ID » / « Password » ;
/// - **Saisies** `CT_LG_EDIT_ID` et `CT_LG_EDIT_PASSWORD` (masquée) ; Entrée valide, Tab passe d'un
///   champ à l'autre (`LG_IdPasswordEditTab`) ;
/// - **« Confirm »** (`LG_IdPasswordOK` : rien n'est encore envoyé du compte, la liste des mondes
///   s'ouvre) et **« Finish »** (`Quit_Event_Call`) ;
/// - **Images** `LG_Title.png`, `LG_CopyRight.png` ; le classement d'âge coréen
///   (`LG_RestrictAge_loading.png`, `CtrlShow=1:IsAreaKorea`) n'est pas affiché.
///
/// `waiting` : message pendant la connexion au serveur de connexion (sysstr 457).
pub fn draw_id_password(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, user: &mut String, password: &mut String, waiting: Option<&str>, out: &mut Vec<LoginUi>) {
    let screen = ctx.screen_rect();
    let Some(w) = login_variant(hud, assets, "UI_LG_IDPASSWORD", screen) else { return };
    let lay = Screen1024::fit(screen, w.size);
    let s = lay.scale;
    let font = (11.0 * s).max(9.0);
    let busy = waiting.is_some();
    let id_edit = egui::Id::new("archonia-login-id");
    let pw_edit = egui::Id::new("archonia-login-password");
    egui::Area::new(egui::Id::new("archonia-login"))
        .fixed_pos(screen.min)
        .order(egui::Order::Background)
        .show(ctx, |ui| {
            ui.set_min_size(screen.size());
            let painter = ui.painter().clone();
            login_back(ctx, hud, assets, &painter, screen, &w);
            let mut enter = false;
            for c in &w.controls {
                let r = lay.rect(c.pos, c.size);
                let name = base_control(&c.name);
                match name {
                    // Classement d'âge coréen : sans objet.
                    "CONTROL_32728" => {}
                    "CT_LG_EDIT_ID" | "CT_LG_EDIT_PASSWORD" => {
                        let pw = name == "CT_LG_EDIT_PASSWORD";
                        let (text, id) = if pw { (&mut *password, pw_edit) } else { (&mut *user, id_edit) };
                        let edit = egui::TextEdit::singleline(text)
                            .id(id)
                            .frame(false)
                            .password(pw)
                            .char_limit(250)
                            .interactive(!busy)
                            .font(FontId::proportional(font))
                            .text_color(Color32::WHITE)
                            .margin(egui::Margin::symmetric(2, 0));
                        let resp = ui.put(r, edit);
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            enter = true;
                        }
                    }
                    "CT_LG_BUTTON_OK" | "CONTROL_1723" => {
                        let ok = name == "CT_LG_BUTTON_OK";
                        if button(ctx, ui, hud, assets, &painter, r, c, c.fixed_label(), font, !busy || !ok, ("login", name)).clicked() {
                            if ok {
                                enter = true;
                            } else {
                                out.push(LoginUi::Quit);
                            }
                        }
                    }
                    _ => {
                        if let Some((_, tex)) = c.textures.first() {
                            control_image(ctx, hud, assets, &painter, r, c.size, &tex.clone(), 1.0);
                        }
                        let text = c.fixed_label();
                        if !text.is_empty() {
                            // `CtrlFont=…:17` centré (titre), sinon à gauche.
                            let centered = c.font.is_some_and(|(_, a)| a == 17);
                            let (at, align) = if centered { (r.center(), Align2::CENTER_CENTER) } else { (r.left_center(), Align2::LEFT_CENTER) };
                            let size = if name == "CONTROL_23657" { font * 1.15 } else { font };
                            shadow_text(&painter, at, align, text, size, Color32::WHITE);
                        }
                    }
                }
            }
            // `LG_IdPasswordOK` : identifiant et mot de passe non vides.
            if enter && !busy && !user.trim().is_empty() && !password.is_empty() {
                out.push(LoginUi::Confirm);
            }
            if let Some(text) = waiting {
                if let Some(c) = w.controls.iter().find(|c| base_control(&c.name) == "CT_LG_MAIN") {
                    let r = lay.rect(c.pos, c.size);
                    shadow_text(&painter, Pos2::new(r.center().x, r.bottom() + 16.0 * s), Align2::CENTER_CENTER, text, font * 1.1, Color32::from_rgb(255, 210, 120));
                }
            }
        });
    // Focus de départ : l'identifiant, ou le mot de passe si l'identifiant est déjà là.
    if !busy && ctx.memory(|m| m.focused().is_none()) {
        ctx.memory_mut(|m| m.request_focus(if user.trim().is_empty() { id_edit } else { pw_edit }));
    }
}

/// Ligne de la liste `Login_Servers` : en-tête de groupe ou monde.
#[derive(Debug, Clone, PartialEq)]
enum ServerRow<'a> {
    Group { name: &'a str, open: bool },
    World(&'a archrust_netclient::login::WorldInfo),
}

/// Lignes de la liste : chaque groupe (ordre d'arrivée) puis ses mondes s'il est ouvert ; 50 lignes au
/// plus (FUN_1407c3660).
fn server_rows<'a>(worlds: &'a [archrust_netclient::login::WorldInfo], folded: &[String]) -> Vec<ServerRow<'a>> {
    let mut groups: Vec<&str> = Vec::new();
    for w in worlds {
        if !groups.contains(&w.group.as_str()) {
            groups.push(&w.group);
        }
    }
    let mut rows = Vec::new();
    for g in groups {
        let open = !folded.iter().any(|f| f == g);
        rows.push(ServerRow::Group { name: g, open });
        if open {
            rows.extend(worlds.iter().filter(|w| w.group == g).map(ServerRow::World));
        }
    }
    rows.truncate(50);
    rows
}

/// État d'un monde → (message de `uimessage.txt`, rang du `Status2` de `Server_Status`), d'après
/// FUN_1407c2c80 : 1 Good, 2 AboveNormal, 3 Normal, 4 BelowNormal, 5 Bad, autre Unknown.
fn server_status(status: i32) -> (&'static str, usize) {
    match status {
        1 => ("ServerStatusGood", 0),
        2 => ("ServerStatusAboveNormal", 1),
        3 => ("ServerStatusNormal", 2),
        4 => ("ServerStatusBelowNormal", 3),
        5 => ("ServerStatusBad", 4),
        _ => ("ServerStatusUnknown", 4),
    }
}

/// État 5 : serveur saturé, connexion refusée par le client (FUN_1407c3d00).
pub const SERVER_STATUS_FULL: i32 = 5;

/// Choix du monde d'Archonia (`UI_SelectServer`) :
/// - **Cadre** `SelectServer_Base.png`, titre « Select World », `LG_Title.png`, `LG_CopyRight.png` ;
/// - **Liste** `Login_Servers` (`CONTROL_24986`, lignes `CONTROL_25949` de 22 px) : en-tête de groupe
///   (couleur « Server » de `Server_Name`, `Common_Button_12_Plus/Minus_A.png`), puis ses mondes (nom,
///   état `ServerStatus*` coloré d'après `Server_Status`, barre `LG_ServerListBar.png` au survol et sur
///   le monde choisi) ; défilement à la molette et par `Common_Button_PGUP/PGDN`, curseur
///   `Skill_ScrollButtonA.png` ;
/// - **« Connect »** (`LG_SelectServer`) et **« Exit »** (`Quit_Event_Call`).
///
/// Écarts : marques `CTL_NEWSVR`, `CTL_RATE`, `CTL_RC15` (classements coréens) et « personnages ici »
/// (`LG_IsCharMark.png`, inconnu avant l'authentification) non affichées.
#[allow(clippy::too_many_arguments)]
pub fn draw_select_server(
    ctx: &egui::Context,
    hud: &mut HudState,
    assets: &Assets,
    worlds: &[archrust_netclient::login::WorldInfo],
    folded: &[String],
    selected: Option<&str>,
    waiting: Option<&str>,
    out: &mut Vec<LoginUi>,
) {
    let screen = ctx.screen_rect();
    let Some(w) = login_variant(hud, assets, "UI_SelectServer", screen) else { return };
    let lay = Screen1024::fit(screen, w.size);
    let s = lay.scale;
    let font = (11.0 * s).max(9.0);
    let busy = waiting.is_some();
    let rows = server_rows(worlds, folded);
    let ctl = |n: &str| w.controls.iter().find(|c| base_control(&c.name) == n);
    let (Some(list), Some(row)) = (ctl("CONTROL_24986"), ctl("CONTROL_25949")) else { return };
    let visible = ((list.size[1] / row.size[1].max(1.0)).floor() as usize).max(1);
    let max_first = rows.len().saturating_sub(visible);
    let first_id = egui::Id::new("archonia-select-server-first");
    let mut first = ctx.data(|d| d.get_temp::<usize>(first_id)).unwrap_or(0).min(max_first);
    // Contrôles d'une ligne (positions absolues de la première ligne, décalées de 22 px par ligne).
    const ROW_CONTROLS: [&str; 7] = ["CONTROL_2433", "CONTROL_4910", "CONTROL_11453", "CONTROL_977", "Server_Select_Plus", "Server_Select_Minus", "CONTROL_18508"];
    // Marques non affichées (voir l'en-tête de la fonction).
    const HIDDEN: [&str; 9] = ["CONTROL_32728", "CONTROL_24986", "CONTROL_24266", "CONTROL_25949", "CONTROL_13069", "CTL_NEWSVR", "CTL_RATE", "CTL_RC15", "CONTROL_18508"];
    let row_children: Vec<UiControl> = w.controls.iter().filter(|c| ROW_CONTROLS.contains(&base_control(&c.name))).cloned().collect();
    let status_texts: Vec<String> = rows
        .iter()
        .map(|r| match r {
            ServerRow::World(wi) => ui_message(hud, assets, server_status(wi.status).0),
            ServerRow::Group { .. } => String::new(),
        })
        .collect();
    let connectable = |n: &str| worlds.iter().any(|wi| wi.name == n && wi.status != SERVER_STATUS_FULL);
    egui::Area::new(egui::Id::new("archonia-select-server"))
        .fixed_pos(screen.min)
        .order(egui::Order::Background)
        .show(ctx, |ui| {
            ui.set_min_size(screen.size());
            let painter = ui.painter().clone();
            login_back(ctx, hud, assets, &painter, screen, &w);
            for c in &w.controls {
                let name = base_control(&c.name);
                if ROW_CONTROLS.contains(&name) || HIDDEN.contains(&name) {
                    continue;
                }
                let r = lay.rect(c.pos, c.size);
                match name {
                    "CONTROL_24146" | "CONTROL_3962" => {
                        let connect = name == "CONTROL_24146";
                        let target = selected.filter(|n| connectable(n));
                        let enabled = !connect || (!busy && target.is_some());
                        if button(ctx, ui, hud, assets, &painter, r, c, c.fixed_label(), font, enabled, ("select-server", name)).clicked() {
                            match (connect, target) {
                                (true, Some(n)) => out.push(LoginUi::Connect(n.to_string())),
                                (false, _) => out.push(LoginUi::Quit),
                                _ => {}
                            }
                        }
                    }
                    "CONTROL_26455" | "CONTROL_1617" => {
                        let up = name == "CONTROL_26455";
                        let enabled = if up { first > 0 } else { first < max_first };
                        if button(ctx, ui, hud, assets, &painter, r, c, "", font, enabled, ("select-server", name)).clicked() {
                            first = if up { first.saturating_sub(1) } else { (first + 1).min(max_first) };
                        }
                    }
                    // Curseur de défilement, sur la piste `CONTROL_24266` entre les deux boutons.
                    "CONTROL_18022" => {
                        let track = ctl("CONTROL_24266").map(|t| lay.rect(t.pos, t.size)).unwrap_or(r);
                        let span = (track.height() - 32.0 * s - r.height()).max(0.0);
                        let y = track.top() + 16.0 * s + if max_first == 0 { 0.0 } else { span * first as f32 / max_first as f32 };
                        let thumb = Rect::from_min_size(Pos2::new(r.left(), y), r.size());
                        if let Some((_, tex)) = c.textures.first() {
                            control_image(ctx, hud, assets, &painter, thumb, c.size, &tex.clone(), 1.0);
                        }
                    }
                    _ => {
                        if let Some((_, tex)) = c.textures.first() {
                            control_image(ctx, hud, assets, &painter, r, c.size, &tex.clone(), 1.0);
                        }
                        let text = c.fixed_label();
                        if !text.is_empty() {
                            shadow_text(&painter, r.center(), Align2::CENTER_CENTER, text, font * 1.15, Color32::WHITE);
                        }
                    }
                }
            }
            // Molette sur la liste.
            if ui.rect_contains_pointer(lay.rect(list.pos, list.size)) {
                let wheel = ui.input(|i| i.raw_scroll_delta.y);
                if wheel > 0.5 {
                    first = first.saturating_sub(1);
                } else if wheel < -0.5 {
                    first = (first + 1).min(max_first);
                }
            }
            for (k, entry) in rows.iter().enumerate().skip(first).take(visible) {
                let dy = (k - first) as f32 * row.size[1];
                let at = |c: &UiControl| lay.rect([c.pos[0], c.pos[1] + dy], c.size);
                let resp = ui.interact(at(row), egui::Id::new(("select-server-row", k)), if busy { egui::Sense::hover() } else { egui::Sense::click() });
                let chosen = matches!(entry, ServerRow::World(wi) if Some(wi.name.as_str()) == selected);
                for c in &row_children {
                    let r = at(c);
                    let image = |hud: &mut HudState| {
                        if let Some((_, tex)) = c.textures.first() {
                            control_image(ctx, hud, assets, &painter, r, c.size, tex, 1.0);
                        }
                    };
                    match (base_control(&c.name), entry) {
                        // Barre : monde choisi (`CONTROL_11453` « On »), survol (`CONTROL_977`, `Button=0:1`).
                        ("CONTROL_11453", ServerRow::World(_)) if chosen => image(hud),
                        ("CONTROL_977", ServerRow::World(_)) if resp.hovered() && !chosen && !busy => image(hud),
                        ("CONTROL_2433", ServerRow::World(wi)) => {
                            let color = c.status_colors.first().map_or(Color32::WHITE, |x| argb(*x));
                            shadow_text(&painter, r.left_center(), Align2::LEFT_CENTER, &wi.name, font, color);
                        }
                        ("CONTROL_2433", ServerRow::Group { name, .. }) => {
                            let color = c.status_colors.get(1).map_or(Color32::WHITE, |x| argb(*x));
                            shadow_text(&painter, r.left_center(), Align2::LEFT_CENTER, name, font, color);
                        }
                        ("CONTROL_4910", ServerRow::World(wi)) => {
                            let color = c.status_colors.get(server_status(wi.status).1).map_or(Color32::WHITE, |x| argb(*x));
                            shadow_text(&painter, r.left_center(), Align2::LEFT_CENTER, &status_texts[k], font, color);
                        }
                        ("Server_Select_Plus", ServerRow::Group { open: false, .. }) | ("Server_Select_Minus", ServerRow::Group { open: true, .. }) => image(hud),
                        _ => {}
                    }
                }
                if resp.clicked() {
                    match entry {
                        ServerRow::Group { name, .. } => out.push(LoginUi::ToggleGroup(name.to_string())),
                        // FUN_1407c2960 : premier clic = choix, second clic sur le monde choisi = connexion.
                        ServerRow::World(wi) if chosen && connectable(&wi.name) => out.push(LoginUi::Connect(wi.name.clone())),
                        ServerRow::World(wi) => out.push(LoginUi::Select(wi.name.clone())),
                    }
                }
            }
            if let Some(text) = waiting {
                if let Some(c) = ctl("CONTROL_13696") {
                    let r = lay.rect(c.pos, c.size);
                    shadow_text(&painter, Pos2::new(r.center().x, r.bottom() + 16.0 * s), Align2::CENTER_CENTER, text, font * 1.1, Color32::from_rgb(255, 210, 120));
                }
            }
        });
    ctx.data_mut(|d| d.insert_temp(first_id, first));
    // Entrée : « Connect » sur le monde choisi.
    if !busy && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
        if let Some(n) = selected.filter(|n| connectable(n)) {
            out.push(LoginUi::Connect(n.to_string()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_netclient::login::parse_world_list;

    #[test]
    fn liste_des_mondes_par_groupe() {
        let worlds = parse_world_list("ArchCore=ArchCore=0=4=4;Autre=G=0=1=2;Test=ArchCore=0=5=0;");
        let label = |rows: &[ServerRow]| -> Vec<String> {
            rows.iter()
                .map(|r| match r {
                    ServerRow::Group { name, .. } => format!("+{name}"),
                    ServerRow::World(w) => w.name.clone(),
                })
                .collect()
        };
        assert_eq!(label(&server_rows(&worlds, &[])), ["+ArchCore", "ArchCore", "Test", "+G", "Autre"]);
        // Groupe replié : son en-tête seul.
        assert_eq!(label(&server_rows(&worlds, &["ArchCore".to_string()])), ["+ArchCore", "+G", "Autre"]);
        assert_eq!(server_status(1), ("ServerStatusGood", 0));
        assert_eq!(server_status(4), ("ServerStatusBelowNormal", 3));
        assert_eq!(server_status(0), ("ServerStatusUnknown", 4));
        assert_eq!(base_control("CT_LG_EDIT_ID_1280"), "CT_LG_EDIT_ID");
        assert_eq!(argb(-15479316i64 as u32), Color32::from_rgba_unmultiplied(0x13, 0xCD, 0xEC, 0xFF));
    }
}
