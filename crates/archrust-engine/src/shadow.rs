//! Ombres projetées des objets du décor sur le sol, comme `AgcmShadow2` d'Archonia (relevés dans
//! `Archonia.exe`, sha256 e9c7609b…, voir `docs/FORMATS.md`, « Ombres projetées ») :
//!
//! * **Option** — `OP_Shadow` (`FUN_140711840`) : niveaux 2, 3 et 4 allument AgcmShadow2
//!   (`FUN_140621da0(…, 1)`), 0 et 1 l'éteignent ; l'ombre ronde des personnages (`app_decals.rs`)
//!   reste à tous les niveaux sauf 0.
//! * **Lumière** — repère fixe construit une fois (`FUN_140621b90`) : rotation de 50° autour de Y puis
//!   de 300° autour de Z (RenderWare, sens direct). `FUN_1406214d0` (angle `15 × heure + 90`) n'est
//!   appelée nulle part : la direction ne suit ni l'heure ni le ciel. Direction de la lumière (axe
//!   `at`) ≈ (0,383 ; −0,663 ; 0,643).
//! * **Zone** — caméra parallèle de demi-fenêtre `4 × 4524,8` unités (`+0x1770` = 4, `FUN_140621fe0`),
//!   plan lointain au double, proche au millième ; posée en reculant de la demi-fenêtre le long de la
//!   lumière depuis le centre du secteur courant à la hauteur 0.
//! * **Objets** — ceux des secteurs chargés à ±4 secteurs dont le type d'objet porte le bit
//!   `0x100000` (drapeaux AgcmObject, colonne 2 d'`objecttemplate.ini`) et dont le rayon dépasse 100.
//!   Les personnages n'y sont pas.
//! * **Texture** — 512 × 512 (`+0x17cc`), filtrage linéaire : silhouettes dessinées sans éclairage sur
//!   du blanc, inversées puis recopiées × 130/255 (`+0x9a1c` = 0x82) : 0 hors ombre, 0,51 dans l'ombre.
//! * **Sol** — second passage du terrain (`FUN_1406d31b0`, shader `DAT_140f99368`) : coordonnées de
//!   texture `0,5 − 0,5 × (repère lumière) / demi-fenêtre`, couleur `texture × facteur de brouillard ×
//!   (N·L ≥ 0)`, mélange `destination × (1 − source)`. Seul le sol reçoit l'ombre (ni objets, ni herbe).
//! * **Mise à jour** — au changement de secteur courant (rappels 2 et 4 d'AgcmMap, `FUN_140621440`),
//!   puis toutes les 120 s (`FUN_140621b20`), deux passages d'`OnIdle` après la demande.
//!
//! Le rendu GPU est dans `shadow_render.rs`, l'échantillonnage dans `terrain.wgsl`, la colle dans
//! `app_shadow.rs`.

use crate::world::{sector_origin, SECTOR_SIZE};
use glam::{Mat4, Vec3, Vec4};

/// Côté de la texture d'ombre (`AgcmShadow2+0x17cc`, `FUN_140621880`).
pub const TEXTURE_SIZE: u32 = 512;
/// Secteurs pris de part et d'autre du secteur courant (`+0x1770`).
pub const SECTOR_RANGE: i32 = 4;
/// Demi-fenêtre de la caméra par secteur de `SECTOR_RANGE` (`FUN_140621fe0`).
pub const HALF_WIDTH_PER_SECTOR: f32 = 4524.8003;
/// Intensité de l'ombre (`+0x9a1c` = 0x82, couleur du second recopiage).
pub const INTENSITY: f32 = 130.0 / 255.0;
/// Bit des drapeaux AgcmObject des objets qui projettent une ombre (`FUN_1406215c0`).
pub const CASTER_FLAG: u32 = 0x10_0000;
/// Rayon minimal d'un objet qui projette (`100.0 <` dans `FUN_140621fe0`).
pub const MIN_RADIUS: f32 = 100.0;
/// Nouvelle prise toutes les… (`FUN_140621b20`, 120 000 ms).
pub const REFRESH_MS: u64 = 120_000;
/// Passages d'`OnIdle` entre la demande (état 1) et le rendu (état 3).
pub const IDLE_DELAY: u32 = 2;

/// Ombres projetées allumées pour ce niveau d'`OP_Shadow`.
pub fn enabled(level: u8) -> bool {
    level >= 2
}

