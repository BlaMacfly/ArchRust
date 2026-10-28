//! Fenêtres du courrier d'Archonia (AgcmUIMailBox), d'après `UI_1024X768.ini` :
//!
//! * `MailBox` (350 × 550, `MailBox_Base.png`) : titre, croix et « Close », « Write letter »
//!   (`MAIL_LIST_WRITE`, actif avec un papier à lettre), papiers restants `CTL_EDIT_AVAIL`
//!   (`MAIL_LETTER_COUNT`), liste `CTL_LIST` de 8 éléments de 53 px (`CTL_LISTITEM` : fond
//!   `MailBox_Item.png`, objet `CTL_ST_SUBJECT`, date `CTL_ST_DATE`, « Delete » actif pour un courrier lu,
//!   « Read », icône de pièce jointe `CTL_ST_ITICON`, icône lu / non lu `CTL_ST_READICON`), défilement ;
//! * `MailWrite` (`Letter_Base.png`) : destinataire `CTL_ED_TOCHAR` (20), objet `CTL_ED_SUBJECT` (50),
//!   texte `CTL_ED_CONT` (782), case de l'objet joint `CTL_GRID_ITEM`, aide `CTL_ED_DESC`
//!   (`MAIL_WRITE_DESC`), expéditeur `CTL_ST_FROMCHAR`, « Send », « Cancel » et la croix ;
//! * `MailRead` : expéditeur, destinataire, objet, texte, pièce jointe (`CTL_GRID_ITEM`), « Save Item »,
//!   « Reply », « Confirm » et la croix.
//!
//! Les demandes reviennent au moteur en [`MailUi`] (voir `app_mail.rs`).

use super::items_ui::icon as item_icon;
use super::npc_ui::{frame, window};
use super::portal_ui::{argb, control_text, state_button, static_text};
use super::*;

/// Fenêtres du courrier, chargées avec le HUD (textes fixes d'`ini_fr`).
pub(super) const MAIL_WINDOWS: [&str; 3] = ["MailBox", "MailWrite", "MailRead"];
/// Éléments visibles de la liste (`List=8`) et hauteur d'un élément.
pub const MAIL_ROWS: usize = 8;
const ROW_H: f32 = 53.0;
/// Contrôles d'un élément de la liste (enfants de `CTL_LISTITEM`).
const ROW_CONTROLS: [&str; 7] = ["CTL_ST_LISTBACK", "CTL_ST_SUBJECT", "CTL_ST_DATE", "CTL_BTN_DEL", "CTL_BTN_READ", "CTL_ST_ITICON", "CTL_ST_READICON"];

/// Ligne de la boîte : objet, date, lu, pièce jointe.
pub struct MailRow {
    pub subject: String,
    pub date: String,
    pub read: bool,
    pub attached: bool,
}

pub struct MailBoxView<'a> {
    pub rows: &'a [MailRow],
    pub first: usize,
    /// `MAIL_LETTER_COUNT` rempli.
    pub letters: &'a str,
    pub can_write: bool,
}

/// Lettre en cours d'écriture.
pub struct MailWriteView<'a> {
    pub to: &'a mut String,
    pub subject: &'a mut String,
    pub body: &'a mut String,
    /// Objet joint : TID et quantité.
    pub item: Option<(i32, i32)>,
    pub desc: &'a str,
    pub from: &'a str,
}

/// Lettre ouverte.
pub struct MailReadView<'a> {
    pub from: &'a str,
    pub to: &'a str,
    pub subject: &'a str,
    pub body: &'a str,
    pub item: Option<(i32, i32)>,
}

/// Demandes des fenêtres du courrier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MailUi {
    CloseBox,
    Scroll(i32),
    Write,
    Read(usize),
    Delete(usize),
    /// Croix ou « Cancel » de `MailWrite` (`MAIL_WRITE_CLOSE` : confirmation).
    CancelWrite,
    Send,
    /// Objet de l'inventaire lâché sur la case (`MAIL_WRITE_MOVE`).
    Attach(ItemDrag),
    /// Clic droit sur la case : objet retiré.
    Detach,
    CloseRead,
    SaveItem,
    Reply,
}

/// Pièce jointe dans une case de 50 px : icône du modèle et quantité.
fn grid_item(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, r: Rect, item: Option<(i32, i32)>, font: f32) {
    if let Some((tid, count)) = item {
        item_icon(ctx, hud, assets, painter, r, Some(tid), Color32::WHITE);
        if count > 1 {
            shadow_text(painter, r.right_bottom() - Vec2::new(3.0, 2.0), Align2::RIGHT_BOTTOM, &count.to_string(), font, Color32::WHITE);
        }
    }
}

