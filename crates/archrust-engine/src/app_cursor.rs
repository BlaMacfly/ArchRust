//! Curseur de la souris d'Archonia.
//!
//! Archonia charge cinq curseurs Windows (`FUN_1404e5510`) : `CURSOR_M1.CUR` (pointeur),
//! `CURSOR_A1.CUR` (attaque), `CURSOR_T1.CUR` (dialogue), `CURSOR_P1.CUR` (ramassage) et
//! `CURSOR_D1.CUR` (interdit), plus `CURSOR_MOVEITEM1.CUR` pour un objet saisi. Les mêmes images
//! sont dans le pack `texture/ui` (`cursor_m1.png`…, 64 × 64) : ArchRust en fait des curseurs
//! matériels (winit `CustomCursor`), comme le client d'origine, sans dessin logiciel.
//!
//! Choix du curseur : sur une fenêtre, le pointeur ; sur un monstre, l'attaque ; sur un PNJ qui a
//! des fonctions, le dialogue ; sur un objet au sol, le ramassage ; objet ou compétence saisis à la
//! souris, `cursor_moveitem1`. Le point chaud est le pixel opaque le plus proche du coin haut gauche
//! (pointe du doigt ou de l'épée : les `.CUR` d'origine, qui le donnent, ne sont pas dans les packs).
//! Les curseurs propres à egui (saisie de texte, redimensionnement) restent ceux du système.

use super::{App, Phase};
use winit::event_loop::ActiveEventLoop;
use winit::window::{CustomCursor, Window};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorKind {
    #[default]
    Main,
    Attack,
    Talk,
    Pick,
    Disabled,
    MoveItem,
}

const KINDS: [CursorKind; 6] = [CursorKind::Main, CursorKind::Attack, CursorKind::Talk, CursorKind::Pick, CursorKind::Disabled, CursorKind::MoveItem];

impl CursorKind {
    fn file(self) -> &'static str {
        match self {
            CursorKind::Main => "cursor_m1.png",
            CursorKind::Attack => "cursor_a1.png",
            CursorKind::Talk => "cursor_t1.png",
            CursorKind::Pick => "cursor_p1.png",
            CursorKind::Disabled => "cursor_d1.png",
            CursorKind::MoveItem => "cursor_moveitem1.png",
        }
    }
}

#[derive(Default)]
pub struct CursorGlue {
    /// Curseurs créés (même ordre que `KINDS`) ; `None` tant que la boucle d'événements ne les a pas créés.
    cursors: Option<Vec<Option<CustomCursor>>>,
    /// Curseur voulu pour l'image en cours.
    want: CursorKind,
    /// Dernier état appliqué : curseur et icône demandée par egui.
    applied: Option<(CursorKind, egui::CursorIcon)>,
}

/// Image ramenée à la taille de l'image d'origine du client (un patch HD la double) : moyenne des
/// pixels de chaque bloc.
fn shrink(data: &[u8], w: usize, h: usize, ow: usize, oh: usize) -> Vec<u8> {
    if ow == 0 || oh == 0 || (ow >= w && oh >= h) {
        return data.to_vec();
    }
    let (fx, fy) = (w / ow.max(1), h / oh.max(1));
    let mut out = vec![0u8; ow * oh * 4];
    for y in 0..oh {
        for x in 0..ow {
            let mut acc = [0u32; 4];
            for dy in 0..fy.max(1) {
                for dx in 0..fx.max(1) {
                    let (sx, sy) = ((x * fx + dx).min(w - 1), (y * fy + dy).min(h - 1));
                    let k = (sy * w + sx) * 4;
                    // Couleur pondérée par l'alpha : pas de liseré sombre autour des bords.
                    let a = data[k + 3] as u32;
                    for c in 0..3 {
                        acc[c] += data[k + c] as u32 * a;
                    }
                    acc[3] += a;
                }
            }
            let n = (fx.max(1) * fy.max(1)) as u32;
            let o = (y * ow + x) * 4;
            for c in 0..3 {
                out[o + c] = if acc[3] > 0 { (acc[c] / acc[3]) as u8 } else { 0 };
            }
            out[o + 3] = (acc[3] / n) as u8;
        }
    }
    out
}

/// Point chaud : pixel assez opaque le plus proche du coin haut gauche.
fn hotspot(data: &[u8], w: usize, h: usize) -> (u16, u16) {
    let mut best: Option<(usize, usize, usize)> = None;
    for y in 0..h {
        for x in 0..w {
            if data[(y * w + x) * 4 + 3] >= 128 && best.is_none_or(|(d, _, _)| x + y < d) {
                best = Some((x + y, x, y));
            }
        }
    }
    best.map_or((0, 0), |(_, x, y)| (x as u16, y as u16))
}

