//! Onglets de la discussion d'Archonia (AgcmUIChatting2) : menus flottants et `UI_ChatTab_Option`.
//!
//! * **Bouton `MainChat_Btn_Option`** (`FUN_140512880` → `FUN_1407a3b80`, menu construit par `FUN_1407a4a30`) : « Font
//!   Size » (sysstr 833, sous-menu 12, 13, 14, 15 → polices 2, 0, 9, 4, `FUN_1407a21e0`), « Tab Function » (834,
//!   sous-menu ON / OFF, `LAB_1407a22b0`), « Add Tab » (835 → `FUN_140510ce0`, `TabOptionAdd_OpenUI`). Le choix courant
//!   est grisé. Menu dessiné avec `PopupBtn_Base1…9` et `PopupBtn_Select1…3` (`0x140784f9b`).
//! * **Menu d'un onglet** (`FUN_1407a4160`, onglet non permanent `+0x24 == 0`) : « Delete Menu » (814, `FUN_1405129d0`),
//!   « Change Menu » (815 → `FUN_140512aa0`, `TabOptionEdit_OpenUI`). ArchRust tient le premier onglet pour le
//!   permanent, et ouvre ce menu au clic droit (déduit).
//! * **`UI_ChatTab_Option`** (`Chat_InitOpenAddTab` `FUN_140511840` : nom vide, rien de coché ; `Chat_InitOpenEditTab`
//!   `FUN_140511900` : nom et bits de l'onglet) : 12 canaux, bits 0 à 11 du masque (`FUN_140516320`), libellés
//!   sysstr 821, 822, 823, 824, 825, 828, 829, 830, 836, 838, 839, 840 (`FUN_140516e10`), couleurs de `color.ini`
//!   (`FUN_140516c60`). OK (`FUN_140511b70`) : nom vide → `TabName_NotFound` ; en ajout, nom déjà pris →
//!   `TabName_Duplicate` ; ces deux clés manquent aux `uimessage` du client, donc rien ne s'affiche et la fenêtre reste
//!   ouverte, comme dans Archonia (`FUN_1405b8640` nul). 8 onglets au plus (`FUN_1407a1e60`).
//! * **Enregistrement** : à la connexion, `INI\ChatSetting.SYJ` puis `ini\CUSTOMDATA\<personnage>.SYJ`
//!   (`FUN_1404d7660`), réécrit après un changement (`+0x168`). ArchRust garde ce format dans ses SavedVariables
//!   (`chatsetting-<personnage>.syj`, UTF-8) et, sans fichier, ses quatre onglets `Chat_Tab_*`. Une seule fenêtre de
//!   discussion : les `CreateChat` suivants d'un fichier sont fusionnés à la première.

use crate::assets::Assets;
use crate::hud::{ChatMenuAction, HudState, TabOptionUi};
use crate::ui::{ChatMenu, UiState};
use std::path::{Path, PathBuf};

/// Tailles des choix de « Font Size » : (police d'Archonia, libellé).
pub const FONT_CHOICES: [(u8, f32); 4] = [(2, 12.0), (0, 13.0), (9, 14.0), (4, 15.0)];
/// 8 onglets au plus (`FUN_1407a1e60`).
pub const MAX_TABS: usize = 8;
const ALL_BITS: u16 = 0xFFF;

/// `UI_ChatTab_Option` ouverte.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TabOption {
    /// Onglet modifié ; `None` : ajout (`+0x210`).
    pub edit: Option<usize>,
    pub name: String,
    pub mask: u16,
}

#[derive(Default)]
pub struct ChatTabsGlue {
    option: Option<TabOption>,
    /// Personnage dont les réglages sont chargés.
    loaded_for: Option<String>,
    /// Lu depuis un fichier, ou changé : à réécrire.
    dirty: bool,
}

/// Réglages d'un fichier `.SYJ`.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatSetting {
    pub font: u8,
    pub tab_enable: bool,
    pub tabs: Vec<(String, u16)>,
}

