//! Extensions des atomics d'un modèle (identifiant de partie…) — diagnostic privé.
//! Usage : `cargo run --release -p archrust-engine --example atomic_ext -- <Game> b0000060.ecl …`
use archrust_engine::assets::Assets;
use std::path::PathBuf;

fn u32_at(d: &[u8], p: usize) -> u32 {
    u32::from_le_bytes([d[p], d[p + 1], d[p + 2], d[p + 3]])
}

/// Parcourt les chunks de `d[start..end]` ; affiche les extensions des atomics.
fn walk(d: &[u8], start: usize, end: usize, depth: usize, in_atomic: bool) {
    let mut p = start;
    while p + 12 <= end {
        let (id, size) = (u32_at(d, p), u32_at(d, p + 4) as usize);
        let body = p + 12;
        let stop = (body + size).min(end);
        match id {
            0x10 | 0x03 if depth < 6 => walk(d, body, stop, depth + 1, in_atomic),
            0x14 => {
                // Atomic : structure (frame, géométrie, drapeaux) puis extension.
                let st = body + 12;
                if st + 12 <= stop {
                    println!("  atomic frame {} géom {} drapeaux {:#x}", u32_at(d, st), u32_at(d, st + 4), u32_at(d, st + 8));
                }
                walk(d, body, stop, depth + 1, true);
            }
            0x1a | 0x0f => {} // listes de géométries et de frames : ignorées
            _ if in_atomic && depth >= 2 && id != 0x01 => {
                let n = size.min(48);
                let bytes: Vec<String> = d[body..body + n].iter().map(|b| format!("{b:02x}")).collect();
                println!("    extension {id:#x} ({size} o) : {}", bytes.join(" "));
            }
            _ => {}
        }
        p = body + size;
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let assets = Assets::new(Some(PathBuf::from(&args[0])), &[]);
    if args.get(1).map(String::as_str) == Some("colonnes") {
        let t = assets.table("ini", "ItemTemplateAll.ini")?;
        let h = t.section("").unwrap();
        println!("{:?}", h.entries.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>());
        for tid in &args[2..] {
            if let Some(sec) = t.section(tid) {
                println!("{tid} : {:?}", h.entries.iter().filter_map(|(k, v)| sec.get(k).filter(|x| !x.trim().is_empty()).map(|x| format!("{v}={x}"))).collect::<Vec<_>>());
            }
        }
        return Ok(());
    }
    for arg in &args[1..] {
        // « tid:336 » : modèle d'objet (BASE_DFF d'ItemTemplateAll.ini).
        let name = match arg.strip_prefix("tid:") {
            Some(t) => archrust_engine::scene::item_model_for_tid(&assets, t.parse()?).unwrap_or_default(),
            None => arg.clone(),
        };
        let name = &name;
        let d = assets.read_packed("character", name)?;
        println!("== {name} ({} o)", d.len());
        walk(&d, 0, d.len(), 0, false);
        if let Ok(c) = assets.clump("character", name) {
            for a in &c.atomics {
                let g = &c.geometries[a.geometry as usize];
                println!("  partie {:?}", archrust_engine::scene::atomic_part(&c, a.geometry as usize));
                println!("  pièce frame {} : {} sommets, peau {}, centre {:?}, rayon {:.0}, textures {:?}", a.frame, g.vertices.len(), g.skin.is_some(), g.bound_center, g.bound_radius, g.materials.iter().map(|m| m.texture.as_ref().map(|t| t.name.clone()).unwrap_or_default()).collect::<Vec<_>>());
                let nodes: Vec<u32> = c.frames.iter().find_map(|f| f.hanim.as_ref().filter(|h| !h.nodes.is_empty())).map(|h| h.nodes.iter().map(|n| n.id).collect()).unwrap_or_default();
                if let Some(sk) = &g.skin {
                    let mut w: std::collections::BTreeMap<u32, f32> = Default::default();
                    for (ix, ws) in sk.indices.iter().zip(&sk.weights) {
                        for l in 0..4 {
                            *w.entry(nodes.get(ix[l] as usize).copied().unwrap_or(9999)).or_default() += ws[l];
                        }
                    }
                    let mut v: Vec<_> = w.into_iter().collect();
                    v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                    println!("     os : {:?}", v.iter().take(5).map(|(k, x)| format!("{k}:{x:.0}")).collect::<Vec<_>>());
                }
            }
        }
    }
    Ok(())
}
