//! Bulles de discussion au-dessus des têtes (AgcmTextBoard, option `OP_ViewBaloonChat` /
//! `VH_BALOONCHAT` de l'onglet Affichage).
//!
//! **Relevés dans Archonia.exe** :
//! - création (FUN_140564e30, appelée par FUN_14055f180 / FUN_14055f270 pour le type 0) : refusée si
//!   l'option est coupée (`AgcmTextBoardMng+0x1e18`) ; texte copié (369 octets au plus) et coupé en
//!   lignes de 120 octets (un caractère double compte 2), **3 lignes au plus** ; largeur = plus longue
//!   ligne + 16 px, hauteur = lignes × hauteur de police + 10 ; boîte centrée, remontée de 30 px
//!   (`+0x24 = −l/2`, `+0x28 = −h/2 − 30`) ; queue tirée au hasard (`rand() % 3` : vers la droite, vers
//!   le bas, vers la gauche) ; une bulle par personnage, remplacée par la suivante ;
//! - paramètres de la discussion (FUN_140514950) : police 0, **durée 7000 ms** (`0x1b58`), texte blanc ;
//! - ancre (FUN_14082b250) : position du personnage, hauteur de son panneau + 30 unités ;
//! - dessin (FUN_14082a930, `TextBoard.ini` `[Textures1]` / `[Textures2]`) : coins et bords de 8 px
//!   (`TB_lefttop`… `TB_bottomline`) posés à 6 px hors de la boîte, fond `TB_back`, queue 16 × 16
//!   (`TB_tailtoright` / `tobottom` / `toleft`) sous le bord bas, à `−4 + l/2` ; texte à (+5, +8),
//!   une ligne par hauteur de police ; dans la dernière seconde, l'alpha suit `durée restante × 255 / 1000`.
//!
//! Types de discussion : 0 (normale) et 1 (bulle seule, `AP_CHAT_TYPE_WORD_BALLOON` de la référence)
//! donnent une bulle ; les autres canaux non.

use super::App;
use crate::assets::Assets;
use crate::hud::HudState;
use egui::{Color32, FontId, Pos2, Rect, Vec2};
use glam::Vec3;
use std::collections::HashMap;
use std::time::Instant;

/// Durée d'une bulle (FUN_140514950).
pub const LIFE_MS: u128 = 7000;
/// Fondu de la dernière seconde (FUN_14082a930).
pub const FADE_MS: u128 = 1000;
/// Lignes au plus et octets par ligne (FUN_140564e30).
pub const MAX_LINES: usize = 3;
pub const LINE_BYTES: usize = 120;
/// Taille des coins et bords, décalage hors de la boîte, queue (FUN_14082a930).
const EDGE: f32 = 8.0;
const OUTSET: f32 = 6.0;
const TAIL: f32 = 16.0;
const TAIL_GAP: f32 = 9.0;
/// Remontée de la boîte au-dessus de l'ancre (`DAT_140e4a0e0`) et de l'ancre au-dessus du panneau.
const RAISE: f32 = 30.0;
/// Police des bulles (police 0 d'AgcmFont, la même que les noms).
const FONT_SIZE: f32 = crate::boards::ID_FONT_SIZE;

#[derive(Debug, Clone)]
struct Balloon {
    lines: Vec<String>,
    born: Instant,
    /// 0 vers la droite, 1 vers le bas, 2 vers la gauche (`+0x1e0`).
    tail: usize,
}

#[derive(Default)]
pub struct BalloonGlue {
    balloons: HashMap<i32, Balloon>,
    rng: u32,
}

/// Bulle à dessiner : ancre (écran), lignes, queue, alpha.
pub struct BalloonDraw {
    anchor: Pos2,
    lines: Vec<String>,
    tail: usize,
    alpha: u8,
}

#[derive(Default)]
pub struct BalloonView {
    draws: Vec<BalloonDraw>,
}

/// Lignes d'une bulle : coupures à 120 octets (cp1252 : un caractère par octet, au-delà du latin 2),
/// 3 lignes au plus, 369 octets lus au plus.
pub fn split_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let (mut line, mut bytes, mut total) = (String::new(), 0usize, 0usize);
    for c in text.chars() {
        let w = if (c as u32) < 0x100 { 1 } else { 2 };
        if total + w > 368 {
            break;
        }
        if bytes + w > LINE_BYTES {
            lines.push(std::mem::take(&mut line));
            bytes = 0;
            if lines.len() == MAX_LINES {
                return lines;
            }
        }
        line.push(c);
        bytes += w;
        total += w;
    }
    if !line.is_empty() && lines.len() < MAX_LINES {
        lines.push(line);
    }
    lines
}

/// Alpha d'une bulle d'âge `age` ms : opaque, puis fondu sur la dernière seconde ; `None` : expirée.
pub fn alpha_at(age: u128) -> Option<u8> {
    let left = LIFE_MS.checked_sub(age).filter(|&l| l > 0)?;
    Some(if left >= FADE_MS { 255 } else { (left * 255 / FADE_MS) as u8 })
}

impl App {
    /// Ligne de discussion reçue : bulle au-dessus de l'émetteur (types 0 et 1, option active).
    pub(super) fn balloon_chat(&mut self, cid: i32, channel: i8, message: &str) {
        if !self.options.display.balloon_chat || !(channel == 0 || channel == 1) || cid == 0 || message.trim().is_empty() {
            return;
        }
        let lines = split_lines(message);
        if lines.is_empty() {
            return;
        }
        let g = &mut self.balloons;
        g.rng = g.rng.wrapping_mul(214_013).wrapping_add(2_531_011);
        let tail = ((g.rng >> 16) & 0x7fff) as usize % 3;
        g.balloons.insert(cid, Balloon { lines, born: Instant::now(), tail });
    }

