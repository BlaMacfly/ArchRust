//! Groupe d'Archonia : cadres `UI_Party`, menus `UI_SelfCharacterMenu` / `UI_TargetCharacterMenu`
//! et options `UI_PartyOption`, à la place de la fenêtre egui « Groupe ».
//!
//! `UI_Party` (610 × 200, en 4, 6) : cadre du joueur `CONTROL_29338` (194 × 36,
//! `Party_Base_Self_*`), puis la liste `CONTROL_19570` (en 0, 45) dont chaque ligne de 36 px
//! porte le cadre d'un membre `CONTROL_22437` (`Party_Base_*`). Dans chaque cadre :
//! - jauge de PV `Party_HP_Bar.png` (37, 3, 153 × 14) ;
//! - niveau (36, 4) et nom (70, 4) ;
//! - marque du chef `Party_Leader_Mark.png` (0, 0) ;
//! - visage 32 × 32 `Party_Face_<race><classe>.png`.
//!
//! Le bouton « Menu » (`TargetMenuBase.png` + `TargetMenuButton_*`) est en 175, 0 pour le joueur,
//! et en 175, 45 + 36 × ligne pour un membre (liste `CONTROL_5282`). Un cadre mort prend
//! `Party_Base_Dead.png` (texture 4) ; la cible sélectionnée, la texture « c ».
//!
//! Écarts : les listes déroulantes d'`UI_PartyOption` (type 5) passent au choix suivant à chaque
//! clic ; les entrées du menu d'un membre qu'ArchCore ne gère pas encore (échange, guilde,
//! chuchotement, suivre, ami, ignorer) ne sont pas affichées ; les améliorations des membres
//! (grille `CONTROL_26869`) ne sont pas dessinées faute de données.

use super::dialogs_ui::ui_message;
use super::npc_ui::{button, window};
use super::*;

/// Un cadre de groupe.
#[derive(Debug, Clone)]
pub struct PartyFrame {
    pub cid: i32,
    pub name: String,
    pub level: Option<i32>,
    pub hp: Option<i32>,
    pub max_hp: Option<i32>,
    pub leader: bool,
    /// `Party_Face_XX.png` d'après la race et la classe du modèle.
    pub face: Option<String>,
    /// Statut du nom (`Status2` de `CONTROL_29971` / `CONTROL_26137`) : 0 à 2 dans la zone d'effet du chef
    /// (chevalier, archer, mage), 3 hors de la zone, 4 mort.
    pub area_status: usize,
}

