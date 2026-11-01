//! Systèmes de particules d'Archonia (`AgcdEffParticleSystem`, type 1, et
//! `AgcdEffParticleSys_SimpleBlackHole`, type 2), simulés comme `AgcdEffCtrl_ParticleSysTem` et
//! `AgcdEffCtrl_ParticleSyst_SBH`. Relevé sur Archonia.exe :
//!
//! * émission (type 1, FUN_1408f9a30) : au plus une émission par image, quand `interval` ms se sont
//!   écoulées depuis la précédente ; rien si le drapeau « plafond » est mis et le maximum atteint, ni
//!   quand une particule née maintenant survivrait à la base (base sans boucle) ; angles d'émission
//!   `t × vitesse` bornés ; direction (FUN_1408fad00) : sphère, cône ou `dir` tournée autour de Y puis
//!   de l'axe, puis par le repère de l'émetteur ; position = repère (sauf repère local) + décalage
//!   le long de la direction (ou anneau horizontal) + point de la forme (FUN_1408fa690 boîte,
//!   FUN_1408fa7a0 cercle, FUN_1408fa900 sphère) ; création (FUN_1408fb5f0) : vitesse commune à
//!   l'émission, rotation propre, vie, gravité et vent tirés par particule, la plus ancienne
//!   remplacée au-delà du maximum (FUN_14083e2a0) ;
//! * mise à jour (FUN_14083dc30) : `p = p0 + v·t·échelle + ½t²·(gravité·(0, −980, 0) + vent·(1, 0, 0))`
//!   (t en s ; l'animation 9 remplace `t` du terme de vitesse), angle `t × rotation`, couleur,
//!   coordonnées de texture et taille par les animations 0, 1 et 8 sur l'âge de la particule ;
//! * rendu (FUN_14083e4a0) : carré de demi-côté 7,064 × taille × échelle du jeu, orienté selon les
//!   drapeaux (caméra, verticale tournée vers la caméra, axes de l'émetteur, ou plan XY) ;
//! * trou noir (FUN_1408f9110, FUN_14083dff0, FUN_1408397b0) : vitesse dans le plan XY à l'angle
//!   d'émission (ou dans le plan de l'écran), départ éventuel à `−v × vie`, pas de gravité, angle fixe.
//!
//! Le hasard d'Archonia (table de flottants) est remplacé par un générateur propre à chaque émetteur.
//!
//! Traînées (type 3, `AgcdEffCtrl_Tail`) : [`Trail`]. Mode à un repère de FUN_14083d8b0
//! ([`Trail::edges`] : repère de la base parente ou du jeu, voir `app_effects.rs`) ; mode à deux
//! nœuds (drapeau de jeu 0x400) : bords lus directement sur les nœuds HAnim 702 et 701 du porteur
//! par `app_effects.rs`, puis mêmes points et même ruban.

use super::{anim_time, base_state, lerp_keys, BOARD_HALF};
use archrust_data::effset::{anim_kind, psys_dir, psys_flag, sbh_flag, tail_flag, AnimKeys, EffBase, Particles, PsysParams, SbhParams, TailParams};
use glam::{Mat4, Quat, Vec3};

/// Forces constantes d'Archonia (0x140f48df8) : gravité et vent.
pub const GRAVITY: Vec3 = Vec3::new(0.0, -980.0, 0.0);
pub const WIND: Vec3 = Vec3::X;
/// Intervalle d'émission d'un trou noir sans intervalle (FUN_1408f9110).
const SBH_DEFAULT_INTERVAL: u32 = 30;

/// Repère de l'émetteur dans le monde (sans l'échelle du jeu, appliquée à part comme Archonia).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub position: Vec3,
    pub rotation: Quat,
}

impl Frame {
    fn matrix(&self) -> Mat4 {
        Mat4::from_rotation_translation(self.rotation, self.position)
    }
}

/// Axes de l'écran dans le monde.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraAxes {
    pub right: Vec3,
    pub up: Vec3,
}

impl CameraAxes {
    /// Planche face à la caméra (matrice +0x80 de la caméra des effets) : droite et haut de l'écran.
    fn billboard(&self) -> (Vec3, Vec3) {
        (self.right, self.up)
    }

