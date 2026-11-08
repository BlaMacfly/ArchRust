//! Profil CPU réel du TID 96, sans fenêtre ni GPU. Chargement et poses exclus des boucles chaudes.
//! Utilise les mêmes modèles, clips, sommets et `deform` que `anim_audit`.
// Le script de profil choisit ce cfg privé lors d'une compilation directe, hors des features Cargo.
#![allow(unexpected_cfgs)]
#[cfg(not(archrust_cpu_snapshot))]
use archrust_engine::animation::{deform, load_clip, SkinBinding, SkinPoseBatch};
#[cfg(not(archrust_cpu_snapshot))]
use archrust_engine::assets::Assets;
#[cfg(not(archrust_cpu_snapshot))]
use archrust_engine::scene::{geometry_vertices, models_for_tid};
#[cfg(archrust_cpu_snapshot)]
#[path = "../../../target/skinning_cpu_snapshot.rs"]
mod snapshot;
#[cfg(archrust_cpu_snapshot)]
use snapshot::{deform, load_clip, SkinBinding, SkinPoseBatch, Assets, geometry_vertices, models_for_tid};
use std::collections::HashMap;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::Instant;

fn measure(mut f: impl FnMut()) -> u128 {
    for _ in 0..16 { f(); }
    let started = Instant::now();
    for _ in 0..600 { f(); }
    started.elapsed().as_micros()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let game = std::env::args().nth(1).ok_or("usage : skinning_cpu <Game>")?;
    let assets = Assets::new(Some(PathBuf::from(game)), &[]);
    // Même choix vérifié par anim_audit : TID 96, course, ensemble 0.
    let file = "a0060200.ean";
    let clip = load_clip(&assets, file).ok_or("clip illisible")?;
    let poses: Vec<_> = (0..60).map(|i| clip.sample(clip.duration * i as f32 / 60.0)).collect();
    println!("TID 96 clip={file} poses=60 appels_par_mesure=600 ; CPU seulement, temps en microsecondes");
    for model in models_for_tid(&assets, 96) {
        let clump = assets.clump("character", &model)?;
        let mut bindings = Vec::new();
        for (geometry, geom) in clump.geometries.iter().enumerate() {
            if geom.skin.is_some() {
                bindings.push(SkinBinding { clump: clump.clone(), geometry, base: geometry_vertices(geom), wearer: None });
            }
        }
        let vertices: usize = bindings.iter().map(|b| b.base.len()).sum();
        println!("modele={model} frames={} pieces={} sommets={vertices}", clump.frames.len(), bindings.len());
        if bindings.is_empty() { continue; }
        // Comparaison des bits de tous les attributs : pas de tolérance numérique.
        for pose in &poses {
            let mut batch = SkinPoseBatch::new(pose);
            for binding in &bindings {
                let reference = deform(binding, pose).ok_or("déformation de référence absente")?;
                let prepared = batch.deform(binding).ok_or("déformation préparée absente")?;
                assert_eq!(reference.len(), prepared.len());
                for (a, b) in reference.iter().zip(&prepared) {
                    assert!(a.position.into_iter().chain(a.normal).chain(a.uv).chain(a.color).map(f32::to_bits)
                        .eq(b.position.into_iter().chain(b.normal).chain(b.uv).chain(b.color).map(f32::to_bits)), "sommet différent");
                }
            }
        }
        println!("egalite_bits positions/normales/UV/couleurs : 60 poses x {} pieces OK", bindings.len());
        let hierarchy = clump.frames.iter().filter_map(|f| f.hanim.as_ref()).find(|h| !h.nodes.is_empty()).ok_or("HAnim absent")?;
        let linear = || -> Vec<Option<usize>> {
            hierarchy.nodes.iter().map(|node| clump.frames.iter().position(|f| f.hanim.as_ref().is_some_and(|h| h.node_id == node.id))).collect()
        };
        let indexed = || -> Vec<Option<usize>> {
            let mut ids = HashMap::with_capacity(clump.frames.len());
            for (i, frame) in clump.frames.iter().enumerate() {
                if let Some(h) = &frame.hanim { ids.entry(h.node_id).or_insert(i); }
            }
            hierarchy.nodes.iter().map(|node| ids.get(&node.id).copied()).collect()
        };
        assert_eq!(linear(), indexed(), "correspondance exacte des ID");
        for trial in 0..5 {
            let mut i = 0;
            let sample = measure(|| { black_box(clip.sample(clip.duration * (i % 60) as f32 / 60.0)); i += 1; });
            let mut i = 0;
            let world = measure(|| { black_box(clump.world_matrices_with_pose(black_box(&poses[i % 60]))); i += 1; });
            let mut i = 0;
            let matrices = measure(|| {
                for binding in &bindings { black_box(clump.skin_matrices(binding.geometry, black_box(&poses[i % 60])).unwrap()); }
                i += 1;
            });
            let mut before_i = 0;
            let mut after_i = 0;
            let mut original = || {
                for binding in &bindings { black_box(deform(black_box(binding), black_box(&poses[before_i % 60])).unwrap()); }
                before_i += 1;
            };
            let mut prepared = || {
                let mut batch = SkinPoseBatch::new(black_box(&poses[after_i % 60]));
                for binding in &bindings { black_box(batch.deform(black_box(binding)).unwrap()); }
                after_i += 1;
            };
            let (skinning, skinning_prepared) = if trial % 2 == 0 {
                (measure(&mut original), measure(&mut prepared))
            } else {
                let after = measure(&mut prepared);
                (measure(&mut original), after)
            };
            let (lookup_before, lookup_after) = if trial % 2 == 0 {
                (measure(|| { black_box(linear()); }), measure(|| { black_box(indexed()); }))
            } else {
                let after = measure(|| { black_box(indexed()); });
                (measure(|| { black_box(linear()); }), after)
            };
            if trial > 0 {
                println!("essai={trial} sample={sample} world={world} matrices_toutes_pieces={matrices} deform_toutes_pieces={skinning} deform_preparee_toutes_pieces={skinning_prepared} recherche_lineaire={lookup_before} recherche_indexee_construction_incluse={lookup_after}");
            }
        }
    }
    Ok(())
}
