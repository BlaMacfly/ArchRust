//! Capture d'écran d'Archonia : touche Impr. écran (« PrintScreen », FUN_140667760) → FUN_1404a61b0.
//!
//! Archonia copie le tampon arrière dans `screen\SC%s.bmp`, `%s` = heure locale `%y%m%d-%H%M%S`
//! (`strftime`), relatif au dossier du jeu ; si le fichier ne peut pas être créé, il crée le dossier
//! `screen` et réessaie. L'image est écrite telle quelle (4 octets par pixel pour un tampon
//! X8R8G8B8). ArchRust écrit le même nom dans `<Game>/screen/` (sans dossier de jeu : à côté des
//! SavedVariables), en BMP 32 bits par la capture du rendu (`Renderer::capture_next`).
//!
//! **Réglages d'affichage d'`Option_Video` / `Option_VideoRes`** (FUN_1407049d0, FUN_140707970,
//! FUN_14070f5d0), appliqués à la fenêtre à chaque changement :
//! - `OP_WindowMode` 0 plein écran (mode vidéo du moniteur à la résolution choisie, sinon sans bordure),
//!   1 fenêtré (taille = résolution choisie), 2 sans bordure ; `OP_Resolution` : modes du moniteur
//!   (largeur × hauteur distincts, 12 au plus comme `AgcmUIOption+0x38c8`, les plus grands gardés) ;
//! - `OP_HighOptimization` (« V-Sync ») : présentation `Fifo`, sinon sans attente ;
//! - gamma `BRIGHT` : rampe de [`crate::options::gamma_ramp`]. Archonia la passe à
//!   `IDirect3DDevice9::SetGammaRamp`, sans effet en fenêtré ; ArchRust l'applique de même en plein écran
//!   seulement (`SetDeviceGammaRamp`), et remet la rampe neutre en quittant ce mode ou le jeu ;
//! - `OP_AntiAliasing` : gardé dans les options et sans effet, comme dans Archonia (`FUN_14070ed10` et
//!   `FUN_140711e10` passent toujours 1 à `RwD3D9EngineSetMultiSamplingLevels`, voir `docs/FORMATS.md`).

use super::App;
use std::path::PathBuf;

#[derive(Default)]
pub struct CaptureGlue {
    /// Dernier fichier demandé (pour le journal).
    pub last: Option<PathBuf>,
    /// Réglages d'affichage appliqués : mode, résolution, V-Sync, gamma.
    applied: Option<(u8, Option<(u32, u32)>, bool, u8)>,
    /// Rampe gamma modifiée (à remettre à neuf).
    gamma_set: bool,
}

impl Drop for CaptureGlue {
    fn drop(&mut self) {
        if self.gamma_set {
            set_gamma(50);
        }
    }
}

/// Rampe gamma de l'écran (plein écran seulement), neutre à 50.
fn set_gamma(bright: u8) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Graphics::Gdi::{GetDC, ReleaseDC};
        use windows_sys::Win32::UI::ColorSystem::SetDeviceGammaRamp;
        let one = crate::options::gamma_ramp(bright);
        let mut ramp = [0u16; 768];
        for k in 0..3 {
            ramp[k * 256..(k + 1) * 256].copy_from_slice(&one);
        }
        // SAFETY : DC de l'écran obtenu puis rendu ; la rampe (3 × 256 mots) vit pendant l'appel.
        unsafe {
            let dc = GetDC(std::ptr::null_mut());
            if !dc.is_null() {
                SetDeviceGammaRamp(dc, ramp.as_ptr() as *const core::ffi::c_void);
                ReleaseDC(std::ptr::null_mut(), dc);
            }
        }
    }
    #[cfg(not(windows))]
    let _ = bright;
}

/// Résolutions proposées : tailles distinctes des modes vidéo, croissantes, les 12 plus grandes.
pub fn resolution_list(mut sizes: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    sizes.sort_unstable_by_key(|&(w, h)| (w as u64 * h as u64, w));
    sizes.dedup();
    let skip = sizes.len().saturating_sub(12);
    sizes.split_off(skip)
}

/// Heure locale (année, mois, jour, heure, minute, seconde).
fn local_time() -> (u32, u32, u32, u32, u32, u32) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::SYSTEMTIME;
        use windows_sys::Win32::System::SystemInformation::GetLocalTime;
        let mut t = SYSTEMTIME { wYear: 0, wMonth: 0, wDayOfWeek: 0, wDay: 0, wHour: 0, wMinute: 0, wSecond: 0, wMilliseconds: 0 };
        // SAFETY : `GetLocalTime` remplit la structure fournie, valide et correctement alignée.
        unsafe { GetLocalTime(&mut t) };
        (t.wYear as u32, t.wMonth as u32, t.wDay as u32, t.wHour as u32, t.wMinute as u32, t.wSecond as u32)
    }
    #[cfg(not(windows))]
    {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let days = secs / 86_400;
        let (h, m, s) = ((secs / 3600 % 24) as u32, (secs / 60 % 60) as u32, (secs % 60) as u32);
        // Jours → date civile (algorithme de Howard Hinnant).
        let z = days as i64 + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let mo = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let y = (yoe + era * 400 + (mo <= 2) as i64) as u32;
        (y, mo, d, h, m, s)
    }
}

