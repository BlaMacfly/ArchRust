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
//!   « Proche » à « Global », un clic passe au choix suivant), `OP_Nature` (environnement bas à haut :
//!   densité de la météo) et son bouton détail, qui ouvre `Option_Detail_Nature` (reflets
//!   `OP_LensFlare`, brouillard `OP_Fog`, effets météo `OP_Sky`, en cases Oui/Non) ; `OP_Effect`
//!   (effets d'Archonia, désactivés à « Off ») et la case `OP_TurnOffAllEffects` (effets des
//!   compétences des autres joueurs).
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

const TABS: [(&str, &str); 6] = [("GFX", "Option_Video"), ("Son", "Option_Sound"), ("Affichage", "Option_System"), ("Fonction", "Option_Option"), ("Avance", "Option_Advanced"), ("Video", "Option_VideoRes")];
pub(super) const OPTION_WINDOWS: [&str; 9] = ["Option_Video", "Option_Sound", "Option_System", "Option_Option", "Option_Advanced", DETAIL_NATURE, "Option_VideoRes", DETAIL_EFFECT, DETAIL_SHADOW];
/// Onglets d'`Option_VideoRes` (« Video », ouverte par `OpTabVid_Option_Video` d'`Option_Video`) : rang de
/// [`TABS`] d'après le nom du bouton (`OpVR_Btn_GFX`…), ses libellés restant en anglais.
const VIDEORES_TABS: [(&str, usize); 5] = [("OpVR_Btn_GFX", 0), ("OpVR_Btn_Sound", 1), ("OpVR_Btn_Display", 2), ("OpVR_Btn_Func", 3), ("OpVR_Btn_Adv", 4)];
/// Modes de fenêtre d'`OP_WindowMode` (chaînes du constructeur d'AgcmUIOption, FUN_140703da0).
const WINDOW_MODES: [&str; 3] = ["Fullscreen", "Windowed", "Borderless"];
/// Libellés d'`OP_Shadow` (0 à 3, `Shadow` de `COption.ini` ; liste d'Archonia non relevée) et des
/// listes Oui / Non (`OP_AntiAliasing` ramené à 0 / 1 par FUN_14070ed10, `OP_HighOptimization` « V-Sync »).
const SHADOW_LABELS: [&str; 4] = ["Option_Msg_OFF", "Option_Msg_LOW", "Option_Msg_MEDIUM", "Option_Msg_HIGH"];
const ON_OFF_LABELS: [&str; 2] = ["Option_Msg_OFF", "Option_Msg_ON"];
/// Libellés d'`OP_Bloom` (FUN_140706500, `+0x1820`).
const BLOOM_LABELS: [&str; 5] = ["Option_Msg_OFF", "Option_Msg_TYPE1", "Option_Msg_TYPE2", "Option_Msg_TYPE3", "Option_Msg_TYPE4"];
/// Fenêtre « Environnement » ouverte par le bouton détail (`OP_Nature`) de l'onglet GFX : reflets
/// (`OP_LensFlare`), brouillard (`OP_Fog`), effets météo (`OP_Sky`) en cases Oui/Non ; « Objets »
/// (`OP_NaturalObj`, herbe) en trois cases exclusives `OP_Detail_Nature_Radio_H/M/L`
/// (`CtrlUserData2=3/2/1:0:OP_NaturalObj`).
const DETAIL_NATURE: &str = "Option_Detail_Nature";
/// Boutons « Adv. » : niveaux d'effets (`OP_EffectField`, `OP_EffectCombat` 3 à 0, `OP_EffectUI`, `OP_EffectSkill` 1 / 0)
/// et ombres (`OP_ShadowChar` 3 à 0, `OP_ShadowSelf`, `OP_ShadowObject` 1 / 0).
const DETAIL_EFFECT: &str = "Option_Detail_Effect";
const DETAIL_SHADOW: &str = "Option_Detail_Shadow";
/// Libellés de la liste `OP_Nature` (`uimessage.txt`).
const NATURE_LABELS: [&str; 3] = ["Option_Msg_LOW", "Option_Msg_MEDIUM", "Option_Msg_HIGH"];
/// Libellés de la liste `OP_Effect` (0 à 4, `Effect` de `COption.ini`) : choix d'`uimessage.txt`
/// (l'ordre suit les niveaux de FUN_14070fb80 ; la liste d'Archonia elle-même n'est pas relevée).
const EFFECT_LABELS: [&str; 5] = ["Option_Msg_OFF", "Option_Msg_LOW", "Option_Msg_MEDIUM", "Option_Msg_HIGH", "Option_Msg_VERY_HIGH"];
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
    /// Résolutions proposées par `OP_Resolution` (modes du moniteur, 12 au plus comme AgcmUIOption +0x38c8).
    pub resolutions: &'a [(u32, u32)],
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
        "OP_ViewBaloonChat" => &mut display.balloon_chat,
        "OP_ViewSelfBars" => &mut display.self_bars,
        "OP_RefuseTrade" => &mut refuse.trade,
        "OP_RefusePartyIn" => &mut refuse.party,
        "OP_RefuseGuildIn" => &mut refuse.guild,
        "OP_RefuseGuildBattle" => &mut refuse.guild_battle,
        "OP_RefuseGuildRelation" => &mut refuse.guild_relation,
        "OP_RefuseBuddy" => &mut refuse.buddy,
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
    let OptionsEdit { audio, language, packs, display, refuse, video, resolutions, .. } = edit;
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
                // Onglet « Video » (`OpTabVid_Option_Video`, 452, 35) : `Option_VideoRes` ; onglets de celle-ci par leur nom.
                let named_tab = if name == "OpTabVid_Option_Video" { Some(5) } else { VIDEORES_TABS.iter().find(|(n, _)| *n == name).map(|(_, i)| *i) };
                let label_tab = || fixed.and_then(|f| TABS.iter().position(|(l, _)| *l == f)).filter(|_| c.textures.iter().any(|(_, t)| t.to_ascii_lowercase().starts_with("op_tab")));
                if let Some(i) = named_tab.or_else(label_tab) {
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
                        // `OpAdv_ResetUIPositions` (FUN_140611eb0) : `ResetUIPos_Question` demande d'abord (voir plus bas).
                        if button(ctx, ui, hud, assets, &painter, r, c, fixed.unwrap_or(""), font, true, ("options-reset", tab)).clicked() {
                            let text = crate::app::portal_glue::ui_event_text(assets, "ResetUIPos_Question");
                            ctx.data_mut(|d| d.insert_temp(egui::Id::new("options-reset-ask"), text));
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
                    // GFX et Video : ombre, multiéchantillonnage, V-Sync, résolution, mode de fenêtre.
                    (5, _) if (tab == 0 && matches!(c.action.as_deref(), Some("OP_Shadow" | "OP_AntiAliasing" | "OP_HighOptimization" | "OP_WideView" | "OP_Bloom" | "OP_AutoExposure" | "OP_MatEffect" | "OP_Impact"))) || (tab == 5 && matches!(c.action.as_deref(), Some("OP_Resolution" | "OP_WindowMode"))) => {
                        let action = c.action.as_deref().unwrap_or("");
                        let text = match action {
                            "OP_Shadow" => ui_message(hud, assets, SHADOW_LABELS[video.shadow.min(3) as usize]),
                            "OP_AntiAliasing" => ui_message(hud, assets, ON_OFF_LABELS[video.multisampling as usize]),
                            "OP_HighOptimization" => ui_message(hud, assets, ON_OFF_LABELS[video.vsync as usize]),
                            "OP_WideView" => ui_message(hud, assets, ON_OFF_LABELS[video.wide_view as usize]),
                            "OP_AutoExposure" => ui_message(hud, assets, ON_OFF_LABELS[video.auto_exposure as usize]),
                            "OP_MatEffect" => ui_message(hud, assets, ON_OFF_LABELS[video.mat_effect as usize]),
                            "OP_Impact" => ui_message(hud, assets, ON_OFF_LABELS[video.impact_fx as usize]),
                            "OP_Bloom" => ui_message(hud, assets, BLOOM_LABELS[video.bloom.min(4) as usize]),
                            "OP_Resolution" => video.resolution.map(|(w, h)| format!("{w} x {h}")).unwrap_or_default(),
                            _ => WINDOW_MODES[video.window_mode.min(2) as usize].to_string(),
                        };
                        if button(ctx, ui, hud, assets, &painter, r, c, "", font, true, ("options-combo", action)).clicked() {
                            cycle_option(video, action, resolutions);
                            out.changed = true;
                        }
                        shadow_text(&painter, r.left_center() + Vec2::new(4.0 * s, 0.0), Align2::LEFT_CENTER, &text, font, Color32::WHITE);
                        combos.push((r, action));
                    }
                    // Intensité du bloom (`OP_BloomVal`, `GlowIntensity`) : curseur « Low » → « High ».
                    (14, _) if tab == 0 && c.action.as_deref() == Some("OP_BloomVal") => {
                        let cursor = w.controls.iter().skip_while(|x| x.name != name).nth(1).filter(|x| x.kind == 15);
                        let mut value = video.glow_intensity.min(100) as f32 / 100.0;
                        if slider(ctx, ui, hud, assets, &painter, r, c, cursor, &mut value, s, ("options-slide", name)) {
                            video.glow_intensity = (value * 100.0).round() as u8;
                            out.changed = true;
                        }
                    }
                    // « Optimiser la vitesse du jeu » (`OP_SpeedTuning`, `AUTOTUNING`).
                    (3, _) if tab == 0 && c.action.as_deref() == Some("OP_SpeedTuning") => {
                        if checkbox(ctx, ui, hud, assets, &painter, r, c, video.speed_tuning, true, ("options-check", tab, name)).clicked() {
                            video.speed_tuning = !video.speed_tuning;
                            out.changed = true;
                        }
                    }
                    // Gamma (`OP_BrightVal`, `BRIGHT=100:valeur`) : curseur « Dark » → « Bright ».
                    (14, _) if tab == 0 && c.action.as_deref() == Some("OP_BrightVal") => {
                        let cursor = w.controls.iter().skip_while(|x| x.name != name).nth(1).filter(|x| x.kind == 15);
                        let mut value = video.bright.min(100) as f32 / 100.0;
                        if slider(ctx, ui, hud, assets, &painter, r, c, cursor, &mut value, s, ("options-slide", name)) {
                            video.bright = (value * 100.0).round() as u8;
                            out.changed = true;
                        }
                    }
                    // Video : « Turn off other players skill effects » (`DisableOtherPlayerSkillEffect`).
                    (3, _) if tab == 5 && c.action.as_deref() == Some("OP_DisableOtherPlayerSkillEffect") => {
                        if checkbox(ctx, ui, hud, assets, &painter, r, c, video.hide_other_effects, true, ("options-check", tab, name)).clicked() {
                            video.hide_other_effects = !video.hide_other_effects;
                            out.changed = true;
                        }
                    }
                    // Onglet GFX : distances des personnages et de la carte, un clic passe au choix suivant.
                    (5, _) if tab == 0 && matches!(c.action.as_deref(), Some("OP_CharView" | "OP_MapView" | "OP_Nature" | "OP_Effect")) => {
                        let action = match c.action.as_deref() {
                            Some("OP_CharView") => "OP_CharView",
                            Some("OP_Nature") => "OP_Nature",
                            Some("OP_Effect") => "OP_Effect",
                            _ => "OP_MapView",
                        };
                        let text = match action {
                            "OP_CharView" => ui_message(hud, assets, VIEW_LABELS[video.char_view.min(3) as usize]),
                            "OP_Nature" => ui_message(hud, assets, NATURE_LABELS[video.nature.min(2) as usize]),
                            "OP_Effect" => ui_message(hud, assets, EFFECT_LABELS[video.effect.min(4) as usize]),
                            _ => ui_message(hud, assets, VIEW_LABELS[video.map_view.min(3) as usize]),
                        };
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
                            cycle_option(video, action, resolutions);
                            out.changed = true;
                        }
                    }
                    // « Désactiver les effets des autres joueurs » (`DisableOtherPlayerSkillEffect`).
                    (3, _) if tab == 0 && c.action.as_deref() == Some("OP_TurnOffAllEffects") => {
                        if checkbox(ctx, ui, hud, assets, &painter, r, c, video.hide_other_effects, true, ("options-check", tab, name)).clicked() {
                            video.hide_other_effects = !video.hide_other_effects;
                            out.changed = true;
                        }
                    }
                    // Boutons « Adv. » : `Option_Detail_Effect` (`CONTROL_28534`), `Option_Detail_Shadow` (`CONTROL_7608`).
                    (3, "CONTROL_28534" | "CONTROL_7608") if tab == 0 => {
                        if button(ctx, ui, hud, assets, &painter, r, c, fixed.unwrap_or(""), font, true, ("options-detail", name)).clicked() {
                            let id = egui::Id::new(if name == "CONTROL_28534" { DETAIL_EFFECT } else { DETAIL_SHADOW });
                            ctx.data_mut(|d| {
                                let open = d.get_temp_mut_or_default::<bool>(id);
                                *open = !*open;
                            });
                        }
                    }
                    // Liste `OP_Water` (`CONTROL_6118`, valeurs 0 à 4, FUN_1407128a0).
                    (5, _) if tab == 0 && c.action.as_deref() == Some("OP_Water") => {
                        let text = ui_message(hud, assets, EFFECT_LABELS[video.water.min(4) as usize]);
                        if button(ctx, ui, hud, assets, &painter, r, c, "", font, true, ("options-combo", "OP_Water")).clicked() {
                            cycle_option(video, "OP_Water", resolutions);
                            out.changed = true;
                        }
                        shadow_text(&painter, r.left_center() + Vec2::new(4.0 * s, 0.0), Align2::LEFT_CENTER, &text, font, Color32::WHITE);
                        combos.push((r, "OP_Water"));
                    }
                    // Bouton détail de l'environnement : fenêtre `Option_Detail_Nature`.
                    (3, _) if tab == 0 && c.action.as_deref() == Some("OP_Nature") => {
                        if button(ctx, ui, hud, assets, &painter, r, c, fixed.unwrap_or(""), font, true, ("options-detail-nature", name)).clicked() {
                            ctx.data_mut(|d| {
                                let open = d.get_temp_mut_or_default::<bool>(egui::Id::new(DETAIL_NATURE));
                                *open = !*open;
                            });
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
    if draw_detail_nature(ctx, hud, assets, video) {
        out.changed = true;
    }
    if draw_detail_radio(ctx, hud, assets, video) {
        out.changed = true;
    }
    if let Some(text) = ctx.data(|d| d.get_temp::<String>(egui::Id::new("options-reset-ask"))).filter(|t| !t.is_empty()) {
        if let Some(yes) = crate::hud::confirm_box(ctx, hud, assets, "options-reset-confirm", &text) {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("options-reset-ask"), String::new()));
            if yes {
                ctx.memory_mut(|m| m.reset_areas());
            }
        }
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        out.close = true;
    }
    if out.close {
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new(DETAIL_NATURE), false);
            d.insert_temp(egui::Id::new(DETAIL_EFFECT), false);
            d.insert_temp(egui::Id::new(DETAIL_SHADOW), false);
        });
    }
    Some(out)
}

