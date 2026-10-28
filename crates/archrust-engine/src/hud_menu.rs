//! Menus principaux d'Archonia : les boutons `MainBtnCharacter`, `MainBtnCommunity`,
//! `MainBtnWorld`, `MainBtnChantra` et `MainBtnOption` de `UI_CharInfo` (35 × 54, textures
//! `main_button_<n><a|b|c|d>.png`) ouvrent chacun leur liste (`UI_MainMenuCharacter`…, entrées
//! 92 × 21 `CB_Button_Small*`, posées par `UIPos=1`). Ils remplacent les boutons egui
//! « Inventaire » et « Groupe ».
//!
//! Écart : seules les entrées dont la fenêtre existe dans ArchRust sont affichées (quêtes,
//! inventaire, personnage, compétences de combat et de production, amis, ignorés, courrier, guilde, groupe, options, menu) ; un bouton
//! sans entrée gérée (monde, Chantra) reste inactif.

use super::npc_ui::{button, window};
use super::*;

/// Fenêtre demandée depuis un menu principal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuUi {
    Quests,
    Inventory,
    Character,
    Skills,
    /// Compétences de production (`UI_PDSkill`, `MenuBtnProductSkill`).
    ProductSkills,
    Guild,
    /// Liste d'amis (`UI_BuddyList`, `MenuBtnBuddyList`).
    Friends,
    /// Liste d'ignorés (`UI_BanList`, `MenuBtnBanList`).
    IgnoreList,
    /// Boîte aux lettres (`MailBox`, `MenuBtn`).
    Mailbox,
    /// Salons de discussion : « Create Room » (`UI_CreateChatRoom`) et « Enter Chat » (`UI_EnterChatRoom`).
    CreateChannel,
    JoinChannel,
    Party,
    Options,
    Exit,
}

/// Bouton principal → (liste, entrées gérées).
const MENUS: [(&str, &str, &[(&str, MenuUi)]); 5] = [
    (
        "MainBtnCharacter",
        "UI_MainMenuCharacter",
        &[("MenuBtnQuest", MenuUi::Quests), ("MenuBtnInventory", MenuUi::Inventory), ("MenuBtnEquip", MenuUi::Character), ("MenuBtnCombatSkill", MenuUi::Skills), ("MenuBtnProductSkill", MenuUi::ProductSkills)],
    ),
    (
        "MainBtnCommunity",
        "UI_MainMenuCommunity",
        &[("MenuBtnBuddyList", MenuUi::Friends), ("MenuBtnBanList", MenuUi::IgnoreList), ("MenuBtnCreateChannel", MenuUi::CreateChannel), ("MenuBtnJoinChannel", MenuUi::JoinChannel), ("MenuBtn", MenuUi::Mailbox), ("MenuBtnGuild", MenuUi::Guild), ("CONTROL_15350", MenuUi::Party)],
    ),
    ("MainBtnWorld", "UI_MainMenuWorld", &[]),
    ("MainBtnChantra", "UI_MainMenuChantra", &[]),
    ("MainBtnOption", "UI_MainMenuOptions", &[("MenuBtnEnvironment", MenuUi::Options), ("MenuBtnExit", MenuUi::Exit)]),
];

/// Noms des fenêtres de liste à charger avec le reste de l'interface.
pub(super) const MENU_WINDOWS: [&str; 5] = ["UI_MainMenuCharacter", "UI_MainMenuCommunity", "UI_MainMenuWorld", "UI_MainMenuChantra", "UI_MainMenuOptions"];

/// Échap sans fenêtre ouverte : liste du bouton `MainBtnOption` (`UI_MainMenuOptions`), ou la ferme.
pub fn toggle_options_menu(hud: &mut HudState) {
    const LIST: &str = "UI_MainMenuOptions";
    hud.main_menu = if hud.main_menu.as_deref() == Some(LIST) { None } else { Some(LIST.to_string()) };
}

/// Vrai si une liste de menu principal est ouverte.
pub fn main_menu_open(hud: &HudState) -> bool {
    hud.main_menu.is_some()
}

/// Boutons principaux (cliquables, survol et appui) et liste ouverte ; les choix s'ajoutent à
/// `hud.menu_requests`, que le moteur traite à l'image suivante.
pub fn draw_main_menu(ctx: &egui::Context, hud: &mut HudState, assets: &Assets) {
    let mut open = hud.main_menu.take();
    let mut out = Vec::new();
    draw_menus(ctx, hud, assets, &mut open, &mut out);
    hud.main_menu = open;
    hud.menu_requests.extend(out);
}

fn draw_menus(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, open: &mut Option<String>, out: &mut Vec<MenuUi>) {
    let Some(bar) = window(hud, assets, "UI_CharInfo") else { return };
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let origin = layout.window_origin(&bar);
    let font = (10.0 * s).max(8.0);
    egui::Area::new(egui::Id::new("archonia-main-buttons"))
        .fixed_pos(origin)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let painter = ui.painter().clone();
            for (btn, list, entries) in MENUS {
                let Some(c) = bar.control(btn) else { continue };
                let r = layout.rect(origin, c.pos, c.size);
                // Surface réservée : le clic ne part pas au monde (marche) sous les boutons.
                ui.allocate_rect(r, egui::Sense::hover());
                let enabled = !entries.is_empty();
                if button(ctx, ui, hud, assets, &painter, r, c, "", font, enabled, ("main-button", btn)).clicked() {
                    *open = if open.as_deref() == Some(list) { None } else { Some(list.to_string()) };
                }
            }
        });
    let Some(list) = open.clone() else { return };
    let Some((_, _, entries)) = MENUS.iter().find(|(_, l, _)| *l == list) else { return };
    let Some(w) = window(hud, assets, &list) else { return };
    let at = layout.window_origin(&w);
    let mut chosen = None;
    egui::Area::new(egui::Id::new(("archonia-main-menu", &list)))
        .fixed_pos(at)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let painter = ui.painter().clone();
            let mut y = 0.0;
            for (name, action) in entries.iter() {
                let Some(c) = w.control(name) else { continue };
                let r = Rect::from_min_size(at + Vec2::new(0.0, y) * s, Vec2::new(c.size[0], c.size[1]) * s);
                y += c.size[1];
                ui.allocate_rect(r, egui::Sense::hover());
                let label = c.display.as_deref().and_then(|d| d.strip_prefix("1:")).unwrap_or("");
                if button(ctx, ui, hud, assets, &painter, r, c, label, font, true, ("main-menu-entry", &list, *name)).clicked() {
                    chosen = Some(*action);
                }
            }
        });
    if let Some(a) = chosen {
        out.push(a);
        *open = None;
    } else if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        *open = None;
    }
}