    /// Positions à l'écran des bulles vivantes.
    pub(super) fn balloon_view(&mut self) -> BalloonView {
        let mut view = BalloonView::default();
        let now = Instant::now();
        self.balloons.balloons.retain(|_, b| now.duration_since(b.born).as_millis() < LIFE_MS);
        if self.balloons.balloons.is_empty() || !self.options.display.balloon_chat {
            return view;
        }
        let Some(gpu) = &self.gpu else { return view };
        let ppp = gpu.window.scale_factor() as f32;
        let (w, h) = (gpu.renderer.config.width as f32 / ppp, gpu.renderer.config.height as f32 / ppp);
        let vp = self.camera.view_proj(gpu.renderer.aspect());
        let to_screen = move |p: Vec3| {
            let c = vp * p.extend(1.0);
            (c.w > 1e-3 && c.z / c.w <= 1.0).then(|| Pos2::new(((c.x / c.w) + 1.0) * 0.5 * w, (1.0 - c.y / c.w) * 0.5 * h))
        };
        let chars: Vec<(i32, i32, Vec3)> = self.player.iter().chain(self.entities.iter()).map(|e| (e.cid, e.tid, e.position)).collect();
        let list: Vec<(i32, Balloon)> = self.balloons.balloons.iter().map(|(c, b)| (*c, b.clone())).collect();
        for (cid, b) in list {
            let Some(&(_, tid, pos)) = chars.iter().find(|c| c.0 == cid) else { continue };
            let Some(alpha) = alpha_at(now.duration_since(b.born).as_millis()) else { continue };
            let height = self.boards_head_height(cid, tid);
            let Some(anchor) = to_screen(pos + Vec3::Y * (height + RAISE)) else { continue };
            view.draws.push(BalloonDraw { anchor, lines: b.lines, tail: b.tail, alpha });
        }
        view
    }
}

/// Dessin egui des bulles, sous le HUD (textures de `TextBoard.ini`).
pub fn panel(ctx: &egui::Context, view: &BalloonView, hud: &mut HudState, assets: &Assets) {
    if view.draws.is_empty() {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("archrust-balloons")));
    let font = FontId::proportional(FONT_SIZE);
    let full = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    for d in &view.draws {
        let tint = Color32::from_white_alpha(d.alpha);
        let galleys: Vec<_> = d.lines.iter().map(|l| painter.layout_no_wrap(l.clone(), font.clone(), Color32::from_white_alpha(d.alpha))).collect();
        let line_h = galleys.first().map_or(FONT_SIZE, |g| g.size().y);
        let width = galleys.iter().map(|g| g.size().x).fold(0.0, f32::max).ceil() + 16.0;
        let height = line_h * galleys.len() as f32 + 10.0;
        let (x0, y0) = ((d.anchor.x - width / 2.0).trunc(), (d.anchor.y - height / 2.0 - RAISE).trunc());
        let mut image = |name: &str, r: Rect| {
            if let Some((tex, _)) = hud.image(ctx, assets, name) {
                painter.image(tex, r, full, tint);
            }
        };
        let rect = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(Pos2::new(x, y), Vec2::new(w.max(0.0), h.max(0.0)));
        image("TB_back.tif", rect(x0, y0, width, height));
        image("TB_lefttop.tif", rect(x0 - OUTSET, y0 - OUTSET, EDGE, EDGE));
        image("TB_righttop.tif", rect(x0 + width, y0 - OUTSET, EDGE, EDGE));
        image("TB_leftbottom.tif", rect(x0 - OUTSET, y0 + height, EDGE, EDGE));
        image("TB_rightbottom.tif", rect(x0 + width, y0 + height, EDGE, EDGE));
        image("TB_topline.tif", rect(x0, y0 - OUTSET, width, EDGE));
        image("TB_leftline.tif", rect(x0 - OUTSET, y0, EDGE, height));
        image("TB_rightline.tif", rect(x0 + width, y0, EDGE, height));
        // Bord bas en deux morceaux de part et d'autre de la queue.
        let tail_x = -4.0 + width * 0.5;
        image("TB_bottomline.tif", rect(x0, y0 + height, tail_x, EDGE));
        image("TB_bottomline.tif", rect(x0 + tail_x + TAIL_GAP, y0 + height, width - tail_x - TAIL_GAP, EDGE));
        let tail = ["TB_tailtoright.tif", "TB_tailtobottom.tif", "TB_tailtoleft.tif"][d.tail.min(2)];
        image(tail, rect(x0 + tail_x, y0 + height, TAIL, TAIL));
        for (i, g) in galleys.into_iter().enumerate() {
            painter.galley(Pos2::new(x0 + 5.0, y0 + 8.0 + i as f32 * line_h), g, Color32::WHITE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_cut_at_120_bytes_three_at_most() {
        assert_eq!(split_lines("salut"), vec!["salut".to_string()]);
        let long = "a".repeat(500);
        let l = split_lines(&long);
        assert_eq!(l.len(), 3);
        assert!(l.iter().all(|x| x.len() == 120));
        // Caractères doubles : 60 par ligne.
        let k = split_lines(&"가".repeat(70));
        assert_eq!(k.iter().map(|x| x.chars().count()).collect::<Vec<_>>(), vec![60, 10]);
    }

    #[test]
    fn balloon_fades_in_its_last_second() {
        assert_eq!(alpha_at(0), Some(255));
        assert_eq!(alpha_at(6000), Some(255));
        assert_eq!(alpha_at(6500), Some(127));
        assert_eq!(alpha_at(7000), None);
    }
}