/// Choix suivant des listes d'`Option_Video` / `Option_VideoRes` ajoutées (ombre, multiéchantillonnage,
/// V-Sync, résolution, mode de fenêtre), sinon [`cycle_view`].
fn cycle_option(video: &mut VideoOptions, action: &str, resolutions: &[(u32, u32)]) {
    match action {
        // La liste récrit le détail (`OP_ShadowChar`, `OP_ShadowObject`).
        "OP_Shadow" => {
            video.shadow = (video.shadow.min(3) + 1) % 4;
            video.shadow_detail = None;
        }
        "OP_Water" => video.water = (video.water.min(4) + 1) % 5,
        "OP_AntiAliasing" => video.multisampling = !video.multisampling,
        "OP_HighOptimization" => video.vsync = !video.vsync,
        "OP_WideView" => video.wide_view = !video.wide_view,
        // FUN_140705400 : `Bloom` recopié dans `AutoExposure` ; FUN_14070ee50, FUN_140711210, FUN_140710eb0 : un réglage allumé
        // coupe le réglage automatique.
        "OP_Bloom" => {
            video.bloom = (video.bloom.min(4) + 1) % 5;
            video.auto_exposure = video.bloom != 0;
            video.speed_tuning &= !video.auto_exposure;
        }
        "OP_AutoExposure" => {
            video.auto_exposure = !video.auto_exposure;
            video.speed_tuning &= !video.auto_exposure;
        }
        "OP_MatEffect" => {
            video.mat_effect = !video.mat_effect;
            video.speed_tuning &= !video.mat_effect;
        }
        "OP_Impact" => {
            video.impact_fx = !video.impact_fx;
            video.speed_tuning &= !video.impact_fx;
        }
        "OP_WindowMode" => video.window_mode = (video.window_mode.min(2) + 1) % 3,
        "OP_Resolution" => {
            if !resolutions.is_empty() {
                let i = video.resolution.and_then(|r| resolutions.iter().position(|x| *x == r)).map_or(0, |i| (i + 1) % resolutions.len());
                video.resolution = Some(resolutions[i]);
            }
        }
        _ => cycle_view(video, action),
    }
}

