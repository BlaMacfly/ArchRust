//! Écart, sur un cycle d'animation, entre chaque triangle « noir » d'une pièce et la surface qui
//! le recouvre en pose de bind (négatif : la surface noire passe devant) — diagnostic privé.
//! Usage : `cargo run --release -p archrust-engine --example gap -- <Game> b0000060.ecl 2 96 2`
use archrust_engine::animation::{load_clip, AnimationSet};
use archrust_engine::assets::Assets;
use glam::{Mat4, Vec3};
use std::path::PathBuf;

fn ray_plane(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> (f32, bool) {
    let n = (b - a).cross(c - a);
    let t = (a - o).dot(n) / d.dot(n);
    let p = o + d * t;
    // Le point est-il dans le triangle (barycentriques) ?
    let (v0, v1, v2) = (b - a, c - a, p - a);
    let (d00, d01, d11, d20, d21) = (v0.dot(v0), v0.dot(v1), v1.dot(v1), v2.dot(v0), v2.dot(v1));
    let den = d00 * d11 - d01 * d01;
    let v = (d11 * d20 - d01 * d21) / den;
    let w = (d00 * d21 - d01 * d20) / den;
    (t, v >= -0.05 && w >= -0.05 && v + w <= 1.05)
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let assets = Assets::new(Some(PathBuf::from(&a[0])), &[]);
    let c = assets.clump("character", &a[1])?;
    let gi: usize = a[2].parse()?;
    let (tid, kind): (i32, u32) = (a[3].parse()?, a[4].parse()?);
    let g = &c.geometries[gi];
    let skin = g.skin.as_ref().unwrap();
    let uv = &g.uv_sets[0];
    let img = assets.texture(&g.materials[0].texture.as_ref().unwrap().name)?;
    let lum = |t: &archrust_data::rw::Triangle| {
        let (u, v) = t.v.iter().fold((0.0, 0.0), |s, &i| (s.0 + uv[i as usize][0] / 3.0, s.1 + uv[i as usize][1] / 3.0));
        let x = (u.rem_euclid(1.0) * img.width as f32) as usize % img.width as usize;
        let y = (v.rem_euclid(1.0) * img.height as f32) as usize % img.height as usize;
        let p = &img.data[(y * img.width as usize + x) * 4..][..4];
        (p[0] as u32 * 3 + p[1] as u32 * 6 + p[2] as u32) / 10
    };
    let deform = |mats: &[Mat4]| -> Vec<Vec3> {
        (0..g.vertices.len())
            .map(|i| {
                let (mut p, mut tot) = (Vec3::ZERO, 0.0);
                for l in 0..4 {
                    let w = skin.weights[i][l];
                    if w > 0.0 {
                        p += mats[skin.indices[i][l] as usize].transform_point3(g.vertices[i]) * w;
                        tot += w;
                    }
                }
                p / tot
            })
            .collect()
    };
    let tri = |vs: &[Vec3], t: &archrust_data::rw::Triangle| t.v.map(|i| vs[i as usize]);
    // Couples (triangle noir, triangle couvrant) en pose de bind.
    let bind = g.vertices.clone();
    let mut pairs = Vec::new();
    for (k, t) in g.triangles.iter().enumerate().filter(|(_, t)| lum(t) < 20) {
        let [p0, p1, p2] = tri(&bind, t);
        let (o, n) = ((p0 + p1 + p2) / 3.0, (p1 - p0).cross(p2 - p0).normalize());
        let best = g
            .triangles
            .iter()
            .enumerate()
            .filter(|(j, o2)| *j != k && lum(o2) >= 20)
            .filter_map(|(j, o2)| {
                let [a2, b2, c2] = tri(&bind, o2);
                let (t, inside) = ray_plane(o, n, a2, b2, c2);
                (inside && t > 1e-3).then_some((t, j))
            })
            .min_by(|x, y| x.0.total_cmp(&y.0));
        if let Some((t, j)) = best {
            pairs.push((k, j, t));
        }
    }
    let set = AnimationSet::for_tid(&assets, tid, 0, true);
    let file = set.pick(kind, 0.0).unwrap().file.clone();
    let clip = load_clip(&assets, &file).unwrap();
    let mut worst = vec![f32::MAX; pairs.len()];
    for s in 0..(clip.duration * 60.0) as usize {
        let pose = clip.sample(s as f32 / 60.0);
        let vs = deform(&c.skin_matrices(gi, &pose)?);
        for (pi, &(k, j, _)) in pairs.iter().enumerate() {
            let [p0, p1, p2] = tri(&vs, &g.triangles[k]);
            let (o, n) = ((p0 + p1 + p2) / 3.0, (p1 - p0).cross(p2 - p0).normalize());
            let [a2, b2, c2] = tri(&vs, &g.triangles[j]);
            let (t, _) = ray_plane(o, n, a2, b2, c2);
            worst[pi] = worst[pi].min(t);
        }
    }
    for (pi, &(k, j, t)) in pairs.iter().enumerate() {
        println!("noir {k} sous {j} : écart en bind {t:.2}, minimum en animation {:.2}", worst[pi]);
    }
    Ok(())
}
