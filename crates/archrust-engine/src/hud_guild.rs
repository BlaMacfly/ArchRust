//! Fenêtres de guilde d'Archonia (AgcmUIGuild, sha256 e9c7609b…) dessinées d'après `UI_1024X768.ini`
//! (textes fixes de `ini_fr`), et boîte de confirmation OK / Annuler (AcUIOKCancelDialog).
//!
//! * `Guild_Main` (gestionnaire de guilde), `Guild_Create`, `Guild_Destroy`, `Guild_Create_Request`,
//!   `Guild_Create_Complete`, `Guild_Destroy_Confirm`, `Guild_Destroy_Complete`, `Guild_Notice`
//!   (onglet « Infos ») et `Guild_Member` (onglet « Membres ») : chaque contrôle est dessiné à sa place,
//!   avec ses textures ; les textes variables (affichages `CtrlDisplay=2:…`) et les champs de saisie
//!   viennent de la vue préparée par `app_guild.rs` ; un bouton inactif prend sa texture « d ».
//! * Alignement de `CtrlFont` (5ᵉ valeur) : bits 0-1 horizontal (0 gauche, 1 centre, 2 droite), bit
//!   0x10 centré verticalement (sinon en haut) ; `___` est un saut de ligne des textes d'Archonia.
//! * Liste des membres (`GD_Member_List`, `List=19`) : 19 lignes de `GD_Member_List_Item` (15 px), barre
//!   `GD_Bar.png` sur la ligne choisie, état `GD_On.png` / `GD_Off.png` (`Status2` 0 et 1) ; numéros de
//!   page `<C…>%d` (FUN_1408bf760) en `Guild_SelectPageColor` / `Guild_DeselectPageColor`.
//! * Boîte OK / Annuler : `Common_Message_Base2.png` 412 × 180 (FUN_1408ea820), boutons
//!   `Common_Button_Small` 64 × 17 en (135, 140) et (213, 140) (FUN_14078b940), textes `UI_Name_OK` /
//!   `UI_Name_Cancel` ; message coupé à 311 px, 10 lignes au plus, lignes centrées sur x = 206, bloc
//!   centré sur y = 77, interligne 14 px, blanc (FUN_14078c390, FUN_14078bc60). Sa position à l'écran
//!   n'est pas établie : ArchRust la centre.
//! * Entrepôt (`UI_GuildWarehouse`, AgcmUIGuildWarehouse) : grille `CTL_GRID_GWARE` de 7 × 4 cases de 50 px,
//!   pages `CTL_BTN_LAYER1` à 4 (ouvertes selon les pages achetées, `GWARE_ACT_SLOT1` à 3), argent `CTL_ST_MONEY`
//!   (`%I64d`), boutons de dépôt et de retrait (droits 4 et 8, FUN_14030b830) et d'agrandissement ; montant dans
//!   `UI_GWareConfirm` (`Bank_InputMoneyConfirm` / `Bank_OutputMoneyConfirm`, FUN_1405fbef0).

use super::npc_ui::{button, frame, window};
use super::portal_ui::argb;
use super::*;
use std::collections::{HashMap, HashSet};

/// Clic dans une fenêtre de guilde.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuildUi {
    /// Bouton `control` de la fenêtre `window`.
    Click { window: String, control: String },
    /// Ligne d'une liste (contrôle de la liste, indice dans la page).
    Row(&'static str, usize),
    /// Bouton de la boîte OK / Annuler : `true` = OK.
    MessageBox(bool),
    /// Case d'une grille (contrôle, identifiant de la case).
    Cell(&'static str, usize),
    /// Entrepôt : fermer, argent (dépôt ou retrait), agrandir, objet déposé dans une case, objet repris.
    GwareClose,
    GwareMoney(bool),
    GwareExpand,
    GwareStore { iid: i32, grid: [i16; 3] },
    GwareTake(i32),
}

/// Objet de l'entrepôt : IID, TID, quantité, case, nom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GwareItem {
    pub iid: i32,
    pub tid: i32,
    pub count: i32,
    pub grid: [i16; 3],
    pub name: String,
}