/// Choix suivant d'une liste de distances (proche → moyen → loin → global → proche) ou de
/// l'environnement (bas → moyen → haut → bas).
fn cycle_view(video: &mut VideoOptions, action: &str) {
    match action {
        "OP_Nature" => video.nature = (video.nature.min(2) + 1) % 3,
        // FUN_14070fb80 récrit les quatre niveaux du détail.
        "OP_Effect" => {
            video.effect = (video.effect.min(4) + 1) % 5;
            video.effect_detail = None;
        }
        "OP_CharView" => video.char_view = (video.char_view + 1) % 4,
        _ => video.map_view = (video.map_view + 1) % 4,
    }
}

/// Réglage d'une case Oui/Non de `Option_Detail_Nature` et sa valeur (« On » = `true`), d'après le
/// nom du contrôle (`OP_Detail_Nature_LF_On`…) : le premier rappel de ces cases est vide.
fn detail_nature_flag<'b>(name: &str, video: &'b mut VideoOptions) -> Option<(&'b mut bool, bool)> {
    let rest = name.strip_prefix("OP_Detail_Nature_")?;
    let (what, on) = rest.rsplit_once('_')?;
    let on = match on {
        "On" => true,
        "Off" => false,
        _ => return None,
    };
    Some((
        match what {
            "LF" => &mut video.lens_flare,
            "Fog" => &mut video.fog,
            "Sky" => &mut video.sky,
            _ => return None,
        },
        on,
    ))
}

