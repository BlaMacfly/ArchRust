//! Modèles de personnages dont plusieurs fichiers (DFF, DEFAULT_ARMOUR_DFF) portent de la géométrie,
//! et pièces qui se recouvrent (même partie, centres proches) — diagnostic privé.
//! Usage : `cargo run --release -p archrust-engine --example model_overlap -- <Game> [TID…]`
use archrust_engine::assets::Assets;
use archrust_engine::scene::{atomic_part, models_for_tid};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let assets = Assets::new(Some(PathBuf::from(&args[0])), &[]);
    let tids: Vec<i32> = if args.len() > 1 {
        args[1..].iter().filter_map(|t| t.parse().ok()).collect()
    } else {
        let t = assets.table("ini", "charactertemplateclient.ini")?;
        t.sections.iter().filter_map(|s| s.name.parse().ok()).collect()
    };
    println!("{} modèle(s) examinés", tids.len());
    let mut doubles = 0;
    for tid in tids {
        let files = models_for_tid(&assets, tid);
        let mut pieces = Vec::new();
        for f in &files {
            let Ok(c) = assets.clump("character", f) else { continue };
            for a in &c.atomics {
                let g = &c.geometries[a.geometry as usize];
                if g.vertices.len() > 3 {
                    pieces.push((f.clone(), atomic_part(&c, a.geometry as usize), g.bound_center, g.bound_radius, g.vertices.len()));
                }
            }
        }
        let with_geom: std::collections::BTreeSet<&String> = pieces.iter().map(|p| &p.0).collect();
        // Recouvrements : deux pièces de fichiers différents, centres à moins de la moitié du plus petit rayon.
        let mut overlaps = Vec::new();
        for i in 0..pieces.len() {
            for j in i + 1..pieces.len() {
                let (a, b) = (&pieces[i], &pieces[j]);
                let d = a.2.distance(b.2);
                if a.0 != b.0 && d < a.3.min(b.3) * 0.5 {
                    overlaps.push(format!("{}:{:?} ~ {}:{:?} (écart {d:.0})", a.0, a.1, b.0, b.1));
                }
            }
        }
        if with_geom.len() > 1 && !overlaps.is_empty() {
            doubles += 1;
            if doubles <= 40 {
                println!("TID {tid} {files:?} : {} recouvrement(s) : {}", overlaps.len(), overlaps.iter().take(3).cloned().collect::<Vec<_>>().join(" ; "));
            }
        }
    }
    println!("{doubles} modèle(s) avec des pièces en double");
    Ok(())
}
