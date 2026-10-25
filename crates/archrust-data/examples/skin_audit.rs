//! Audit des peaux (Skin PLG) des modèles du pack `character` : sommes de poids, os hors palette,
//! normales nulles, matrices de liaison dégénérées (usage privé, diagnostic du rendu).
//!
//! Usage : `cargo run --release -p archrust-data --example skin_audit -- <dossier Game> [limite]`

use archrust_data::{pack::PackIndex, rw::Clump};
use std::{
    fs::{self, File},
    path::PathBuf,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let game = PathBuf::from(args.next().expect("dossier Game"));
    let limit: usize = args.next().and_then(|s| s.to_string_lossy().parse().ok()).unwrap_or(usize::MAX);
    let dir = game.join("character");
    let index = PackIndex::parse_encrypted(&fs::read(dir.join("Reference.Dat"))?)?;
    let mut data = File::open(dir.join("Data.Dat"))?;
    let (mut models, mut skinned, mut verts) = (0usize, 0usize, 0usize);
    let (mut sum_low, mut sum_high, mut sum_zero, mut out_of_range, mut zero_normals, mut no_normals, mut bad_bind) = (0usize, 0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    let mut worst: Vec<(String, usize, usize, usize)> = Vec::new();
    let mut flags: std::collections::BTreeMap<u32, usize> = Default::default();
    let (mut multi, mut same_frame) = (0usize, 0usize);
    let mut multi_examples: Vec<String> = Vec::new();
    for entry in index.entries.iter().filter(|e| e.name.ends_with(".ecl") || e.name.ends_with(".dff")).take(limit) {
        let Ok(bytes) = index.read_entry(&mut data, entry) else { continue };
        let Ok(clump) = Clump::parse(&bytes) else { continue };
        models += 1;
        for a in &clump.atomics {
            *flags.entry(a.flags).or_default() += 1;
        }
        if clump.atomics.len() > 1 {
            multi += 1;
            let mut fr: Vec<u32> = clump.atomics.iter().map(|a| a.frame).collect();
            fr.sort();
            fr.dedup();
            if fr.len() < clump.atomics.len() {
                same_frame += 1;
                if multi_examples.len() < 8 {
                    multi_examples.push(format!("{} ({} atomics, drapeaux {:?})", entry.name, clump.atomics.len(), clump.atomics.iter().map(|a| a.flags).collect::<Vec<_>>()));
                }
            }
        }
        for g in &clump.geometries {
            let Some(skin) = &g.skin else { continue };
            skinned += 1;
            let (mut bad_sum, mut bad_bone, mut zn) = (0usize, 0usize, 0usize);
            if g.normals.is_empty() {
                no_normals += 1;
            }
            for (i, w) in skin.weights.iter().enumerate() {
                verts += 1;
                let s: f32 = w.iter().sum();
                if s.abs() < 1e-6 {
                    sum_zero += 1;
                    bad_sum += 1;
                } else if s < 0.99 {
                    sum_low += 1;
                    bad_sum += 1;
                } else if s > 1.01 {
                    sum_high += 1;
                    bad_sum += 1;
                }
                for lane in 0..4 {
                    if w[lane] > 0.0 && skin.indices[i][lane] as usize >= skin.inverse_bind.len() {
                        out_of_range += 1;
                        bad_bone += 1;
                    }
                }
                if g.normals.get(i).is_some_and(|n| n.length_squared() < 1e-12 || !n.is_finite()) {
                    zero_normals += 1;
                    zn += 1;
                }
            }
            bad_bind += skin.inverse_bind.iter().filter(|m| !m.is_finite() || m.determinant().abs() < 1e-9).count();
            if bad_sum + bad_bone + zn > 0 {
                worst.push((entry.name.clone(), bad_sum, bad_bone, zn));
            }
        }
    }
    println!("modèles lus : {models}, géométries skinnées : {skinned}, sommets skinnés : {verts}");
    println!("somme des poids : nulle {sum_zero}, < 0,99 {sum_low}, > 1,01 {sum_high}");
    println!("os hors palette (poids > 0) : {out_of_range}");
    println!("normales nulles ou NaN : {zero_normals} ; géométries skinnées sans normales : {no_normals}");
    println!("matrices de liaison dégénérées : {bad_bind}");
    println!("drapeaux d'atomics : {flags:?}");
    println!("modèles à plusieurs atomics : {multi}, dont atomics sur le même cadre : {same_frame}");
    for e in &multi_examples {
        println!("  {e}");
    }
    worst.sort_by_key(|w| std::cmp::Reverse(w.1 + w.2 + w.3));
    for (name, s, b, n) in worst.iter().take(15) {
        println!("  {name} : poids {s}, os {b}, normales {n}");
    }
    Ok(())
}