/// Fenêtre de l'entrepôt : objets, argent, pages ouvertes, page affichée, droits de retrait d'argent et
/// d'agrandissement.
pub struct GwareView<'a> {
    pub items: &'a [GwareItem],
    pub money: i64,
    pub pages: i16,
    pub page: &'a mut i16,
    pub money_out: bool,
    pub expand: bool,
}

/// Champ de saisie : contrôle, texte, mot de passe, longueur maximale, plusieurs lignes.
pub struct GuildEdit<'a> {
    pub control: &'static str,
    pub text: &'a mut String,
    pub password: bool,
    pub max: usize,
    pub multiline: bool,
}

/// Case d'une grille (`Grid=origine x:origine y:écart x:écart y:largeur:hauteur:…`) : place dans la grille
/// (de gauche à droite, puis de haut en bas), identifiant rendu au clic, texture, teinte ARGB, choisie
/// (la texture du contrôle, ex. `GD_Mark_Active.png`, est posée dessus).
pub struct GridCell {
    pub control: &'static str,
    pub slot: usize,
    pub id: usize,
    pub texture: String,
    pub tint: u32,
    pub selected: bool,
}

/// Ligne d'une liste : textes des colonnes de `ListSpec::columns`, état en ligne (liste des membres).
pub struct GuildRow {
    pub cols: Vec<String>,
    pub online: Option<bool>,
}

/// Liste d'une fenêtre : contrôle de la liste, ligne, barre de sélection, bouton de ligne, colonnes,
/// état (image ON/OFF) et numéros de page.
#[derive(Clone, Copy)]
pub struct ListSpec {
    pub list: &'static str,
    pub item: &'static str,
    pub bar: &'static str,
    pub button: &'static str,
    pub columns: &'static [&'static str],
    pub status: Option<&'static str>,
    pub pages: &'static str,
}

impl ListSpec {
    /// Liste des membres (`Guild_Member`) : nom, niveau, classe, race, état.
    pub const MEMBERS: ListSpec = ListSpec {
        list: "GD_Member_List",
        item: "GD_Member_List_Item",
        bar: "GD_Member_Button_Static",
        button: "GD_Member_Button",
        columns: &["GD_Member_Name", "GD_Member_Level", "GD_Member_Class", "GD_Member_Race"],
        status: Some("GD_Member_Status"),
        pages: "CONTROL_19921",
    };
    /// Liste des guildes (`Guild_List`) : nom, membres, classement.
    pub const GUILDS: ListSpec = ListSpec {
        list: "GD_LIST_LIST",
        item: "GD_LIST_LIST_ITEM",
        bar: "GD_LIST_STATIC",
        button: "GD_LIST_BUTTON",
        columns: &["GD_LIST_NAME", "GD_LIST_MEMBER", "GD_LIST_WINP"],
        status: None,
        pages: "CONTROL_31900",
    };

    fn owns(&self, control: &str) -> bool {
        [self.item, self.bar, self.button].contains(&control) || self.columns.contains(&control) || self.status == Some(control)
    }
}

/// Contenu variable d'une fenêtre.
#[derive(Default)]
pub struct GuildView<'a> {
    /// Texte des contrôles dont l'affichage est variable.
    pub texts: HashMap<&'static str, String>,
    pub edits: Vec<GuildEdit<'a>>,
    /// Boutons inactifs (texture « d », sans clic).
    pub disabled: HashSet<&'static str>,
    /// Onglets du moment (texture « c »).
    pub active: HashSet<&'static str>,
    /// Images posées sur un contrôle, dans l'ordre (texture étirée au contrôle, teinte ARGB) : couches
    /// d'un emblème.
    pub images: Vec<(&'static str, String, u32)>,
    /// Cases des grilles.
    pub cells: Vec<GridCell>,
    /// Liste de la fenêtre, ses lignes de la page et la ligne choisie.
    pub list: Option<ListSpec>,
    pub rows: Vec<GuildRow>,
    pub selected: Option<usize>,
    /// Numéros de page (1 à n) et page affichée (0 à n-1).
    pub pages: usize,
    pub page: usize,
}

/// Couleurs des numéros de page (`Guild_SelectPageColor`, `Guild_DeselectPageColor` de uimessage).
pub struct PageColors {
    pub selected: u32,
    pub other: u32,
}

