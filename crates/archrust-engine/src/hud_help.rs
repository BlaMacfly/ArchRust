//! Fenêtres d'Archonia de l'aide (`UI_Help`), des astuces (`UI_TIPS`) et image d'aide de l'écran de chargement
//! (`TipControl` d'`UI_LG_Loading`) ; données et minuterie : `app_help.rs`.
//!
//! * `UI_Help` (`Help_Base.png`, 550 × 500) : titre `CONTROL_2103`, en-têtes `CONTROL_23803` « Category » et
//!   `CONTROL_1379` « Content (description) » ; arbre `CONTROL_10830` (17,72, 217 × 359, `Tree=10:Help_Depth`,
//!   16 lignes `CONTROL_6208` de 22 px) : bouton `TreePlus` / `TreeMinus` (5,5) d'une catégorie qui a des pages,
//!   barre `CONTROL_24480` (22,2, 175 × 18, `Quest_Main_Bar.png` au survol et sur la ligne choisie,
//!   `Help_ChangeItem`) ; texte `CONTROL_18725` (255,72, 270 × 359, `Help_Content`) ; défilements `CONTROL_5818`
//!   (`MainTreeUp` / `MainTreeDown`) et `CONTROL_2697` (`MainTextUp` / `MainTextDown`) ; « Close » `CONTROL_16212`,
//!   croix `CONTROL_21619`. Choix d'ArchRust : le titre d'une page (profondeur 1) est décalé de 10 px.
//! * `UI_TIPS` (`TipText_Back.png`, 660 × 20) : texte `CONTROL_5879` (`Tip_Text`, centré, couleurs `<HHRRGGBB>`),
//!   `CONTROL_18080` (`Tip_Prev`) et `CONTROL_8188` (`Tip_Next`).
//! * `TipControl` (`CtrlType=16:Tip1024`, 728 × 426) : image de `Ini\Tips.txt` (`Texture`, `texture/ui`).
//!   Choix d'ArchRust : posée au centre de la zone au-dessus du texte `Warning` (y < 610), l'emplacement que le
//!   client calcule n'ayant pas été relevé.

use super::npc_ui::{frame, window};
use super::portal_ui::{control_text, state_button};
use super::*;

/// Fenêtres chargées avec le HUD.
pub(super) const HELP_WINDOWS: [&str; 2] = ["UI_Help", "UI_TIPS"];

/// Ligne de `Ini\Help.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpEntry {
    pub item: String,
    /// 0 catégorie, 1 page.
    pub depth: u8,
    pub content: String,
}

/// État d'`UI_Help` : données, branches ouvertes, ligne choisie et défilements.
#[derive(Debug, Clone, Default)]
pub struct HelpState {
    pub entries: Vec<HelpEntry>,
    pub expanded: Vec<bool>,
    pub selected: Option<usize>,
    /// Première ligne visible de l'arbre.
    pub first: usize,
    /// Défilement du texte, en lignes.
    pub text_line: usize,
}

impl HelpState {
    pub fn new(entries: Vec<HelpEntry>) -> Self {
        let n = entries.len();
        Self { selected: (n > 0).then_some(0), entries, expanded: vec![false; n], first: 0, text_line: 0 }
    }

    /// Lignes visibles de l'arbre : catégories, et pages des catégories ouvertes.
    pub fn visible(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut open = true;
        for (i, e) in self.entries.iter().enumerate() {
            if e.depth == 0 {
                open = self.expanded.get(i).copied().unwrap_or(false);
                out.push(i);
            } else if open {
                out.push(i);
            }
        }
        out
    }

    /// Vrai si l'entrée `i` est suivie d'une entrée plus profonde (catégorie à pages).
    pub fn has_children(&self, i: usize) -> bool {
        match (self.entries.get(i), self.entries.get(i + 1)) {
            (Some(a), Some(b)) => b.depth > a.depth,
            _ => false,
        }
    }
}

/// Lignes visibles de l'arbre (`List=16`).
const TREE_ROWS: usize = 16;

/// `<HHRRGGBB>` des fichiers d'astuces et d'aide en segments colorés (`<C…>` / `<H…>` d'Archonia compris).
pub fn tip_segments(text: &str, base: Color32) -> Vec<(String, Color32)> {
    super::convert_ui::colored_segments(&text.replace("<HH", "<H"), base)
}

