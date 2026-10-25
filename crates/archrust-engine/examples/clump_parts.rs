//! Géométries d'un modèle : sommets, os, textures, centre et rayon (diagnostic privé).
//! Usage : `cargo run --release -p archrust-engine --example clump_parts -- <Game> a0000060.ecl …`
use archrust_engine::assets::Assets;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let assets = Assets::new(Some(PathBuf::from(&args[0])), &[]);
    for name in &args[1..] {
        let clump = assets.clump("character", name)?;
        println!("== {name} : {} atomic(s), {} géométrie(s)", clump.atomics.len(), clump.geometries.len());
        for a in &clump.atomics {
            let g = &clump.geometries[a.geometry as usize];
            let tex: Vec<String> = g.materials.iter().map(|m| m.texture.as_ref().map(|t| t.name.clone()).unwrap_or_default()).collect();
            println!("   atomic frame {} géom {} : {} sommets, peau {}, textures {:?}, centre {:?}, rayon {:.0}, drapeaux {:#x}", a.frame, a.geometry, g.vertices.len(), g.skin.is_some(), tex, g.bound_center, g.bound_radius, g.flags);
            for (mi, m) in g.materials.iter().enumerate() {
                let tris = g.triangles.iter().filter(|t| t.material as usize == mi).count();
                let bin: usize = g.meshes.iter().filter(|b| b.material as usize == mi).map(|b| b.indices.len()).sum();
                println!("      matériau {mi} : couleur {:?}, ambiante {}, diffuse {}, couches {:?}, triangles {tris}, indices binmesh {bin}", m.color, m.ambient, m.diffuse, m.layers);
            }
            // Bandes déroulées comme le moteur (scene.rs), comparées à la liste de triangles.
            for (mi, _) in g.materials.iter().enumerate() {
                let mut listed: std::collections::HashSet<[u32; 3]> = g.triangles.iter().filter(|t| t.material as usize == mi).map(|t| { let mut k = [t.v[0] as u32, t.v[1] as u32, t.v[2] as u32]; k.sort(); k }).collect();
                let total_listed = listed.len();
                let (mut strip_tris, mut extra) = (0usize, 0usize);
                for b in g.meshes.iter().filter(|b| b.material as usize == mi) {
                    for tri in b.indices.windows(3) {
                        if tri[0] == tri[1] || tri[1] == tri[2] || tri[0] == tri[2] { continue; }
                        strip_tris += 1;
                        let mut k = [tri[0], tri[1], tri[2]];
                        k.sort();
                        if !listed.remove(&k) { extra += 1; }
                    }
                }
                println!("      bandes matériau {mi} : {strip_tris} triangles déroulés, {extra} absents de la liste, {} de la liste jamais produits (liste : {total_listed})", listed.len());
            }
            // Texels sous chaque matériau : part de noir et de transparent aux UV des sommets.
            if let Some(tex) = g.materials.first().and_then(|m| m.texture.as_ref()).and_then(|t| assets.texture(&t.name).ok()) {
                for (mi, _) in g.materials.iter().enumerate() {
                    let (mut n, mut black, mut clear) = (0usize, 0usize, 0usize);
                    for t in g.triangles.iter().filter(|t| t.material as usize == mi) {
                        for &v in &t.v {
                            let Some(uv) = g.uv_sets.first().and_then(|s| s.get(v as usize)) else { continue };
                            let x = ((uv[0].rem_euclid(1.0)) * tex.width as f32) as u32 % tex.width.max(1);
                            let y = ((uv[1].rem_euclid(1.0)) * tex.height as f32) as u32 % tex.height.max(1);
                            let p = &tex.data[((y * tex.width + x) * 4) as usize..][..4];
                            n += 1;
                            if p[0] < 20 && p[1] < 20 && p[2] < 20 { black += 1; }
                            if p[3] < 77 { clear += 1; }
                        }
                    }
                    println!("      texels matériau {mi} : {n} sommets, noirs {black}, alpha < 0,3 : {clear}");
                }
            }
            let black = g.colors.iter().filter(|c| c[0] < 16 && c[1] < 16 && c[2] < 16).count();
            println!("      couleurs de sommet : {} dont quasi noires {black}", g.colors.len());
        }
    }
    Ok(())
}
