//! Rejoue les animations d'un modèle comme le moteur (`animation::deform`) et cherche ce qui peut
//! donner des taches noires : normales nulles ou NaN (le shader fait `normalize`, NaN → noir),
//! positions NaN, sommets éjectés loin de leur position de bind, poids dont la somme n'est pas 1.
//!
//! Usage : `cargo run --release -p archrust-engine --example anim_audit -- <Game> <tid> [tid…]`

use archrust_engine::animation::{deform, load_clip, AnimationSet, SkinBinding};
use archrust_engine::assets::Assets;
use archrust_engine::scene::{geometry_vertices, models_for_tid};
use glam::Vec3;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let assets = Assets::new(Some(PathBuf::from(&args[0])), &[]);
    for tid in args[1..].iter().filter_map(|t| t.parse::<i32>().ok()) {
        let set = AnimationSet::for_tid(&assets, tid, 0);
        let models = models_for_tid(&assets, tid);
        println!("== TID {tid} : modèles {models:?}, {} type(s) d'animation", set.by_type.len());
        for model in &models {
            let Ok(clump) = assets.clump("character", model) else {
                println!("   {model} : illisible");
                continue;
            };
            for (gi, geom) in clump.geometries.iter().enumerate() {
                let Some(skin) = &geom.skin else { continue };
                let base = geometry_vertices(geom);
                let binding = SkinBinding { clump: clump.clone(), geometry: gi, base: base.clone() };
                let radius = geom.bound_radius.max(1.0);
                let bad_weights = skin.weights.iter().filter(|w| (w.iter().sum::<f32>() - 1.0).abs() > 0.01).count();
                for (kind, variants) in &set.by_type {
                    for v in variants {
                        let Some(clip) = load_clip(&assets, &v.file) else { continue };
                        let (mut zero_n, mut nan_p, mut far, mut frames_bad) = (0usize, 0usize, 0usize, 0usize);
                        let mut max_jump = 0.0f32;
                        let steps = 60;
                        for s in 0..steps {
                            let pose = clip.sample(clip.duration * s as f32 / steps as f32);
                            let Some(out) = deform(&binding, &pose) else { continue };
                            let mut bad = false;
                            for (i, vtx) in out.iter().enumerate() {
                                let n = Vec3::from_array(vtx.normal);
                                let p = Vec3::from_array(vtx.position);
                                if !n.is_finite() || n.length_squared() < 1e-6 {
                                    zero_n += 1;
                                    bad = true;
                                }
                                if !p.is_finite() {
                                    nan_p += 1;
                                    bad = true;
                                }
                                let d = (p - Vec3::from_array(base[i].position)).length();
                                max_jump = max_jump.max(d);
                                if d > 4.0 * radius {
                                    far += 1;
                                    bad = true;
                                }
                            }
                            frames_bad += bad as usize;
                        }
                        println!(
                            "   {model} géom {gi} ({} sommets, {bad_weights} poids ≠ 1) clip {} type {kind} : normales nulles/NaN {zero_n}, positions NaN {nan_p}, sommets éjectés {far}, images touchées {frames_bad}/{steps}, déplacement max {max_jump:.0} (rayon {radius:.0})",
                            base.len(),
                            v.file
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