/// Texte coloré coupé à la largeur `width`.
fn colored_job(text: &str, font: f32, width: f32, halign: egui::Align) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = width;
    job.halign = halign;
    for (part, color) in tip_segments(&text.replace("___", "\n"), Color32::WHITE) {
        job.append(&part, 0.0, egui::TextFormat { font_id: FontId::proportional(font), color, ..Default::default() });
    }
    job
}

/// Fenêtre `UI_Help` ; rend `true` quand elle est fermée.
pub fn draw_help(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, st: &mut HelpState) -> bool {
    let Some(w) = window(hud, assets, "UI_Help") else { return false };
    let mut closed = false;
    frame(ctx, hud, assets, &w, "archonia-help", |ui, hud, layout, origin, painter| {
        let s = layout.scale;
        let font = (11.0 * s).max(9.0);
        let rows = st.visible();
        st.first = st.first.min(rows.len().saturating_sub(TREE_ROWS));
        let ctl = |n: &str| w.control(n);
        // Texte de la ligne choisie, coupé et défilé par lignes.
        let mut text_lines = 0usize;
        if let Some(c) = ctl("CONTROL_18725") {
            let r = layout.rect(origin, c.pos, c.size);
            let content = st.selected.and_then(|i| st.entries.get(i)).map(|e| e.content.clone()).unwrap_or_default();
            let galley = painter.layout_job(colored_job(&content, font, r.width() - 4.0, egui::Align::Min));
            text_lines = galley.rows.len();
            let line_h = galley.rows.first().map_or(font, |r| r.height());
            st.text_line = st.text_line.min(text_lines.saturating_sub((r.height() / line_h.max(1.0)) as usize));
            let at = r.min + Vec2::new(2.0, -(st.text_line as f32) * line_h);
            painter.with_clip_rect(r).galley(at, galley, Color32::WHITE);
            if ui.rect_contains_pointer(r) {
                let d = ctx.input(|i| i.raw_scroll_delta.y);
                if d > 0.5 {
                    st.text_line = st.text_line.saturating_sub(1);
                } else if d < -0.5 {
                    st.text_line += 1;
                }
            }
        }
        for c in &w.controls {
            let r = layout.rect(origin, c.pos, c.size);
            match c.name.as_str() {
                "CONTROL_2103" | "CONTROL_23803" | "CONTROL_1379" => control_text(painter, r, c, c.fixed_label(), font, None),
                "CONTROL_16212" | "CONTROL_21619" => {
                    let label = if c.kind == 3 { c.fixed_label() } else { "" };
                    if state_button(ctx, ui, hud, assets, painter, r, c, label, font, true, ("archonia-help-btn", c.name.as_str())).clicked() {
                        closed = true;
                    }
                }
                "MainTreeUp" | "MainTreeDown" | "MainTextUp" | "MainTextDown" => {
                    if state_button(ctx, ui, hud, assets, painter, r, c, "", font, true, ("archonia-help-scroll", c.name.as_str())).clicked() {
                        match c.name.as_str() {
                            "MainTreeUp" => st.first = st.first.saturating_sub(1),
                            "MainTreeDown" => st.first = (st.first + 1).min(rows.len().saturating_sub(TREE_ROWS)),
                            "MainTextUp" => st.text_line = st.text_line.saturating_sub(1),
                            _ => st.text_line += 1,
                        }
                    }
                }
                // Curseurs des deux défilements.
                "CONTROL_24470" | "CONTROL_11385" => {
                    let tree = c.name == "CONTROL_24470";
                    let (pos, span) = if tree { (st.first, rows.len().saturating_sub(TREE_ROWS)) } else { (st.text_line, text_lines.saturating_sub(1)) };
                    let travel = (359.0 - 32.0 - c.size[1]).max(0.0);
                    let y = if span == 0 { 0.0 } else { (pos as f32 / span as f32).clamp(0.0, 1.0) * travel };
                    if let Some((_, t)) = c.textures.first() {
                        control_image(ctx, hud, assets, painter, layout.rect(origin, [c.pos[0], c.pos[1] + y], c.size), c.size, t, 1.0);
                    }
                }
                _ => {}
            }
        }
        // Arbre : gabarit `CONTROL_6208` répété tous les 22 px.
        let (Some(row), Some(bar), Some(plus), Some(minus)) = (ctl("CONTROL_6208"), ctl("CONTROL_24480"), ctl("TreePlus"), ctl("TreeMinus")) else { return };
        for (k, &i) in rows.iter().enumerate().skip(st.first).take(TREE_ROWS) {
            let dy = (k - st.first) as f32 * row.size[1];
            let e = &st.entries[i];
            if st.has_children(i) {
                let b = if st.expanded[i] { minus } else { plus };
                let rb = layout.rect(origin, [b.pos[0], b.pos[1] + dy], b.size);
                if state_button(ctx, ui, hud, assets, painter, rb, b, "", font, true, ("archonia-help-tree", i)).clicked() {
                    st.expanded[i] = !st.expanded[i];
                }
            }
            let rr = layout.rect(origin, [bar.pos[0], bar.pos[1] + dy], bar.size);
            let resp = ui.interact(rr, egui::Id::new(("archonia-help-item", i)), egui::Sense::click());
            if resp.hovered() || st.selected == Some(i) {
                if let Some((_, t)) = bar.textures.first() {
                    control_image(ctx, hud, assets, painter, rr, bar.size, t, 1.0);
                }
            }
            let indent = if e.depth > 0 { 10.0 * s } else { 0.0 };
            let rt = Rect::from_min_max(rr.min + Vec2::new(4.0 * s + indent, 0.0), rr.max);
            control_text(painter, rt, bar, &super::convert_ui::colored_text(&e.item.replace("<HH", "<H")), font, None);
            if resp.clicked() {
                st.selected = Some(i);
                st.text_line = 0;
            }
        }
        if let Some(c) = ctl("CONTROL_10830") {
            let r = layout.rect(origin, c.pos, c.size);
            if ui.rect_contains_pointer(r) {
                let d = ctx.input(|i| i.raw_scroll_delta.y);
                if d > 0.5 {
                    st.first = st.first.saturating_sub(1);
                } else if d < -0.5 {
                    st.first = (st.first + 1).min(rows.len().saturating_sub(TREE_ROWS));
                }
            }
        }
    });
    closed
}

