//! Sonde des jeux d'effets `effect/ini/new/efffile_set.eff` + `efffile_var.eff` : liste les jeux
//! (id, nom, bloc, sons), ou détaille un jeu et affiche son bloc en hexadécimal.
//!
//! Usage : `cargo run -p archrust-data --example eff_probe -- <Game> [id] [--all]`
//! (sans `--all`, seuls les jeux ayant un son sont listés).

use archrust_data::EffectSets;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let game = PathBuf::from(args.first().expect("dossier Game"));
    let want: Option<u32> = args.get(1).and_then(|s| s.parse().ok());
    let all = args.iter().any(|a| a == "--all");
    let set_file = std::fs::read(game.join("effect/ini/new/efffile_set.eff"))?;
    let var_file = std::fs::read(game.join("effect/ini/new/efffile_var.eff"))?;
    let sets = EffectSets::parse(&set_file, &var_file);
    println!("{} jeux, blocs : {} octets", sets.len(), var_file.len());
    if args.iter().any(|a| a == "--survey") {
        // Relevé des types de base, drapeaux et paramètres de particules sur tous les jeux.
        let mut seen: std::collections::BTreeMap<String, (usize, u32)> = Default::default();
        for set in sets.iter() {
            let Some(d) = sets.decode(set.id) else { continue };
            for b in &d.bases {
                let mut keys = vec![format!("type {} drapeaux {:#x}", b.kind, b.flags)];
                match &b.particles {
                    Some(archrust_data::effset::Particles::System(p)) => keys.push(format!("psys dir {:#x} forme {} max {} intervalle {}", p.dir_flags, p.shape, p.max, p.interval)),
                    Some(archrust_data::effset::Particles::BlackHole(p)) => keys.push(format!("sbh max {} nb {}+{} intervalle {} vie {}", p.max, p.count, p.count_var, p.interval, p.life)),
                    None => {}
                }
                for a in &b.anims {
                    keys.push(format!("type {} anim {} boucle {}", b.kind, a.kind, a.loop_opt));
                }
                for k in keys {
                    seen.entry(k).or_insert((0, set.id)).0 += 1;
                }
            }
        }
        for (k, (n, id)) in seen {
            println!("{n:6} {k} (ex. {id})");
        }
        return Ok(());
    }
    for set in sets.iter() {
        if let Some(w) = want {
            if w != set.id {
                continue;
            }
        } else if !all && set.sounds.is_empty() {
            continue;
        }
        println!(
            "[{}] {:?} bloc {}+{} sons {:?}{}",
            set.id,
            set.name,
            set.offset,
            set.len,
            set.sounds,
            if set.scanned { " (balayage)" } else { "" }
        );
        if want.is_some() && args.iter().any(|a| a == "--bases") {
            // Bases décodées : en-tête, rendu, octets propres au type (u32 et f32) et animations.
            if let Some(d) = sets.decode(set.id) {
                println!("durée {} ms, textures {:?}", d.life, d.textures);
                for (i, b) in d.bases.iter().enumerate() {
                    println!(
                        "base {i} type {} {:?} délai {} durée {} boucle {} drapeaux {:#x} mélange {:?} texture {} pos {:?} rot {:?}",
                        b.kind, b.title, b.delay, b.life, b.loop_opt, b.flags, b.blend, b.texture, b.position, b.rotation
                    );
                    for (k, w) in b.extra.chunks(4).enumerate() {
                        let v = u32::from_le_bytes(w.try_into().unwrap_or([0; 4]));
                        print!("  [{:#05x}] {v:#010x} {}", k * 4, f32::from_bits(v));
                        if k % 3 == 2 {
                            println!();
                        }
                    }
                    println!();
                    if let Some(p) = &b.particles {
                        println!("  particules {p:?}");
                    }
                    for a in &b.anims {
                        println!("  anim type {} drapeaux {:#x} durée {} boucle {} {:?}", a.kind, a.flags, a.life, a.loop_opt, a.keys);
                    }
                }
            }
            continue;
        }
        if want.is_some() {
            let block = &var_file[set.offset as usize..(set.offset + set.len) as usize];
            for (k, line) in block.chunks(32).enumerate() {
                let hex: Vec<String> = line.iter().map(|b| format!("{b:02x}")).collect();
                let asc: String = line
                    .iter()
                    .map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '.' })
                    .collect();
                println!("{:06x} {} {asc}", k * 32, hex.join(" "));
            }
        }
    }
    Ok(())
}
