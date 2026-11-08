//! Mesures CPU uniquement, sans fenêtre, adaptateur GPU ni données externes.
//! Le stockage simulé ci-dessous mesure une copie hôte, PAS Queue::write_buffer.
#[path = "../src/render_upload_cache.rs"]
mod upload_cache;
#[path = "../src/app_animator_retention.rs"]
mod animator_retention;

use std::collections::HashMap;
use std::hint::black_box;
use std::time::Instant;

fn uploads(cached: bool, dynamic_percent: usize) -> (u128, usize) {
    const COUNT: usize = 4096;
    const FRAMES: usize = 240;
    let mut caches: Vec<_> = (0..COUNT).map(|_| upload_cache::UploadCache::default()).collect();
    let mut storage = vec![[0u8; 176]; COUNT];
    let mut writes = 0;
    let started = Instant::now();
    for frame in 0..FRAMES {
        for i in 0..COUNT {
            let mut data = [0u8; 176];
            data[..8].copy_from_slice(&(i as u64).to_le_bytes());
            if i % 100 < dynamic_percent {
                data[168..176].copy_from_slice(&(frame as u64).to_le_bytes());
            }
            let bytes = black_box(&data);
            if !cached || caches[i].changed(bytes) {
                // Même copie préallouée dans les deux variantes ; aucun coût GPU simulé.
                black_box(&mut storage[i]).copy_from_slice(bytes);
                writes += 1;
            }
        }
    }
    black_box(storage);
    (started.elapsed().as_micros(), writes)
}

fn retain(indexed: bool, count: i32) -> u128 {
    let entities: Vec<i32> = (0..count).collect();
    let source: HashMap<i32, ()> = (0..count + count / 10).map(|id| (id, ())).collect();
    let mut elapsed = 0;
    for _ in 0..100 {
        let mut animators = source.clone();
        let started = Instant::now();
        if indexed {
            animator_retention::retain_present(&mut animators, black_box(&entities), |id| *id);
        } else {
            animators.retain(|id, _| black_box(&entities).iter().any(|e| e == id));
        }
        elapsed += started.elapsed().as_nanos();
        assert_eq!(animators.len(), count as usize);
        black_box(animators);
    }
    elapsed / 1000
}

fn main() {
    println!("CPU, profil dev opt-level=1 ; aucune mesure FPS/GPU. Temps en microsecondes.");
    println!("Copie hôte synthétique : 4096 instances x 240 images ; cache de production inclus.");
    for percent in [0, 10, 100] {
        for sample in 0..5 {
            // Ordre alterné pour limiter le biais de chauffe ; première paire d'échauffement exclue.
            let (before, after) = if sample % 2 == 0 {
                let a = uploads(false, percent);
                (a, uploads(true, percent))
            } else {
                let b = uploads(true, percent);
                (uploads(false, percent), b)
            };
            if sample > 0 {
                println!("uploads dynamique={percent}% essai={sample} avant={} apres={} copies_avant={} copies_apres={}", before.0, after.0, before.1, after.1);
            }
        }
    }
    for count in [32, 127, 128, 256, 2000] {
        for sample in 0..5 {
            let (before, after) = if sample % 2 == 0 {
                (retain(false, count), retain(true, count))
            } else {
                let b = retain(true, count);
                (retain(false, count), b)
            };
            if sample > 0 {
                println!("retain n={count} x100 essai={sample} avant={before} apres={after}");
            }
        }
    }
    for fps in [60, 144] {
        let count = |hz: f32| {
            let mut since = f32::MAX;
            let mut poses = 0;
            for _ in 0..fps {
                since += 1.0 / fps as f32;
                if since >= 1.0 / hz { poses += 1; since = 0.0; }
            }
            poses
        };
        println!("seuil_pose images={fps} ancien={} fluide={}", count(30.0), count(f32::INFINITY));
    }
}