impl CursorGlue {
    /// Icône demandée par egui que le curseur d'Archonia remplace (les autres restent au système).
    fn replaces(icon: egui::CursorIcon) -> bool {
        use egui::CursorIcon as C;
        matches!(icon, C::Default | C::PointingHand | C::Grab | C::Grabbing | C::Move | C::NotAllowed | C::NoDrop | C::Alias | C::Copy)
    }

    /// Pose le curseur voulu après `handle_platform_output` d'egui (qui ne touche au curseur que
    /// lorsque son icône change).
    pub fn apply(&mut self, window: &Window, egui_icon: egui::CursorIcon) {
        let Some(cursors) = &self.cursors else { return };
        if !Self::replaces(egui_icon) {
            self.applied = Some((self.want, egui_icon));
            return;
        }
        if self.applied == Some((self.want, egui_icon)) {
            return;
        }
        let index = KINDS.iter().position(|k| *k == self.want).unwrap_or(0);
        if let Some(c) = cursors.get(index).and_then(Option::as_ref).or(cursors.first().and_then(Option::as_ref)) {
            window.set_cursor(c.clone());
        }
        self.applied = Some((self.want, egui_icon));
    }
}

impl App {
    /// Crée les curseurs une fois (il faut la boucle d'événements active).
    pub(super) fn cursor_load(&mut self, event_loop: &ActiveEventLoop) {
        if self.cursor.cursors.is_some() {
            return;
        }
        let assets = self.assets.clone();
        let list = KINDS
            .iter()
            .map(|k| {
                let img = assets.image("texture/ui", k.file()).ok()?;
                let (w, h) = (img.width as usize, img.height as usize);
                // Taille d'origine : en-tête PNG de l'entrée du pack (le patch HD l'agrandit).
                let (ow, oh) = assets
                    .read_packed("texture/ui", k.file())
                    .ok()
                    .filter(|b| b.len() >= 24 && b.starts_with(b"\x89PNG"))
                    .map(|b| (u32::from_be_bytes(b[16..20].try_into().unwrap()) as usize, u32::from_be_bytes(b[20..24].try_into().unwrap()) as usize))
                    .unwrap_or((w, h));
                let data = shrink(&img.data, w, h, ow.min(w), oh.min(h));
                let (cw, ch) = (ow.min(w), oh.min(h));
                let (hx, hy) = hotspot(&data, cw, ch);
                match CustomCursor::from_rgba(data, cw as u16, ch as u16, hx, hy) {
                    Ok(source) => Some(event_loop.create_custom_cursor(source)),
                    Err(e) => {
                        log::warn!("curseur {} : {e}", k.file());
                        None
                    }
                }
            })
            .collect::<Vec<_>>();
        log::info!("curseurs d'Archonia : {} sur {}", list.iter().flatten().count(), KINDS.len());
        self.cursor.cursors = Some(list);
    }

    /// Curseur voulu pour cette image, d'après ce qui est sous la souris.
    pub(super) fn cursor_update(&mut self) {
        let over_ui = self.gpu.as_ref().is_some_and(|g| {
            let ctx = g.egui_state.egui_ctx();
            ctx.is_pointer_over_area() || ctx.is_using_pointer()
        });
        let dragging = self.gpu.as_ref().is_some_and(|g| egui::DragAndDrop::has_any_payload(g.egui_state.egui_ctx()));
        self.cursor.want = if dragging {
            CursorKind::MoveItem
        } else if over_ui || self.phase != Phase::InWorld || self.addon_glue.pointer_captured {
            CursorKind::Main
        } else if self.combat_loot_under_cursor().is_some() {
            CursorKind::Pick
        } else {
            match self.entity_under_cursor() {
                Some(cid) if self.player.as_ref().is_some_and(|p| p.cid == cid) => CursorKind::Main,
                Some(cid) if self.npc_functions(cid).is_empty() && !self.entities.iter().any(|e| e.cid == cid && e.is_player) => CursorKind::Attack,
                Some(cid) if !self.npc_functions(cid).is_empty() => CursorKind::Talk,
                _ => CursorKind::Main,
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_chaud_et_reduction() {
        // 4 × 4 : un seul pixel opaque en (2, 1) → point chaud (2, 1).
        let mut img = vec![0u8; 4 * 4 * 4];
        img[(4 + 2) * 4 + 3] = 255;
        assert_eq!(hotspot(&img, 4, 4), (2, 1));
        // Réduction de moitié : le bloc qui contient le pixel garde un quart de son alpha.
        let small = shrink(&img, 4, 4, 2, 2);
        assert_eq!(small.len(), 2 * 2 * 4);
        assert_eq!(small[(0 * 2 + 1) * 4 + 3], 63);
    }
}
