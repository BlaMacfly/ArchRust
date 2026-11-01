//! Sonde des collisions d'un lieu : objets placés proches (modèle visible et `COLLISION_DFF`,
//! bornes monde), cellules `.amf` du secteur (octet de blocage) et hauteur du terrain.
//!
//! Usage : `cargo run -p archrust-engine --example collision_probe -- <Game> <x> <z> [rayon]`

use archrust_engine::assets::Assets;
use archrust_engine::objects::{root_inverse, tile_placements, WorldObjects};
use archrust_engine::world::{sector_of, sector_origin, tile_of, Terrain, CELL_SIZE};
use archrust_engine::zones::parse_amf;
use glam::{Mat4, Vec3};
use std::path::PathBuf;

fn bounds(assets: &Assets, file: &str, object: Mat4) -> Option<(Vec3, Vec3, usize)> {
    let clump = assets.clump("object", file).ok()?;
    let root = root_inverse(assets, file);
    let world = clump.world_matrices();
    let (mut lo, mut hi, mut n) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN), 0);
    for a in &clump.atomics {
        let (Some(g), Some(f)) = (clump.geometries.get(a.geometry as usize), world.get(a.frame as usize)) else { continue };
        let m = object * root * *f;
        for v in &g.vertices {
            let p = m.transform_point3(*v);
            lo = lo.min(p);
            hi = hi.max(p);
        }
        n += g.triangles.len();
    }
    (n > 0).then_some((lo, hi, n))
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let assets = Assets::new(Some(PathBuf::from(&args[0])), &[]);
    let (x, z): (f32, f32) = (args[1].parse()?, args[2].parse()?);
    let r: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1500.0);
    let (sx, sy) = sector_of(x, z);
    let templates = assets.table("ini", "objecttemplate.ini")?;
    for p in tile_placements(&assets, tile_of(sx, sy)) {
        if (p.position.x - x).hypot(p.position.z - z) > r {
            continue;
        }
        let Some(t) = templates.section(&p.tid.to_string()) else { continue };
        let dff = t.get_all("10").find_map(|v| v.split_once(':').map(|(_, f)| f.trim().to_ascii_lowercase()));
        let coll = t.get("5").map(|s| s.trim().to_ascii_lowercase());
        println!(
            "TID {} {} pos {:?} rot ({}, {}) échelle {:?} drapeaux {:?} ridable {:?}",
            p.tid,
            t.get("1").unwrap_or(""),
            p.position,
            p.degree_x,
            p.degree_y,
            p.scale,
            t.get_all("2").collect::<Vec<_>>(),
            t.get("6")
        );
        for (what, file) in [("visible", dff), ("collision", coll)] {
            let Some(file) = file else { continue };
            match bounds(&assets, &file, p.matrix()) {
                Some((lo, hi, n)) => println!("  {what} {file} : {n} triangles, {lo:?} .. {hi:?}"),
                None => println!("  {what} {file} : introuvable"),
            }
            if let Ok(c) = assets.clump("object", &file) {
                println!("    frames {} atomics {} racine {:?}", c.frames.len(), c.atomics.len(), c.frames.first().map(|f| f.position));
            }
        }
    }
    for (prefix, ext) in [('a', "ma1"), ('a', "ma2"), ('b', "ma2")] {
        if let Ok(p) = assets.magpack(prefix, tile_of(sx, sy), ext) {
            let mine: Vec<String> = p.entries.iter().filter(|e| e.name.contains(&format!("{sx},{sy}"))).map(|e| format!("{} ({} o)", e.name, e.size)).collect();
            println!("{prefix}{:06}x.{ext} : {} entrées, secteur : {mine:?}", tile_of(sx, sy), p.entries.len());
        }
    }
    // Cellules bloquées (octet 1, bit 0) des secteurs voisins.
    for dy in -2..=2 {
        for dx in -2..=2 {
            let (tx, ty) = (sx + dx, sy + dy);
            let Ok(p) = assets.magpack('a', tile_of(tx, ty), "ma1") else { continue };
            let Some(Ok(bytes)) = p.unpack_named(&format!("C{tx},{ty}.amf")) else { continue };
            let grid = parse_amf(&bytes).unwrap_or_default();
            let blocked = grid.iter().filter(|c| (*c >> 8) & 1 != 0).count();
            let mut kinds: Vec<u32> = grid.iter().map(|c| c & 0xffff).collect();
            kinds.sort();
            kinds.dedup();
            let lines = bytes.get(1036..1040).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
            println!("C{tx},{ty} : {blocked} cellules bloquées, {lines:?} segments, {} octets, valeurs {:x?}", bytes.len(), kinds);
        }
    }
    // Hauteurs le long de z (± rayon) : terrain, sol praticable (collision), modèle dessiné.
    let mut terrain = Terrain::new(1);
    for dy in -1..=1 {
        for dx in -1..=1 {
            terrain.load_ground(&assets, sx + dx, sy + dy);
        }
    }
    let mut objects = WorldObjects::new(1);
    objects.load_tiles(&assets, Vec3::new(x, 0.0, z));
    let mut zz = z - r;
    while zz <= z + r {
        let t = terrain.height_at(x, zz);
        let o = objects.ridable_height(&assets, x, zz);
        let v = objects.floor_height(&assets, x, zz, 1e9);
        println!("  z {zz:.0} : terrain {t:?}, praticable {o:?}, dessiné {v:?}, bloqué {}", terrain.blocked(x, zz));
        zz += r / 10.0;
    }
    let pack = assets.magpack('a', tile_of(sx, sy), "ma1")?;
    if let Some(Ok(bytes)) = pack.unpack_named(&format!("C{sx},{sy}.amf")) {
        let u = |o: usize| bytes.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
        println!("C{sx},{sy}.amf : {} octets, en-tête {:x?} {:x?} {:x?} {:x?}, après la grille {:?}", bytes.len(), u(0), u(4), u(8), u(12), u(1036));
        for k in 0..u(1036).unwrap_or(0).min(400) as usize {
            let f: Vec<f32> = (0..7).filter_map(|i| u(1040 + k * 28 + i * 4).map(f32::from_bits)).collect();
            println!("  segment {k} : {f:?}");
        }
        let grid = parse_amf(&bytes).unwrap_or_default();
        let o = sector_origin(sx, sy);
        println!("C{sx},{sy}.amf (ligne = z de {} par {CELL_SIZE}, colonne = x de {}), octets 0 et 1 :", o.z, o.x);
        for j in 0..16 {
            let row: Vec<String> = (0..16).map(|i| format!("{:04x}", grid[j * 16 + i] & 0xffff)).collect();
            println!("  {j:2} {}", row.join(" "));
        }
    }
    Ok(())
}
