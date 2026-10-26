//! Options d'Archonia : fenêtres à onglets `Option_Video` (GFX), `Option_Sound` (Son),
//! `Option_System` (Affichage), `Option_Option` (Fonction) et `Option_Advanced` (Avancé),
//! 564 × 494, à la place de la fenêtre egui « Options ».
//!
//! - **Son** : réglages réels d'ArchRust. Cases « (activé/désactivé) » `OP_BGM_Enable`,
//!   `CONTROL_25797` (effets) et `CONTROL_29336` (ambiance), curseurs `OP_BGM_Slide`,
//!   `OP_ESound_Slide` et `EnvSound` (piste `OP_Sound_Volume_a`, remplissage `_b`, curseur
//!   `OP_Slide_Cursor`).
//! - **Avancé** : la liste générique `OpAdv_List_Op` (catégories et cases à cocher, lignes de
//!   20 px) porte ce qu'Archonia n'a pas : langue du jeu (textes du serveur, extension ArchCore, et du
//!   client, dossier `ini_*` lu au démarrage) et
//!   packs de ressources (activation ; l'ordre se règle dans `packs.toml`), AddOns (activation,
//!   `AddOns.txt`, appliquée aussitôt en jeu par un rechargement de l'interface).
//! - **GFX** : listes `OP_CharView` et `OP_MapView` (distances des personnages et de la carte,
//!   « Proche » à « Global », un clic passe au choix suivant).
//! - **Affichage** : noms au-dessus de soi, des monstres, des autres joueurs, ligne de guilde, cadre
//!   des membres du groupe, casque (envoyé au serveur).
//! - **Fonction** : refus d'échange, d'invitation de groupe et de guilde (drapeaux envoyés au
//!   serveur ; les invitations reçues sont aussi déclinées par le client).
//! - Les contrôles reliés le sont par leur rappel `Action=3:OP_…`. Les autres sont dessinés inactifs
//!   (texture « d ») : ArchRust ne sait pas les honorer (liste dans `docs/FORMATS.md`).
//!
//! « Réinitialiser les positions de l'interface » remet les fenêtres déplaçables à leur place
//! d'origine. « Fermer » et la croix ferment.

use super::npc_ui::{button, window};
use super::*;
use crate::audio::AudioSettings;
use crate::options::{DisplayOptions, RefuseOptions, VideoOptions};
use crate::ui::PackAction;

const TABS: [(&str, &str); 5] = [("GFX", "Option_Video"), ("Son", "Option_Sound"), ("Affichage", "Option_System"), ("Fonction", "Option_Option"), ("Avance", "Option_Advanced")];
pub(super) const OPTION_WINDOWS: [&str; 5] = ["Option_Video", "Option_Sound", "Option_System", "Option_Option", "Option_Advanced"];
/// Hauteur d'une ligne d'`OpAdv_List_Op`.
const ADV_ROW: f32 = 20.0;

/// Réglages modifiables depuis les options.
pub struct OptionsEdit<'a> {
    pub audio: &'a mut AudioSettings,
    pub language: &'a mut Option<String>,
    /// Langues proposées : (code, nom).
    pub languages: &'a [(&'static str, &'static str)],
    pub packs: &'a mut archrust_patch::PatchStack,
    pub packs_restart: bool,
    /// AddOns : (nom, titre, activé).
    pub addons: &'a [(String, String, bool)],
    pub display: &'a mut DisplayOptions,
    pub refuse: &'a mut RefuseOptions,
    pub video: &'a mut VideoOptions,
}

/// Case des onglets Affichage et Fonction reliée à un réglage d'ArchRust, d'après son rappel.
fn option_flag<'b>(action: &str, display: &'b mut DisplayOptions, refuse: &'b mut RefuseOptions) -> Option<&'b mut bool> {
    Some(match action {
        "OP_ViewNameMine" => &mut display.name_self,
        "OP_ViewNameMonster" => &mut display.name_monster,
        "OP_ViewNameGuild" => &mut display.name_guild,
        "OP_ViewNameParty" => &mut display.name_party,
        "OP_ViewNameOthers" => &mut display.name_others,
        "OP_VisibleViewHelmet" => &mut display.helmet,
        "OP_RefuseTrade" => &mut refuse.trade,
        "OP_RefusePartyIn" => &mut refuse.party,
        "OP_RefuseGuildIn" => &mut refuse.guild,
        _ => return None,
    })
}

/// Libellés des listes `OP_CharView` et `OP_MapView` (`uimessage.txt`).
const VIEW_LABELS: [&str; 4] = ["Option_Msg_NEAR", "Option_Msg_NORMAL", "Option_Msg_FAR", "Option_Msg_ALL"];