/// Données affichées : le joueur d'abord, puis les autres membres.
#[derive(Debug, Clone, Default)]
pub struct PartyFrames {
    pub me: Option<PartyFrame>,
    pub others: Vec<PartyFrame>,
    /// CID de la cible actuelle (cadre en texture « c »).
    pub selected: Option<i32>,
    pub i_lead: bool,
    /// Partage actuel du butin et de l'expérience (valeurs du protocole).
    pub item_division: i32,
    pub exp_type: i32,
    /// Choix proposés : (valeur, clé de `uimessage.txt`).
    pub item_choices: Vec<(i32, &'static str)>,
    pub exp_choices: Vec<(i32, &'static str)>,
}

/// État des menus et des options, gardé d'une image à l'autre.
#[derive(Debug, Default)]
pub struct PartyUiState {
    /// Menu ouvert : CID du cadre (le joueur ou un membre).
    pub menu: Option<i32>,
    pub options: bool,
    /// Choix en cours dans `UI_PartyOption` (appliqués par « Appliquer »).
    pub pending_item: Option<i32>,
    pub pending_exp: Option<i32>,
    pub invite: String,
}

/// Demande issue des fenêtres du groupe.
#[derive(Debug, Clone, PartialEq)]
pub enum PartyUi {
    Select(i32),
    Leave,
    Banish(i32),
    Delegate(i32),
    Invite(String),
    SetItemDivision(i32),
    SetExpType(i32),
}

/// Visage d'un modèle : race (1 humain H, 2 orc O, 3 elfe de lune M, 4 scion D) et classe
/// (1 K mêlée, 2 A archer, 4 W magie, 3 S scion ; K pour les autres races).
pub fn party_face(race: i32, class: i32) -> Option<String> {
    let r = match race {
        1 => 'H',
        2 => 'O',
        3 => 'M',
        4 => 'D',
        _ => return None,
    };
    let c = match (race, class) {
        (_, 2) => 'A',
        (_, 4) => 'W',
        (4, 3) => 'S',
        _ => 'K',
    };
    Some(format!("Party_Face_{r}{c}.png"))
}

const ME: [&str; 8] = ["CONTROL_29338", "CONTROL_28779", "CONTROL_26030", "CONTROL_26137", "CONTROL_27876", "CONTROL_22448", "CONTROL_23207", "CONTROL_443"];
const MEMBER: [&str; 8] = ["CONTROL_22437", "CONTROL_29096", "CONTROL_1999", "CONTROL_29971", "CONTROL_26946", "CONTROL_13072", "CONTROL_2787", "CONTROL_546"];
/// Hauteur d'une ligne des listes `CONTROL_19570` / `CONTROL_5282`.
const ROW: f32 = 36.0;

/// Cadres du groupe, menus et options ; rien hors d'un groupe.
pub fn draw_party(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &PartyFrames, st: &mut PartyUiState, out: &mut Vec<PartyUi>) {
    let Some(me) = &view.me else {
        st.menu = None;
        st.options = false;
        return;
    };
    let Some(w) = window(hud, assets, "UI_Party") else { return };
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let origin = layout.window_origin(&w);
    let font = (10.0 * s).max(8.0);
    let mut menu_anchor: Option<(i32, Pos2)> = None;
    egui::Area::new(egui::Id::new("archonia-party"))
        .fixed_pos(origin)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let painter = ui.painter().clone();
            let rows = std::iter::once((me, &ME, 0.0)).chain(view.others.iter().enumerate().map(|(i, f)| (f, &MEMBER, i as f32 * ROW)));
            for (frame, names, dy) in rows {
                let rect_of = |name: &str| w.control(name).map(|c| (c, layout.rect(origin, [c.pos[0], c.pos[1] + dy], c.size)));
                let dead = frame.hp == Some(0);
                // Surface réservée (cadre et bouton « Menu ») : le clic ne part pas au monde.
                for n in [names[0], names[6]] {
                    if let Some((_, r)) = rect_of(n) {
                        ui.allocate_rect(r, egui::Sense::hover());
                    }
                }
                // Cadre : bouton à états (repos, survol, sélection, mort).
                if let Some((c, r)) = rect_of(names[0]) {
                    let resp = ui.interact(r, egui::Id::new(("party-frame", frame.cid)), egui::Sense::click());
                    let index = if dead {
                        4
                    } else if view.selected == Some(frame.cid) || resp.is_pointer_button_down_on() {
                        3
                    } else if resp.hovered() {
                        2
                    } else {
                        1
                    };
                    if let Some(tex) = c.textures.iter().find(|(i, _)| *i == index).or(c.textures.first()).map(|(_, t)| t.clone()) {
                        control_image(ctx, hud, assets, &painter, r, c.size, &tex, 1.0);
                    }
                    if resp.clicked() {
                        out.push(PartyUi::Select(frame.cid));
                    }
                }
                if let Some((c, r)) = rect_of(names[1]) {
                    let ratio = match (frame.hp, frame.max_hp) {
                        (Some(hp), Some(max)) if max > 0 => (hp.max(0) as f32 / max as f32).min(1.0),
                        (Some(0), _) => 0.0,
                        _ => 1.0,
                    };
                    if let Some((_, tex)) = c.textures.first() {
                        control_image(ctx, hud, assets, &painter, r, c.size, &tex.clone(), ratio);
                    }
                }
                if let (Some((_, r)), Some(level)) = (rect_of(names[2]), frame.level) {
                    shadow_text(&painter, r.left_center(), Align2::LEFT_CENTER, &level.to_string(), font, Color32::from_rgb(255, 230, 160));
                }
                // Nom à la couleur du statut de zone d'effet (`PartyActiveEffectArea*`, `PartyDisableEffectArea`,
                // `PartySetDead`).
                if let Some((c, r)) = rect_of(names[3]) {
                    let color = c.status_colors.get(frame.area_status).map_or(Color32::WHITE, |&v| super::portal_ui::argb(v));
                    shadow_text(&painter, r.left_center(), Align2::LEFT_CENTER, &frame.name, font, color);
                }
                if frame.leader {
                    if let Some((c, r)) = rect_of(names[4]) {
                        if let Some((_, tex)) = c.textures.first() {
                            control_image(ctx, hud, assets, &painter, r, c.size, &tex.clone(), 1.0);
                        }
                    }
                }
                if let (Some((c, r)), Some(face)) = (rect_of(names[5]), &frame.face) {
                    control_image(ctx, hud, assets, &painter, r, c.size, face, 1.0);
                }
                if let Some((c, r)) = rect_of(names[6]) {
                    if let Some((_, tex)) = c.textures.first() {
                        control_image(ctx, hud, assets, &painter, r, c.size, &tex.clone(), 1.0);
                    }
                }
                if let Some((c, r)) = rect_of(names[7]) {
                    let label = c.display.as_deref().and_then(|d| d.strip_prefix("1:")).unwrap_or("");
                    if button(ctx, ui, hud, assets, &painter, r, c, label, font, true, ("party-menu", frame.cid)).clicked() {
                        st.menu = if st.menu == Some(frame.cid) { None } else { Some(frame.cid) };
                    }
                    if st.menu == Some(frame.cid) {
                        menu_anchor = Some((frame.cid, r.left_bottom() + Vec2::new(0.0, 2.0 * s)));
                    }
                }
            }
        });
    if let Some((cid, at)) = menu_anchor {
        draw_menu(ctx, hud, assets, view, st, cid, at, out);
    }
    if st.options {
        draw_options(ctx, hud, assets, view, st, out);
    }
}

