//! CamÃ©ra orbitale autour d'une cible (le personnage), rÃ©glÃ©e comme Archonia (AgcmCamera2,
//! AgcuCamMode, `ini/CamSetting.xml`).
//!
//! Le monde ArchLord utilise des coordonnÃ©es en centimÃ¨tres-ish (positions de l'ordre de 10âµ),
//! Y vers le haut ; on garde ce repÃ¨re.
//!
//! **RelevÃ©s dans Archonia.exe** (sha256 e9c7609bâ€¦, dÃ©tail dans `docs/FORMATS.md`, Â« CamÃ©ra Â») :
//! - `CamSetting.xml` (FUN_1408d1480) : quatre modes `<Default><Mode Index MaskFlag>` (0 libre, 1 haut,
//!   2 trois-quarts, 3 premiÃ¨re personne), chacun `Distance Default/Min/Max` et `Pitch Default/Min/Max`
//!   en degrÃ©s ; `<Specific>` par TID : `Character Height` (hauteur visÃ©e de loin) et `Face` (hauteur
//!   visÃ©e de prÃ¨s). Les `Distance` / `Pitch` des `<Specific>` sont lus mais jamais appliquÃ©s
//!   (FUN_1408d1a60 ne reprend que `Height` et `Face`) ; TID absent : 100 et 160.
//! - Hauteur visÃ©e (FUN_1406aa740) : `Face âˆ’ (Face âˆ’ Height) Ã— d / s` tant que la distance `d` reste
//!   sous `s = (Max âˆ’ Min) Ã— 0,3`, `Height` au-delÃ .
//! - Molette (FUN_14085ea20 â†’ FUN_1406a9fe0) : la distance change de `delta Ã— 0,6` (120 par cran, soit
//!   72 unitÃ©s), bornÃ©e Ã  `[Min, Max]` du mode ; souris (FUN_14085e8e0) : 0,3Â° de lacet et 0,4Â° de
//!   tangage par pixel (FUN_1406a9960, FUN_1406a9830), tangage bornÃ© Ã  `[Min, Max]`.
//! - Touches tenues (FUN_14085ee60, temps Ã©coulÃ© en ms) : flÃ¨ches gauche / droite 0,3Â°/ms, haut / bas
//!   0,4Â°/ms, pavÃ© âˆ’ / + 0,6 unitÃ©/ms ; F10 â†’ mode 1, F11 â†’ mode 2, F12 â†’ mode 0 (FUN_1408d1d20).
//! - Changement de mode (FUN_1408d1d20) : refusÃ© pendant une transition ; le lacet est gardÃ©, la
//!   distance et le tangage repartent des valeurs `Default` du nouveau mode, atteintes par un
//!   glissement de la position de l'Å“il (FUN_1408fc920 de 90Â° Ã  270Â°, 0,5). ArchRust lisse ce
//!   glissement en cosinus sur `longueur / 0,5` ms (lecture de FUN_1408fc920 dÃ©duite).
//! - Collision (FUN_1408cf7f0, AgcdCamConstrain_Terrain / _Obj) : le bras raccourcit devant le sol et
//!   les objets du dÃ©cor ; il se rallonge ensuite en douceur.

use glam::{Mat4, Vec3};

/// RÃ©glages d'un mode de `CamSetting.xml` (distances en unitÃ©s, angles en degrÃ©s).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CamMode {
    pub distance: f32,
    pub distance_min: f32,
    pub distance_max: f32,
    pub pitch: f32,
    pub pitch_min: f32,
    pub pitch_max: f32,
    pub mask: i32,
}

/// Contenu de `ini/CamSetting.xml`.
#[derive(Debug, Clone, PartialEq)]
pub struct CamSetting {
    pub modes: [CamMode; 4],
    /// TID â†’ (`Height`, `Face`).
    pub specific: Vec<(i32, f32, f32)>,
}