/// Résultat d'une image : réglage audio ou langue changé, action sur les packs, fermeture.
#[derive(Debug, Default)]
pub struct OptionsOut {
    pub changed: bool,
    pub pack_action: PackAction,
    /// AddOn à activer (`true`) ou désactiver.
    pub addon_toggle: Option<(String, bool)>,
    pub close: bool,
}

/// Ligne de la liste « Avancé ».
enum AdvRow {
    Category(String),
    Item { label: String, checked: bool, action: AdvAction },
}

#[derive(Clone)]
enum AdvAction {
    Language(Option<String>),
    Pack(String, bool),
    PacksReset,
    Addon(String, bool),
}

/// Options ; `None` si les fenêtres d'Archonia manquent (le moteur garde alors sa fenêtre egui).
pub fn draw_options(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, edit: OptionsEdit) -> Option<OptionsOut> {
    let tab = (hud.options_tab as usize).min(TABS.len() - 1);
    let w = window(hud, assets, TABS[tab].1)?;
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let font = (10.5 * s).max(8.0);
    let gold = Color32::from_rgb(255, 230, 160);
    let mut out = OptionsOut::default();
    let mut new_tab = None;
    let rows = if tab == 4 { advanced_rows(&edit) } else { Vec::new() };
    let mut adv_click: Option<AdvAction> = None;
    let OptionsEdit { audio, language, packs, display, refuse, video, .. } = edit;
    // Listes déroulantes actives (rectangle, rappel) : leur flèche `Common_Button_PGDN` est un
    // contrôle enfant sans nom posé dedans.
    let mut combos: Vec<(Rect, &str)> = Vec::new();
    egui::Area::new(egui::Id::new("archonia-options"))
        .default_pos(layout.window_origin(&w))
        .movable(true)
        .constrain(true)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w.size[0], w.size[1]) * s, egui::Sense::hover());
            let painter = ui.painter().clone();
            if let Some((_, tex)) = w.textures.first() {
                control_image(ctx, hud, assets, &painter, rect, w.size, tex, 1.0);
            }
            for c in &w.controls {
                let r = layout.rect(rect.min, c.pos, c.size);
                let fixed = c.display.as_deref().and_then(|d| d.strip_prefix("1:"));
                let name = c.name.as_str();
                // Onglet « Video » (452, 35) : sans fenêtre propre, non affiché.
                if fixed == Some("Video") {
                    continue;
                }
                if let Some(i) = fixed.and_then(|f| TABS.iter().position(|(l, _)| *l == f)).filter(|_| c.textures.iter().any(|(_, t)| t.to_ascii_lowercase().starts_with("op_tab"))) {
                    if c.kind == 3 {
                        if button(ctx, ui, hud, assets, &painter, r, c, fixed.unwrap_or(""), font, true, ("options-tab", tab, name)).clicked() {
                            new_tab = Some(i);
                        }
                    } else {
                        if let Some((_, t)) = c.textures.first() {
                            control_image(ctx, hud, assets, &painter, r, c.size, &t.clone(), 1.0);
                        }
                        shadow_text(&painter, r.center(), Align2::CENTER_CENTER, fixed.unwrap_or(""), font, gold);
                    }
                    continue;
                }
                match (c.kind, name) {
                    (3, _) if fixed == Some("Fermer") || c.textures.iter().any(|(_, t)| t.starts_with("Common_Button_X")) => {
                        if button(ctx, ui, hud, assets, &painter, r, c, fixed.unwrap_or(""), font, true, ("options-close", tab, name)).clicked() {
                            out.close = true;
                        }
                    }
                    (3, n) if n.ends_with("ResetUIPos") => {
                        if button(ctx, ui, hud, assets, &painter, r, c, fixed.unwrap_or(""), font, true, ("options-reset", tab)).clicked() {
                            ctx.memory_mut(|m| m.reset_areas());
                        }
                    }
                    // Onglet Son : interrupteurs et curseurs reliés aux réglages audio.
                    (3, "OP_BGM_Enable" | "CONTROL_25797" | "CONTROL_29336") if tab == 1 => {
                        let flag = match name {
                            "OP_BGM_Enable" => &mut audio.music_on,
                            "CONTROL_25797" => &mut audio.effects_on,
                            _ => &mut audio.ambience_on,
                        };
                        if checkbox(ctx, ui, hud, assets, &painter, r, c, *flag, true, ("options-check", name)).clicked() {
                            *flag = !*flag;
                            out.changed = true;
                        }
                    }
                    (14, "OP_BGM_Slide" | "OP_ESound_Slide" | "EnvSound") if tab == 1 => {
                        let value = match name {
                            "OP_BGM_Slide" => &mut audio.music,
                            "OP_ESound_Slide" => &mut audio.effects,
                            _ => &mut audio.ambience,
                        };
                        let cursor = w.controls.iter().skip_while(|x| x.name != name).nth(1).filter(|x| x.kind == 15);
                        if slider(ctx, ui, hud, assets, &painter, r, c, cursor, value, s, ("options-slide", name)) {
                            out.changed = true;
                        }
                    }
                    // Onglets Affichage et Fonction : cases reliées aux réglages d'ArchRust.
                    (3, _) if (tab == 2 || tab == 3) && c.action.as_deref().is_some_and(|a| option_flag(a, display, refuse).is_some()) => {
                        if let Some(flag) = c.action.as_deref().and_then(|a| option_flag(a, display, refuse)) {
                            if checkbox(ctx, ui, hud, assets, &painter, r, c, *flag, true, ("options-check", tab, c.pos[0] as i32, c.pos[1] as i32)).clicked() {
                                *flag = !*flag;
                                out.changed = true;
                            }
                        }
                    }
                    // Onglet GFX : distances des personnages et de la carte, un clic passe au choix suivant.
                    (5, _) if tab == 0 && matches!(c.action.as_deref(), Some("OP_CharView" | "OP_MapView")) => {
                        let action = if c.action.as_deref() == Some("OP_CharView") { "OP_CharView" } else { "OP_MapView" };
                        let value = if action == "OP_CharView" { video.char_view } else { video.map_view };
                        let text = ui_message(hud, assets, VIEW_LABELS[value.min(3) as usize]);
                        if button(ctx, ui, hud, assets, &painter, r, c, "", font, true, ("options-combo", action)).clicked() {
                            cycle_view(video, action);
                            out.changed = true;
                        }
                        shadow_text(&painter, r.left_center() + Vec2::new(4.0 * s, 0.0), Align2::LEFT_CENTER, &text, font, Color32::WHITE);
                        combos.push((r, action));
                    }
                    (3, _) if c.textures.iter().any(|(_, t)| t.starts_with("Common_Button_PGDN")) && combos.iter().any(|(cr, _)| cr.contains(r.center())) => {
                        let action = combos.iter().find(|(cr, _)| cr.contains(r.center())).map_or("", |(_, a)| *a);
                        if button(ctx, ui, hud, assets, &painter, r, c, "", font, true, ("options-combo-arrow", action)).clicked() {
                            cycle_view(video, action);
                            out.changed = true;
                        }
                    }
                    (15, _) => {}
                    // Liste générique de l'onglet Avancé.
                    (10, "OpAdv_List_Op") => {
                        if let Some(a) = advanced_list(ctx, ui, hud, assets, &painter, &w, r, &rows, s, font) {
                            adv_click = Some(a);
                        }
                    }
                    (11, _) | (14, "OpAdv_Scroll") => {}
                    (_, n) if n.starts_with("OpAdv_") => {}
                    // Réglages qu'ArchRust n'a pas : contrôles inactifs.
                    (3, _) | (5, _) => {
                        let _ = button(ctx, ui, hud, assets, &painter, r, c, fixed.unwrap_or(""), font, false, ("options-off", tab, name));
                    }
                    (14, _) => {
                        if let Some((_, t)) = c.textures.first() {
                            control_image(ctx, hud, assets, &painter, r, c.size, &t.clone(), 1.0);
                        }
                    }
                    _ => {
                        if let Some((_, t)) = c.textures.first() {
                            control_image(ctx, hud, assets, &painter, r, c.size, &t.clone(), 1.0);
                        }
                        if let Some(text) = fixed.filter(|t| !t.is_empty()) {
                            // Titre de la fenêtre (431 px de large) centré, libellés à gauche.
                            if c.size[0] > 300.0 {
                                shadow_text(&painter, r.center(), Align2::CENTER_CENTER, text, font * 1.05, gold);
                            } else {
                                shadow_text(&painter, r.left_center(), Align2::LEFT_CENTER, text, font, Color32::WHITE);
                            }
                        }
                    }
                }
            }
        });
    match adv_click {
        Some(AdvAction::Language(code)) => {
            if *language != code {
                *language = code;
                out.changed = true;
            }
        }
        Some(AdvAction::Pack(id, enabled)) => {
            packs.set_enabled(&id, enabled);
            out.pack_action = PackAction::Changed;
        }
        Some(AdvAction::PacksReset) => out.pack_action = PackAction::Reset,
        Some(AdvAction::Addon(name, enabled)) => out.addon_toggle = Some((name, enabled)),
        None => {}
    }
    if let Some(t) = new_tab {
        hud.options_tab = t as u8;
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        out.close = true;
    }
    Some(out)
}