/// Couleur du texte d'un contrôle (`CtrlFont`), blanc par défaut.
fn text_color(c: &UiControl) -> Color32 {
    c.font.map_or(Color32::WHITE, |(color, _)| argb(color))
}

/// Texte coupé à la largeur et défilant dans `r`.
fn text_area(ui: &mut egui::Ui, r: Rect, id: &str, text: &str, font: f32, color: Color32) {
    ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
        egui::ScrollArea::vertical().id_salt(id).max_height(r.height()).show(ui, |ui| {
            ui.set_width(r.width() - 12.0);
            ui.label(egui::RichText::new(text).size(font).color(color));
        });
    });
}

/// Boîte aux lettres.
pub fn draw_mailbox(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &MailBoxView, out: &mut Vec<MailUi>) {
    let Some(w) = window(hud, assets, "MailBox") else { return };
    frame(ctx, hud, assets, &w, "archonia-mailbox", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        if let Some(list) = w.control("CTL_LIST") {
            if ui.rect_contains_pointer(layout.rect(origin, list.pos, list.size)) {
                let wheel = ui.input(|i| i.raw_scroll_delta.y);
                if wheel.abs() > 0.5 {
                    out.push(MailUi::Scroll(if wheel > 0.0 { -1 } else { 1 }));
                }
            }
        }
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            if ROW_CONTROLS.contains(&c.name.as_str()) {
                for (k, row) in view.rows.iter().enumerate().skip(view.first).take(MAIL_ROWS) {
                    let rr = layout.rect(origin, [c.pos[0], c.pos[1] + (k - view.first) as f32 * ROW_H], c.size);
                    match c.name.as_str() {
                        "CTL_ST_LISTBACK" => {
                            if let Some((_, t)) = c.textures.first() {
                                control_image(ctx, hud, assets, painter, rr, c.size, t, 1.0);
                            }
                        }
                        "CTL_ST_SUBJECT" => control_text(painter, rr, c, &row.subject, font, None),
                        "CTL_ST_DATE" => control_text(painter, rr, c, &row.date, font, None),
                        // `Status2` : UNATTACH sans texture, ATTACH texture 1 ; UNREAD texture 2, READ texture 1.
                        "CTL_ST_ITICON" | "CTL_ST_READICON" => {
                            let index = if c.name == "CTL_ST_ITICON" { row.attached as u32 } else if row.read { 1 } else { 2 };
                            if let Some((_, t)) = c.textures.iter().find(|(i, _)| index != 0 && *i == index) {
                                control_image(ctx, hud, assets, painter, rr, c.size, t, 1.0);
                            }
                        }
                        name => {
                            let (action, enabled) = if name == "CTL_BTN_DEL" { (MailUi::Delete(k), row.read) } else { (MailUi::Read(k), true) };
                            if state_button(ctx, ui, hud, assets, painter, rr, c, static_text(c), font, enabled, ("mail-row", name, k)).clicked() {
                                out.push(action);
                            }
                        }
                    }
                }
                continue;
            }
            match c.name.as_str() {
                "CTL_ST_TITLE" => control_text(painter, r, c, static_text(c), font, None),
                "CTL_EDIT_AVAIL" => super::convert_ui::edit_box_aligned(painter, r, view.letters, font, egui::Align::Min),
                "CTL_BTN_SYSCLOSE" | "CTL_BYN_CLOSE" | "CTL_BTN_WRITE" => {
                    let write = c.name == "CTL_BTN_WRITE";
                    let label = if c.name == "CTL_BTN_SYSCLOSE" { "" } else { static_text(c) };
                    if state_button(ctx, ui, hud, assets, painter, r, c, label, font, !write || view.can_write, ("mailbox-btn", c.name.as_str())).clicked() {
                        out.push(if write { MailUi::Write } else { MailUi::CloseBox });
                    }
                }
                "CTL_BTN_SCUP" | "CTL_BTN_SCDN" => {
                    let up = c.name == "CTL_BTN_SCUP";
                    let enabled = if up { view.first > 0 } else { view.first + MAIL_ROWS < view.rows.len() };
                    if state_button(ctx, ui, hud, assets, painter, r, c, "", font, enabled, ("mailbox-scroll", c.name.as_str())).clicked() {
                        out.push(MailUi::Scroll(if up { -1 } else { 1 }));
                    }
                }
                "CTL_SCROLLBTN" => {
                    // Curseur entre les deux flèches (barre de 427 px).
                    let span = view.rows.len().saturating_sub(MAIL_ROWS).max(1) as f32;
                    let travel = (427.0 - 32.0 - c.size[1]).max(0.0);
                    let y = (view.first as f32 / span).clamp(0.0, 1.0) * travel;
                    if let Some((_, t)) = c.textures.first() {
                        control_image(ctx, hud, assets, painter, layout.rect(origin, [c.pos[0], c.pos[1] + y], c.size), c.size, t, 1.0);
                    }
                }
                _ => {}
            }
        }
    });
}