/// Valeurs de la section `<Default>` du fichier d'Archonia, gardÃ©es si le fichier manque.
impl Default for CamSetting {
    fn default() -> Self {
        let m = |d, dmin, dmax, p, pmin, pmax, mask| CamMode { distance: d, distance_min: dmin, distance_max: dmax, pitch: p, pitch_min: pmin, pitch_max: pmax, mask };
        Self {
            modes: [
                m(600.0, 96.0, 2750.0, 15.0, -45.0, 55.0, 0),
                m(2750.0, 96.0, 2750.0, 15.0, -45.0, 89.0, 0),
                m(1571.0, 200.0, 1571.0, 45.0, 45.0, 65.0, 0),
                m(0.1, 0.1, 0.1, 0.0, -89.0, 89.0, 500),
            ],
            specific: Vec::new(),
        }
    }
}

/// Hauteurs visÃ©es sans rÃ©glage propre au TID (FUN_1408d1a60).
pub const DEFAULT_HEIGHT: f32 = 100.0;
pub const DEFAULT_FACE: f32 = 160.0;
/// UnitÃ©s de distance par unitÃ© de molette (`_DAT_140f44514`) ; un cran de Windows vaut 120.
pub const ZOOM_PER_UNIT: f32 = 0.6;
pub const WHEEL_NOTCH: f32 = 120.0;
/// DegrÃ©s par pixel de souris ou par ms de touche tenue (`_DAT_140f4450c`, `_DAT_140f44510`).
pub const YAW_PER_UNIT: f32 = 0.3;
pub const PITCH_PER_UNIT: f32 = 0.4;
/// Part de l'Ã©cart `Max âˆ’ Min` sous laquelle la visÃ©e descend vers `Face` (`DAT_140d3c930`).
pub const FACE_SPAN: f32 = 0.3;
/// Vitesse du glissement entre deux modes (unitÃ©s par ms, 5áµ‰ argument de FUN_1408fc920).
pub const MODE_SPEED: f32 = 0.5;

/// Valeur d'un attribut `nom = "valeur"` d'une balise XML.
fn attr(tag: &str, name: &str) -> Option<f32> {
    let mut rest = tag;
    while let Some(i) = rest.find(name) {
        let before_ok = i == 0 || !rest.as_bytes()[i - 1].is_ascii_alphanumeric();
        let after = rest[i + name.len()..].trim_start();
        if before_ok {
            if let Some(v) = after.strip_prefix('=') {
                let v = v.trim_start().strip_prefix('"')?;
                return v.split('"').next()?.trim().parse().ok();
            }
        }
        rest = &rest[i + name.len()..];
    }
    None
}

impl CamSetting {
    /// Lecture tolÃ©rante de `CamSetting.xml` : modes `<Mode>` de `<Default>`, puis `<Specific>`.
    /// Un mode absent garde sa valeur d'Archonia.
    pub fn parse(text: &str) -> CamSetting {
        let mut out = CamSetting::default();
        // Commentaires retirÃ©s (le fichier en contient entre les balises d'un mÃªme mode).
        let mut clean = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(i) = rest.find("<!--") {
            clean.push_str(&rest[..i]);
            rest = rest[i..].find("-->").map_or("", |j| &rest[i + j + 3..]);
        }
        clean.push_str(rest);
        let tags: Vec<&str> = clean.split('<').skip(1).map(|t| t.split('>').next().unwrap_or("")).collect();
        let (mut mode, mut specific): (Option<usize>, Option<(i32, f32, f32)>) = (None, None);
        for tag in tags {
            let name = tag.split_whitespace().next().unwrap_or("");
            match name {
                "Mode" => mode = attr(tag, "Index").map(|i| i as usize).filter(|&i| i < 4),
                "/Mode" | "/Default" => mode = None,
                "Distance" | "Pitch" if mode.is_some() => {
                    let m = &mut out.modes[mode.unwrap()];
                    let (d, lo, hi) = (attr(tag, "Default"), attr(tag, "Min"), attr(tag, "Max"));
                    if name == "Distance" {
                        m.distance = d.unwrap_or(m.distance);
                        m.distance_min = lo.unwrap_or(m.distance_min);
                        m.distance_max = hi.unwrap_or(m.distance_max);
                    } else {
                        m.pitch = d.unwrap_or(m.pitch);
                        m.pitch_min = lo.unwrap_or(m.pitch_min);
                        m.pitch_max = hi.unwrap_or(m.pitch_max);
                    }
                }
                "Specific" => specific = None,
                "Character" => {
                    if let (Some(tid), Some(h)) = (attr(tag, "TID"), attr(tag, "Height")) {
                        specific = Some((tid as i32, h, attr(tag, "Face").unwrap_or(0.0)));
                    }
                }
                "/Specific" => {
                    if let Some(s) = specific.take() {
                        out.specific.push(s);
                    }
                }
                _ => {}
            }
            if name == "Mode" {
                if let (Some(i), Some(mask)) = (mode, attr(tag, "MaskFlag")) {
                    out.modes[i].mask = mask as i32;
                }
            }
        }
        out
    }