/// Choix suivant d'une liste de distances (proche → moyen → loin → global → proche).
fn cycle_view(video: &mut VideoOptions, action: &str) {
    let v = if action == "OP_CharView" { &mut video.char_view } else { &mut video.map_view };
    *v = (*v + 1) % 4;
}

/// Case `OP_Check` : « a » vide, « c » cochée, « b » survol, « d » inactive.
#[allow(clippy::too_many_arguments)]
fn checkbox(ctx: &egui::Context, ui: &mut egui::Ui, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, r: Rect, c: &UiControl, checked: bool, enabled: bool, id: impl std::hash::Hash) -> egui::Response {
    let resp = ui.interact(r, egui::Id::new(id), if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let pick = |suffix: &str| c.textures.iter().find(|(_, n)| n.to_ascii_lowercase().ends_with(suffix)).map(|(_, n)| n.clone());
    let tex = if !enabled {
        pick("d.png")
    } else if checked {
        pick("c.png")
    } else if resp.hovered() {
        pick("b.png")
    } else {
        pick("a.png")
    };
    if let Some(t) = tex.or_else(|| c.textures.first().map(|(_, n)| n.clone())) {
        control_image(ctx, hud, assets, painter, r, c.size, &t, 1.0);
    }
    resp
}

/// Curseur de volume : piste « a », remplissage « b » jusqu'à la valeur, curseur 17 × 17.
#[allow(clippy::too_many_arguments)]
fn slider(ctx: &egui::Context, ui: &mut egui::Ui, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, r: Rect, c: &UiControl, cursor: Option<&UiControl>, value: &mut f32, s: f32, id: impl std::hash::Hash) -> bool {
    let resp = ui.interact(r.expand(4.0 * s), egui::Id::new(id), egui::Sense::click_and_drag());
    let mut changed = false;
    if let Some(p) = resp.interact_pointer_pos().filter(|_| resp.dragged() || resp.clicked()) {
        let v = ((p.x - r.left()) / r.width().max(1.0)).clamp(0.0, 1.0);
        if (v - *value).abs() > f32::EPSILON {
            *value = v;
            changed = true;
        }
    }
    let pick = |suffix: &str| c.textures.iter().find(|(_, n)| n.to_ascii_lowercase().ends_with(suffix)).map(|(_, n)| n.clone());
    if let Some(t) = pick("a.png") {
        control_image(ctx, hud, assets, painter, r, c.size, &t, 1.0);
    }
    if let Some(t) = pick("b.png") {
        control_image(ctx, hud, assets, painter, r, c.size, &t, value.clamp(0.0, 1.0));
    }
    if let Some(cur) = cursor {
        let size = Vec2::new(cur.size[0], cur.size[1]) * s;
        let at = Pos2::new(r.left() + r.width() * value.clamp(0.0, 1.0), r.center().y);
        let cr = Rect::from_center_size(at, size);
        let state = if resp.dragged() { "c.png" } else if resp.hovered() { "b.png" } else { "a.png" };
        if let Some(t) = cur.textures.iter().find(|(_, n)| n.to_ascii_lowercase().ends_with(state)).or(cur.textures.first()).map(|(_, n)| n.clone()) {
            control_image(ctx, hud, assets, painter, cr, cur.size, &t, 1.0);
        }
    }
    changed
}

/// Lignes de l'onglet Avancé : langue, puis packs graphismes et sons.
fn advanced_rows(edit: &OptionsEdit) -> Vec<AdvRow> {
    use archrust_patch::Category;
    let mut rows = vec![AdvRow::Category("Langue du jeu (textes du client au prochain démarrage)".into())];
    rows.push(AdvRow::Item { label: "Automatique".into(), checked: edit.language.is_none(), action: AdvAction::Language(None) });
    for (code, name) in edit.languages {
        rows.push(AdvRow::Item {
            label: format!("{name} ({code})"),
            checked: edit.language.as_deref() == Some(*code),
            action: AdvAction::Language(Some(code.to_string())),
        });
    }
    for (title, category) in [("Packs de ressources : graphismes", Category::Graphics), ("Packs de ressources : sons", Category::Sounds)] {
        rows.push(AdvRow::Category(title.into()));
        for i in edit.packs.in_category(category) {
            let p = &edit.packs.patches[i];
            let m = &p.manifest;
            let label = format!("{}{}{}", m.patch.name, if m.patch.version.is_empty() { String::new() } else { format!(" {}", m.patch.version) }, if p.official { " (officiel)" } else { "" });
            rows.push(AdvRow::Item { label, checked: p.enabled, action: AdvAction::Pack(m.patch.id.clone(), !p.enabled) });
        }
    }
    rows.push(AdvRow::Item { label: "Rétablir la configuration des packs par défaut".into(), checked: false, action: AdvAction::PacksReset });
    rows.push(AdvRow::Category("AddOns".into()));
    for (name, title, enabled) in edit.addons {
        let label = if title.is_empty() || title == name { name.clone() } else { format!("{title} ({name})") };
        rows.push(AdvRow::Item { label, checked: *enabled, action: AdvAction::Addon(name.clone(), !*enabled) });
    }
    if edit.packs_restart {
        rows.push(AdvRow::Category("Redémarrez le client pour appliquer les changements.".into()));
    }
    rows
}

/// `OpAdv_List_Op` : lignes `OpAdv_Item` de 20 px, catégorie en `OpAdv_Label_Category`, élément
/// en `OpAdv_Label_ItemName` avec sa case `OpAdv_CheckBtn_ItemCheck` ; molette et flèches
/// `OpAdv_Btn_Up` / `OpAdv_Btn_Down` pour défiler.
#[allow(clippy::too_many_arguments)]
fn advanced_list(ctx: &egui::Context, ui: &mut egui::Ui, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, w: &UiWindow, r: Rect, rows: &[AdvRow], s: f32, font: f32) -> Option<AdvAction> {
    let visible = (r.height() / (ADV_ROW * s)).floor().max(1.0) as usize;
    let max_scroll = rows.len().saturating_sub(visible);
    let mut scroll = (hud.options_scroll as usize).min(max_scroll);
    if ui.rect_contains_pointer(r) {
        let dy = ctx.input(|i| i.raw_scroll_delta.y);
        if dy < 0.0 {
            scroll = (scroll + 1).min(max_scroll);
        } else if dy > 0.0 {
            scroll = scroll.saturating_sub(1);
        }
    }
    let ctl = |n: &str| w.control(n);
    let check = ctl("OpAdv_CheckBtn_ItemCheck").cloned();
    let mut clicked = None;
    for (i, row) in rows.iter().skip(scroll).take(visible).enumerate() {
        let y = r.top() + i as f32 * ADV_ROW * s;
        match row {
            AdvRow::Category(t) => {
                shadow_text(painter, Pos2::new(r.left() + 2.0 * s, y + ADV_ROW * s / 2.0), Align2::LEFT_CENTER, t, font, Color32::from_rgb(255, 230, 160));
            }
            AdvRow::Item { label, checked, action } => {
                if let Some(c) = &check {
                    let cr = Rect::from_min_size(Pos2::new(r.left() + 2.0 * s, y + 2.0 * s), Vec2::new(c.size[0], c.size[1]) * s);
                    let row_rect = Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(480.0 * s, ADV_ROW * s));
                    let resp = checkbox(ctx, ui, hud, assets, painter, cr, c, *checked, true, ("options-adv", i + scroll));
                    let row_resp = ui.interact(row_rect, egui::Id::new(("options-adv-row", i + scroll)), egui::Sense::click());
                    if resp.clicked() || row_resp.clicked() {
                        clicked = Some(action.clone());
                    }
                }
                shadow_text(painter, Pos2::new(r.left() + 22.0 * s, y + ADV_ROW * s / 2.0), Align2::LEFT_CENTER, label, font, Color32::WHITE);
            }
        }
    }
    // Barre de défilement : flèches haut / bas.
    for (name, delta) in [("OpAdv_Btn_Up", -1i32), ("OpAdv_Btn_Down", 1)] {
        if let Some(c) = ctl(name) {
            let br = Rect::from_min_size(r.min + (Vec2::new(c.pos[0], c.pos[1]) - Vec2::new(32.0, 74.0)) * s, Vec2::new(c.size[0], c.size[1]) * s);
            if button(ctx, ui, hud, assets, painter, br, c, "", font, true, ("options-adv-scroll", name)).clicked() {
                scroll = (scroll as i32 + delta).clamp(0, max_scroll as i32) as usize;
            }
        }
    }
    hud.options_scroll = scroll as u16;
    clicked
}