/// Fenêtre `Option_Detail_Nature` si elle est ouverte ; rend `true` si un réglage a changé.
fn draw_detail_nature(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, video: &mut VideoOptions) -> bool {
    let id = egui::Id::new(DETAIL_NATURE);
    if !ctx.data(|d| d.get_temp::<bool>(id).unwrap_or(false)) {
        return false;
    }
    let Some(w) = window(hud, assets, DETAIL_NATURE) else { return false };
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let font = (10.5 * s).max(8.0);
    let mut changed = false;
    let mut close = false;
    egui::Area::new(egui::Id::new("archonia-options-detail-nature"))
        .default_pos(layout.window_origin(&w))
        .movable(true)
        .constrain(true)
        .order(egui::Order::Tooltip)
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
                match c.kind {
                    3 if name == "OP_Detail_Nature_Btn1" || name == "OP_Detail_Nature_Btn2" => {
                        if button(ctx, ui, hud, assets, &painter, r, c, fixed.unwrap_or(""), font, true, ("detail-nature", name)).clicked() {
                            close = true;
                        }
                    }
                    // « Objets » : herbe haute / moyenne / basse (`OP_NaturalObj` 3 / 2 / 1).
                    3 if name.starts_with("OP_Detail_Nature_Radio_") => {
                        let value = match name.trim_start_matches("OP_Detail_Nature_Radio_") {
                            "H" => 3,
                            "M" => 2,
                            _ => 1,
                        };
                        if checkbox(ctx, ui, hud, assets, &painter, r, c, video.natural_obj == value, true, ("detail-nature", name)).clicked() && video.natural_obj != value {
                            video.natural_obj = value;
                            changed = true;
                        }
                    }
                    3 => match detail_nature_flag(name, video) {
                        Some((flag, on)) => {
                            if checkbox(ctx, ui, hud, assets, &painter, r, c, *flag == on, true, ("detail-nature", name)).clicked() && *flag != on {
                                *flag = on;
                                changed = true;
                            }
                        }
                        // « Objets » (`OP_NaturalObj`) : ArchRust n'a pas ce réglage.
                        None => {
                            let _ = checkbox(ctx, ui, hud, assets, &painter, r, c, false, false, ("detail-nature", name));
                        }
                    },
                    _ => {
                        if let Some((_, t)) = c.textures.first() {
                            control_image(ctx, hud, assets, &painter, r, c.size, &t.clone(), 1.0);
                        }
                        if let Some(text) = fixed.filter(|t| !t.is_empty()) {
                            shadow_text(&painter, r.left_center(), Align2::LEFT_CENTER, text, font, Color32::WHITE);
                        }
                    }
                }
            }
        });
    if close {
        ctx.data_mut(|d| d.insert_temp(id, false));
    }
    changed
}