/// Texte d'un contrôle placé selon `CtrlFont` ; `___` = saut de ligne. Seules les zones de texte
/// (type 2) passent à la ligne d'elles-mêmes.
fn area_text(p: &egui::Painter, r: Rect, c: &UiControl, text: &str, size: f32, color: Option<Color32>) {
    let wrap = if c.kind == 2 { r.width().max(1.0) } else { f32::INFINITY };
    if text.is_empty() {
        return;
    }
    let (font_color, align) = c.font.unwrap_or((u32::MAX, 0));
    let color = color.unwrap_or(argb(font_color));
    let mut job = egui::text::LayoutJob::simple(text.replace("___", "\n"), FontId::proportional(size), color, wrap);
    job.halign = match align & 3 {
        1 => egui::Align::Center,
        2 => egui::Align::RIGHT,
        _ => egui::Align::LEFT,
    };
    let galley = p.layout_job(job);
    let x = match align & 3 {
        1 => r.center().x,
        2 => r.right(),
        _ => r.left(),
    };
    let y = if align & 0x10 != 0 { r.center().y - galley.size().y / 2.0 } else { r.top() };
    let pos = Pos2::new(x, y);
    p.galley_with_override_text_color(pos + Vec2::splat(1.0), galley.clone(), Color32::from_black_alpha(210));
    p.galley(pos, galley, color);
}

/// Texte fixe d'un contrôle (`CtrlDisplay=1:texte`).
fn fixed_text(c: &UiControl) -> &str {
    c.display.as_deref().and_then(|d| d.strip_prefix("1:")).unwrap_or("")
}

/// Bouton dessiné comme appuyé (onglet du moment).
fn pressed_tab(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, r: Rect, c: &UiControl, label: &str, font: f32) {
    if let Some((_, t)) = c.textures.iter().find(|(_, n)| n.to_ascii_lowercase().ends_with("c.png")).cloned() {
        control_image(ctx, hud, assets, painter, r, c.size, &t, 1.0);
    }
    shadow_text(painter, r.center(), Align2::CENTER_CENTER, label, font, Color32::WHITE);
}

