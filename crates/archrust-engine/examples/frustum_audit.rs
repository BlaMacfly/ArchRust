//! Part des objets du décor qui restent après le tri du champ de la caméra (`render_frustum.rs`), avec les
//! placements réels (`ini/obj%05d.ini`) autour d'un point, la caméra par défaut tournée dans 8 directions
//! et la distance d'affichage des objets par défaut (diagnostic privé, CPU seulement).
//! Chaque placement compte pour une sphère de `rayon` unités (par défaut 500) : les vraies pièces, plus
//! petites, sont triées au moins aussi souvent.
//! Usage : `cargo run -p archrust-engine --example frustum_audit -- <Game> [x z [rayon]]` (défaut : Tullan).

use archrust_engine::assets::Assets;
use archrust_engine::camera::OrbitCamera;
use archrust_engine::objects::tile_placements;
use archrust_engine::options::VideoOptions;
use archrust_engine::render::Frustum;
use archrust_engine::world::{sector_of, tile_of};
use glam::Vec3;
use std::collections::HashSet;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let assets = Assets::new(Some(PathBuf::from(&args[0])), &[]);
    let num = |i: usize, d: f32| args.get(i).and_then(|v| v.parse::<f32>().ok()).unwrap_or(d);
    let (x, z, radius) = (num(1, -356_500.0), num(2, 90_000.0), num(3, 500.0));
    let distance = VideoOptions::default().object_distance();
    let (sx, sy) = sector_of(x, z);
    let reach = (distance / 6400.0).ceil() as i32 + 1;
    let mut tiles = HashSet::new();
    for dx in -reach..=reach {
        for dy in -reach..=reach {
            tiles.insert(tile_of(sx + dx, sy + dy));
        }
    }
    let placements: Vec<Vec3> = tiles.iter().flat_map(|t| tile_placements(&assets, *t)).map(|p| p.position).collect();
    println!("point ({x}, {z}), {} tuile(s), {} placement(s), distance des objets {distance}, rayon {radius}", tiles.len(), placements.len());
    let (mut total_near, mut total_kept) = (0usize, 0usize);
    for k in 0..8 {
        let mut camera = OrbitCamera::default();
        camera.target = Vec3::new(x, placements.iter().map(|p| p.y).fold(0.0, f32::max).min(10_000.0), z);
        camera.yaw = k as f32 * std::f32::consts::FRAC_PI_4;
        let eye = camera.eye();
        let frustum = Frustum::new(camera.render_view_proj(16.0 / 9.0));
        let near: Vec<&Vec3> = placements.iter().filter(|p| p.distance(eye) - radius <= distance).collect();
        let kept = near.iter().filter(|p| frustum.sphere(***p, radius)).count();
        println!("lacet {:>3}° : {} à portée, {} dans le champ ({:.0} %)", k * 45, near.len(), kept, 100.0 * kept as f32 / near.len().max(1) as f32);
        total_near += near.len();
        total_kept += kept;
    }
    println!("moyenne : {:.0} % des objets à portée restent à dessiner", 100.0 * total_kept as f32 / total_near.max(1) as f32);
    Ok(())
}
