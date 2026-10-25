//! Boîtes de dialogue d'Archonia, à la place des fenêtres egui : confirmation (« Oui / Non »),
//! séparation de pile (`UI_SplitItem`), quantité à acheter (`UI_NPCTrade_BuyStackCount`) et
//! montant de la banque (`UI_BankConfirm`).
//!
//! Tout vient du client : fonds, boutons à quatre états et positions d'`UI_1024X768.ini`, libellés
//! des boutons d'`ini_fr/UI_1024X768.ini`, phrases d'`uimessage.txt` (`ini_fr` d'abord) ou des
//! messages de l'`[EventMap]` (`Action=4:1:-1:…`, cités à l'appel). La boîte de confirmation
//! reprend la mise en page de `Common_Message_Base2.png` (412 × 180) : texte centré, boutons
//! « Confirmer » en 135, 140 et « Annuler » en 213, 140, comme `UI_NPCTrade_BuyStackCount`.

use super::npc_ui::{button, window};
use super::*;
use archrust_data::ini::CodePage;

/// Résultat d'une image d'une boîte de dialogue.
#[derive(Debug, Default)]
pub struct DialogOut {
    /// Contrôle cliqué (`CtrlName`).
    pub clicked: Option<String>,
    /// Entrée appuyée (validation).
    pub enter: bool,
    /// Échap appuyé (annulation).
    pub escape: bool,
}

/// Texte fixe d'un contrôle (`CtrlDisplay=1:…`).
fn fixed(c: &UiControl) -> Option<&str> {
    c.display.as_deref()?.strip_prefix("1:")
}

/// Phrase de `uimessage.txt` (`ini_fr` puis `ini`), `___` en retour à la ligne ; vide si absente.
pub fn ui_message(hud: &mut HudState, assets: &Assets, key: &str) -> String {
    let map = hud.ui_messages.get_or_insert_with(|| {
        let mut map = HashMap::new();
        for dir in ["ini", "ini_fr"] {
            let Ok(raw) = assets.read_file(&format!("{dir}/uimessage.txt")) else { continue };
            let cp = CodePage::for_dir(dir);
            let plain = raw.iter().take(256).filter(|&&b| b < 0x20 && b != b'\r' && b != b'\n' && b != b'\t').count() * 20 < raw.len().min(256);
            let text = if plain { archrust_data::ini::decode_text(&raw, cp) } else { archrust_data::ini::decrypt_ini_with(&raw, cp) };
            for (k, v) in text.lines().filter_map(|l| l.split_once('\t')) {
                map.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        map
    });
    map.get(key).map(|t| t.replace("___", "\n")).unwrap_or_default()
}

/// Fenêtre `name` posée comme dans Archonia (devant le reste, déplaçable). `slot` dessine les
/// contrôles propres à l'appelant (champ de saisie, texte variable) et rend `true` s'il s'en est
/// chargé ; les autres sont dessinés d'après le fichier d'interface : boutons (libellé fixe),
/// images et textes fixes.
pub fn draw_dialog(
    ctx: &egui::Context,
    hud: &mut HudState,
    assets: &Assets,
    name: &str,
    mut slot: impl FnMut(&mut egui::Ui, &mut HudState, &egui::Painter, &UiControl, Rect, f32) -> bool,
) -> Option<DialogOut> {
    let w = window(hud, assets, name)?;
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let size = Vec2::new(w.size[0], w.size[1]) * s;
    let mut out = DialogOut::default();
    egui::Area::new(egui::Id::new(("archonia-dialog", name)))
        .default_pos(layout.window_origin(&w))
        .movable(true)
        .constrain(true)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let painter = ui.painter().clone();
            if let Some((_, tex)) = w.textures.first() {
                control_image(ctx, hud, assets, &painter, rect, w.size, tex, 1.0);
            }
            let font = (12.0 * s).max(9.0);
            for c in &w.controls {
                let r = layout.rect(rect.min, c.pos, c.size);
                if slot(ui, hud, &painter, c, r, font) {
                    continue;
                }
                match c.kind {
                    3 => {
                        let label = fixed(c).unwrap_or("");
                        if button(ctx, ui, hud, assets, &painter, r, c, label, font * 0.95, true, ("archonia-dialog", name, &c.name)).clicked() {
                            out.clicked = Some(c.name.clone());
                        }
                    }
                    _ => {
                        if let Some((_, tex)) = c.textures.first() {
                            control_image(ctx, hud, assets, &painter, r, c.size, tex, 1.0);
                        }
                        if let Some(text) = fixed(c).filter(|t| !t.is_empty()) {
                            shadow_text(&painter, r.center(), Align2::CENTER_CENTER, text, font, Color32::WHITE);
                        }
                    }
                }
            }
        });
    ctx.input(|i| {
        out.enter = i.key_pressed(egui::Key::Enter);
        out.escape = i.key_pressed(egui::Key::Escape);
    });
    Some(out)
}