/// Une fenêtre de guilde d'Archonia.
pub fn draw_guild_window(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, name: &str, mut view: GuildView, colors: &PageColors, out: &mut Vec<GuildUi>) {
    let Some(w) = window(hud, assets, name) else { return };
    let id = format!("archonia-{}", name.to_ascii_lowercase());
    frame(ctx, hud, assets, &w, &id, |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let click = |out: &mut Vec<GuildUi>, c: &UiControl| out.push(GuildUi::Click { window: w.name.clone(), control: c.name.clone() });
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            // Champs de saisie.
            if let Some(e) = view.edits.iter_mut().find(|e| e.control == c.name) {
                let edit = if e.multiline { egui::TextEdit::multiline(e.text) } else { egui::TextEdit::singleline(e.text) };
                let edit = edit
                    .id(egui::Id::new((&id, e.control)))
                    .frame(false)
                    .password(e.password)
                    .char_limit(e.max)
                    .font(FontId::proportional(font))
                    .text_color(Color32::WHITE)
                    .vertical_align(if e.multiline { egui::Align::Min } else { egui::Align::Center })
                    .horizontal_align(if c.font.is_some_and(|(_, a)| a & 3 == 1) { egui::Align::Center } else { egui::Align::Min });
                ui.put(r, edit);
                continue;
            }
            match c.kind {
                // Liste : les lignes et leurs colonnes.
                10 if view.list.is_some_and(|l| l.list == c.name) => draw_rows(ctx, ui, hud, assets, painter, &w, layout, origin, &view, font, out),
                3 if view.list.is_some_and(|l| l.button == c.name) => {}
                // Liste déroulante (`Guild_GBattle`) : son choix ; un clic passe au suivant, comme la flèche.
                5 => {
                    let label = view.texts.get(c.name.as_str()).cloned().unwrap_or_default();
                    let on = !view.disabled.contains(c.name.as_str());
                    if button(ctx, ui, hud, assets, painter, r, c, "", font, on, (&id, &c.name)).clicked() {
                        click(out, c);
                    }
                    shadow_text(painter, r.left_center() + Vec2::new(4.0 * s, 0.0), Align2::LEFT_CENTER, &label, font, Color32::WHITE);
                }
                3 => {
                    let label = view.texts.get(c.name.as_str()).cloned().unwrap_or_else(|| fixed_text(c).to_string());
                    if view.active.contains(c.name.as_str()) {
                        pressed_tab(ctx, hud, assets, painter, r, c, &label, font);
                    } else if button(ctx, ui, hud, assets, painter, r, c, &label, font, !view.disabled.contains(c.name.as_str()), (&id, &c.name)).clicked() {
                        click(out, c);
                    }
                }
                1 | 2 => {
                    // Contrôles de ligne de la liste : dessinés avec les lignes.
                    if view.list.is_some_and(|l| l.owns(&c.name)) {
                        continue;
                    }
                    if view.list.is_some_and(|l| l.pages == c.name) {
                        page_numbers(painter, r, &view, colors, font);
                        continue;
                    }
                    let text = match view.texts.get(c.name.as_str()) {
                        Some(t) => t.clone(),
                        None => fixed_text(c).to_string(),
                    };
                    area_text(painter, r, c, &text, font, None);
                }
                _ => {}
            }
        }
        for (control, texture, tint) in &view.images {
            if let Some(c) = w.control(control) {
                tinted(ctx, hud, assets, painter, layout.rect(origin, c.pos, c.size), texture, *tint);
            }
        }
        for cell in &view.cells {
            let Some(c) = w.control(cell.control) else { continue };
            let ([gx, gy, cw, ch], [ox, oy]) = (c.grid.unwrap_or([0.0, 0.0, 32.0, 32.0]), c.grid_origin.unwrap_or([0.0, 0.0]));
            let cols = (((c.size[0] - ox + gx) / (cw + gx)).floor() as usize).max(1);
            let (col, row) = (cell.slot % cols, cell.slot / cols);
            let at = [c.pos[0] + ox + col as f32 * (cw + gx), c.pos[1] + oy + row as f32 * (ch + gy)];
            if at[1] + ch > c.pos[1] + c.size[1] + 0.5 {
                continue;
            }
            let r = layout.rect(origin, at, [cw, ch]);
            tinted(ctx, hud, assets, painter, r, &cell.texture, cell.tint);
            if cell.selected {
                if let Some((_, t)) = c.textures.first() {
                    tinted(ctx, hud, assets, painter, r, t, 0xFFFF_FFFF);
                }
            }
            if ui.interact(r, egui::Id::new((cell.control, cell.id)), egui::Sense::click()).clicked() {
                out.push(GuildUi::Cell(cell.control, cell.id));
            }
        }
    });
}

/// Textes d'écran d'un combat de guildes (FUN_1408505a0, FUN_140850160) : (texte, couleur RVB, y en pixels
/// 1024 × 768), centrés ; et l'annonce du centre (FUN_140854c80 : acceptation, début).
pub fn draw_battle_texts(ctx: &egui::Context, lines: &[(String, u32, f32)], notice: Option<(&str, u32)>) {
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("archonia-guild-battle")));
    for (text, rgb, y) in lines {
        // FUN_140854a60 ne donne que la hauteur, comme FUN_140854c80 pour l'annonce : centré.
        let pos = Pos2::new(ctx.screen_rect().center().x, ctx.screen_rect().min.y + *y * s);
        shadow_text(&painter, pos, Align2::CENTER_TOP, text, (15.0 * s).max(10.0), argb(0xFF00_0000 | rgb));
    }
    if let Some((text, color)) = notice {
        let pos = Pos2::new(ctx.screen_rect().center().x, ctx.screen_rect().min.y + 180.0 * s);
        shadow_text(&painter, pos, Align2::CENTER_CENTER, text, (22.0 * s).max(14.0), argb(color));
    }
}

/// Image étirée au rectangle, teinte ARGB (couleur d'une couche d'emblème, FUN_1407c6270).
fn tinted(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, r: Rect, name: &str, tint: u32) {
    if let Some((tex, _)) = hud.image(ctx, assets, name) {
        painter.image(tex, r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), argb(tint));
    }
}