    /// Planche verticale (matrice +0xc0) : droite RenderWare de la caméra (vers la gauche de
    /// l'écran, non inversée par FUN_1408dcb70) et haut = Y.
    fn billboard_y(&self) -> (Vec3, Vec3) {
        let r = Vec3::new(-self.right.x, 0.0, -self.right.z);
        (if r.length_squared() > 1e-8 { r.normalize() } else { Vec3::NEG_X }, Vec3::Y)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Particle {
    pub start: Vec3,
    pub velocity: Vec3,
    /// Naissance (ms, temps du jeu d'effets).
    pub birth: u32,
    /// Vie (ms) ; 0 = sans fin.
    pub life: u32,
    /// Type 1 : vitesse de rotation (degrés/s) ; type 2 : angle fixe (degrés).
    pub spin: f32,
    pub gravity: f32,
    pub wind: f32,
    /// Couleur tirée à la naissance (drapeau « couleur au hasard »).
    pub color: Option<[u8; 3]>,
}

/// Une particule à dessiner : coins dans l'ordre des planches (−,+) (−,−) (+,+) (+,−).
#[derive(Debug, Clone, PartialEq)]
pub struct Sprite {
    pub corners: [Vec3; 4],
    pub uv: [f32; 4],
    pub color: [f32; 4],
}

/// État d'un émetteur (une base de type 1 ou 2 d'un jeu joué).
#[derive(Debug, Clone)]
pub struct Emitter {
    pub particles: Vec<Particle>,
    /// Dernière émission (ms, temps du jeu).
    clock: u32,
    rng: u32,
}

/// Angle d'émission : `v` borné à `[min, max]`, `min` si les bornes sont égales (FUN_1408f9a30).
fn clamp_angle(v: f32, [min, max]: [f32; 2]) -> f32 {
    if max == min || v < min {
        min
    } else if v > max {
        max
    } else {
        v
    }
}

fn sin_cos_deg(a: f32) -> (f32, f32) {
    a.to_radians().sin_cos()
}

impl Emitter {
    pub fn new(seed: u32) -> Self {
        Self { particles: Vec::new(), clock: 0, rng: seed.wrapping_mul(0x9e37_79b9) | 1 }
    }

    /// Hasard dans [0, 1).
    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Hasard dans [−1, 1).
    fn rand_sym(&mut self) -> f32 {
        self.rand() * 2.0 - 1.0
    }

    /// Émission à l'instant `t` (ms depuis le début du jeu) : au plus une par appel, comme Archonia.
    pub fn emit(&mut self, base: &EffBase, t: u32, frame: &Frame, cam: &CameraAxes) {
        match &base.particles {
            Some(Particles::System(p)) => self.emit_system(base, p, t, frame),
            Some(Particles::BlackHole(p)) => self.emit_black_hole(base, p, t, frame, cam),
            None => {}
        }
    }

    /// Avance l'horloge d'émission ; `true` si une émission est due.
    fn due(&mut self, t: u32, interval: u32) -> bool {
        self.clock = self.clock.min(t);
        let elapsed = t - self.clock;
        if elapsed == 0 || elapsed < interval {
            return false;
        }
        self.clock = t;
        true
    }

    fn push(&mut self, p: Particle, max: u32) {
        if self.particles.len() >= max.max(1) as usize {
            self.particles.remove(0);
        }
        self.particles.push(p);
    }

    fn direction(&mut self, base: &EffBase, p: &PsysParams, clock_s: f32, frame: &Frame) -> Vec3 {
        let d = Vec3::from(p.dir);
        let v = if p.dir_flags & psys_dir::NONE != 0 {
            return Vec3::ZERO;
        } else if p.dir_flags & psys_dir::RANDOM != 0 {
            let ((st, ct), (sp, cp)) = (sin_cos_deg(self.rand() * 360.0), sin_cos_deg(self.rand() * 360.0));
            Vec3::new(ct * cp, sp, cp * st)
        } else if p.dir_flags & psys_dir::CONE != 0 {
            let (st, ct) = sin_cos_deg(self.rand() * 360.0);
            let (sp, cp) = sin_cos_deg(self.rand_sym() * p.cone);
            // Cône autour de Y, incliné vers la direction tournée par l'émetteur.
            Vec3::new(ct * sp, cp, st * sp) + frame.rotation * d - Vec3::Y
        } else {
            let a1 = clamp_angle(clock_s * p.angle_rate[0], p.angle1);
            let a2 = clamp_angle(clock_s * p.angle_rate[1], p.angle2);
            let r1 = Quat::from_rotation_y(a1.to_radians());
            let axis = r1 * Vec3::from(p.axis);
            let mut q = if axis.length_squared() > 1e-8 { Quat::from_axis_angle(axis.normalize(), a2.to_radians()) * r1 } else { r1 };
            if p.dir_flags & psys_dir::NO_FRAME == 0 && base.flags & psys_flag::LOCAL_FRAME == 0 {
                q = frame.rotation * q;
            }
            q * d
        };
        v.normalize_or_zero()
    }

    fn shape_point(&mut self, p: &PsysParams) -> Vec3 {
        let [dx, dy, dz] = p.dims;
        match p.shape {
            1 => Vec3::new(self.rand_sym() * dx, self.rand_sym() * dy, self.rand_sym() * dz),
            // Deux angles tirés séparément pour x et z (FUN_1408fa7a0).
            2 => {
                let x = sin_cos_deg(self.rand() * 360.0).1 * dx;
                let y = self.rand_sym() * dy;
                let z = sin_cos_deg(self.rand() * 360.0).0 * dx;
                Vec3::new(x, y, z)
            }
            3 => {
                let a = self.rand() * self.rand() * 360.0 * (10.0 / 11.0);
                let b = self.rand() * self.rand() * 360.0 * (10.0 / 11.0);
                let ((sa, ca), (sb, cb)) = (sin_cos_deg(a), sin_cos_deg(b));
                Vec3::new(cb * ca, sa, ca * sb) * dx
            }
            _ => Vec3::ZERO,
        }
    }

    /// Point d'un anneau horizontal (angle = produit de deux tirages × 327,27°, comme Archonia).
    fn ring(&mut self, radius: f32, random_radius: bool) -> Vec3 {
        let a = self.rand() * self.rand() * 360.0 * (10.0 / 11.0);
        let r = if random_radius { self.rand() * radius } else { radius };
        let (s, c) = sin_cos_deg(a);
        Vec3::new(r * c, 0.0, r * s)
    }

    fn emit_system(&mut self, base: &EffBase, p: &PsysParams, t: u32, frame: &Frame) {
        let tb = t.saturating_sub(base.delay);
        let capped = base.flags & psys_flag::CAPPED != 0;
        let outlives = base.loop_opt == 0 && tb as u64 + p.life as u64 + p.life_var as u64 > base.life as u64;
        if outlives || (capped && self.particles.len() >= p.max as usize) || !self.due(t, p.interval) {
            return;
        }
        let dir = self.direction(base, p, t as f32 * 0.001, frame);
        let mut pos = Vec3::ZERO;
        if base.flags & psys_flag::RING == 0 {
            pos = dir * (p.offset + self.rand_sym() * p.offset_var);
        }
        if base.flags & psys_flag::LOCAL_FRAME == 0 {
            pos += frame.position;
        }
        if base.flags & psys_flag::RING != 0 {
            pos += self.ring(p.offset, base.flags & psys_flag::RING_RANDOM_RADIUS != 0);
        }
        let count = p.count + (p.count_var as f32 * self.rand()) as i32;
        let velocity = dir * (p.speed + self.rand_sym() * p.speed_var);
        for _ in 0..count.max(0) {
            if capped && self.particles.len() >= p.max as usize {
                break;
            }
            let start = pos + self.shape_point(p);
            let color = (base.flags & psys_flag::RANDOM_COLOR != 0).then(|| [0; 3].map(|_| (self.rand() * 256.0) as u8));
            let spin = p.spin + self.rand_sym() * p.spin_var;
            let life = p.life + (p.life_var as f32 * self.rand()) as u32;
            let gravity = p.gravity + self.rand_sym() * p.gravity_var;
            let wind = p.wind + self.rand_sym() * p.wind_var;
            self.push(Particle { start, velocity, birth: t, life, spin, gravity, wind, color }, p.max);
        }
    }

    fn emit_black_hole(&mut self, base: &EffBase, p: &SbhParams, t: u32, frame: &Frame, cam: &CameraAxes) {
        let tb = t.saturating_sub(base.delay);
        let f = base.flags;
        if (base.loop_opt == 0 && tb as u64 + p.life as u64 > base.life as u64) || !self.due(t, if p.interval == 0 { SBH_DEFAULT_INTERVAL } else { p.interval }) {
            return;
        }
        let count = p.count + (p.count_var as f32 * self.rand()) as i32;
        let step = if p.max > 0 { (p.angle_max - p.angle_min) / p.max as f32 } else { 0.0 };
        let mut angle = 0.0;
        for i in 0..count.max(0) {
            if f & sbh_flag::CAPPED != 0 && self.particles.len() >= p.max as usize {
                return;
            }
            if f & sbh_flag::SEQUENTIAL == 0 {
                angle = (p.angle_max - p.angle_min) * self.rand() + p.angle_min;
            } else {
                angle = if i == 0 { self.particles.last().map_or(p.angle_min, |l| l.spin + step) } else { angle + step };
                if angle > p.angle_max {
                    angle = p.angle_min;
                }
            }
            let speed = p.speed + self.rand_sym() * p.speed_var;
            let mut velocity = if f & sbh_flag::NO_ANGLE == 0 {
                let (s, c) = sin_cos_deg(180.0 - angle);
                Vec3::new(speed * s, speed * c, 0.0)
            } else {
                angle = 0.0;
                Vec3::new(0.0, speed, 0.0)
            };
            if f & sbh_flag::SCREEN != 0 {
                let (r, u) = cam.billboard();
                velocity = r * velocity.x + u * velocity.y;
            }
            let mut start = if f & sbh_flag::INWARD != 0 { velocity * (p.life as f32 * -0.001) } else { Vec3::ZERO };
            if f & (sbh_flag::SCREEN | sbh_flag::LOCAL_FRAME) == sbh_flag::SCREEN {
                start += frame.position;
            }
            if f & sbh_flag::RING != 0 {
                start += self.ring(p.ring, f & sbh_flag::RING_RANDOM_RADIUS != 0);
            }
            let color = (f & sbh_flag::RANDOM_COLOR != 0).then(|| [0; 3].map(|_| (self.rand() * 256.0) as u8));
            self.push(Particle { start, velocity, birth: t, life: p.life, spin: angle, gravity: 0.0, wind: 0.0, color }, p.max);
        }
    }

    /// Retire les particules mortes à l'instant `t` et rend les autres, prêtes à dessiner.
    pub fn sprites(&mut self, base: &EffBase, t: u32, frame: &Frame, cam: &CameraAxes, scale: f32) -> Vec<Sprite> {
        self.particles.retain(|p| p.life == 0 || t.saturating_sub(p.birth) < p.life);
        let black_hole = matches!(base.particles, Some(Particles::BlackHole(_)));
        let pos_scale = base.anims.iter().find(|a| a.kind == anim_kind::PARTICLE_POS_SCALE);
        let fm = frame.matrix();
        let f = base.flags;
        let mut out = Vec::with_capacity(self.particles.len());
        for p in &self.particles {
            let age = t.saturating_sub(p.birth);
            let s = age as f32 * 0.001;
            let k = match pos_scale.map(|a| (a, &a.keys)) {
                Some((a, AnimKeys::Scalar(keys))) => lerp_keys(keys, anim_time(a, age), |x, y, m| x + (y - x) * m).unwrap_or(1.0),
                _ => s,
            } * scale;
            let mut pos = p.start + p.velocity * k;
            if !black_hole {
                pos += (GRAVITY * p.gravity + WIND * p.wind) * (0.5 * s * s);
            }
            let st = base_state(base, age);
            let color = match p.color {
                Some([r, g, b]) => {
                    let fade = if p.life == 0 { s } else { age as f32 / p.life as f32 };
                    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, (1.0 - fade).clamp(0.0, 1.0)]
                }
                None => st.color,
            };
            if color[3] <= 0.0 {
                continue;
            }
            let (hw, hh) = (BOARD_HALF * scale * st.scale.x, BOARD_HALF * scale * st.scale.y);
            let corners = if black_hole {
                // Carré tourné autour de Z (FUN_1408d3350), puis orienté et placé (FUN_1408397b0).
                let (sn, cs) = sin_cos_deg(p.spin);
                let local = [(-hw, hh), (-hw, -hh), (hw, hh), (hw, -hh)].map(|(x, y)| (x * cs - y * sn, x * sn + y * cs));
                if f & sbh_flag::SCREEN != 0 {
                    let (r, u) = if f & 2 != 0 {
                        cam.billboard_y()
                    } else if f & 1 != 0 {
                        cam.billboard()
                    } else {
                        (Vec3::X, Vec3::Y)
                    };
                    let center = if f & sbh_flag::LOCAL_FRAME != 0 { pos + frame.position } else { pos };
                    local.map(|(x, y)| center + r * x + u * y)
                } else {
                    local.map(|(x, y)| fm.transform_point3(pos + Vec3::new(x, y, 0.0)))
                }
            } else {
                let center = if f & psys_flag::LOCAL_FRAME != 0 { fm.transform_point3(pos) } else { pos };
                let (r, u) = if f & psys_flag::BILLBOARD_Y != 0 {
                    cam.billboard_y()
                } else if f & psys_flag::BILLBOARD != 0 {
                    cam.billboard()
                } else if f & psys_flag::LOCAL_FRAME != 0 {
                    (frame.rotation * Vec3::X, frame.rotation * Vec3::Y)
                } else {
                    (Vec3::X, Vec3::Y)
                };
                // FUN_14083e4a0 : A = (R cos − U sin)·demi-largeur, B = (U cos + R sin)·demi-hauteur.
                let (sn, cs) = sin_cos_deg(s * p.spin);
                let a = (r * cs - u * sn) * hw;
                let b = (u * cs + r * sn) * hh;
                [center - a + b, center - a - b, center + a + b, center + a - b]
            };
            out.push(Sprite { corners, uv: st.uv, color });
        }
        out
    }
}

/// Points de contrôle gardés par une traînée (FUN_14083f2f0).
const TAIL_POINTS: usize = 5;
/// Subdivisions de la courbe entre deux points (Archonia : 16, ou 20 en mode à un repère ; ici une
/// Catmull-Rom remplace ses courbes FUN_140840120 à FUN_140840c30, non relevées en détail).
const TAIL_STEPS: usize = 8;

/// Traînée : points (naissance, bord haut, bord bas) du plus ancien au plus récent ; le plus ancien
/// glisse vers le suivant entre deux ajouts, la tête suit l'émetteur.
#[derive(Debug, Clone, Default)]
pub struct Trail {
    points: Vec<(u32, Vec3, Vec3)>,
    /// Position de départ du plus ancien point (il glisse vers le suivant).
    tail_from: (Vec3, Vec3),
    last: u32,
}

/// Un sommet de ruban : position, coordonnées de texture, opacité (0 à la queue, 1 à la tête).
pub type TrailVertex = (Vec3, [f32; 2], f32);

impl Trail {
    /// Bords de la traînée sur le repère de l'émetteur (FUN_14083d8b0, mode à un repère).
    pub fn edges(base: &EffBase, p: &TailParams, frame: &Frame, scale: f32) -> (Vec3, Vec3) {
        let f = base.flags;
        let axis = if f & tail_flag::AXIS_X != 0 {
            frame.rotation * Vec3::X
        } else {
            let tilt = if f & tail_flag::TILT_45 != 0 {
                45f32
            } else if f & tail_flag::TILT_135 != 0 {
                135.0
            } else {
                0.0
            };
            frame.rotation * (Quat::from_rotation_z(tilt.to_radians()) * Vec3::Y)
        };
        (frame.position + axis * p.top * scale, frame.position + axis * p.bottom * scale)
    }

