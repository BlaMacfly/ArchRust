//! Textures manquantes des objets (diagnostic privé) : pour chaque objet d'`ItemTemplateAll.ini`
//! (et `ItemTemplateAll2.ini`, fusionnée par `Assets::table`), modèles `BASE_DFF` / `FIELD_DFF` /
//! `SECOND_DFF` (`d%07x.ecl`) et icône `TEXTURE` (`l%07x`, dossier `texture/item`), puis chaque texture
//! de matériau cherchée comme le moteur (patchs, chemin de recherche du client).
//! Usage : `cargo run -p archrust-engine --example item_texture_audit -- <Game> [nombre d'exemples]`
use archrust_engine::assets::Assets;
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let assets = Assets::new(Some(PathBuf::from(&args[0])), &[]);
    let shown: usize = args.get(1).and_then(|n| n.parse().ok()).unwrap_or(15);
    let table = assets.table("ini", "ItemTemplateAll.ini")?;
    let header = table.section("").unwrap();
    let col = |n: &str| header.entries.iter().find(|(_, v)| v.eq_ignore_ascii_case(n)).map(|(k, _)| k.clone());
    let (name_c, dff_c, field_c, second_c, tex_c, part_c) = (col("Name"), col("BASE_DFF"), col("FIELD_DFF"), col("SECOND_DFF"), col("TEXTURE"), col("Part"));
    let mut checked: BTreeMap<String, bool> = BTreeMap::new();
    let mut missing_models: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut missing_tex: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut missing_icons = Vec::new();
    let mut seen_models = HashSet::new();
    let mut items = 0;
    for sec in table.sections.iter().filter(|s| !s.name.is_empty()) {
        items += 1;
        let get = |c: &Option<String>| c.as_ref().and_then(|c| sec.get(c)).map(str::trim).unwrap_or("");
        let label = format!("{} {} (partie {})", sec.name, get(&name_c), get(&part_c));
        if let Ok(icon) = get(&tex_c).parse::<u32>() {
            if icon > 0 && !assets.texture_found(&format!("l{icon:07x}")) {
                missing_icons.push(format!("{label} : l{icon:07x}"));
            }
        }
        for c in [&dff_c, &field_c, &second_c] {
            let Ok(dff) = get(c).parse::<u32>() else { continue };
            if dff == 0 {
                continue;
            }
            let file = format!("d{dff:07x}.ecl");
            let clump = match assets.clump("character", &file) {
                Ok(c) => c,
                Err(e) => {
                    missing_models.entry(file).or_default().push(format!("{label} : {e:#}"));
                    continue;
                }
            };
            for g in &clump.geometries {
                for m in &g.materials {
                    let Some(t) = &m.texture else { continue };
                    if t.name.is_empty() {
                        continue;
                    }
                    let found = *checked.entry(t.name.to_ascii_lowercase()).or_insert_with(|| assets.texture_found(&t.name));
                    if !found && seen_models.insert((file.clone(), t.name.to_ascii_lowercase())) {
                        missing_tex.entry(t.name.clone()).or_default().push(format!("{file} ← {label}"));
                    }
                }
            }
        }
    }
    println!("{items} objets ; {} textures de matériau vérifiées, {} manquantes ; {} modèles illisibles ; {} icônes manquantes", checked.len(), missing_tex.len(), missing_models.len(), missing_icons.len());
    for (t, users) in missing_tex.iter().take(shown) {
        println!("texture {t} : {} modèle(s), ex. {}", users.len(), users[0]);
    }
    for (m, users) in missing_models.iter().take(shown) {
        println!("modèle {m} : {} objet(s), ex. {}", users.len(), users[0]);
    }
    for i in missing_icons.iter().take(shown) {
        println!("icône {i}");
    }
    Ok(())
}