/// Valeur d'une case d'`Option_Detail_Effect` / `Option_Detail_Shadow` : rappel et valeur (`CtrlUserData2`,
/// 0 si absente). Attention, dans le fichier d'interface, la ligne « Object » appelle `OP_ShadowSelf` et la ligne
/// « Own Shadow » `OP_ShadowObject` : c'est le rappel qui décide, comme dans Archonia.
fn detail_value(c: &UiControl) -> Option<(&str, u8)> {
    let action = c.action.as_deref()?;
    let value = match c.name.rsplit('_').next().unwrap_or("") {
        "H" => 3,
        "M" => 2,
        "L" => 1,
        "On" => 1,
        "Off" => 0,
        _ => match c.name.as_str() {
            // « Skill Effects » : On `CONTROL_14854`, Off `CONTROL_6648`.
            "CONTROL_14854" => 1,
            "CONTROL_6648" => 0,
            _ => return None,
        },
    };
    Some((action, value))
}

/// Niveaux d'une action du détail (`FUN_140707120`, `FUN_140707150`, `FUN_140707180`, `FUN_140705ea0`…).
fn detail_get(video: &VideoOptions, action: &str) -> Option<u8> {
    let (field, combat, ui, skill) = video.effect_levels();
    Some(match action {
        "OP_EffectField" => field.min(3),
        "OP_EffectCombat" => combat.min(3),
        "OP_EffectUI" => ui,
        "OP_EffectSkill" => skill,
        "OP_ShadowChar" => video.shadow_char(),
        "OP_ShadowObject" => video.shadow_object() as u8,
        "OP_ShadowSelf" => video.shadow_detail.map_or(0, |d| d.own),
        _ => return None,
    })
}

