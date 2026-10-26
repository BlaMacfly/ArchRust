//! Forgeron d'Archonia (fonction 22 d'un PNJ) : fenêtres `UI_ConvertPhysics` (renforcement avec une
//! potion), `UI_ConvertSocket` (« anneau de contrat ») et `UI_ConvertRune` (rune, « Prayer Stone »), qui
//! partagent la même place et passent de l'une à l'autre par leurs onglets ; confirmation
//! `UI_ConvertConfirm` ; infobulle d'un objet (nom, rang, sockets, pierres).
//!
//! Tout vient d'`UI_1024X768.ini` (fonds, onglets, grilles, boutons, libellés d'`ini_fr`). La fenêtre
//! du renforcement n'a pas d'onglets dans le fichier : ceux d'`UI_ConvertRune` (même fond, même
//! taille, mêmes positions) y sont reportés pour pouvoir revenir aux deux autres. Les résultats
//! s'écrivent dans la zone de texte de la fenêtre (`CONTROL_*Edit`) avec les codes de couleur
//! d'Archonia (`<C10092288>` vert, `<C16711680>` rouge, `<C16773320>` crème : RVB en décimal).

use super::items_ui::{icon, ItemDrag};
use super::npc_ui::{button, window};
use super::*;

/// Fenêtres du forgeron, chargées avec le HUD (libellés d'`ini_fr`).
pub(super) const CONVERT_WINDOWS: [&str; 4] = ["UI_ConvertPhysics", "UI_ConvertSocket", "UI_ConvertRune", "UI_ConvertConfirm"];

/// Onglet du forgeron.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConvertTab {
    /// Renforcement (« Niveau de base / Amélioration »).
    #[default]
    Physical,
    /// Ajout de socket (« Contrat de / Créer un anneau »).
    Socket,
    /// Rune (« Prayer Stone / Équiper »).
    Rune,
}

impl ConvertTab {
    fn window(self) -> &'static str {
        match self {
            ConvertTab::Physical => "UI_ConvertPhysics",
            ConvertTab::Socket => "UI_ConvertSocket",
            ConvertTab::Rune => "UI_ConvertRune",
        }
    }

    /// Onglet d'un bouton `Common_Button_Tab_Large` d'après sa colonne (9, 79, 149).
    fn of_tab_button(x: f32) -> ConvertTab {
        if x < 60.0 {
            ConvertTab::Physical
        } else if x < 130.0 {
            ConvertTab::Socket
        } else {
            ConvertTab::Rune
        }
    }
}

/// Ce que la fenêtre montre.
pub struct ConvertView<'a> {
    pub tab: ConvertTab,
    /// Objet posé : TID et ligne de nom (« Nom + 2 [1/2] »).
    pub item: Option<(i32, String)>,
    /// Potion ou pierre posée : TID.
    pub material: Option<i32>,
    /// Texte de la zone de résultat, avec codes `<C…>`.
    pub edit: &'a str,
    /// Prix d'un socket (table des coûts du client vide : 0) et or porté.
    pub cost: i64,
    pub money: i64,
}

/// Demande des fenêtres du forgeron.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvertUi {
    Close,
    Tab(ConvertTab),
    /// Objet du sac lâché sur la case de l'objet à améliorer.
    DropItem(ItemDrag),
    /// Objet du sac lâché sur la case de la potion ou de la pierre.
    DropMaterial(ItemDrag),
    /// Clic droit sur une case : la vider.
    ClearItem,
    ClearMaterial,
    /// Bouton d'amélioration (`Convert_Start`).
    Start,
}