/// Choix dans `UI_TIPS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipUi {
    Prev,
    Next,
}

/// Fenêtre `UI_TIPS` avec le texte `text`.
pub fn draw_tips(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, text: &str) -> Option<TipUi> {
    let w = window(hud, assets, "UI_TIPS")?;
    let layout = Layout::new(ctx.screen_rect());
    let s = layout.scale;
    let origin = layout.window_origin(&w);
    let size = Vec2::new(w.size[0], w.size[1]) * s;
    let mut out = None;
    egui::Area::new(egui::Id::new("archonia-tips")).fixed_pos(origin).order(egui::Order::Middle).show(ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter().clone();
        if let Some((_, tex)) = w.textures.first() {
            control_image(ctx, hud, assets, &painter, rect, w.size, tex, 1.0);
        }
        let font = (11.0 * s).max(9.0);
        for c in &w.controls {
            let r = layout.rect(rect.min, c.pos, c.size);
            match c.name.as_str() {
                "CONTROL_5879" => {
                    let galley = painter.layout_job(colored_job(text, font, f32::INFINITY, egui::Align::Center));
                    let at = Pos2::new(r.center().x, r.center().y - galley.size().y / 2.0);
                    painter.with_clip_rect(r).galley(at, galley, Color32::WHITE);
                }
                "CONTROL_18080" | "CONTROL_8188" => {
                    if state_button(ctx, ui, hud, assets, &painter, r, c, "", font, true, ("archonia-tips-btn", c.name.as_str())).clicked() {
                        out = Some(if c.name == "CONTROL_18080" { TipUi::Prev } else { TipUi::Next });
                    }
                }
                _ => {}
            }
        }
    });
    out
}