/// Menu d'un cadre : entrées d'`UI_SelfCharacterMenu` pour le joueur, d'`UI_TargetCharacterMenu`
/// pour un membre (celles qu'ArchCore gère), empilées comme dans le fichier d'interface.
#[allow(clippy::too_many_arguments)]
fn draw_menu(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &PartyFrames, st: &mut PartyUiState, cid: i32, at: Pos2, out: &mut Vec<PartyUi>) {
    let is_me = view.me.as_ref().is_some_and(|m| m.cid == cid);
    let (name, entries): (&str, Vec<&str>) = if is_me {
        ("UI_SelfCharacterMenu", vec!["CONTROL_29576", "CONTROL_13648"])
    } else if view.i_lead {
        ("UI_TargetCharacterMenu", vec!["CONTROL_1455", "CONTROL_7086", "CONTROL_3250"])
    } else {
        ("UI_TargetCharacterMenu", vec!["CONTROL_3250"])
    };
    let Some(w) = window(hud, assets, name) else { return };
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let font = (10.0 * s).max(8.0);
    let mut chosen = None;
    egui::Area::new(egui::Id::new(("archonia-party-menu", cid)))
        .fixed_pos(at)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let painter = ui.painter().clone();
            for (i, entry) in entries.iter().enumerate() {
                let Some(c) = w.control(entry) else { continue };
                let r = Rect::from_min_size(at + Vec2::new(0.0, i as f32 * c.size[1]) * s, Vec2::new(c.size[0], c.size[1]) * s);
                ui.allocate_rect(r, egui::Sense::hover());
                let label = c.display.as_deref().and_then(|d| d.strip_prefix("1:")).unwrap_or("");
                if button(ctx, ui, hud, assets, &painter, r, c, label, font, true, ("party-menu-entry", cid, *entry)).clicked() {
                    chosen = Some(*entry);
                }
            }
        });
    match chosen {
        Some("CONTROL_29576") => out.push(PartyUi::Leave),
        Some("CONTROL_1455") => out.push(PartyUi::Banish(cid)),
        Some("CONTROL_7086") => out.push(PartyUi::Delegate(cid)),
        Some("CONTROL_13648" | "CONTROL_3250") => st.options = true,
        _ => {}
    }
    if chosen.is_some() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        st.menu = None;
    }
}