/// Boîte de confirmation d'Archonia (`Common_Message_Base2.png`, boutons « Confirmer » /
/// « Annuler ») : `Some(true)` accepté (clic ou Entrée), `Some(false)` refusé (clic, croix, Échap).
pub fn confirm_box(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, id: impl std::hash::Hash, text: &str) -> Option<bool> {
    const W: f32 = 412.0;
    const H: f32 = 180.0;
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let size = Vec2::new(W, H) * s;
    let ok = ui_message(hud, assets, "UI_Name_OK");
    let cancel = ui_message(hud, assets, "UI_Name_Cancel");
    let mut answer = None;
    egui::Area::new(egui::Id::new(("archonia-confirm", id)))
        .default_pos(ctx.screen_rect().center() - size / 2.0)
        .movable(true)
        .constrain(true)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let painter = ui.painter().clone();
            control_image(ctx, hud, assets, &painter, rect, [W, H], "Common_Message_Base2.png", 1.0);
            let font = (12.0 * s).max(9.0);
            // Lignes coupées à 311 px, centrées autour de y = 84 (même calcul que la guilde).
            let mut lines: Vec<String> = Vec::new();
            for part in text.split(['\n']).flat_map(|p| p.split("___")) {
                let galley = painter.layout(part.trim().to_string(), FontId::proportional(font), Color32::WHITE, 311.0 * s);
                for row in &galley.rows {
                    lines.push(row.glyphs.iter().map(|g| g.chr).collect::<String>().trim().to_string());
                }
            }
            lines.truncate(8);
            let n = lines.len() as f32;
            for (i, l) in lines.iter().enumerate() {
                let y = 77.0 - 7.0 * n + 14.0 * i as f32 + 7.0;
                shadow_text(&painter, rect.min + Vec2::new(206.0, y) * s, Align2::CENTER_CENTER, l, font, Color32::WHITE);
            }
            let small = UiControl {
                name: "confirm".into(),
                kind: 3,
                size: [64.0, 17.0],
                textures: ["A", "B", "C", "D"].iter().enumerate().map(|(i, x)| (i as u32 + 1, format!("Common_Button_Small_{x}.png"))).collect(),
                ..Default::default()
            };
            let ok = if ok.is_empty() { "OK".to_string() } else { ok };
            let cancel = if cancel.is_empty() { "Cancel".to_string() } else { cancel };
            for (x, label, value) in [(135.0, ok.as_str(), true), (213.0, cancel.as_str(), false)] {
                let r = Rect::from_min_size(rect.min + Vec2::new(x, 140.0) * s, Vec2::new(64.0, 17.0) * s);
                if button(ctx, ui, hud, assets, &painter, r, &small, label, font * 0.95, true, ("archonia-confirm-btn", value, ui.id())).clicked() {
                    answer = Some(value);
                }
            }
            let close = UiControl {
                name: "confirm-x".into(),
                kind: 3,
                size: [16.0, 16.0],
                textures: ["A", "B", "C", "D"].iter().enumerate().map(|(i, x)| (i as u32 + 1, format!("Common_Button_X_{x}.png"))).collect(),
                ..Default::default()
            };
            let r = Rect::from_min_size(rect.min + Vec2::new(387.0, 7.0) * s, Vec2::new(16.0, 16.0) * s);
            if button(ctx, ui, hud, assets, &painter, r, &close, "", font, true, ("archonia-confirm-x", ui.id())).clicked() {
                answer = Some(false);
            }
        });
    if answer.is_none() {
        ctx.input(|i| {
            if i.key_pressed(egui::Key::Enter) {
                answer = Some(true);
            } else if i.key_pressed(egui::Key::Escape) {
                answer = Some(false);
            }
        });
    }
    answer
}