/// Repère de la caméra de lumière (lignes `right`, `up`, `at` de la matrice RenderWare).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightBasis {
    pub right: Vec3,
    pub up: Vec3,
    pub at: Vec3,
}

/// Repère fixe de `FUN_140621b90` : rotation de 50° autour de Y (remplace), puis de 300° autour de Z
/// (post-concaténée), au sens direct comme `RwMatrixRotate`.
pub fn light_basis() -> LightBasis {
    let m = glam::Mat3::from_rotation_z(300f32.to_radians()) * glam::Mat3::from_rotation_y(50f32.to_radians());
    LightBasis { right: m * Vec3::X, up: m * Vec3::Y, at: m * Vec3::Z }
}

/// Demi-fenêtre de la caméra de lumière.
pub fn half_width() -> f32 {
    SECTOR_RANGE as f32 * HALF_WIDTH_PER_SECTOR
}

/// Caméra de lumière d'une prise.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowView {
    pub eye: Vec3,
    pub basis: LightBasis,
    pub half: f32,
}

impl ShadowView {
    /// Caméra du secteur courant `(sx, sy)` : centre du secteur à la hauteur 0, reculé d'une
    /// demi-fenêtre le long de la lumière.
    pub fn for_sector(sx: i32, sy: i32) -> ShadowView {
        let basis = light_basis();
        let half = half_width();
        let center = sector_origin(sx, sy) + Vec3::new(SECTOR_SIZE * 0.5, 0.0, SECTOR_SIZE * 0.5);
        ShadowView { eye: center - basis.at * half, basis, half }
    }

    pub fn near(&self) -> f32 {
        self.half * 0.001
    }

    pub fn far(&self) -> f32 {
        self.half * 2.0
    }

    /// Monde → (u, v, 0, 1) de la texture (`FUN_140621160` : inverse du repère, échelle
    /// `−0,5 / demi-fenêtre`, translation 0,5).
    pub fn uv_matrix(&self) -> Mat4 {
        let s = -0.5 / self.half;
        let row = |axis: Vec3, offset: f32| Vec4::new(axis.x * s, axis.y * s, axis.z * s, -self.eye.dot(axis) * s + offset);
        Mat4::from_cols(row(self.basis.right, 0.5), row(self.basis.up, 0.5), Vec4::ZERO, Vec4::W).transpose()
    }

    /// Monde → découpage wgpu de la passe d'ombre (x = 2u − 1, y = 1 − 2v, z de 0 au plan proche à 1
    /// au plan lointain).
    pub fn clip_matrix(&self) -> Mat4 {
        let inv = 1.0 / self.half;
        let row = |axis: Vec3, k: f32, offset: f32| Vec4::new(axis.x * k, axis.y * k, axis.z * k, -self.eye.dot(axis) * k + offset);
        let depth = 1.0 / (self.far() - self.near());
        Mat4::from_cols(
            row(self.basis.right, -inv, 0.0),
            row(self.basis.up, inv, 0.0),
            row(self.basis.at, depth, -self.near() * depth),
            Vec4::W,
        )
        .transpose()
    }
}

/// Un objet d'un modèle à `flags` (AgcmObject) et de rayon `radius` projette une ombre.
pub fn casts_shadow(flags: u32, radius: f32) -> bool {
    flags & CASTER_FLAG != 0 && radius > MIN_RADIUS
}

/// Demandes de prise, comme la machine à états d'AgcmShadow2 (`+0x1758`) : une demande (changement
/// de secteur, 120 s écoulées) passe à l'état 1, chaque `OnIdle` avance jusqu'à 3, où la prise est
/// faite et le compteur remis à zéro.
#[derive(Debug, Default, Clone)]
pub struct Schedule {
    sector: Option<(i32, i32)>,
    /// Passages d'`OnIdle` depuis la demande (`None` : aucune demande).
    pending: Option<u32>,
    since_ms: u64,
}