/// Options du groupe (`UI_PartyOption`) : partage du butin et de l'XP (chef seulement), invitation
/// par nom.
fn draw_options(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &PartyFrames, st: &mut PartyUiState, out: &mut Vec<PartyUi>) {
    let Some(w) = window(hud, assets, "UI_PartyOption") else { return };
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let font = (10.5 * s).max(8.0);
    let label_of = |hud: &mut HudState, choices: &[(i32, &'static str)], v: i32| {
        choices.iter().find(|(c, _)| *c == v).map(|(_, k)| ui_message(hud, assets, k)).unwrap_or_default()
    };
    let next = |choices: &[(i32, &'static str)], v: i32| {
        let i = choices.iter().position(|(c, _)| *c == v).map_or(0, |i| (i + 1) % choices.len().max(1));
        choices.get(i).map_or(v, |(c, _)| *c)
    };
    let item = st.pending_item.unwrap_or(view.item_division);
    let exp = st.pending_exp.unwrap_or(view.exp_type);
    let mut clicked = None;
    egui::Area::new(egui::Id::new("archonia-party-options"))
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
                match (c.kind, c.name.as_str()) {
                    (5, "CONTROL_30485") | (5, "CONTROL_29990") => {
                        let item_combo = c.name == "CONTROL_30485";
                        let text = if item_combo { label_of(hud, &view.item_choices, item) } else { label_of(hud, &view.exp_choices, exp) };
                        let resp = button(ctx, ui, hud, assets, &painter, r, c, "", font, view.i_lead, ("party-combo", &c.name));
                        shadow_text(&painter, r.left_center() + Vec2::new(4.0 * s, 0.0), Align2::LEFT_CENTER, &text, font, Color32::WHITE);
                        if resp.clicked() {
                            clicked = Some(c.name.clone());
                        }
                    }
                    (2, "CONTROL_6401") => {
                        let edit = egui::TextEdit::singleline(&mut st.invite)
                            .id(egui::Id::new("archonia-party-invite"))
                            .frame(false)
                            .font(FontId::proportional(font))
                            .text_color(Color32::WHITE);
                        let resp = ui.put(r, edit);
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            clicked = Some("CONTROL_8003".into());
                        }
                    }
                    (3, name) => {
                        let enabled = !matches!(name, "CONTROL_11091") || view.i_lead;
                        if button(ctx, ui, hud, assets, &painter, r, c, fixed.unwrap_or(""), font, enabled, ("party-option", name)).clicked() {
                            clicked = Some(name.to_string());
                        }
                    }
                    _ => {
                        if let Some(text) = fixed {
                            shadow_text(&painter, r.left_center(), Align2::LEFT_CENTER, text, font, Color32::from_rgb(255, 230, 160));
                        }
                    }
                }
            }
        });
    match clicked.as_deref() {
        Some("CONTROL_30485" | "CONTROL_16250") if view.i_lead => st.pending_item = Some(next(&view.item_choices, item)),
        Some("CONTROL_29990" | "CONTROL_19560") if view.i_lead => st.pending_exp = Some(next(&view.exp_choices, exp)),
        Some("CONTROL_8003") => {
            let name = st.invite.trim().to_string();
            if !name.is_empty() {
                out.push(PartyUi::Invite(name));
                st.invite.clear();
            }
        }
        Some("CONTROL_11091") => {
            if let Some(v) = st.pending_item.take().filter(|v| *v != view.item_division) {
                out.push(PartyUi::SetItemDivision(v));
            }
            if let Some(v) = st.pending_exp.take().filter(|v| *v != view.exp_type) {
                out.push(PartyUi::SetExpType(v));
            }
        }
        Some("CONTROL_15538" | "CONTROL_18572") => {
            st.options = false;
            st.pending_item = None;
            st.pending_exp = None;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::party_face;

    #[test]
    fn faces_follow_race_and_class_letters() {
        assert_eq!(party_face(1, 1).as_deref(), Some("Party_Face_HK.png"));
        assert_eq!(party_face(2, 4).as_deref(), Some("Party_Face_OW.png"));
        assert_eq!(party_face(3, 2).as_deref(), Some("Party_Face_MA.png"));
        assert_eq!(party_face(4, 3).as_deref(), Some("Party_Face_DS.png"));
        assert_eq!(party_face(1, 3).as_deref(), Some("Party_Face_HK.png"));
        assert_eq!(party_face(11, 5), None);
    }
}