/// Saisie d'un nombre dans un contrôle d'édition d'Archonia (texte centré, sans cadre egui) ;
/// garde seulement les chiffres et borne la valeur à `max`.
fn number_edit(ui: &mut egui::Ui, r: Rect, id: impl std::hash::Hash, value: &mut i64, max: i64, font: f32) {
    let mut text = value.to_string();
    let edit = egui::TextEdit::singleline(&mut text)
        .id(egui::Id::new(("archonia-number", id)))
        .frame(false)
        .horizontal_align(egui::Align::Center)
        .font(FontId::proportional(font))
        .text_color(Color32::WHITE);
    let resp = ui.put(r, edit);
    if !resp.has_focus() && !ui.ctx().memory(|m| m.focused().is_some()) {
        resp.request_focus();
    }
    let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
    *value = digits.parse::<i64>().unwrap_or(0).clamp(0, max.max(0));
}

/// Séparation d'une pile (`UI_SplitItem`) : pavé numérique, + / −, « CR » efface, « ◀ » retire un
/// chiffre. `count` est modifié en place ; rend `Some(true)` à la validation, `Some(false)` à
/// l'annulation.
pub fn draw_split(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, count: &mut i64, max: i64) -> Option<bool> {
    let title = ui_message(hud, assets, "Item_Split_Stack_Count");
    let out = draw_dialog(ctx, hud, assets, "UI_SplitItem", |ui, _hud, painter, c, r, font| match c.name.as_str() {
        "CONTROL_StackCount" => {
            number_edit(ui, r, "split", count, max, font);
            true
        }
        "CONTROL_SplitItemTitle" => {
            let galley = painter.layout(title.clone(), FontId::proportional(font), Color32::WHITE, r.width());
            painter.galley(r.left_top(), galley, Color32::WHITE);
            true
        }
        _ => false,
    })?;
    if let Some(name) = &out.clicked {
        let label = window(hud, assets, "UI_SplitItem").and_then(|w| w.control(name).and_then(|c| fixed(c).map(str::to_string)));
        match (name.as_str(), label.as_deref()) {
            ("CONTROL_31219", _) => *count = (*count + 1).min(max),
            ("CONTROL_7462", _) => *count = (*count - 1).max(0),
            ("CONTROL_29441", _) => return Some(true),
            ("CONTROL_26396" | "CONTROL_14440", _) => return Some(false),
            (_, Some("CR")) => *count = 0,
            (_, Some("◀")) => *count /= 10,
            (_, Some(d)) if d.len() == 1 && d.chars().all(|c| c.is_ascii_digit()) => {
                *count = (*count * 10 + d.parse::<i64>().unwrap_or(0)).min(max);
            }
            _ => {}
        }
    }
    if out.enter {
        return Some(true);
    }
    out.escape.then_some(false)
}