/// Nom du fichier d'Archonia : `SC` + `%y%m%d-%H%M%S` + `.bmp`.
pub fn file_name((y, mo, d, h, mi, s): (u32, u32, u32, u32, u32, u32)) -> String {
    format!("SC{:02}{mo:02}{d:02}-{h:02}{mi:02}{s:02}.bmp", y % 100)
}

impl App {
    /// Impr. écran : capture de la prochaine image vers `screen\SC….bmp`.
    pub(super) fn capture_request(&mut self) {
        let dir = self.config.game_dir.clone().unwrap_or_else(|| self.config.saved_dir.clone()).join("screen");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            log::warn!("capture : {} ({e})", dir.display());
            return;
        }
        let path = dir.join(file_name(local_time()));
        log::info!("capture demandée : {}", path.display());
        self.capture.last = Some(path.clone());
        // Même chemin que `Screenshot()` des AddOns : écrite après le rendu de l'image, le client continue.
        self.addon_glue.ui.screenshot = Some(path);
        if let Some(g) = &mut self.gpu {
            g.renderer.capture_next = true;
        }
    }
}

impl App {
    /// Applique mode de fenêtre, résolution, V-Sync et gamma quand ils changent (et au démarrage).
    pub(super) fn display_frame(&mut self) {
        use winit::window::Fullscreen;
        let Some(gpu) = self.gpu.as_mut() else { return };
        if self.ui.resolutions.is_empty() {
            if let Some(m) = gpu.window.current_monitor() {
                self.ui.resolutions = resolution_list(m.video_modes().map(|v| (v.size().width, v.size().height)).collect());
            }
        }
        let v = &self.options.video;
        let want = (v.window_mode.min(2), v.resolution, v.vsync, v.bright.min(100));
        let before = self.capture.applied.replace(want);
        if before == Some(want) {
            return;
        }
        let window = gpu.window.clone();
        if before.is_none_or(|b| (b.0, b.1) != (want.0, want.1)) {
            match want.0 {
                0 => {
                    let monitor = window.current_monitor();
                    let mode = monitor.as_ref().and_then(|m| {
                        let (w, h) = want.1?;
                        m.video_modes().filter(|x| x.size().width == w && x.size().height == h).max_by_key(|x| (x.bit_depth(), x.refresh_rate_millihertz()))
                    });
                    window.set_fullscreen(Some(match mode {
                        Some(m) => Fullscreen::Exclusive(m),
                        None => Fullscreen::Borderless(monitor),
                    }));
                }
                2 => window.set_fullscreen(Some(Fullscreen::Borderless(None))),
                _ => {
                    // Au démarrage en fenêtré, la fenêtre garde la taille donnée par le moteur.
                    if before.is_some() {
                        window.set_fullscreen(None);
                        if let Some((w, h)) = want.1 {
                            let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(w, h));
                        }
                    }
                }
            }
        }
        let present = if want.2 { wgpu::PresentMode::Fifo } else { wgpu::PresentMode::AutoNoVsync };
        if gpu.renderer.config.present_mode != present {
            gpu.renderer.config.present_mode = present;
            let (w, h) = (gpu.renderer.config.width, gpu.renderer.config.height);
            gpu.renderer.resize(w, h);
        }
        if want.0 == 0 {
            set_gamma(want.3);
            self.capture.gamma_set = true;
        } else if self.capture.gamma_set {
            set_gamma(50);
            self.capture.gamma_set = false;
        }
        log::info!("affichage : mode {} {:?}, V-Sync {}, gamma {}", WINDOW_MODE_NAMES[want.0 as usize], want.1, want.2, want.3);
    }
}

/// Noms des modes pour le journal (FUN_140703da0).
const WINDOW_MODE_NAMES: [&str; 3] = ["plein écran", "fenêtré", "sans bordure"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archonia_screenshot_name() {
        assert_eq!(file_name((2026, 11, 2, 9, 5, 7)), "SC261102-090507.bmp");
        let (y, mo, d, ..) = local_time();
        assert!(y >= 2024 && (1..=12).contains(&mo) && (1..=31).contains(&d));
    }

    #[test]
    fn gamma_ramp_and_minimap_zoom() {
        // 50 : courbe neutre ; 100 : éclaircie (exposant 1/1,5).
        let n = crate::options::gamma_ramp(50);
        assert_eq!((n[0], n[128], n[255]), (0, (128.0 / 255.0 * 65536.0) as u16, 65535));
        assert!(crate::options::gamma_ramp(100)[64] > n[64] && crate::options::gamma_ramp(0)[64] < n[64]);
        // Zoom de la minicarte : 1 → 0,8 → 0,6 → 0,4 (plancher) ; 1,4 → 1,6 (plafond).
        let z = crate::hud::minimap_zoom;
        assert!((z(1.0, true) - 0.8).abs() < 1e-6 && (z(0.4, true) - 0.4).abs() < 1e-6);
        assert!((z(1.4, false) - 1.6).abs() < 1e-6 && (z(1.6, false) - 1.6).abs() < 1e-6);
    }

    #[test]
    fn resolutions_distinct_twelve_largest() {
        let mut modes: Vec<(u32, u32)> = (0..20).map(|i| (640 + i * 64, 480 + i * 48)).collect();
        modes.extend(modes.clone());
        let l = resolution_list(modes);
        assert_eq!(l.len(), 12);
        assert_eq!(l.first(), Some(&(640 + 8 * 64, 480 + 8 * 48)));
        assert_eq!(l.last(), Some(&(640 + 19 * 64, 480 + 19 * 48)));
    }
}
