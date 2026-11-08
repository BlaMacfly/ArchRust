//! Champ de la caméra (choix d'ArchRust, pour la fluidité des grandes villes) : quatre plans latéraux
//! tirés des lignes de la matrice de vue-projection (méthode de Gribb et Hartmann). Les plans proche et
//! lointain ne servent pas : la distance d'affichage est déjà réglée par les options (`ObjView`,
//! `CharView`), et la profondeur inversée de `OrbitCamera::render_view_proj` ne change que la ligne z.
//! Une pièce dont la sphère englobante sort entièrement de l'un des plans ne serait pas visible : elle
//! n'est ni envoyée au GPU ni dessinée.

use super::DrawItem;
use glam::{Mat4, Vec3, Vec4};

/// Pièces animées (skinnées ou de personnage) : la pose peut sortir de la sphère de la pose de liaison
/// (bras levés, arme tendue, monture) ; leur rayon est doublé, 150 unités au moins.
const ANIMATED_MARGIN: f32 = 2.0;
const ANIMATED_MIN_RADIUS: f32 = 150.0;

#[derive(Debug, Clone, Copy)]
pub struct Frustum {
    planes: [Vec4; 4],
}

impl Frustum {
    pub fn new(view_proj: Mat4) -> Frustum {
        let (r0, r1, r3) = (view_proj.row(0), view_proj.row(1), view_proj.row(3));
        let planes = [r3 + r0, r3 - r0, r3 + r1, r3 - r1].map(|p| {
            let n = p.truncate().length();
            if n > 0.0 { p / n } else { p }
        });
        Frustum { planes }
    }

    /// La sphère touche le champ (bord compris).
    pub fn sphere(&self, center: Vec3, radius: f32) -> bool {
        self.planes.iter().all(|p| p.truncate().dot(center) + p.w >= -radius)
    }

    /// La pièce peut être visible ; une pièce sans bornes connues l'est toujours.
    pub fn keeps(&self, item: &DrawItem) -> bool {
        match world_sphere(item.model, item.mesh.bounds_center, item.mesh.bounds_radius, item.character || item.mesh.skin.is_some()) {
            Some((center, radius)) => self.sphere(center, radius),
            None => true,
        }
    }
}

/// Sphère englobante dans le monde d'une pièce de bornes locales (`center`, `radius`) posée par `model` ;
/// `None` si les bornes sont inutilisables (rayon nul ou non fini : sol de secours, boîtes).
pub fn world_sphere(model: Mat4, center: Vec3, radius: f32, animated: bool) -> Option<(Vec3, f32)> {
    if !(radius.is_finite() && radius > 0.0) {
        return None;
    }
    let scale = model.x_axis.truncate().length().max(model.y_axis.truncate().length()).max(model.z_axis.truncate().length());
    let mut r = radius * scale;
    if animated {
        r = (r * ANIMATED_MARGIN).max(ANIMATED_MIN_RADIUS);
    }
    Some((model.transform_point3(center), r))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn champ_de_la_camera() {
        // Caméra à l'origine regardant vers −Z (convention de `OrbitCamera`), profondeur inversée.
        let view = Mat4::look_at_rh(Vec3::ZERO, Vec3::NEG_Z, Vec3::Y);
        let f = Frustum::new(Mat4::perspective_rh(60f32.to_radians(), 1.5, 50_000.0, 10.0) * view);
        assert!(f.sphere(Vec3::new(0.0, 0.0, -1000.0), 1.0), "devant");
        assert!(!f.sphere(Vec3::new(0.0, 0.0, 1000.0), 50.0), "derrière");
        assert!(!f.sphere(Vec3::new(-5000.0, 0.0, -1000.0), 50.0), "loin à gauche");
        assert!(f.sphere(Vec3::new(-5000.0, 0.0, -1000.0), 5000.0), "grande sphère qui déborde dans le champ");
        assert!(f.sphere(Vec3::ZERO, 1.0), "la caméra elle-même");
        assert!(!f.sphere(Vec3::new(0.0, 3000.0, -1000.0), 10.0), "au-dessus");
    }

    #[test]
    fn sphere_d_une_piece() {
        let model = Mat4::from_scale_rotation_translation(Vec3::splat(2.0), glam::Quat::IDENTITY, Vec3::new(100.0, 0.0, 0.0));
        assert_eq!(world_sphere(model, Vec3::X, 10.0, false), Some((Vec3::new(102.0, 0.0, 0.0), 20.0)));
        assert_eq!(world_sphere(model, Vec3::ZERO, 10.0, true), Some((Vec3::new(100.0, 0.0, 0.0), 150.0)));
        assert_eq!(world_sphere(model, Vec3::ZERO, 100.0, true).map(|s| s.1), Some(400.0));
        assert_eq!(world_sphere(model, Vec3::ZERO, 0.0, false), None);
    }
}