/// `<Option Font="0" TabEnable="1" />`, `<Tab Name="General" MsgType=0#1#2 />`.
pub fn parse_setting(text: &str) -> ChatSetting {
    let quoted = |line: &str, key: &str| -> Option<String> {
        let rest = line.split_once(key)?.1.trim_start().strip_prefix('=')?.trim_start().strip_prefix('"')?;
        Some(rest[..rest.find('"')?].to_string())
    };
    let mut out = ChatSetting { font: 0, tab_enable: true, tabs: Vec::new() };
    for line in text.lines().map(str::trim) {
        if line.starts_with("<Option") {
            out.font = quoted(line, "Font").and_then(|v| v.parse().ok()).unwrap_or(0);
            out.tab_enable = quoted(line, "TabEnable").is_none_or(|v| v != "0");
        } else if line.starts_with("<Tab") && out.tabs.len() < MAX_TABS {
            let Some(name) = quoted(line, "Name") else { continue };
            let bits = line.split_once("MsgType=").map_or("", |(_, r)| r.split(['/', ' ']).next().unwrap_or(""));
            let mask = bits.split('#').filter_map(|b| b.trim().parse::<u32>().ok()).filter(|b| *b < 12).fold(0u16, |m, b| m | 1 << b);
            out.tabs.push((name, if mask & ALL_BITS == ALL_BITS { u16::MAX } else { mask }));
        }
    }
    out
}

/// Même format ; position et taille de la fenêtre sont celles du fichier d'origine (ArchRust les fixe).
pub fn format_setting(s: &ChatSetting) -> String {
    let mut out = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\n<ChatBase>\n<CreateChat Index=0>\n<Position X=\"0.000000\" Y=\"0.584000\" />\n<Size Width=\"370\" Height=\"200\" />\n<Option Font=\"{}\" TabEnable=\"{}\" />\n", s.font, s.tab_enable as u8);
    for (name, mask) in &s.tabs {
        let bits: Vec<String> = (0..12).filter(|b| mask & (1 << b) != 0).map(|b: u32| b.to_string()).collect();
        out.push_str(&format!("<Tab Name=\"{}\" MsgType={} />\n", name.replace('"', ""), bits.join("#")));
    }
    out.push_str("</CreateChat>\n</ChatBase>\n");
    out
}

/// OK de la fenêtre (`FUN_140511b70`) : rend `true` si la fenêtre se ferme.
pub fn apply_option(tabs: &mut Vec<(String, u16)>, o: &TabOption) -> bool {
    let name = o.name.trim();
    if name.is_empty() {
        return false;
    }
    match o.edit {
        Some(i) => {
            if let Some(t) = tabs.get_mut(i) {
                *t = (name.to_string(), o.mask);
            }
        }
        None => {
            if tabs.iter().any(|(n, _)| n == name) {
                return false;
            }
            // Au-delà de 8, `FUN_1407a1e60` refuse sans rien dire ; la fenêtre se ferme.
            if tabs.len() < MAX_TABS {
                tabs.push((name.to_string(), o.mask));
            }
        }
    }
    true
}

fn setting_path(dir: &Path, name: &str) -> PathBuf {
    let safe: String = name.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
    dir.join(format!("chatsetting-{safe}.syj"))
}

fn apply_setting(ui: &mut UiState, s: ChatSetting) {
    ui.chat_font = s.font;
    ui.chat_tab_enable = s.tab_enable;
    ui.chat_tabs = s.tabs;
    ui.chat_tab = 0;
}