/// Couleur d'un code `<C…>` d'Archonia (RVB en décimal).
fn decimal_color(code: &str) -> Option<Color32> {
    let v: u32 = code.parse().ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// Texte à codes `<C…>` en segments colorés.
/// Couleur d'un code `<H…>` d'Archonia (RVB, ou ARVB, en hexadécimal).
fn hex_color(code: &str) -> Option<Color32> {
    let v = u32::from_str_radix(code, 16).ok().filter(|_| code.len() == 6 || code.len() == 8)?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// Texte à codes `<C…>` (RVB décimal) ou `<H…>` (RVB hexadécimal) en segments colorés.
pub(super) fn colored_segments(text: &str, default: Color32) -> Vec<(String, Color32)> {
    let mut out = Vec::new();
    let mut color = default;
    let mut rest = text;
    while let Some(i) = rest.find("<C").into_iter().chain(rest.find("<H")).min() {
        let tail = &rest[i + 2..];
        let Some(end) = tail.find('>') else { break };
        let parsed = if rest[i..].starts_with("<C") { decimal_color(&tail[..end]) } else { hex_color(&tail[..end]) };
        let Some(c) = parsed else {
            out.push((rest[..i + 2].to_string(), color));
            rest = tail;
            continue;
        };
        if i > 0 {
            out.push((rest[..i].to_string(), color));
        }
        color = c;
        rest = &tail[end + 1..];
    }
    if !rest.is_empty() {
        out.push((rest.to_string(), color));
    }
    out
}

/// Texte à codes `<C…>` sans ses codes.
pub fn colored_text(text: &str) -> String {
    colored_segments(text, Color32::WHITE).into_iter().map(|(t, _)| t).collect()
}

/// Zone de texte d'Archonia (`CtrlType` 2) : texte coloré, centré, coupé à la largeur.
pub(super) fn edit_box(painter: &egui::Painter, r: Rect, text: &str, font: f32) {
    edit_box_aligned(painter, r, text, font, egui::Align::Center);
}

/// Zone de texte coloré alignée (`___` et `\n` : retours à la ligne), coupée à la largeur.
pub(super) fn edit_box_aligned(painter: &egui::Painter, r: Rect, text: &str, font: f32, align: egui::Align) {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = r.width() - 6.0;
    job.halign = align;
    for (part, color) in colored_segments(&text.replace("___", "\n"), Color32::WHITE) {
        job.append(&part, 0.0, egui::TextFormat { font_id: FontId::proportional(font), color, ..Default::default() });
    }
    let galley = painter.layout_job(job);
    let x = match align {
        egui::Align::Center => r.center().x,
        egui::Align::Min => r.left() + 3.0,
        egui::Align::Max => r.right() - 3.0,
    };
    painter.with_clip_rect(r).galley(Pos2::new(x, r.top() + 4.0), galley, Color32::WHITE);
}

/// Couleur de texte d'un contrôle (`CtrlFont`, ARGB, -1 blanc).
fn font_color(c: &UiControl) -> Color32 {
    match c.font.map(|(color, _)| color) {
        Some(v) if v != u32::MAX => Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8),
        _ => Color32::WHITE,
    }
}

/// Fenêtre du forgeron de l'onglet choisi.
pub fn draw_convert(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: ConvertView, out: &mut Vec<ConvertUi>) {
    let Some(w) = window(hud, assets, view.tab.window()) else { return };
    // Onglets et libellés d'`UI_ConvertRune`, reportés sur le renforcement qui n'en a pas.
    let borrowed: Vec<UiControl> = if view.tab == ConvertTab::Physical {
        window(hud, assets, "UI_ConvertRune")
            .map(|r| {
                let tabs: Vec<UiControl> = r.controls.iter().filter(|c| c.kind == 3 && c.textures.iter().any(|(_, t)| t.contains("Tab_Large"))).cloned().collect();
                let inside = |c: &UiControl| tabs.iter().any(|t| c.pos[0] >= t.pos[0] && c.pos[0] < t.pos[0] + t.size[0] && c.pos[1] >= t.pos[1] && c.pos[1] < t.pos[1] + t.size[1]);
                let labels: Vec<UiControl> = r.controls.iter().filter(|c| c.kind == 1 && inside(c)).cloned().collect();
                tabs.into_iter().chain(labels).collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let size = Vec2::new(w.size[0], w.size[1]) * s;
    let dragging = egui::DragAndDrop::payload::<ItemDrag>(ctx).map(|p| *p);
    egui::Area::new(egui::Id::new("archonia-convert"))
        .default_pos(layout.window_origin(&w))
        .movable(true)
        .constrain(true)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let origin = rect.min;
            let painter = ui.painter().clone();
            if let Some((_, tex)) = w.textures.first() {
                control_image(ctx, hud, assets, &painter, rect, w.size, tex, 1.0);
            }
            let font = (11.0 * s).max(9.0);
            let under_tab = |c: &UiControl| {
                borrowed.iter().any(|t| t.kind == 3 && c.pos[1] + c.size[1] > t.pos[1] && c.pos[1] < t.pos[1] + t.size[1] && c.pos[0] + c.size[0] > t.pos[0] && c.pos[0] < t.pos[0] + t.size[0])
            };
            for c in w.controls.iter().filter(|c| c.kind != 1 || !under_tab(c)).chain(borrowed.iter()) {
                let r = layout.rect(origin, c.pos, c.size);
                let fixed = c.display.as_deref().and_then(|d| d.strip_prefix("1:"));
                let tex_has = |part: &str| c.textures.iter().any(|(_, t)| t.contains(part));
                match c.kind {
                    3 if tex_has("Button_X") => {
                        if button(ctx, ui, hud, assets, &painter, r, c, "", font, true, ("convert-x", &c.name)).clicked() {
                            out.push(ConvertUi::Close);
                        }
                    }
                    3 if tex_has("Tab_Large") => {
                        let tab = ConvertTab::of_tab_button(c.pos[0]);
                        if tab == view.tab {
                            // Onglet choisi : texture « appuyé » (`Button=3:3:3:4`), sans clic.
                            if let Some(t) = c.textures.iter().find(|(_, t)| t.to_ascii_lowercase().ends_with("c.png")).map(|(_, t)| t.clone()) {
                                control_image(ctx, hud, assets, &painter, r, c.size, &t, 1.0);
                            }
                        } else if button(ctx, ui, hud, assets, &painter, r, c, "", font, true, ("convert-tab", &c.name)).clicked() {
                            out.push(ConvertUi::Tab(tab));
                        }
                    }
                    3 => {
                        if button(ctx, ui, hud, assets, &painter, r, c, fixed.unwrap_or(""), font, true, ("convert-start", &c.name)).clicked() {
                            out.push(ConvertUi::Start);
                        }
                    }
                    8 => {
                        let material = c.grid_name.as_deref() == Some("Convert_Grid_Socket");
                        let tid = if material { view.material } else { view.item.as_ref().map(|(t, _)| *t) };
                        let resp = ui.interact(r, egui::Id::new(("convert-grid", &c.name)), egui::Sense::click());
                        if tid.is_some() {
                            icon(ctx, hud, assets, &painter, r, tid, Color32::WHITE);
                        }
                        if resp.secondary_clicked() && tid.is_some() {
                            out.push(if material { ConvertUi::ClearMaterial } else { ConvertUi::ClearItem });
                        }
                        if let Some(drag) = resp.dnd_release_payload::<ItemDrag>() {
                            if !drag.worn && !drag.bank {
                                out.push(if material { ConvertUi::DropMaterial(*drag) } else { ConvertUi::DropItem(*drag) });
                            }
                        }
                        let target = dragging.is_some() && resp.contains_pointer();
                        if resp.hovered() || target {
                            let color = if target { Color32::from_rgb(255, 210, 110) } else { Color32::from_rgb(120, 200, 255) };
                            painter.rect_stroke(r, 2.0, egui::Stroke::new(1.0_f32, color), egui::StrokeKind::Inside);
                        }
                    }
                    2 => edit_box(&painter, r, view.edit, font),
                    1 => {
                        let display = c.display.as_deref().unwrap_or("");
                        let (text, color) = if display.contains("Display_ConvertItemName") {
                            (view.item.as_ref().map(|(_, n)| n.clone()).unwrap_or_default(), font_color(c))
                        } else if display.contains("Display_ConvertCostPayResult") {
                            let left = view.money - view.cost;
                            let status = c.status_colors.get(if left >= 0 { 0 } else { 1 }).copied();
                            let color = status.map(|v| Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)).unwrap_or(Color32::WHITE);
                            (left.to_string(), color)
                        } else if display.contains("Display_ConvertCost") {
                            (view.cost.to_string(), font_color(c))
                        } else if display.contains("InventoryMoney") {
                            (view.money.to_string(), font_color(c))
                        } else {
                            (fixed.unwrap_or("").to_string(), font_color(c))
                        };
                        if !text.is_empty() {
                            let align = match c.font.map(|(_, a)| a) {
                                Some(16) => Align2::LEFT_CENTER,
                                Some(18) | Some(2) => Align2::RIGHT_CENTER,
                                _ => Align2::CENTER_CENTER,
                            };
                            let at = match align {
                                Align2::LEFT_CENTER => r.left_center(),
                                Align2::RIGHT_CENTER => r.right_center(),
                                _ => r.center(),
                            };
                            // Police réduite si le texte dépasse le contrôle (onglets « Niveau de base »…).
                            let width = painter.layout_no_wrap(text.clone(), FontId::proportional(font), color).size().x;
                            let fitted = if width > r.width() && width > 0.0 { (font * r.width() / width).max(6.0) } else { font };
                            shadow_text(&painter, at, align, &text, fitted, color);
                        }
                    }
                    _ => {}
                }
            }
        });
}

/// Confirmation du forgeron (`UI_ConvertConfirm`) : phrase dans `CONTROL_16073`, « Confirmer »
/// (`Convert_OK`) ou « Annuler » / croix (`Convert_Cancel`).
pub fn draw_convert_confirm(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, text: &str) -> Option<bool> {
    let out = super::dialogs_ui::draw_dialog(ctx, hud, assets, "UI_ConvertConfirm", |_ui, _hud, painter, c, r, font| {
        if c.kind == 2 {
            edit_box(painter, r, text, font);
            return true;
        }
        false
    })?;
    match out.clicked.as_deref() {
        Some("CONTROL_26951") => return Some(true),
        Some("CONTROL_21410" | "CONTROL_29110") => return Some(false),
        _ => {}
    }
    if out.enter {
        return Some(true);
    }
    out.escape.then_some(false)
}

/// Infobulle d'un objet, près du curseur : lignes colorées (couleurs `[ItemTooltip]` de `color.ini`),
/// les vides faisant un intervalle, puis l'aide d'ArchRust en gris.
pub fn item_tooltip(ctx: &egui::Context, lines: &[(String, Color32)], hint: &str) {
    if let Some(pos) = ctx.pointer_latest_pos() {
        item_tooltip_at(ctx, pos, lines, hint);
    }
}

/// Infobulle d'un objet près du point `pos`.
pub fn item_tooltip_at(ctx: &egui::Context, pos: Pos2, lines: &[(String, Color32)], hint: &str) {
    let s = Layout::new(ctx.screen_rect()).scale;
    let font = (12.0 * s).max(9.0);
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("archrust-item-tooltip")));
    let mut galleys = Vec::new();
    for (i, (text, color)) in lines.iter().enumerate() {
        let size = if i == 0 { font * 1.15 } else { font };
        galleys.push(painter.layout(if text.is_empty() { " ".into() } else { text.clone() }, FontId::proportional(size), *color, 320.0 * s));
    }
    if !hint.is_empty() {
        galleys.push(painter.layout(hint.to_string(), FontId::proportional(font * 0.9), Color32::from_gray(160), 320.0 * s));
    }
    let width = galleys.iter().map(|g| g.size().x).fold(0.0, f32::max) + 16.0;
    let height = galleys.iter().map(|g| g.size().y + 2.0).sum::<f32>() + 12.0;
    let screen = ctx.screen_rect();
    let mut min = pos + Vec2::new(18.0, 18.0);
    if min.x + width > screen.right() {
        min.x = (pos.x - width - 8.0).max(screen.left());
    }
    if min.y + height > screen.bottom() {
        min.y = (screen.bottom() - height).max(screen.top());
    }
    let rect = Rect::from_min_size(min, Vec2::new(width, height));
    painter.rect_filled(rect, 2.0, Color32::from_black_alpha(210));
    painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0_f32, Color32::from_gray(110)), egui::StrokeKind::Inside);
    let mut y = rect.top() + 6.0;
    for g in galleys {
        let h = g.size().y;
        painter.galley(Pos2::new(rect.left() + 8.0, y), g, Color32::WHITE);
        y += h + 2.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_de_couleur() {
        let parts = colored_segments("<C10092288>Réussi\n<C16773320>Coût", Color32::WHITE);
        assert_eq!(parts, vec![("Réussi\n".to_string(), Color32::from_rgb(0x99, 0xff, 0x00)), ("Coût".to_string(), Color32::from_rgb(0xff, 0xf0, 0xc8))]);
        assert_eq!(ConvertTab::of_tab_button(79.0), ConvertTab::Socket);
        let parts = colored_segments("A <HFF0000>B 3<HFFFFFF>___", Color32::WHITE);
        assert_eq!(parts[1], ("B 3".to_string(), Color32::from_rgb(0xff, 0, 0)));
    }
}