/// Table à tabulations de `Game/ini` (chiffrée RC4 « 1111 » comme Archonia la lit, ou déjà en clair) : une ligne
/// par entrée, colonnes nommées par la première ligne (noms comparés sans casse).
pub fn read_tab_table(assets: &Assets, path: &str) -> Vec<HashMap<String, String>> {
    let Ok(raw) = assets.read_file(path) else { return Vec::new() };
    let cp = archrust_data::ini::CodePage::Cp949;
    // En clair ou chiffré : la forme qui a le moins d'octets hors du texte ordinaire (petits fichiers compris,
    // comme les 28 octets de `Tips.txt`).
    let odd = |b: &[u8]| b.iter().take(256).filter(|&&c| (c < 0x20 && c != b'\r' && c != b'\n' && c != b'\t') || c >= 0x80).count();
    let decrypted = archrust_crypto::capi_crypt(archrust_crypto::KEY_INI, &raw);
    let text = archrust_data::ini::decode_text(if odd(&decrypted) < odd(&raw) { &decrypted } else { &raw }, cp);
    parse_tab_table(&text)
}

/// Analyse d'une table à tabulations (première ligne : en-têtes, mis en minuscules).
pub fn parse_tab_table(text: &str) -> Vec<HashMap<String, String>> {
    let mut lines = text.lines().map(|l| l.trim_end_matches('\r'));
    let Some(header) = lines.next() else { return Vec::new() };
    let names: Vec<String> = header.split('\t').map(|h| h.trim().to_ascii_lowercase()).collect();
    lines
        .filter(|l| !l.trim().is_empty())
        .map(|l| names.iter().cloned().zip(l.split('\t').map(|v| v.trim().to_string())).collect())
        .collect()
}

/// Images d'aide de l'écran de chargement (`Ini\Tips.txt`, colonne `Texture`, FUN_1404f4430), lues une fois.
fn loading_textures(assets: &Assets) -> &'static [String] {
    static LIST: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    LIST.get_or_init(|| read_tab_table(assets, "ini/Tips.txt").into_iter().filter_map(|r| r.get("texture").cloned()).filter(|t| !t.is_empty()).collect())
}

/// Image d'aide de l'écran de chargement : une de `Ini\Tips.txt`, tirée à chaque chargement.
pub(super) fn draw_loading_tips(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, rect: Rect, w: &UiWindow) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static PICK: AtomicUsize = AtomicUsize::new(usize::MAX);
    let list = loading_textures(assets);
    if list.is_empty() {
        return;
    }
    // Nouveau tirage quand l'écran de chargement n'a pas été dessiné depuis plus d'une seconde.
    let id = egui::Id::new("archonia-loading-tip-time");
    let now = ctx.input(|i| i.time);
    let last: Option<f64> = ctx.data(|d| d.get_temp(id));
    ctx.data_mut(|d| d.insert_temp(id, now));
    if last.is_none_or(|t| now - t > 1.0) || PICK.load(Ordering::Relaxed) == usize::MAX {
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos() as usize);
        PICK.store(seed % list.len(), Ordering::Relaxed);
    }
    let texture = &list[PICK.load(Ordering::Relaxed) % list.len()];
    draw_loading_tip(ctx, hud, assets, painter, rect, w, texture);
}

/// Image d'aide de l'écran de chargement dans `TipControl` ; `rect` : écran couvert par `UI_LG_Loading`.
fn draw_loading_tip(ctx: &egui::Context, hud: &mut HudState, assets: &Assets, painter: &egui::Painter, rect: Rect, w: &UiWindow, texture: &str) {
    let Some(c) = w.control("TipControl") else { return };
    let name = if texture.contains('.') { texture.to_string() } else { format!("{texture}.png") };
    let Some((tex, orig)) = hud.image(ctx, assets, &name) else { return };
    let (sx, sy) = (rect.width() / w.size[0].max(1.0), rect.height() / w.size[1].max(1.0));
    let warning_y = w.control("Warning").map_or(w.size[1], |x| x.pos[1]);
    let x = (w.size[0] - c.size[0]) / 2.0;
    let y = ((warning_y - c.size[1]) / 2.0).max(0.0);
    let r = Rect::from_min_size(rect.min + Vec2::new(x * sx, y * sy), Vec2::new(c.size[0] * sx, c.size[1] * sy));
    let uv = Pos2::new((c.size[0] / orig[0]).min(1.0), (c.size[1] / orig[1]).min(1.0));
    painter.image(tex, r, Rect::from_min_max(Pos2::ZERO, uv), Color32::WHITE);
}