/// Menus et fenêtre de l'image ; réglages du personnage lus à son arrivée, réécrits après un changement.
pub fn panel(ctx: &egui::Context, glue: &mut ChatTabsGlue, owner: &str, saved_dir: &Path, hud: &mut HudState, assets: &Assets, ui: &mut UiState) {
    let me = Some(owner.to_string()).filter(|n| !n.is_empty());
    if me != glue.loaded_for {
        if let Some(name) = &me {
            // Sans fichier : réglages par défaut (les onglets vidés sont refaits par `guild_frame`).
            let setting = std::fs::read_to_string(setting_path(saved_dir, name)).map(|t| parse_setting(&t)).unwrap_or(ChatSetting { font: 0, tab_enable: true, tabs: Vec::new() });
            apply_setting(ui, setting);
        }
        glue.loaded_for = me;
        glue.option = None;
    }
    let mut action = None;
    if let Some(menu) = ui.chat_menu {
        if matches!(menu, ChatMenu::Tab { index: 0, .. }) {
            ui.chat_menu = None;
        } else {
            match crate::hud::draw_chat_menu(ctx, hud, assets, menu, ui.chat_font, ui.chat_tab_enable) {
                Some(ChatMenuAction::Close) => ui.chat_menu = None,
                Some(a) => {
                    ui.chat_menu = None;
                    action = Some((a, menu));
                }
                None => {}
            }
        }
    }
    if let Some((a, menu)) = action {
        match a {
            ChatMenuAction::Font(f) => {
                ui.chat_font = f;
                glue.dirty = true;
            }
            ChatMenuAction::TabEnable(on) => {
                ui.chat_tab_enable = on;
                glue.dirty = true;
            }
            ChatMenuAction::AddTab => glue.option = Some(TabOption::default()),
            ChatMenuAction::Delete => {
                if let ChatMenu::Tab { index, .. } = menu {
                    if index < ui.chat_tabs.len() {
                        ui.chat_tabs.remove(index);
                        ui.chat_tab = ui.chat_tab.min(ui.chat_tabs.len().saturating_sub(1));
                        glue.dirty = true;
                    }
                }
            }
            ChatMenuAction::Change => {
                if let ChatMenu::Tab { index, .. } = menu {
                    if let Some((name, mask)) = ui.chat_tabs.get(index).cloned() {
                        glue.option = Some(TabOption { edit: Some(index), name, mask: if mask == u16::MAX { ALL_BITS } else { mask } });
                    }
                }
            }
            ChatMenuAction::Close => {}
        }
    }
    if let Some(mut o) = glue.option.take() {
        match crate::hud::draw_tab_option(ctx, hud, assets, &mut o.name, &mut o.mask) {
            Some(TabOptionUi::Ok) => {
                let mut applied = o.clone();
                if applied.mask & ALL_BITS == ALL_BITS {
                    applied.mask = u16::MAX;
                }
                if apply_option(&mut ui.chat_tabs, &applied) {
                    glue.dirty = true;
                } else {
                    glue.option = Some(o);
                }
            }
            Some(TabOptionUi::Close) => {}
            None => glue.option = Some(o),
        }
    }
    if std::mem::take(&mut glue.dirty) {
        if let Some(name) = &glue.loaded_for {
            let s = ChatSetting { font: ui.chat_font, tab_enable: ui.chat_tab_enable, tabs: ui.chat_tabs.clone() };
            if let Err(e) = std::fs::write(setting_path(saved_dir, name), format_setting(&s)) {
                log::warn!("réglages de discussion : {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fichier_de_reglages() {
        // `chatsetting.syj` du client : deux `CreateChat`, fusionnés ici.
        let s = parse_setting("<ChatBase>\n<CreateChat Index=0>\n<Option Font=\"9\" TabEnable=\"0\" />\n<Tab Name=\"General\" MsgType=0#1#2#3#5#6#7#8#9#10#11 />\n</CreateChat>\n<CreateChat Index=1>\n<Tab Name=\"System\" MsgType=4#5#  />\n</CreateChat>\n");
        assert_eq!((s.font, s.tab_enable), (9, false));
        assert_eq!(s.tabs, vec![("General".to_string(), 0xFEF), ("System".to_string(), 0x30)]);
        // Aller-retour ; tous les canaux reviennent à « tout ».
        let s2 = ChatSetting { font: 2, tab_enable: true, tabs: vec![("Tout".into(), u16::MAX), ("Quêtes".into(), 0x40)] };
        assert_eq!(parse_setting(&format_setting(&s2)), s2);
    }

    #[test]
    fn regles_du_bouton_ok() {
        let mut tabs = vec![("A".to_string(), 1u16)];
        let add = |name: &str| TabOption { edit: None, name: name.into(), mask: 2 };
        assert!(!apply_option(&mut tabs, &add("  ")));
        assert!(!apply_option(&mut tabs, &add("A")));
        assert!(apply_option(&mut tabs, &add("B")));
        assert_eq!(tabs[1], ("B".to_string(), 2));
        // Modifier : le nom peut être repris (le contrôle de doublon ne vaut qu'en ajout).
        assert!(apply_option(&mut tabs, &TabOption { edit: Some(1), name: "A".into(), mask: 4 }));
        assert_eq!(tabs[1], ("A".to_string(), 4));
        for i in 0..10 {
            apply_option(&mut tabs, &add(&format!("T{i}")));
        }
        assert_eq!(tabs.len(), MAX_TABS);
    }
}