    /// (`Height`, `Face`) du TID ; 100 et 160 sans rÃ©glage (ou rÃ©glage Ã  0), comme FUN_1408d1a60.
    pub fn heights(&self, tid: i32) -> (f32, f32) {
        match self.specific.iter().find(|s| s.0 == tid) {
            Some(&(_, h, f)) if h != 0.0 && f != 0.0 => (h, f),
            _ => (DEFAULT_HEIGHT, DEFAULT_FACE),
        }
    }
}

/// Glissement de l'Å“il d'un mode Ã  l'autre : distance et tangage de dÃ©part et d'arrivÃ©e.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transition {
    from: (f32, f32),
    to: (f32, f32),
    elapsed_ms: f32,
    duration_ms: f32,
}

#[derive(Debug, Clone)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
    /// Longueur du bras laissÃ©e libre par le terrain et les objets (voir [`OrbitCamera::collide`]) ;
    /// infinie quand rien ne l'a raccourci.
    pub collision: f32,
    /// Mode de `CamSetting.xml` (0 libre, 1 haut, 2 trois-quarts, 3 premiÃ¨re personne).
    pub mode: usize,
    /// Bornes du mode : distance (unitÃ©s) et tangage (radians).
    pub distance_min: f32,
    pub distance_max: f32,
    pub pitch_min: f32,
    pub pitch_max: f32,
    /// Hauteurs visÃ©es au-dessus des pieds, de loin et de prÃ¨s (`Height`, `Face`).
    pub look_height: f32,
    pub face_height: f32,
    /// Glissement en cours entre deux modes.
    pub transition: Option<Transition>,
}

impl Default for OrbitCamera {
    /// Mode 0 de `CamSetting.xml` d'Archonia : 600 unitÃ©s, 15Â°, bornes 96â€“2750 et âˆ’45..55Â°.
    fn default() -> Self {
        let m = CamSetting::default().modes[0];
        Self {
            target: Vec3::ZERO,
            yaw: 0.6,
            pitch: m.pitch.to_radians(),
            distance: m.distance,
            fov_y: 55f32.to_radians(),
            near: 10.0,
            far: 200_000.0,
            collision: f32::INFINITY,
            mode: 0,
            distance_min: m.distance_min,
            distance_max: m.distance_max,
            pitch_min: m.pitch_min.to_radians(),
            pitch_max: m.pitch_max.to_radians(),
            look_height: DEFAULT_HEIGHT,
            face_height: DEFAULT_FACE,
            transition: None,
        }
    }
}