    /// Ajoute, retire et fait glisser les points à l'instant `t` (ms du jeu), bords courants donnés.
    pub fn update(&mut self, p: &TailParams, t: u32, edges: (Vec3, Vec3)) {
        if self.points.is_empty() && self.last == 0 {
            self.points.push((t, edges.0, edges.1));
            self.tail_from = edges;
            self.last = t;
        }
        let interval = p.interval.max(1);
        let mut since = t.saturating_sub(self.last);
        if since >= interval {
            since -= interval;
            self.last = t;
            self.points.push((t, edges.0, edges.1));
            if self.points.len() == 1 {
                self.tail_from = edges;
            }
            if self.points.len() > TAIL_POINTS {
                self.points.remove(0);
                self.tail_from = (self.points[0].1, self.points[0].2);
            }
            if self.points.len() >= p.max as usize && self.points.len() > 1 {
                self.points.remove(0);
            }
        }
        while self.points.first().is_some_and(|q| t.saturating_sub(q.0) >= p.life) {
            self.points.remove(0);
            if let Some(q) = self.points.first() {
                self.tail_from = (q.1, q.2);
            }
        }
        if self.points.is_empty() {
            return;
        }
        let target = self.points.get(1).map_or(edges, |q| (q.1, q.2));
        let m = (since % interval) as f32 / interval as f32;
        self.points[0].1 = self.tail_from.0.lerp(target.0, m);
        self.points[0].2 = self.tail_from.1.lerp(target.1, m);
    }