/// Numéros de page « 1 2 3 » (10 au plus à partir de la page affichée), la page affichée en clair.
fn page_numbers(p: &egui::Painter, r: Rect, view: &GuildView, colors: &PageColors, font: f32) {
    if view.pages == 0 {
        return;
    }
    let first = view.page.saturating_sub(view.page % 10);
    let last = (first + 10).min(view.pages - 1);
    let mut job = egui::text::LayoutJob::default();
    for n in first..=last {
        let color = argb(if n == view.page { colors.selected } else { colors.other });
        let fmt = egui::TextFormat { font_id: FontId::proportional(font), color, ..Default::default() };
        job.append(&format!("{}{}", if n == first { "" } else { " " }, n + 1), 0.0, fmt);
    }
    let galley = p.layout_job(job);
    p.galley(Pos2::new(r.center().x - galley.size().x / 2.0, r.center().y - galley.size().y / 2.0), galley, Color32::WHITE);
}

/// Lignes d'une liste (`ListSpec::item` et ses colonnes).
#[allow(clippy::too_many_arguments)]
fn draw_rows(ctx: &egui::Context, ui: &mut egui::Ui, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, w: &UiWindow, layout: &Layout, origin: Pos2, view: &GuildView, font: f32, out: &mut Vec<GuildUi>) {
    let Some(spec) = view.list else { return };
    let Some(item) = w.control(spec.item) else { return };
    let row_h = item.size[1];
    let col = |n: &str| w.control(n);
    for (i, row) in view.rows.iter().enumerate().take(19) {
        let dy = i as f32 * row_h;
        let at = |c: &UiControl| layout.rect(origin, [c.pos[0], c.pos[1] + dy], c.size);
        let ir = at(item);
        let resp = ui.interact(ir, egui::Id::new((spec.list, i)), egui::Sense::click());
        if resp.clicked() {
            out.push(GuildUi::Row(spec.list, i));
        }
        if view.selected == Some(i) || resp.hovered() {
            if let Some(bar) = col(spec.bar) {
                if let Some((_, t)) = bar.textures.first().cloned() {
                    control_image(ctx, hud, assets, painter, at(bar), bar.size, &t, 1.0);
                }
            }
        }
        for (name, text) in spec.columns.iter().zip(&row.cols) {
            if let Some(c) = col(name) {
                area_text(painter, at(c), c, text, font, None);
            }
        }
        if let (Some(c), Some(online)) = (spec.status.and_then(col), row.online) {
            // Status2 0 (« ON ») : texture 1, Status2 1 (« OFF ») : texture 2.
            let want = if online { 1 } else { 2 };
            if let Some((_, t)) = c.textures.iter().find(|(k, _)| *k == want).cloned() {
                control_image(ctx, hud, assets, painter, at(c), c.size, &t, 1.0);
            }
        }
    }
}

/// Boîte OK / Annuler d'Archonia (AcUIOKCancelDialog), centrée à l'écran.
pub fn draw_message_box(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, text: &str, ok: &str, cancel: &str, out: &mut Vec<GuildUi>) {
    const W: f32 = 412.0;
    const H: f32 = 180.0;
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let size = Vec2::new(W, H) * s;
    let pos = ctx.screen_rect().center() - size / 2.0;
    egui::Area::new(egui::Id::new("archonia-ok-cancel")).fixed_pos(pos).order(egui::Order::Foreground).show(ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter().clone();
        control_image(ctx, hud, assets, &painter, rect, [W, H], "Common_Message_Base2.png", 1.0);
        let font = (12.0 * s).max(9.0);
        // Lignes coupées à 311 px, 10 au plus.
        let mut lines: Vec<String> = Vec::new();
        for part in text.split("___") {
            let galley = painter.layout(part.trim().to_string(), FontId::proportional(font), Color32::WHITE, 311.0 * s);
            for row in &galley.rows {
                lines.push(row.glyphs.iter().map(|g| g.chr).collect::<String>().trim().to_string());
            }
        }
        lines.truncate(10);
        let n = lines.len() as f32;
        for (i, l) in lines.iter().enumerate() {
            let y = 77.0 - 7.0 * n + 14.0 * i as f32 + 7.0;
            shadow_text(&painter, rect.min + Vec2::new(206.0, y) * s, Align2::CENTER_CENTER, l, font, Color32::WHITE);
        }
        let small = UiControl {
            name: "msgbox".into(),
            kind: 3,
            size: [64.0, 17.0],
            textures: ["A", "B", "C", "D"].iter().enumerate().map(|(i, x)| (i as u32 + 1, format!("Common_Button_Small_{x}.png"))).collect(),
            ..Default::default()
        };
        for (x, label, value) in [(135.0, ok, true), (213.0, cancel, false)] {
            let r = Rect::from_min_size(rect.min + Vec2::new(x, 140.0) * s, Vec2::new(64.0, 17.0) * s);
            if button(ctx, ui, hud, assets, &painter, r, &small, label, font * 0.95, true, ("archonia-ok-cancel", value)).clicked() {
                out.push(GuildUi::MessageBox(value));
            }
        }
    });
}