impl OrbitCamera {
    /// Direction de la cible vers l'Å“il.
    fn direction(&self) -> Vec3 {
        Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        )
    }

    pub fn eye(&self) -> Vec3 {
        self.target + self.direction() * self.distance.min(self.collision)
    }

    /// Bornes et mode pris dans `setting` (sans glissement : entrÃ©e en jeu, rechargement).
    pub fn apply_mode(&mut self, setting: &CamSetting, mode: usize) {
        let m = setting.modes[mode.min(3)];
        self.mode = mode.min(3);
        self.distance_min = m.distance_min.min(m.distance_max);
        self.distance_max = m.distance_max.max(m.distance_min);
        self.pitch_min = m.pitch_min.min(m.pitch_max).to_radians();
        self.pitch_max = m.pitch_max.max(m.pitch_min).to_radians();
        self.transition = None;
        self.clamp();
    }

    /// Mode demandÃ© (F10 / F11 / F12, bouton de la minicarte) : refusÃ© pendant un glissement ou s'il
    /// est dÃ©jÃ  actif ; le lacet reste, distance et tangage glissent vers les `Default` du mode.
    pub fn set_mode(&mut self, setting: &CamSetting, mode: usize) -> bool {
        if mode > 3 || mode == self.mode || self.transition.is_some() {
            return false;
        }
        let from = (self.distance.min(self.collision), self.pitch);
        let m = setting.modes[mode];
        self.apply_mode(setting, mode);
        let to = (m.distance.clamp(self.distance_min, self.distance_max), m.pitch.to_radians().clamp(self.pitch_min, self.pitch_max));
        // Longueur parcourue par l'Å“il autour de la cible, comme le vecteur de FUN_1408d1d20.
        let eye = |(d, p): (f32, f32)| Vec3::new(p.cos() * d, p.sin() * d, 0.0);
        let len = eye(from).distance(eye(to));
        self.distance = from.0;
        self.pitch = from.1;
        self.transition = Some(Transition { from, to, elapsed_ms: 0.0, duration_ms: (len / MODE_SPEED).max(1.0) });
        true
    }

    /// Mode suivant du bouton camÃ©ra de la minicarte (`Minimap_CameraView_Click`, FUN_140701530) :
    /// 0 â†’ 1 â†’ 2 â†’ 0.
    pub fn next_mode(&self) -> usize {
        if self.mode + 1 < 3 { self.mode + 1 } else { 0 }
    }

    /// Avance le glissement entre deux modes ; `true` tant qu'il dure.
    pub fn step_transition(&mut self, dt_ms: f32) -> bool {
        let Some(t) = self.transition.as_mut() else { return false };
        t.elapsed_ms += dt_ms;
        let k = (t.elapsed_ms / t.duration_ms).clamp(0.0, 1.0);
        // De 90Â° Ã  270Â° : (1 âˆ’ sin Î¸) / 2 va de 0 Ã  1 sans Ã -coup.
        let theta = (90.0 + 180.0 * k).to_radians();
        let s = (1.0 - theta.sin()) * 0.5;
        let (from, to) = (t.from, t.to);
        self.distance = from.0 + (to.0 - from.0) * s;
        self.pitch = from.1 + (to.1 - from.1) * s;
        if k >= 1.0 {
            self.transition = None;
        }
        true
    }

    /// Hauteur visÃ©e au-dessus des pieds pour la distance `d` (FUN_1406aa740).
    pub fn look_offset(&self) -> f32 {
        let d = self.distance.min(self.collision);
        let span = (self.distance_max - self.distance_min) * FACE_SPAN;
        if span > 0.0 && d <= span {
            self.face_height - (self.face_height - self.look_height) * d / span
        } else {
            self.look_height
        }
    }

    /// Suit des pieds : la cible passe Ã  la hauteur visÃ©e.
    pub fn follow(&mut self, feet: Vec3) {
        self.target = feet + Vec3::Y * self.look_offset();
    }

    /// Distance et tangage ramenÃ©s dans les bornes du mode.
    pub fn clamp(&mut self) {
        if self.transition.is_none() {
            self.distance = self.distance.clamp(self.distance_min, self.distance_max);
            self.pitch = self.pitch.clamp(self.pitch_min, self.pitch_max);
        }
    }

    /// Raccourcit le bras quand le terrain ou un objet du dÃ©cor passe entre la cible et l'Å“il, puis
    /// le rallonge en douceur une fois la vue dÃ©gagÃ©e. `ground` rend la hauteur du sol en (x, z) ;
    /// `obstacle(de, vers)` la distance (depuis `de`) du premier objet touchÃ© par le segment.
    /// RentrÃ©e immÃ©diate, sortie exponentielle Ã  6 par seconde (logique reprise de benilla,
    /// `player/camera.rs`, `seat_camera`, MIT/Apache-2.0).
    pub fn collide(&mut self, dt: f32, ground: impl Fn(f32, f32) -> Option<f32>, obstacle: impl Fn(Vec3, Vec3) -> Option<f32>) {
        const STEPS: usize = 48;
        const MARGIN: f32 = 40.0;
        const RETURN_RATE: f32 = 6.0;
        let dir = self.direction();
        let blocked = |d: f32| {
            let p = self.target + dir * d;
            ground(p.x, p.z).is_some_and(|h| p.y < h + MARGIN)
        };
        let mut open = self.distance;
        let step = self.distance / STEPS as f32;
        if let Some(i) = (1..=STEPS).find(|i| blocked(step * *i as f32)) {
            // Dichotomie entre le dernier point libre et le premier point sous le terrain.
            let (mut lo, mut hi) = (step * (i - 1) as f32, step * i as f32);
            for _ in 0..8 {
                let mid = (lo + hi) / 2.0;
                if blocked(mid) {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            open = lo;
        }
        if let Some(hit) = obstacle(self.target, self.target + dir * open) {
            open = open.min(hit - MARGIN);
        }
        let open = open.max(self.near * 2.0);
        self.collision = if open < self.collision {
            open
        } else {
            let t = 1.0 - (-RETURN_RATE * dt).exp();
            self.collision + (open - self.collision) * t
        };
        if self.collision >= self.distance - 0.5 {
            self.collision = f32::INFINITY;
        }
    }

    pub fn view(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye(), self.target, Vec3::Y)
    }

    pub fn projection(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov_y, aspect.max(0.01), self.near, self.far)
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        self.projection(aspect) * self.view()
    }

    /// Matrice du rendu GPU, en profondeur inversÃ©e (prÃ¨s â†’ 1, loin â†’ 0) : avec un tampon
    /// flottant, la prÃ©cision reste fine jusqu'au loin. Avec la profondeur classique et `near` Ã 
    /// 10, deux surfaces d'une mÃªme piÃ¨ce distantes de quelques centiÃ¨mes d'unitÃ© (haut du
    /// pantalon texturÃ© en noir sous l'ourlet de la chemise, `b0000060.ecl`) n'Ã©taient plus
    /// dÃ©partagÃ©es Ã  quelques centaines d'unitÃ©s : le noir clignotait. Le code CPU (clics,
    /// projections Ã  l'Ã©cran, AddOns) garde [`OrbitCamera::view_proj`].
    pub fn render_view_proj(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov_y, aspect.max(0.01), self.far, self.near) * self.view()
    }

    /// Lacet et tangage d'Archonia : `dx`, `dy` en pixels de souris (ou ms de touche tenue).
    pub fn rotate(&mut self, dx: f32, dy: f32) {
        if self.transition.is_some() {
            return;
        }
        self.yaw += (dx * YAW_PER_UNIT).to_radians();
        self.pitch = (self.pitch + (dy * PITCH_PER_UNIT).to_radians()).clamp(self.pitch_min, self.pitch_max);
    }

    /// Molette : `delta` en crans (positif = vers l'avant, la camÃ©ra s'approche).
    pub fn zoom(&mut self, delta: f32) {
        self.zoom_units(delta * WHEEL_NOTCH * ZOOM_PER_UNIT);
    }

    /// Rapproche (`units` > 0) ou Ã©loigne la camÃ©ra, bornÃ©e au mode.
    pub fn zoom_units(&mut self, units: f32) {
        if self.transition.is_some() {
            return;
        }
        // Un zoom avant part de la distance rÃ©ellement vue, pas de celle demandÃ©e.
        if units > 0.0 && self.collision < self.distance {
            self.distance = self.collision;
        }
        self.distance = (self.distance - units).clamp(self.distance_min, self.distance_max);
    }

    /// DÃ©place la cible dans le plan horizontal, relativement Ã  l'orientation de la camÃ©ra.
    pub fn pan(&mut self, forward: f32, right: f32) {
        let f = Vec3::new(-self.yaw.cos(), 0.0, -self.yaw.sin());
        let r = Vec3::new(-f.z, 0.0, f.x);
        self.target += f * forward + r * right;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_shortens_the_boom_then_releases_it() {
        let mut cam = OrbitCamera {
            target: Vec3::new(0.0, 100.0, 0.0),
            yaw: 0.0,
            pitch: 0.2,
            distance: 1500.0,
            ..OrbitCamera::default()
        };
        // Colline Ã  partir de x = 600 : l'Å“il (x â‰ˆ 1470) serait dessous.
        cam.collide(0.016, |x, _| Some(if x > 600.0 { 1000.0 } else { 0.0 }), |_, _| None);
        let d = cam.collision;
        assert!(d < 700.0 && d > 500.0, "bras raccourci : {d}");
        // Terrain plat : le bras revient progressivement, puis entiÃ¨rement.
        cam.collide(0.016, |_, _| Some(0.0), |_, _| None);
        assert!(cam.collision > d && cam.collision < 1500.0);
        for _ in 0..300 {
            cam.collide(0.016, |_, _| Some(0.0), |_, _| None);
        }
        assert!(cam.collision.is_infinite());
        assert!((cam.eye() - cam.target).length() > 1499.0);
    }

    #[test]
    fn objects_shorten_the_boom_too() {
        let mut cam = OrbitCamera { target: Vec3::new(0.0, 100.0, 0.0), yaw: 0.0, pitch: 0.2, distance: 1500.0, ..OrbitCamera::default() };
        // Mur Ã  800 unitÃ©s de la cible.
        cam.collide(0.016, |_, _| Some(0.0), |_, _| Some(800.0));
        assert!((cam.collision - 760.0).abs() < 1.0, "{}", cam.collision);
    }

    #[test]
    fn render_depth_is_reversed() {
        let cam = OrbitCamera::default();
        let dir = (cam.target - cam.eye()).normalize();
        let depth = |m: Mat4, d: f32| m.project_point3(cam.eye() + dir * d).z;
        let (gpu, cpu) = (cam.render_view_proj(1.5), cam.view_proj(1.5));
        assert!((depth(gpu, cam.near) - 1.0).abs() < 1e-4);
        assert!(depth(gpu, cam.far).abs() < 1e-4);
        // Deux surfaces Ã  0,05 unitÃ© l'une de l'autre, Ã  1600 : distinctes en profondeur inversÃ©e.
        assert!(depth(gpu, 1600.0) > depth(gpu, 1600.05));
        // Le code CPU garde la convention classique (prÃ¨s â†’ 0).
        assert!(depth(cpu, cam.near).abs() < 1e-4);
    }

    const FILE: &str = r#"<?xml version="1.0" encoding="euc-kr" ?>
<CamSetting>
	<Default>
		<Mode Index = "0" MaskFlag = "0"><!-- F12 -->
			<Distance	Default = "600.0"	Min = "96.0"	Max = "2750.0"	/>
			<Pitch		Default = "15.0"	Min = "-45.0"	Max = "55.0"	/>
		</Mode>
		<Mode Index = "2" MaskFlag = "0">
			<Distance	Default = "1571.0"	Min = "200.0"	Max = "1571.0"	/>
			<Pitch		Default = "45.0"	Min = "45.0"	Max = "65.0"	/>
		</Mode>
		<Mode Index = "3" MaskFlag = "500">
			<Distance	Default = "0.1"		Min = "0.1"		Max = "0.1"		/>
			<Pitch		Default = "0.0"		Min = "-89.0"	Max = "89.0"	/>
		</Mode>
	</Default>
	<Specific>
		<Character	TID = "96"	Height = "150.0"	Face = "145.0"		/>
		<Distance	Min = "96"	Max = "1100.0"	/>
		<Pitch		Min = "-45.0"	Max = "75.0"	/>
	</Specific>
</CamSetting>"#;

    #[test]
    fn camsetting_modes_and_specific() {
        let s = CamSetting::parse(FILE);
        assert_eq!(s.modes[0], CamMode { distance: 600.0, distance_min: 96.0, distance_max: 2750.0, pitch: 15.0, pitch_min: -45.0, pitch_max: 55.0, mask: 0 });
        assert_eq!((s.modes[2].distance_min, s.modes[2].pitch_max), (200.0, 65.0));
        assert_eq!(s.modes[3].mask, 500);
        // Mode 1 absent du texte : valeurs d'Archonia.
        assert_eq!(s.modes[1], CamSetting::default().modes[1]);
        assert_eq!(s.heights(96), (150.0, 145.0));
        assert_eq!(s.heights(5), (DEFAULT_HEIGHT, DEFAULT_FACE));
    }

    #[test]
    fn bounds_wheel_and_mouse_follow_archonia() {
        let s = CamSetting::default();
        let mut cam = OrbitCamera::default();
        cam.apply_mode(&s, 0);
        // Un cran : 72 unitÃ©s ; bornÃ© Ã  96 et 2750.
        cam.zoom(1.0);
        assert!((cam.distance - 528.0).abs() < 1e-3);
        cam.zoom(-100.0);
        assert_eq!(cam.distance, 2750.0);
        cam.zoom(100.0);
        assert_eq!(cam.distance, 96.0);
        // 0,4Â° de tangage par pixel, bornÃ© Ã  55Â°.
        cam.rotate(0.0, 1000.0);
        assert!((cam.pitch.to_degrees() - 55.0).abs() < 1e-3);
        let yaw = cam.yaw;
        cam.rotate(10.0, 0.0);
        assert!(((cam.yaw - yaw).to_degrees() - 3.0).abs() < 1e-3);
    }

    #[test]
    fn modes_glide_to_their_defaults() {
        let s = CamSetting::default();
        let mut cam = OrbitCamera::default();
        cam.apply_mode(&s, 0);
        let yaw = cam.yaw;
        assert!(cam.set_mode(&s, 2));
        // Pendant le glissement, un autre mode est refusÃ©.
        assert!(!cam.set_mode(&s, 1));
        for _ in 0..1000 {
            cam.step_transition(16.0);
        }
        assert!(!cam.step_transition(16.0));
        assert!((cam.distance - 1571.0).abs() < 1e-2 && (cam.pitch.to_degrees() - 45.0).abs() < 1e-3);
        assert_eq!(cam.yaw, yaw);
        assert_eq!((cam.distance_min, cam.distance_max), (200.0, 1571.0));
        assert_eq!(cam.next_mode(), 0);
    }

    #[test]
    fn look_height_moves_to_the_face_up_close() {
        let mut cam = OrbitCamera { look_height: 150.0, face_height: 130.0, ..OrbitCamera::default() };
        cam.apply_mode(&CamSetting::default(), 0);
        cam.distance = 2000.0;
        assert_eq!(cam.look_offset(), 150.0);
        cam.distance = 0.0;
        assert_eq!(cam.look_offset(), 130.0);
        cam.distance = (2750.0 - 96.0) * 0.3 / 2.0;
        assert!((cam.look_offset() - 140.0).abs() < 1e-3);
    }
}