/// Écriture d'une lettre.
pub fn draw_mail_write(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: MailWriteView, out: &mut Vec<MailUi>) {
    let Some(w) = window(hud, assets, "MailWrite") else { return };
    let MailWriteView { to, subject, body, item, desc, from } = view;
    frame(ctx, hud, assets, &w, "archonia-mail-write", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let dragging = egui::DragAndDrop::payload::<ItemDrag>(ctx);
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match (c.kind, c.name.as_str()) {
                (2, "CTL_ED_TOCHAR" | "CTL_ED_SUBJECT" | "CTL_ED_CONT") => {
                    let (text, limit, multi) = match c.name.as_str() {
                        "CTL_ED_TOCHAR" => (&mut *to, 20, false),
                        "CTL_ED_SUBJECT" => (&mut *subject, 50, false),
                        _ => (&mut *body, 782, true),
                    };
                    let edit = if multi { egui::TextEdit::multiline(text) } else { egui::TextEdit::singleline(text) };
                    let edit = edit
                        .id(egui::Id::new(("archonia-mail-edit", c.name.as_str())))
                        .frame(false)
                        .char_limit(limit)
                        .font(FontId::proportional(font))
                        .text_color(text_color(c));
                    ui.put(r, if multi { edit.desired_rows((c.size[1] / 15.0) as usize) } else { edit });
                }
                (2, "CTL_ED_DESC") => super::convert_ui::edit_box_aligned(painter, r, desc, font, egui::Align::Min),
                (8, _) => {
                    let resp = ui.interact(r, egui::Id::new("mail-write-grid"), egui::Sense::click());
                    grid_item(ctx, hud, assets, painter, r, item, font);
                    if resp.secondary_clicked() && item.is_some() {
                        out.push(MailUi::Detach);
                    }
                    if let Some(drag) = resp.dnd_release_payload::<ItemDrag>() {
                        out.push(MailUi::Attach(*drag));
                    }
                    let target = dragging.is_some() && resp.contains_pointer();
                    if resp.hovered() || target {
                        let color = if target { Color32::from_rgb(255, 210, 110) } else { Color32::from_rgb(120, 200, 255) };
                        painter.rect_stroke(r, 2.0, egui::Stroke::new(1.0_f32, color), egui::StrokeKind::Inside);
                    }
                }
                (1, "CTL_ST_FROMCHAR") => control_text(painter, r, c, from, font, None),
                (1, _) => control_text(painter, r, c, static_text(c), font, None),
                (3, name) => {
                    let action = if name == "CTL_BTN_SEND" { MailUi::Send } else { MailUi::CancelWrite };
                    let label = if name == "CTL_BTN_SYSCLOSE" { "" } else { static_text(c) };
                    if state_button(ctx, ui, hud, assets, painter, r, c, label, font, true, ("mail-write-btn", name)).clicked() {
                        out.push(action);
                    }
                }
                _ => {}
            }
        }
    });
}

/// Lecture d'une lettre.
pub fn draw_mail_read(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: &MailReadView, can_reply: bool, out: &mut Vec<MailUi>) {
    let Some(w) = window(hud, assets, "MailRead") else { return };
    frame(ctx, hud, assets, &w, "archonia-mail-read", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match (c.kind, c.name.as_str()) {
                (1, "CTL_ST_FROMCHAR") => control_text(painter, r, c, view.from, font, None),
                (1, "CTL_ST_TOCHAR") => control_text(painter, r, c, view.to, font, None),
                (1, "CTL_ST_SUBJECT2") => control_text(painter, r, c, view.subject, font, None),
                (1, _) => control_text(painter, r, c, static_text(c), font, None),
                (2, "CTL_ED_CONT") => text_area(ui, r, "archonia-mail-read-body", view.body, font, text_color(c)),
                (8, _) => grid_item(ctx, hud, assets, painter, r, view.item, font),
                (3, name) => {
                    let (action, enabled) = match name {
                        "CTL_BTN_ATTACH" => (MailUi::SaveItem, view.item.is_some_and(|(t, n)| t != 0 && n > 0)),
                        "CTL_BTN_REPLY" => (MailUi::Reply, can_reply),
                        _ => (MailUi::CloseRead, true),
                    };
                    let label = if name == "CTL_BTN_SYSCLOSE" { "" } else { static_text(c) };
                    if state_button(ctx, ui, hud, assets, painter, r, c, label, font, enabled, ("mail-read-btn", name)).clicked() {
                        out.push(action);
                    }
                }
                _ => {}
            }
        }
    });
}