/// Entrepôt de guilde (`UI_GuildWarehouse`) : grille de la page affichée (glisser depuis le sac : déposer ; vers le
/// sac ou clic droit : reprendre), pages, argent et boutons.
pub fn draw_guild_warehouse(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, view: GwareView, out: &mut Vec<GuildUi>) {
    let Some(w) = window(hud, assets, "UI_GuildWarehouse") else { return };
    let caption = super::dialogs_ui::ui_message(hud, assets, "GWARE_CAPTION");
    frame(ctx, hud, assets, &w, "archonia-guild-warehouse", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let dragging = egui::DragAndDrop::payload::<ItemDrag>(ctx).map(|d| *d);
        let layers = ["CTL_BTN_LAYER1", "CTL_BTN_LAYER2", "CTL_BTN_LAYER3", "CTL_BTN_LAYER4"];
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            // Pages (FUN_1405fc2c0…) : la première toujours, les autres selon les pages en plus.
            if let Some(page) = layers.iter().position(|t| *t == c.name).map(|p| p as i16) {
                let open = page < view.pages;
                let selected = page == *view.page;
                let tex = c.textures.iter().find(|(i, _)| *i == if !open { 3 } else if selected { 2 } else { 1 }).or(c.textures.first()).map(|(_, n)| n.clone());
                if let Some(t) = tex {
                    control_image(ctx, hud, assets, painter, r, c.size, &t, 1.0);
                }
                let color = if !open { Color32::GRAY } else if selected { Color32::from_rgb(255, 230, 160) } else { Color32::from_rgb(190, 190, 190) };
                shadow_text(painter, r.center(), Align2::CENTER_CENTER, &(page + 1).to_string(), font, color);
                if ui.interact(r, egui::Id::new(("gware-layer", page)), if open { egui::Sense::click() } else { egui::Sense::hover() }).clicked() {
                    *view.page = page;
                }
                continue;
            }
            match c.name.as_str() {
                "CTL_STATIC_CAPTION" => shadow_text(painter, r.left_center(), Align2::LEFT_CENTER, &caption, font, Color32::from_rgb(255, 230, 160)),
                "CTL_BTN_SYSCLOSE" => {
                    if button(ctx, ui, hud, assets, painter, r, c, "", font, true, "gware-x").clicked() {
                        out.push(GuildUi::GwareClose);
                    }
                }
                "CTL_ST_MONEY" => shadow_text(painter, r.right_center(), Align2::RIGHT_CENTER, &view.money.to_string(), font, Color32::from_rgb(255, 220, 120)),
                "CTL_BTN_MONEYIN" | "CTL_BTN_MONEYOUT" => {
                    let deposit = c.name == "CTL_BTN_MONEYIN";
                    if button(ctx, ui, hud, assets, painter, r, c, fixed_text(c), font * 0.95, deposit || view.money_out, ("gware-money", deposit)).clicked() {
                        out.push(GuildUi::GwareMoney(deposit));
                    }
                }
                "CTL_BTN_EXPAND" => {
                    if button(ctx, ui, hud, assets, painter, r, c, fixed_text(c), font * 0.95, view.expand, "gware-expand").clicked() {
                        out.push(GuildUi::GwareExpand);
                    }
                }
                "CTL_GRID_GWARE" => {
                    let [gx, gy, cw, ch] = c.grid.unwrap_or([2.0, 2.0, 50.0, 50.0]);
                    let cols = ((c.size[0] + gx) / (cw + gx)).floor().max(1.0) as i16;
                    let rows = ((c.size[1] + gy) / (ch + gy)).floor().max(1.0) as i16;
                    for (row, col) in (0..rows).flat_map(|row| (0..cols).map(move |col| (row, col))) {
                        let cell = layout.rect(origin, [c.pos[0] + col as f32 * (cw + gx), c.pos[1] + row as f32 * (ch + gy)], [cw, ch]);
                        let grid = [*view.page, row, col];
                        let resp = ui.interact(cell, egui::Id::new(("gware-cell", grid)), egui::Sense::click_and_drag());
                        if let Some(item) = view.items.iter().find(|i| i.grid == grid) {
                            let lifted = dragging.is_some_and(|d| d.iid == item.iid);
                            super::items_ui::icon(ctx, hud, assets, painter, cell, Some(item.tid), if lifted { Color32::from_white_alpha(70) } else { Color32::WHITE });
                            if item.count > 1 && !lifted {
                                shadow_text(painter, cell.right_bottom() - Vec2::new(3.0, 2.0) * s, Align2::RIGHT_BOTTOM, &item.count.to_string(), font, Color32::WHITE);
                            }
                            // Glissé hors de l'entrepôt comme un objet de banque (`bank`), repris s'il tombe dans le sac.
                            if resp.dragged() {
                                resp.dnd_set_drag_payload(ItemDrag { iid: item.iid, tid: item.tid, worn: false, bank: true });
                            }
                            if resp.secondary_clicked() {
                                out.push(GuildUi::GwareTake(item.iid));
                            }
                            if dragging.is_none() {
                                let label = if item.count > 1 { format!("{} ×{}", item.name, item.count) } else { item.name.clone() };
                                resp.clone().on_hover_text(label);
                            }
                        }
                        if let Some(drag) = resp.dnd_release_payload::<ItemDrag>() {
                            if !drag.worn && !drag.bank {
                                out.push(GuildUi::GwareStore { iid: drag.iid, grid });
                            }
                        }
                        let target = dragging.is_some_and(|d| !d.worn && !d.bank) && resp.contains_pointer();
                        if resp.hovered() || target {
                            let color = if target { Color32::from_rgb(255, 210, 110) } else { Color32::from_rgb(120, 200, 255) };
                            painter.rect_stroke(cell, 2.0, egui::Stroke::new(1.0_f32, color), egui::StrokeKind::Inside);
                        }
                    }
                }
                _ => {}
            }
        }
    });
}

/// Montant à déposer ou retirer (`UI_GWareConfirm`) : `Some(true)` confirmé, `Some(false)` annulé.
pub fn draw_gware_money(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, deposit: bool, amount: &mut i64, max: i64) -> Option<bool> {
    let question = super::dialogs_ui::ui_message(hud, assets, if deposit { "Bank_InputMoneyConfirm" } else { "Bank_OutputMoneyConfirm" });
    let out = super::dialogs_ui::draw_dialog(ctx, hud, assets, "UI_GWareConfirm", |ui, _hud, painter, c, r, font| match c.name.as_str() {
        "CTL_ED_MONEY" => {
            super::dialogs_ui::number_edit(ui, r, "gware-money", amount, max, font);
            true
        }
        "CTL_ST_MSG" => {
            shadow_text(painter, r.center(), Align2::CENTER_CENTER, &question, font, Color32::WHITE);
            true
        }
        _ => false,
    })?;
    match out.clicked.as_deref() {
        Some("CTL_BTN_OK") => return Some(true),
        Some("CTL_BTN_CANCEL" | "CTL_BTN_SYSCLOSE") => return Some(false),
        _ => {}
    }
    if out.enter {
        return Some(true);
    }
    out.escape.then_some(false)
}