impl Schedule {
    /// Un passage : `sector` = secteur courant, `dt_ms` écoulées. Vrai s'il faut prendre l'ombre
    /// maintenant (pour `sector`).
    pub fn tick(&mut self, sector: (i32, i32), dt_ms: u64) -> bool {
        if self.sector != Some(sector) {
            self.sector = Some(sector);
            self.pending = Some(0);
        }
        self.since_ms += dt_ms;
        if self.since_ms > REFRESH_MS {
            self.since_ms = 0;
            self.pending.get_or_insert(0);
        }
        match self.pending {
            Some(n) if n >= IDLE_DELAY => {
                self.pending = None;
                self.since_ms = 0;
                true
            }
            Some(n) => {
                self.pending = Some(n + 1);
                false
            }
            None => false,
        }
    }

    /// Nouvelle demande (option rallumée, secteurs chargés entre-temps).
    pub fn request(&mut self) {
        self.pending.get_or_insert(0);
    }

    /// Oublie le secteur (option éteinte) : la prochaine activation reprend une ombre.
    pub fn reset(&mut self) {
        *self = Schedule::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lumiere_fixe_de_l_archonia() {
        let b = light_basis();
        // at = Rz(300°) · (sin 50°, 0, cos 50°).
        assert!((b.at - Vec3::new(0.38302, -0.66341, 0.64279)).length() < 1e-4, "{:?}", b.at);
        assert!((b.up - Vec3::new(0.86603, 0.5, 0.0)).length() < 1e-4, "{:?}", b.up);
        assert!(b.right.dot(b.at).abs() < 1e-5 && b.up.dot(b.at).abs() < 1e-5);
        // La lumière descend : élévation d'environ 41,6°.
        assert!((b.at.y.asin().to_degrees() + 41.56).abs() < 0.05);
        assert!((half_width() - 18099.2).abs() < 0.01);
    }

    #[test]
    fn coordonnees_de_texture_et_decoupage() {
        let v = ShadowView::for_sector(400, 400);
        let center = Vec3::new(3200.0, 0.0, 3200.0);
        assert!((v.eye - (center - v.basis.at * v.half)).length() < 1e-3);
        let uv = v.uv_matrix();
        let c = uv * center.extend(1.0);
        assert!((c.x - 0.5).abs() < 1e-4 && (c.y - 0.5).abs() < 1e-4);
        // Un pas d'une demi-fenêtre le long de `right` : bord u = 0 ; le long de `up` : v = 0.
        let p = uv * (center + v.basis.right * v.half).extend(1.0);
        assert!(p.x.abs() < 1e-3 && (p.y - 0.5).abs() < 1e-3);
        let p = uv * (center + v.basis.up * v.half).extend(1.0);
        assert!((p.x - 0.5).abs() < 1e-3 && p.y.abs() < 1e-3);
        // Le découpage suit les mêmes coordonnées : x = 2u − 1, y = 1 − 2v, profondeur au milieu.
        let clip = v.clip_matrix();
        for q in [center, center + Vec3::new(5000.0, 1200.0, -3000.0)] {
            let t = uv * q.extend(1.0);
            let k = clip * q.extend(1.0);
            assert!((k.x - (2.0 * t.x - 1.0)).abs() < 1e-4 && (k.y - (1.0 - 2.0 * t.y)).abs() < 1e-4);
        }
        let k = clip * center.extend(1.0);
        assert!((k.z - (v.half - v.near()) / (v.far() - v.near())).abs() < 1e-4);
        // Seul le sol reçoit, seuls les grands objets marqués projettent.
        assert!(casts_shadow(0x90_2100, 500.0) && !casts_shadow(0x80_2100, 500.0) && !casts_shadow(0x10_0100, 80.0));
        assert!(!enabled(1) && enabled(2) && enabled(3));
    }

    #[test]
    fn prise_au_changement_de_secteur_puis_toutes_les_deux_minutes() {
        let mut s = Schedule::default();
        // Demande au premier passage, prise au troisième (états 1 → 2 → 3).
        assert!(!s.tick((1, 1), 16));
        assert!(!s.tick((1, 1), 16));
        assert!(s.tick((1, 1), 16));
        assert!(!s.tick((1, 1), 16));
        // Changement de secteur : nouvelle prise deux passages plus tard.
        assert!(!s.tick((1, 2), 16));
        assert!(!s.tick((1, 2), 16));
        assert!(s.tick((1, 2), 16));
        // 120 s plus tard.
        assert!(!s.tick((1, 2), REFRESH_MS + 1));
        assert!(!s.tick((1, 2), 16));
        assert!(s.tick((1, 2), 16));
    }
}