    /// Ruban de la queue à la tête (bords courants) : quadrilatères (haut ancien, bas ancien, haut
    /// récent, bas récent). Comme le cas à un point de FUN_140839e80, u va de 0 (queue) à 1 (tête),
    /// v vaut 0 en haut et 1 en bas, l'opacité de 0 à 1.
    pub fn ribbon(&self, edges: (Vec3, Vec3)) -> Vec<[TrailVertex; 4]> {
        let mut ctl: Vec<(Vec3, Vec3)> = self.points.iter().map(|q| (q.1, q.2)).collect();
        if ctl.is_empty() {
            return Vec::new();
        }
        ctl.push(edges);
        let n = ctl.len();
        let at = |i: isize| ctl[i.clamp(0, n as isize - 1) as usize];
        let cr = |a: Vec3, b: Vec3, c: Vec3, d: Vec3, u: f32| {
            let (u2, u3) = (u * u, u * u * u);
            0.5 * ((2.0 * b) + (c - a) * u + (2.0 * a - 5.0 * b + 4.0 * c - d) * u2 + (3.0 * b - a - 3.0 * c + d) * u3)
        };
        let mut line: Vec<(Vec3, Vec3)> = Vec::with_capacity((n - 1) * TAIL_STEPS + 1);
        for i in 0..n as isize - 1 {
            let (a, b, c, d) = (at(i - 1), at(i), at(i + 1), at(i + 2));
            for k in 0..TAIL_STEPS {
                let u = k as f32 / TAIL_STEPS as f32;
                line.push((cr(a.0, b.0, c.0, d.0, u), cr(a.1, b.1, c.1, d.1, u)));
            }
        }
        line.push(edges);
        let last = (line.len() - 1).max(1) as f32;
        line.windows(2)
            .enumerate()
            .map(|(i, w)| {
                let (u0, u1) = (i as f32 / last, (i + 1) as f32 / last);
                [(w[0].0, [u0, 0.0], u0), (w[0].1, [u0, 1.0], u0), (w[1].0, [u1, 0.0], u1), (w[1].1, [u1, 1.0], u1)]
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_data::effset::{base_kind, EffAnim};

    /// Traînée pas à pas : un point par intervalle, au plus cinq, queue qui glisse, vie des points.
    #[test]
    fn trail_step_by_step() {
        let p = TailParams { max: 20, interval: 50, life: 1000, top: 700.0, bottom: -700.0 };
        let base = EffBase { kind: base_kind::TAIL, life: 2000, flags: 0x128, tail: Some(p.clone()), ..Default::default() };
        let frame = |x: f32| Frame { position: Vec3::new(x, 0.0, 0.0), rotation: Quat::IDENTITY };
        let e = Trail::edges(&base, &p, &frame(0.0), 1.0);
        assert_eq!(e, (Vec3::new(0.0, 700.0, 0.0), Vec3::new(0.0, -700.0, 0.0)));
        let tilted = EffBase { flags: tail_flag::TILT_45, ..base.clone() };
        assert!((Trail::edges(&tilted, &p, &frame(0.0), 1.0).0 - Vec3::new(-495.0, 495.0, 0.0)).length() < 1.0);
        let mut tr = Trail::default();
        for t in (0..=400).step_by(10) {
            let x = t as f32;
            tr.update(&p, t, Trail::edges(&base, &p, &frame(x), 1.0));
        }
        assert_eq!(tr.points.len(), TAIL_POINTS);
        // Le plus récent est à x = 400 (ajout à 400), le plus ancien glisse entre 150 et 200.
        assert_eq!(tr.points.last().unwrap().1.x, 400.0);
        assert!((150.0..=200.0).contains(&tr.points[0].1.x), "{}", tr.points[0].1.x);
        let quads = tr.ribbon(Trail::edges(&base, &p, &frame(400.0), 1.0));
        assert_eq!(quads.len(), TAIL_POINTS * TAIL_STEPS);
        assert_eq!((quads[0][0].2, quads.last().unwrap()[3].2), (0.0, 1.0));
        // Sans mouvement après 1 s, tous les points ont expiré.
        tr.update(&p, 1500, Trail::edges(&base, &p, &frame(400.0), 1.0));
        assert!(tr.points.len() <= 1);
    }

    fn psys(flags: u32) -> (EffBase, PsysParams) {
        let p = PsysParams {
            max: 20,
            interval: 50,
            reserved: [0; 64],
            speed: 30.0,
            speed_var: 0.0,
            offset: 0.0,
            offset_var: 0.0,
            count: 1,
            count_var: 0,
            angle_rate: [0.0; 2],
            angle1: [0.0; 2],
            angle2: [0.0; 2],
            dir: [0.0, 0.0, 1.0],
            axis: [1.0, 0.0, 0.0],
            cone: 180.0,
            dir_flags: 0,
            shape: 0,
            dims: [50.0; 3],
            spin: 90.0,
            spin_var: 0.0,
            life: 1000,
            life_var: 0,
            gravity: 0.1,
            wind: 0.0,
            gravity_var: 0.0,
            wind_var: 0.0,
            reserved2: 0,
        };
        let base = EffBase {
            kind: base_kind::PSYS,
            life: 1999,
            flags,
            particles: Some(Particles::System(p.clone())),
            anims: vec![EffAnim { kind: anim_kind::SCALE, flags: 0, life: 1000, loop_opt: 1, keys: AnimKeys::Vec3(vec![(0, [1.5; 3]), (1000, [0.5; 3])]) }],
            ..Default::default()
        };
        (base, p)
    }

    const CAM: CameraAxes = CameraAxes { right: Vec3::X, up: Vec3::Y };

    /// Émetteur pas à pas : cadence, mouvement balistique, plafond, arrêt avant la fin de la base.
    #[test]
    fn system_emitter_step_by_step() {
        let (base, _) = psys(psys_flag::BILLBOARD | psys_flag::CAPPED);
        let frame = Frame { position: Vec3::new(100.0, 0.0, 0.0), rotation: Quat::IDENTITY };
        let mut e = Emitter::new(1);
        // Pas d'émission à t = 0 ni avant 50 ms, puis une par intervalle écoulé (une par image au plus).
        e.emit(&base, 0, &frame, &CAM);
        e.emit(&base, 40, &frame, &CAM);
        assert!(e.particles.is_empty());
        e.emit(&base, 50, &frame, &CAM);
        e.emit(&base, 60, &frame, &CAM);
        assert_eq!(e.particles.len(), 1);
        e.emit(&base, 250, &frame, &CAM);
        assert_eq!(e.particles.len(), 2, "une seule émission malgré 190 ms écoulées");
        // Direction (0, 0, 1) à 30 unités/s, départ au repère ; gravité 0,1 × (−980) ; à 1 s d'âge
        // la particule est morte (vie 1000 ms), à 0,5 s : z = 15, y = −0,5 × 98 × 0,25.
        let p = &e.particles[0];
        assert_eq!((p.start, p.velocity, p.birth), (frame.position, Vec3::new(0.0, 0.0, 30.0), 50));
        let sprites = e.sprites(&base, 550, &frame, &CAM, 1.0);
        let c = (sprites[0].corners[0] + sprites[0].corners[3]) * 0.5;
        assert!((c - Vec3::new(100.0, -12.25, 15.0)).length() < 1e-3, "{c}");
        // Taille 1,5 → 0,5 sur 1 s : demi-côté 7,064 × 1 à mi-vie ; rotation 90°/s × 0,5 s = 45°.
        let d = sprites[0].corners[2] - sprites[0].corners[1];
        assert!((d.length() - 2.0 * 7.064 * 2f32.sqrt()).abs() < 1e-2, "{d}");
        assert!(e.sprites(&base, 1050, &frame, &CAM, 1.0).len() == 1, "la première est morte");
        // Plafond : jamais plus de 20 vivantes ; plus d'émission quand une particule survivrait à la base.
        let mut e = Emitter::new(2);
        for t in (0..2000).step_by(16) {
            e.emit(&base, t, &frame, &CAM);
            assert!(e.particles.len() <= 20);
            if t > 999 {
                assert!(e.particles.iter().all(|p| p.birth <= 999), "émission après 999 ms");
            }
        }
    }

    #[test]
    fn directions_shapes_and_black_hole() {
        let (mut base, mut p) = psys(0);
        p.dir_flags = psys_dir::RANDOM;
        p.shape = 1;
        base.particles = Some(Particles::System(p.clone()));
        let frame = Frame { position: Vec3::ZERO, rotation: Quat::from_rotation_y(1.0) };
        let mut e = Emitter::new(3);
        for i in 1..200 {
            e.emit(&base, i * 50, &frame, &CAM);
        }
        assert!(e.particles.iter().all(|q| (q.velocity.length() - 30.0).abs() < 1e-3 && q.start.abs().max_element() <= 50.0));
        // Direction par défaut : (0, 0, 1) tournée par l'émetteur.
        let mut e = Emitter::new(4);
        let (base0, p0) = psys(0);
        assert!((e.direction(&base0, &p0, 0.0, &frame) - frame.rotation * Vec3::Z).length() < 1e-5);
        // Angle 1 = 90° autour de Y : (0, 0, 1) → (1, 0, 0) en repère local.
        let mut p1 = p0.clone();
        p1.angle1 = [90.0, 90.0];
        p1.dir_flags = psys_dir::NO_FRAME;
        assert!((e.direction(&base0, &p1, 0.0, &frame) - Vec3::X).length() < 1e-5);
        assert_eq!(clamp_angle(5.0, [10.0, 20.0]), 10.0);
        assert_eq!(clamp_angle(25.0, [10.0, 20.0]), 20.0);
        assert_eq!(clamp_angle(15.0, [10.0, 20.0]), 15.0);
        // Trou noir vers l'intérieur : départ à −v × vie, arrivée au centre à la mort.
        let sbh = SbhParams { max: 10, count: 2, count_var: 0, interval: 100, life: 500, speed: 40.0, speed_var: 0.0, angle_min: 0.0, angle_max: 360.0, ring: 0.0 };
        let b = EffBase { kind: archrust_data::effset::base_kind::PSYS_SBH, life: 5000, flags: sbh_flag::INWARD | sbh_flag::SCREEN | 1, particles: Some(Particles::BlackHole(sbh)), ..Default::default() };
        let mut e = Emitter::new(5);
        e.emit(&b, 100, &Frame { position: Vec3::new(0.0, 50.0, 0.0), rotation: Quat::IDENTITY }, &CAM);
        assert_eq!(e.particles.len(), 2);
        let q = &e.particles[0];
        assert!(q.velocity.z.abs() < 1e-5 && (q.velocity.length() - 40.0).abs() < 1e-3);
        assert!((q.start + q.velocity * 0.5 - Vec3::new(0.0, 50.0, 0.0)).length() < 1e-3);
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : les particules de la montée de niveau
    /// (jeu 3611) se lisent et s'émettent sur la durée de leurs bases.
    #[test]
    fn level_up_particles_from_client_data() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let dir = std::path::Path::new(&dir).join("effect/ini/new");
        let (Ok(set), Ok(var)) = (std::fs::read(dir.join("efffile_set.eff")), std::fs::read(dir.join("efffile_var.eff"))) else { return };
        let sets = archrust_data::EffectSets::parse(&set, &var);
        let lv = sets.decode(3611).expect("3611");
        let bases: Vec<&EffBase> = lv.bases.iter().filter(|b| b.particles.is_some()).collect();
        assert_eq!(bases.len(), 4);
        let Some(Particles::System(p)) = &bases[0].particles else { panic!() };
        assert_eq!((p.max, p.interval, p.life, p.dir_flags), (20, 50, 1000, psys_dir::RANDOM));
        assert!(bases[0].flags & psys_flag::RANDOM_COLOR != 0 && bases[0].flags & psys_flag::BILLBOARD != 0);
        let frame = Frame { position: Vec3::ZERO, rotation: Quat::IDENTITY };
        let mut e = Emitter::new(7);
        let mut drawn = 0;
        for t in (0..3000).step_by(16) {
            e.emit(bases[0], t, &frame, &CAM);
            drawn = drawn.max(e.sprites(bases[0], t, &frame, &CAM, 1.0).len());
        }
        assert!(drawn >= 10, "{drawn}");
        assert!(e.sprites(bases[0], 3000, &frame, &CAM, 1.0).is_empty());
        // Tous les systèmes de particules du client se décodent.
        let n = sets.iter().filter_map(|s| sets.decode(s.id)).flat_map(|d| d.bases).filter(|b| matches!(b.kind, 1 | 2)).filter(|b| b.particles.is_none()).count();
        assert_eq!(n, 0);
        // Traînées : toutes décodées ; celles du jeu 127 ont 20 points, 50 ms, 1 s, bords ±700.
        let n = sets.iter().filter_map(|s| sets.decode(s.id)).flat_map(|d| d.bases).filter(|b| b.kind == 3 && b.tail.is_none()).count();
        assert_eq!(n, 0);
        let tail = sets.decode(127).unwrap().bases.into_iter().find_map(|b| b.tail).unwrap();
        assert_eq!((tail.max, tail.interval, tail.life, tail.top, tail.bottom), (20, 50, 1000, 700.0, -700.0));
    }
}