fn detail_set(video: &mut VideoOptions, action: &str, value: u8) {
    if action.starts_with("OP_Effect") {
        let (mut f, mut c, mut u, mut s) = video.effect_levels();
        match action {
            "OP_EffectField" => f = value,
            "OP_EffectCombat" => c = value,
            "OP_EffectUI" => u = value,
            _ => s = value,
        }
        video.effect_detail = Some((f, c, u, s));
    } else {
        let mut d = video.shadow_detail.unwrap_or(crate::options::ShadowDetail { character: video.shadow_char(), object: video.shadow_object(), own: 0 });
        match action {
            "OP_ShadowChar" => d.character = value,
            "OP_ShadowObject" => d.object = value != 0,
            _ => d.own = value,
        }
        video.shadow_detail = Some(d);
    }
}

#[cfg(test)]
mod detail_tests {
    use super::*;

    #[test]
    fn le_detail_remplace_la_liste_jusqu_au_prochain_choix() {
        let mut v = VideoOptions::default();
        assert_eq!(v.effect_levels(), (2, 2, 1, 1));
        detail_set(&mut v, "OP_EffectUI", 0);
        assert_eq!(v.effect_levels(), (2, 2, 0, 1));
        cycle_view(&mut v, "OP_Effect");
        assert_eq!(v.effect_levels(), (3, 3, 1, 1));
        assert!(v.shadow_object() && v.shadow_char() == 3);
        detail_set(&mut v, "OP_ShadowObject", 0);
        assert!(!v.shadow_object() && v.shadow_char() == 3);
        detail_set(&mut v, "OP_ShadowChar", 1);
        assert_eq!(detail_get(&v, "OP_ShadowChar"), Some(1));
        cycle_option(&mut v, "OP_Shadow", &[]);
        assert_eq!(v.shadow_detail, None);
        cycle_option(&mut v, "OP_Water", &[]);
        assert_eq!(v.water, 3);
        // `OP_Bloom` : quatre types puis arrêt, `AutoExposure` suit et coupe le réglage automatique (FUN_140705400).
        v.speed_tuning = true;
        cycle_option(&mut v, "OP_Bloom", &[]);
        assert!(v.bloom == 1 && v.auto_exposure && !v.speed_tuning);
        for _ in 0..4 {
            cycle_option(&mut v, "OP_Bloom", &[]);
        }
        assert!(v.bloom == 0 && !v.auto_exposure);
        v.speed_tuning = true;
        cycle_option(&mut v, "OP_Impact", &[]);
        assert!(v.impact_fx && !v.speed_tuning);
        cycle_option(&mut v, "OP_WideView", &[]);
        assert!(v.wide_view);
    }
}