/// Quantité à acheter (`UI_NPCTrade_BuyStackCount`) : question fixe du fichier d'interface,
/// champ `CONTROL_BUYSTACKCOUNT`.
pub fn draw_buy_count(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, count: &mut i64, max: i64) -> Option<bool> {
    let out = draw_dialog(ctx, hud, assets, "UI_NPCTrade_BuyStackCount", |ui, _hud, _painter, c, r, font| {
        if c.name == "CONTROL_BUYSTACKCOUNT" {
            number_edit(ui, r, "buy", count, max, font);
            return true;
        }
        false
    })?;
    match out.clicked.as_deref() {
        Some("CONTROL_16105") => return Some(true),
        Some("CONTROL_14382" | "CONTROL_20625") => return Some(false),
        _ => {}
    }
    if out.enter {
        return Some(true);
    }
    out.escape.then_some(false)
}

/// Montant à déposer ou retirer (`UI_BankConfirm`) : phrase `Bank_InputMoneyConfirm` ou
/// `Bank_OutputMoneyConfirm` dans `CONTROL_24769`, champ `BANK_MONEY_EDIT`.
pub fn draw_bank_money(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, deposit: bool, amount: &mut i64, max: i64) -> Option<bool> {
    let question = ui_message(hud, assets, if deposit { "Bank_InputMoneyConfirm" } else { "Bank_OutputMoneyConfirm" });
    let out = draw_dialog(ctx, hud, assets, "UI_BankConfirm", |ui, _hud, painter, c, r, font| match c.name.as_str() {
        "BANK_MONEY_EDIT" => {
            number_edit(ui, r, "bank-money", amount, max, font);
            true
        }
        "CONTROL_24769" => {
            shadow_text(painter, r.center(), Align2::CENTER_CENTER, &question, font, Color32::WHITE);
            true
        }
        _ => false,
    })?;
    match out.clicked.as_deref() {
        Some("CONTROL_17448") => return Some(true),
        Some("CONTROL_22720" | "CONTROL_20323") => return Some(false),
        _ => {}
    }
    if out.enter {
        return Some(true);
    }
    out.escape.then_some(false)
}

/// Boîte d'information d'Archonia (`Action=4:0` : `Common_Message_Base2.png`, un seul bouton
/// « OK » centré) : rend `true` quand elle est fermée (clic, croix, Entrée ou Échap).
pub fn info_box(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, text: &str) -> bool {
    const W: f32 = 412.0;
    const H: f32 = 180.0;
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let size = Vec2::new(W, H) * s;
    let ok = ui_message(hud, assets, "UI_Name_OK");
    let mut closed = false;
    egui::Area::new(egui::Id::new(("archonia-info", text)))
        .fixed_pos(ctx.screen_rect().center() - size / 2.0)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let painter = ui.painter().clone();
            control_image(ctx, hud, assets, &painter, rect, [W, H], "Common_Message_Base2.png", 1.0);
            let font = (12.0 * s).max(9.0);
            let galley = painter.layout(text.replace("___", "\n"), FontId::proportional(font), Color32::WHITE, 311.0 * s);
            let at = rect.min + Vec2::new(206.0, 84.0) * s - Vec2::new(galley.size().x / 2.0, galley.size().y / 2.0);
            painter.galley(at, galley, Color32::WHITE);
            let small = UiControl {
                name: "info-ok".into(),
                kind: 3,
                size: [64.0, 17.0],
                textures: ["A", "B", "C", "D"].iter().enumerate().map(|(i, x)| (i as u32 + 1, format!("Common_Button_Small_{x}.png"))).collect(),
                ..Default::default()
            };
            let r = Rect::from_min_size(rect.min + Vec2::new(174.0, 140.0) * s, Vec2::new(64.0, 17.0) * s);
            let label = if ok.is_empty() { "OK" } else { ok.as_str() };
            if button(ctx, ui, hud, assets, &painter, r, &small, label, font * 0.95, true, ("archonia-info-ok", ui.id())).clicked() {
                closed = true;
            }
        });
    closed || ctx.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape))
}