/// Fenêtres `Option_Detail_Effect` et `Option_Detail_Shadow` ouvertes ; rend `true` si un réglage a changé.
fn draw_detail_radio(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, video: &mut VideoOptions) -> bool {
    let mut changed = false;
    for name in [DETAIL_EFFECT, DETAIL_SHADOW] {
        let id = egui::Id::new(name);
        if !ctx.data(|d| d.get_temp::<bool>(id).unwrap_or(false)) {
            continue;
        }
        let Some(w) = window(hud, assets, name) else { continue };
        let layout = Layout::new(ctx.screen_rect());
        let s = layout.scale;
        let font = (10.5 * s).max(8.0);
        let mut close = false;
        egui::Area::new(egui::Id::new(("archonia-options-detail", name)))
            .default_pos(layout.window_origin(&w))
            .movable(true)
            .constrain(true)
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(w.size[0], w.size[1]) * s, egui::Sense::hover());
                let painter = ui.painter().clone();
                if let Some((_, tex)) = w.textures.first() {
                    control_image(ctx, hud, assets, &painter, rect, w.size, tex, 1.0);
                }
                for c in &w.controls {
                    let r = layout.rect(rect.min, c.pos, c.size);
                    let fixed = c.display.as_deref().and_then(|d| d.strip_prefix("1:"));
                    match detail_value(c).filter(|_| c.kind == 3) {
                        Some((action, value)) => {
                            let checked = detail_get(video, action) == Some(value);
                            if checkbox(ctx, ui, hud, assets, &painter, r, c, checked, true, ("detail-radio", name, c.name.as_str())).clicked() && !checked {
                                detail_set(video, action, value);
                                changed = true;
                            }
                        }
                        None if c.kind == 3 => {
                            if button(ctx, ui, hud, assets, &painter, r, c, fixed.unwrap_or(""), font, true, ("detail-radio", name, c.name.as_str())).clicked() {
                                close = true;
                            }
                        }
                        None => {
                            if let Some((_, t)) = c.textures.first() {
                                control_image(ctx, hud, assets, &painter, r, c.size, &t.clone(), 1.0);
                            }
                            if let Some(text) = fixed.filter(|t| !t.is_empty()) {
                                shadow_text(&painter, r.left_center(), Align2::LEFT_CENTER, text, font, Color32::WHITE);
                            }
                        }
                    }
                }
            });
        if close {
            ctx.data_mut(|d| d.insert_temp(id, false));
        }
    }
    changed
}

/// Case `OP_Check` : « a » vide, « c » cochée, « b » survol, « d » inactive.
#[allow(clippy::too_many_arguments)]
pub(super) fn checkbox(ctx: &egui::Context, ui: &mut egui::Ui, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, r: Rect, c: &UiControl, checked: bool, enabled: bool, id: impl std::hash::Hash) -> egui::Response {
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

/// Lignes de l'onglet Avancé : langue, packs graphismes et sons, puis AddOns.
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
