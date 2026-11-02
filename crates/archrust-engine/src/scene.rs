//! Conversion des clumps RenderWare en maillages GPU et entités du monde.
//!
//! Tout modèle passe par [`model_meshes`] : si un patch déclare un `[[models]]` glTF/GLB pour la
//! clé logique (`character/a0000060.ecl`…), c'est lui qui est affiché, avec ses propres textures ;
//! sinon le clump du client est converti.

use crate::assets::Assets;
use crate::gltf_model;
use crate::render::{Mesh, Renderer, SubMesh, Vertex};
use anyhow::Result;
use archrust_data::rw::{Clump, Geometry};
use archrust_netclient::world::EquipmentView;
use glam::{Mat4, Vec3};
use std::collections::HashMap;
use std::sync::Arc;

/// Maillages d'un modèle logique `rel_dir/file` (ex. `character/a0000060.ecl`) : un patch
/// `[[models]]` fournissant un glTF/GLB l'emporte sur le clump du client, et ses textures
/// remplacent celles du client.
pub fn model_meshes(
    renderer: &mut Renderer,
    assets: &Assets,
    rel_dir: &str,
    file: &str,
) -> Result<Vec<(Arc<Mesh>, Mat4)>> {
    let key = format!("{rel_dir}/{file}");
    if let Some(meshes) = patched_model_meshes(renderer, assets, &key) {
        return Ok(meshes);
    }
    let clump = assets.clump(rel_dir, file)?;
    Ok(build_clump_meshes(renderer, assets, &clump))
}

/// Maillages GPU du glTF qu'un patch fournit pour `key`, s'il y en a un. Un glTF illisible est
/// signalé puis ignoré : le modèle du client reprend la main.
pub fn patched_model_meshes(
    renderer: &mut Renderer,
    assets: &Assets,
    key: &str,
) -> Option<Vec<(Arc<Mesh>, Mat4)>> {
    match assets.patched_model(key)? {
        Ok(pm) => Some(gltf_model::upload(
            renderer,
            &pm.model,
            &pm.texture_prefix(),
            pm.scale,
        )),
        Err(e) => {
            log::warn!("{key} : {e:#}");
            None
        }
    }
}

/// Construit les maillages d'un clump : un maillage par atomic, dans l'espace de sa frame.
/// Les textures référencées sont chargées (packs ou patch HD) et téléversées à la demande.
pub fn build_clump_meshes(
    renderer: &mut Renderer,
    assets: &Assets,
    clump: &Arc<Clump>,
) -> Vec<(Arc<Mesh>, Mat4)> {
    build_clump_pieces(renderer, assets, clump)
        .into_iter()
        .map(|(mesh, model, _)| (mesh, model))
        .collect()
}

/// Partie du corps d'une pièce skinnée d'après les os qui la portent (`Part` d'ItemTemplateAll :
/// 2 corps, 5 mains, 6 jambes, 7 pieds) : les atomics ne portent pas leur partie, et leur ordre
/// change d'un modèle à l'autre. Mains : os 101/102 et doigts (1004–1018, 1058–1072) ; pieds :
/// 103/104 et 1024/1078 ; jambes : plus de poids sur les cuisses (41/51) que sur les bras (21, 22,
/// 31, 32) ; corps : le reste. `None` pour une pièce sans peau.
pub fn atomic_part(clump: &Clump, geometry: usize) -> Option<i64> {
    let skin = clump.geometries.get(geometry)?.skin.as_ref()?;
    let nodes: Vec<u32> = clump
        .frames
        .iter()
        .find_map(|f| f.hanim.as_ref().filter(|h| !h.nodes.is_empty()))
        .map(|h| h.nodes.iter().map(|n| n.id).collect())?;
    let (mut hands, mut feet, mut thighs, mut arms) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for (ix, ws) in skin.indices.iter().zip(&skin.weights) {
        for lane in 0..4 {
            let w = ws[lane];
            if w <= 0.0 {
                continue;
            }
            match nodes.get(ix[lane] as usize).copied().unwrap_or(0) {
                101 | 102 | 1004..=1018 | 1058..=1072 => hands += w,
                103 | 104 | 1024 | 1078 => feet += w,
                41 | 51 => thighs += w,
                21 | 22 | 31 | 32 => arms += w,
                _ => {}
            }
        }
    }
    Some(if hands > 0.0 {
        5
    } else if feet > 0.0 {
        7
    } else if thighs > arms {
        6
    } else {
        2
    })
}

/// Comme `build_clump_meshes`, avec la partie du corps de chaque pièce (`atomic_part`).
pub fn build_clump_pieces(
    renderer: &mut Renderer,
    assets: &Assets,
    clump: &Arc<Clump>,
) -> Vec<(Arc<Mesh>, Mat4, Option<i64>)> {
    build_pieces(renderer, assets, clump, false)
}

/// Comme `build_clump_pieces` pour un personnage : cadre 0 du clump à l'identité (RenderWare le
/// remplace par la matrice de l'entité, voir `Clump::world_matrices_rooted`).
pub fn build_character_pieces(
    renderer: &mut Renderer,
    assets: &Assets,
    clump: &Arc<Clump>,
) -> Vec<(Arc<Mesh>, Mat4, Option<i64>)> {
    build_pieces(renderer, assets, clump, true)
}

fn build_pieces(
    renderer: &mut Renderer,
    assets: &Assets,
    clump: &Arc<Clump>,
    rooted: bool,
) -> Vec<(Arc<Mesh>, Mat4, Option<i64>)> {
    let world = if rooted { clump.world_matrices_rooted() } else { clump.world_matrices() };
    let has_hierarchy = clump
        .frames
        .iter()
        .any(|f| f.hanim.as_ref().is_some_and(|h| !h.nodes.is_empty()));
    let mut out = Vec::new();
    for atomic in &clump.atomics {
        let Some(geom) = clump.geometries.get(atomic.geometry as usize) else {
            continue;
        };
        if geom.vertices.is_empty() {
            continue;
        }
        let mut mesh = build_geometry_mesh(renderer, assets, geom);
        if geom.skin.is_some() && has_hierarchy {
            mesh.skin = Some(Arc::new(crate::animation::SkinBinding {
                clump: clump.clone(),
                geometry: atomic.geometry as usize,
                base: geometry_vertices(geom),
                wearer: None,
            }));
        }
        let model = world
            .get(atomic.frame as usize)
            .copied()
            .unwrap_or(Mat4::IDENTITY);
        out.push((Arc::new(mesh), model, atomic_part(clump, atomic.geometry as usize)));
    }
    out
}

/// Pièces d'un modèle de personnage avec leur partie du corps (cadre 0 à l'identité, voir
/// `build_character_pieces`) ; un modèle remplacé par un patch glTF n'en a pas.
fn model_pieces(renderer: &mut Renderer, assets: &Assets, rel_dir: &str, file: &str) -> Result<Vec<(Arc<Mesh>, Mat4, Option<i64>)>> {
    let key = format!("{rel_dir}/{file}");
    if let Some(meshes) = patched_model_meshes(renderer, assets, &key) {
        return Ok(meshes.into_iter().map(|(m, t)| (m, t, None)).collect());
    }
    let clump = assets.clump(rel_dir, file)?;
    Ok(build_character_pieces(renderer, assets, &clump))
}

/// Maillage d'une géométrie : les triangles sont regroupés par matériau.
pub fn build_geometry_mesh(renderer: &mut Renderer, assets: &Assets, geom: &Geometry) -> Mesh {
    let n = geom.vertices.len();
    let vertices = geometry_vertices(geom);
    build_geometry_mesh_from(renderer, assets, geom, n, vertices)
}

/// Sommets GPU d'une géométrie (position, normale, UV0, couleur), normales calculées si absentes.
pub fn geometry_vertices(geom: &Geometry) -> Vec<Vertex> {
    let n = geom.vertices.len();
    let uv0 = geom.uv_sets.first();
    let mut vertices: Vec<Vertex> = Vec::with_capacity(n);
    for i in 0..n {
        vertices.push(Vertex {
            position: geom.vertices[i].to_array(),
            normal: geom.normals.get(i).copied().unwrap_or(Vec3::Y).to_array(),
            uv: uv0.and_then(|s| s.get(i)).copied().unwrap_or([0.0, 0.0]),
            color: geom
                .colors
                .get(i)
                .map(|c| {
                    [
                        c[0] as f32 / 255.0,
                        c[1] as f32 / 255.0,
                        c[2] as f32 / 255.0,
                        c[3] as f32 / 255.0,
                    ]
                })
                .unwrap_or([1.0; 4]),
        });
    }
    if geom.normals.is_empty() {
        compute_normals(&mut vertices, &geom.triangles);
    }
    vertices
}

fn build_geometry_mesh_from(
    renderer: &mut Renderer,
    assets: &Assets,
    geom: &Geometry,
    n: usize,
    vertices: Vec<Vertex>,
) -> Mesh {
    // Indices par matériau (listes de triangles ; les bandes du Bin Mesh sont dépliées si besoin).
    let mut by_material: HashMap<u16, Vec<u32>> = HashMap::new();
    if !geom.meshes.is_empty() {
        for m in &geom.meshes {
            let list = by_material.entry(m.material as u16).or_default();
            if geom.tristrip {
                for w in m.indices.windows(3).enumerate() {
                    let (i, tri) = w;
                    if tri[0] == tri[1] || tri[1] == tri[2] || tri[0] == tri[2] {
                        continue;
                    }
                    if i % 2 == 0 {
                        list.extend_from_slice(&[tri[0], tri[1], tri[2]]);
                    } else {
                        list.extend_from_slice(&[tri[1], tri[0], tri[2]]);
                    }
                }
            } else {
                list.extend_from_slice(&m.indices);
            }
        }
    } else {
        for t in &geom.triangles {
            by_material
                .entry(t.material)
                .or_default()
                .extend_from_slice(&[t.v[0] as u32, t.v[1] as u32, t.v[2] as u32]);
        }
    }
    let mut indices: Vec<u32> = Vec::new();
    let mut parts = Vec::new();
    let mut mats: Vec<_> = by_material.into_iter().collect();
    mats.sort_by_key(|(m, _)| *m);
    for (mat_id, list) in mats {
        let material = geom.materials.get(mat_id as usize);
        let texture = material
            .and_then(|m| m.texture.as_ref())
            .map(|t| t.name.clone());
        if let Some(name) = &texture {
            ensure_texture(renderer, assets, name);
        }
        let color = material
            .map(|m| {
                [
                    m.color[0] as f32 / 255.0,
                    m.color[1] as f32 / 255.0,
                    m.color[2] as f32 / 255.0,
                    m.color[3] as f32 / 255.0,
                ]
            })
            .unwrap_or([1.0; 4]);
        parts.push(SubMesh {
            index_start: indices.len() as u32,
            index_count: list.len() as u32,
            texture,
            color,
        });
        indices.extend(list.into_iter().filter(|&i| (i as usize) < n));
        if let Some(p) = parts.last_mut() {
            p.index_count = indices.len() as u32 - p.index_start;
        }
    }
    renderer.create_mesh(&vertices, &indices, parts)
}

/// Charge et téléverse une texture si elle n'est pas déjà sur le GPU.
pub fn ensure_texture(renderer: &mut Renderer, assets: &Assets, name: &str) {
    if renderer.texture(name).is_some() {
        return;
    }
    match assets.texture(name) {
        Ok(img) => {
            renderer.upload_texture(name, img.width, img.height, &img.data);
        }
        Err(e) => {
            log::debug!("texture {name} : {e}");
            renderer
                .textures
                .insert(name.to_ascii_lowercase(), renderer.white.clone());
        }
    }
}

fn compute_normals(vertices: &mut [Vertex], triangles: &[archrust_data::rw::Triangle]) {
    let mut acc = vec![Vec3::ZERO; vertices.len()];
    for t in triangles {
        let [a, b, c] = t.v.map(|i| i as usize);
        if a >= vertices.len() || b >= vertices.len() || c >= vertices.len() {
            continue;
        }
        let pa = Vec3::from_array(vertices[a].position);
        let pb = Vec3::from_array(vertices[b].position);
        let pc = Vec3::from_array(vertices[c].position);
        let nrm = (pb - pa).cross(pc - pa);
        acc[a] += nrm;
        acc[b] += nrm;
        acc[c] += nrm;
    }
    for (v, n) in vertices.iter_mut().zip(acc) {
        v.normal = n.normalize_or(Vec3::Y).to_array();
    }
}

/// Maillage de secours (boîte) pour les entités sans modèle connu.
pub fn box_mesh(renderer: &Renderer, half: Vec3, color: [f32; 4]) -> Mesh {
    let p = |x: f32, y: f32, z: f32| [x * half.x, y * half.y + half.y, z * half.z];
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        (
            [0.0, 0.0, 1.0],
            [
                p(-1.0, -1.0, 1.0),
                p(1.0, -1.0, 1.0),
                p(1.0, 1.0, 1.0),
                p(-1.0, 1.0, 1.0),
            ],
        ),
        (
            [0.0, 0.0, -1.0],
            [
                p(1.0, -1.0, -1.0),
                p(-1.0, -1.0, -1.0),
                p(-1.0, 1.0, -1.0),
                p(1.0, 1.0, -1.0),
            ],
        ),
        (
            [1.0, 0.0, 0.0],
            [
                p(1.0, -1.0, 1.0),
                p(1.0, -1.0, -1.0),
                p(1.0, 1.0, -1.0),
                p(1.0, 1.0, 1.0),
            ],
        ),
        (
            [-1.0, 0.0, 0.0],
            [
                p(-1.0, -1.0, -1.0),
                p(-1.0, -1.0, 1.0),
                p(-1.0, 1.0, 1.0),
                p(-1.0, 1.0, -1.0),
            ],
        ),
        (
            [0.0, 1.0, 0.0],
            [
                p(-1.0, 1.0, 1.0),
                p(1.0, 1.0, 1.0),
                p(1.0, 1.0, -1.0),
                p(-1.0, 1.0, -1.0),
            ],
        ),
        (
            [0.0, -1.0, 0.0],
            [
                p(-1.0, -1.0, -1.0),
                p(1.0, -1.0, -1.0),
                p(1.0, -1.0, 1.0),
                p(-1.0, -1.0, 1.0),
            ],
        ),
    ];
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for (normal, quad) in faces {
        let base = vertices.len() as u32;
        for (i, pos) in quad.iter().enumerate() {
            vertices.push(Vertex {
                position: *pos,
                normal,
                uv: [(i & 1) as f32, (i >> 1) as f32],
                color,
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let count = indices.len() as u32;
    renderer.create_mesh(
        &vertices,
        &indices,
        vec![SubMesh {
            index_start: 0,
            index_count: count,
            texture: None,
            color: [1.0; 4],
        }],
    )
}

/// Sol de secours (grille plate) quand aucun secteur n'est chargé.
pub fn ground_mesh(renderer: &Renderer, size: f32) -> Mesh {
    let h = size / 2.0;
    let vertices = vec![
        Vertex {
            position: [-h, 0.0, -h],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 0.0],
            color: [0.35, 0.5, 0.3, 1.0],
        },
        Vertex {
            position: [h, 0.0, -h],
            normal: [0.0, 1.0, 0.0],
            uv: [size / 500.0, 0.0],
            color: [0.35, 0.5, 0.3, 1.0],
        },
        Vertex {
            position: [h, 0.0, h],
            normal: [0.0, 1.0, 0.0],
            uv: [size / 500.0, size / 500.0],
            color: [0.35, 0.5, 0.3, 1.0],
        },
        Vertex {
            position: [-h, 0.0, h],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, size / 500.0],
            color: [0.35, 0.5, 0.3, 1.0],
        },
    ];
    renderer.create_mesh(
        &vertices,
        &[0, 2, 1, 0, 3, 2],
        vec![SubMesh {
            index_start: 0,
            index_count: 6,
            texture: None,
            color: [1.0; 4],
        }],
    )
}

/// Entité du monde affichée.
pub struct Entity {
    pub cid: i32,
    pub tid: i32,
    pub name: String,
    /// Derniers PV connus, mis à jour par les facteurs d'action reçus.
    pub hp: Option<i32>,
    pub position: Vec3,
    pub destination: Vec3,
    /// Orientation autour de l'axe vertical, en degrés (`m_fTurnY` d'Archonia, voir
    /// [`turn_towards`]).
    pub turn_y: f32,
    /// Tête par défaut affichée (visage, cheveux).
    pub head: Head,
    pub meshes: Vec<(Arc<Mesh>, Mat4)>,
    pub equipment: Vec<EquipmentView>,
    pub is_player: bool,
    /// Vitesses et allure du déplacement en cours.
    pub speeds: Speeds,
}

/// Vitesses d'un personnage, comme Archonia : facteurs de statut 8 (marche) et 9 (course), en dixièmes
/// d'unité par seconde. Le trajet dure `10000 / vitesse × distance` ms (`FUN_140168070`) ; une vitesse
/// inconnue vaut 4000 en marche (`FUN_14017a750`) et 12000 en course (`FUN_14017a850`, plafond 60000).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Speeds {
    pub walk: i32,
    pub run: i32,
    /// Déplacement en cours à la course (drapeau 0x10).
    pub fast: bool,
    /// Monté (`AgpmCharacter + 0xbfc`) : le plafond de 60000 ne s'applique pas (`FUN_14017a850`).
    pub riding: bool,
}

impl Default for Speeds {
    fn default() -> Self {
        Self { walk: 0, run: 0, fast: true, riding: false }
    }
}

impl Speeds {
    /// Retient les vitesses connues des facteurs.
    pub fn learn(&mut self, walk: Option<i32>, run: Option<i32>) {
        if let Some(w) = walk {
            self.walk = w;
        }
        if let Some(r) = run {
            self.run = r;
        }
    }

    /// Vitesse du déplacement en cours, en unités du monde par seconde.
    pub fn units_per_second(&self) -> f32 {
        let v = if self.fast {
            if self.run > 0 && self.riding { self.run } else if self.run > 0 { self.run.min(60_000) } else { 12_000 }
        } else if self.walk > 0 {
            self.walk
        } else {
            4_000
        };
        v as f32 / 10.0
    }
}

/// Nom du clump de modèle associé à un TID d'objet (colonne `BASE_DFF` de ItemTemplateAll.ini).
/// Partie du corps d'un objet (colonne `Part` d'ItemTemplateAll.ini).
pub fn item_part_for_tid(assets: &Assets, tid: i32) -> Option<i64> {
    let table = assets.table("ini", "ItemTemplateAll.ini").ok()?;
    let header = table.section("")?;
    let column = header.entries.iter().find(|(_, name)| name.eq_ignore_ascii_case("Part")).map(|(index, _)| index.as_str())?;
    table.section(&tid.to_string())?.get(column)?.trim().parse().ok()
}

/// Genre d'équipement d'un objet (colonne `Kind` d'ItemTemplateAll.ini) : 1 armure, 2 arme, 4 bouclier,
/// 8 anneau, 16 collier, 32 monture.
pub fn item_kind_for_tid(assets: &Assets, tid: i32) -> Option<i64> {
    let table = assets.table("ini", "ItemTemplateAll.ini").ok()?;
    let header = table.section("")?;
    let column = header.entries.iter().find(|(_, name)| name.eq_ignore_ascii_case("Kind")).map(|(index, _)| index.as_str())?;
    table.section(&tid.to_string())?.get(column)?.trim().parse().ok()
}

/// Type d'arme d'un objet (colonne `WeaponType` d'ItemTemplateAll.ini) ; `None` si ce n'est pas une
/// arme (`Kind` 2).
pub fn item_weapon_type(assets: &Assets, tid: i32) -> Option<i64> {
    let table = assets.table("ini", "ItemTemplateAll.ini").ok()?;
    let header = table.section("")?;
    let column = |n: &str| header.entries.iter().find(|(_, name)| name.eq_ignore_ascii_case(n)).map(|(index, _)| index.clone());
    let item = table.section(&tid.to_string())?;
    let num = |c: Option<String>| c.and_then(|c| item.get(&c)?.trim().parse::<i64>().ok());
    (num(column("Kind")) == Some(2)).then(|| num(column("WeaponType"))).flatten()
}

pub fn item_model_for_tid(assets: &Assets, tid: i32) -> Option<String> {
    if tid <= 0 {
        return None;
    }
    let table = assets.table("ini", "ItemTemplateAll.ini").ok()?;
    let header = table.section("")?;
    let dff_column = header
        .entries
        .iter()
        .find(|(_, name)| name.eq_ignore_ascii_case("BASE_DFF"))
        .map(|(index, _)| index.as_str())?;
    let item = table.section(&tid.to_string())?;
    let dff = item.get(dff_column)?.trim().parse::<u32>().ok()?;
    (dff > 0).then(|| format!("d{dff:07x}.ecl"))
}

/// Nom affiché de l'objet depuis la colonne `Name` de ItemTemplateAll.ini.
pub fn item_name_for_tid(assets: &Assets, tid: i32) -> Option<String> {
    if tid <= 0 {
        return None;
    }
    let table = assets.table("ini", "ItemTemplateAll.ini").ok()?;
    let header = table.section("")?;
    let name_column = header
        .entries
        .iter()
        .find(|(_, name)| name.eq_ignore_ascii_case("Name"))
        .map(|(index, _)| index.as_str())?;
    table
        .section(&tid.to_string())?
        .get(name_column)
        .map(str::to_owned)
}

/// Race et classe d'un modèle de personnage (facteurs `Race`, `Class` de `CharacterTemplatePublic.ini`) :
/// Archonia les recopie dans les facteurs du personnage à l'évolution (`FUN_14019e640`, type 2).
pub fn template_race_class(assets: &Assets, tid: i32) -> Option<(i32, i32)> {
    let table = assets.table("ini", "CharacterTemplatePublic.ini").ok()?;
    let header = table.section("")?;
    let sec = table.section(&tid.to_string())?;
    let get = |name: &str| {
        let key = header.entries.iter().find(|(_, v)| v.eq_ignore_ascii_case(name)).map(|(k, _)| k.as_str())?;
        sec.get(key)?.trim().parse::<i32>().ok()
    };
    Some((get("Race")?, get("Class")?))
}

/// `ItemEvolutionTable.txt` (`slayer | orbiter | scion | summoner`, AgcmItem `FUN_1408ad360`) : par objet du
/// Scion (colonne `scion`, la clé ; première ligne gardée), ses apparences rangées par classe comme
/// `FUN_1408ad2e0` les rend : `[scion (3), slayer (1), orbiter (2), summoner (4)]`, 0 pour une case vide.
pub fn parse_evolution_table(text: &str) -> HashMap<i32, [i32; 4]> {
    let mut out = HashMap::new();
    for line in text.lines() {
        // L'en-tête et les lignes sans nombre en tête sont ignorés (une première case vide vaut 0), comme dans
        // Archonia.
        if !line.starts_with(|c: char| c.is_ascii_digit() || c == '\t') {
            continue;
        }
        let cells: Vec<i32> = line.trim_end_matches('\r').split('\t').take(4).map(|c| c.trim().parse::<i32>().unwrap_or(0)).collect();
        let [slayer, orbiter, scion, summoner] = [0, 1, 2, 3].map(|i| cells.get(i).copied().unwrap_or(0));
        out.entry(scion).or_insert([scion, slayer, orbiter, summoner]);
    }
    out
}

/// Apparence d'un objet porté par un personnage DragonScion (race 4) de classe `class` (`FUN_140751d70`) :
/// l'objet de sa forme dans `ItemEvolutionTable.txt`, sinon l'objet lui-même (case vide ou objet absent).
pub fn evolution_item(table: &HashMap<i32, [i32; 4]>, class: i32, item_tid: i32) -> i32 {
    let slot = match class {
        3 => 0,
        1 => 1,
        2 => 2,
        4 => 3,
        _ => return item_tid,
    };
    table.get(&item_tid).map(|row| row[slot]).filter(|t| *t > 0).unwrap_or(item_tid)
}

/// Table d'évolution du client, lue une fois par dossier de jeu.
fn evolution_table(assets: &Assets) -> Arc<HashMap<i32, [i32; 4]>> {
    static CACHE: std::sync::Mutex<Option<(Option<std::path::PathBuf>, Arc<HashMap<i32, [i32; 4]>>)>> = std::sync::Mutex::new(None);
    let mut cache = CACHE.lock().unwrap();
    if let Some((_, table)) = cache.as_ref().filter(|(d, _)| *d == assets.game_dir) {
        return table.clone();
    }
    let text = assets.read_file("ini/itemevolutiontable.txt").map(|raw| {
        let cp = archrust_data::ini::CodePage::for_dir("ini");
        let plain = raw.iter().take(256).filter(|&&b| b < 0x20 && b != b'\r' && b != b'\n' && b != b'\t').count() * 20 < raw.len().min(256);
        if plain { archrust_data::ini::decode_text(&raw, cp) } else { archrust_data::ini::decrypt_ini_with(&raw, cp) }
    });
    let table = Arc::new(text.map(|t| parse_evolution_table(&t)).unwrap_or_default());
    log::info!("ItemEvolutionTable : {} objet(s) du Scion", table.len());
    *cache = Some((assets.game_dir.clone(), table.clone()));
    table
}

/// Charge le corps du personnage puis les modèles statiques des objets dans sa vue réseau.
pub fn entity_meshes_with_equipment(
    renderer: &mut Renderer,
    assets: &Assets,
    tid: i32,
    head: Head,
    equipment: &[EquipmentView],
    fallback: &Arc<Mesh>,
) -> Result<Vec<(Arc<Mesh>, Mat4)>> {
    entity_meshes_with_head(renderer, assets, tid, Some(head), equipment, fallback)
}

/// Comme `entity_meshes_with_equipment` ; `head` à `None` : ni visage ni coiffure (personnage métamorphosé,
/// `FUN_1404b48d0` d'Archonia ne les pose pas, voir `app_morph.rs`).
pub fn entity_meshes_with_head(
    renderer: &mut Renderer,
    assets: &Assets,
    tid: i32,
    head: Option<Head>,
    equipment: &[EquipmentView],
    fallback: &Arc<Mesh>,
) -> Result<Vec<(Arc<Mesh>, Mat4)>> {
    let mut meshes = entity_pieces(renderer, assets, tid, head, fallback)?;
    // Squelette du porteur (corps) : mains des armes, peau du Xenon.
    let skeleton = meshes.iter().find_map(|(m, _, _)| m.skin.as_ref().map(|s| s.clump.clone()));
    // Summoner (race 4, classe 4) : seuls les objets de ses ensembles d'avatar (`worn::summoner_items`).
    let summoner = (template_race_class(assets, tid) == Some((4, 4))).then(|| crate::worn::summoner_items(assets));
    let equipment: Vec<EquipmentView> = equipment.iter().filter(|e| summoner.as_ref().is_none_or(|set| set.contains(&e.template_id))).cloned().collect();
    // DragonScion : les objets portés prennent l'apparence de la forme courante (AgcmItem, rappels 0x79 / 0x7B
    // d'AgpmCharacter à l'évolution et à son retour ; classe du modèle courant).
    let equipment: Vec<EquipmentView> = match template_race_class(assets, tid) {
        Some((4, class)) => {
            let table = evolution_table(assets);
            equipment.iter().map(|e| EquipmentView { template_id: evolution_item(&table, class, e.template_id), ..e.clone() }).collect()
        }
        _ => equipment.to_vec(),
    };
    for item in &equipment {
        // Monture (partie 14) : un personnage à part (voir `app_ride.rs`).
        if item_part_for_tid(assets, item.template_id) == Some(14) {
            continue;
        }
        // Armes, boucliers, lances : dans la main du squelette (voir `worn`) ; la lance n'est dessinée
        // qu'à cheval, les mains qu'à pied (filtre au tracé, `worn::piece_model`).
        let look = crate::worn::item_look(assets, item.template_id).filter(|l| matches!(l.kind, crate::worn::KIND_WEAPON | crate::worn::KIND_SHIELD));
        if let Some(look) = look {
            if let Some(skeleton) = &skeleton {
                meshes.extend(crate::worn::worn_pieces(renderer, assets, &look, tid, skeleton).into_iter().map(|(m, t)| (m, t, None)));
            }
            continue;
        }
        if item_part_for_tid(assets, item.template_id) == Some(15) {
            continue;
        }
        // Anneau (genre 8) et collier (genre 0x10) : jamais dessinés sur le personnage. Leur `BASE_DFF`
        // (`d00000c4.ecl`, un coffret) n'est que leur modèle au sol. Archonia (AgcmItem::UnEquipItem,
        // FUN_1407566c0) ne traite un collier que par ses effets attachés (FUN_1407538d0 : capes et
        // ailes serties, par AgcmEventEffect).
        if matches!(item_kind_for_tid(assets, item.template_id), Some(8 | 16)) {
            continue;
        }
        let Some(file) = item_model_for_tid(assets, item.template_id) else {
            continue;
        };
        let pieces = match model_pieces(renderer, assets, "character", &file) {
            Ok(p) => p,
            Err(error) => {
                log::debug!("objet TID {} (modèle {file}) : {error:#}", item.template_id);
                continue;
            }
        };
        // Une armure (corps, mains, jambes, pieds) partage son modèle avec toute sa panoplie : comme
        // Archonia, seule la pièce de sa partie est gardée, et elle remplace la pièce de cette partie
        // (vêtement par défaut ou autre objet). Tout ajouter superposait les pièces, qui clignotaient.
        let part = item_part_for_tid(assets, item.template_id).filter(|p| matches!(p, 2 | 5 | 6 | 7));
        let (kept, others): (Vec<_>, Vec<_>) = match part {
            Some(p) => pieces.into_iter().partition(|(_, _, q)| *q == Some(p)),
            None => (Vec::new(), pieces),
        };
        if let (Some(p), false) = (part, kept.is_empty()) {
            meshes.retain(|(_, _, q)| *q != Some(p));
            // Pièce marquée de son objet : lueur de l'armure sertie (`app_stones.rs`).
            meshes.extend(kept.into_iter().map(|(m, t, q)| (with_item(m, item.template_id), t, q)));
        } else {
            meshes.extend(kept.into_iter().chain(others));
        }
    }
    Ok(meshes.into_iter().map(|(m, t, _)| (m, t)).collect())
}

/// Maillage marqué du TID de l'objet porté dont il vient (`Mesh::item_tid`) ; un maillage partagé
/// (modèle de patch en cache) reste tel quel.
fn with_item(mesh: Arc<Mesh>, tid: i32) -> Arc<Mesh> {
    match Arc::try_unwrap(mesh) {
        Ok(mut m) => {
            m.item_tid = Some(tid);
            Arc::new(m)
        }
        Err(shared) => shared,
    }
}

/// Angle (degrés) qui tourne un personnage de `from` vers `to`, comme `FUN_14016bf60`
/// d'Archonia : `atan(dx / dz)` corrigé par quadrant, soit `atan2(dx, dz)`. Les modèles regardent
/// vers +Z ; l'angle est appliqué par `D3DXMatrixRotationY(turnY × π / 180)` (`FUN_14016a070`),
/// qui envoie +Z sur (sin, 0, cos) comme `Mat4::from_rotation_y`. `None` sans écart horizontal
/// (le client garde alors l'orientation précédente).
pub fn turn_towards(from: Vec3, to: Vec3) -> Option<f32> {
    let (dx, dz) = (to.x - from.x, to.z - from.z);
    (dx != 0.0 || dz != 0.0).then(|| dx.atan2(dz).to_degrees())
}

impl Entity {
    pub fn model_matrix(&self) -> Mat4 {
        Mat4::from_translation(self.position) * Mat4::from_rotation_y(self.turn_y.to_radians())
    }
}

/// Visage et cheveux choisis pour un personnage (indices dans `charactertemplatecustomize.ini`),
/// fournis par le paquet `CHARACTER` (champs 20/21) ; 0 = premier visage / première coiffure.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Head {
    pub face: i32,
    pub hair: i32,
}

/// Nombres de visages et de coiffures par défaut d'un TID (`DEFAULT_FACE_NUM`,
/// `DEFAULT_HAIR_NUM` de `charactertemplatecustomize.ini`).
pub fn head_counts(assets: &Assets, tid: i32) -> (i32, i32) {
    let Ok(table) = assets.table("ini", "charactertemplatecustomize.ini") else {
        return (0, 0);
    };
    let (Some(header), Some(sec)) = (table.section(""), table.section(&tid.to_string())) else {
        return (0, 0);
    };
    let count = |name: &str| {
        header
            .entries
            .iter()
            .find(|(_, v)| v.eq_ignore_ascii_case(name))
            .and_then(|(k, _)| sec.get(k))
            .and_then(|v| v.trim().parse::<i32>().ok())
            .unwrap_or(0)
    };
    (count("DEFAULT_FACE_NUM"), count("DEFAULT_HAIR_NUM"))
}

/// Fichiers de la tête par défaut d'un TID : `(dossier, fichier)` du visage puis des cheveux.
///
/// Archonia (`FUN_14074fc70` visages, `FUN_14074ff40` cheveux) range dans le modèle du TID la
/// liste `DEFAULT_FACE…` / `DEFAULT_HAIR…` de `charactertemplatecustomize.ini` (`DEFAULT_FACE_NUM`,
/// `DEFAULT_HAIR_NUM` entrées), mais charge depuis les packs `characterdefaultheadace` et
/// `…hair` des fichiers nommés `"%s%.*x.%s"` = `V%07x.ecl` : `TID × 0x200 + i` pour le visage i,
/// `TID × 0x100 + i` pour la coiffure i. Vérifié pour le TID 96 : 24 visages `v000c000…v000c017`,
/// 58 coiffures `v0006000…v0006039`, autant que les compteurs de la table. Un indice hors liste
/// retombe sur 0.
pub fn default_head_files(assets: &Assets, tid: i32, head: Head) -> Vec<(&'static str, String)> {
    let Ok(table) = assets.table("ini", "charactertemplatecustomize.ini") else {
        return Vec::new();
    };
    let (Some(header), Some(sec)) = (table.section(""), table.section(&tid.to_string())) else {
        return Vec::new();
    };
    let count = |name: &str| {
        header
            .entries
            .iter()
            .find(|(_, v)| v.eq_ignore_ascii_case(name))
            .and_then(|(k, _)| sec.get(k))
            .and_then(|v| v.trim().parse::<i32>().ok())
            .unwrap_or(0)
    };
    let mut out = Vec::new();
    for (dir, column, index, stride) in [
        (
            "character/defaulthead/face",
            "DEFAULT_FACE_NUM",
            head.face,
            0x200,
        ),
        (
            "character/defaulthead/hair",
            "DEFAULT_HAIR_NUM",
            head.hair,
            0x100,
        ),
    ] {
        let n = count(column);
        if n <= 0 {
            continue;
        }
        let i = if (0..n).contains(&index) { index } else { 0 };
        out.push((dir, format!("v{:07x}.ecl", tid * stride + i)));
    }
    out
}

/// Fichiers de modèle d'un personnage d'après `charactertemplateclient.ini`.
///
/// La table est positionnelle : la section d'en-tête `[]` nomme les colonnes (`0=ModuleData1`,
/// `1=DFF`, `2=DEFAULT_ARMOUR_DFF`, `3=PICK_DFF`…) et chaque section `[TID]` donne
/// `colonne=valeur`. Les numéros sont écrits en **hexadécimal** sur 7 chiffres dans le pack
/// `character` : `a%07x` = squelette et corps (DFF), `b%07x` = armure par défaut
/// (DEFAULT_ARMOUR_DFF), `c%07x` = volume de sélection (PICK_DFF). Vérifié : TID 584 →
/// `a0000248.ecl`, TID 96 → `a0000060.ecl` + `b0000060.ecl`.
pub fn models_for_tid(assets: &Assets, tid: i32) -> Vec<String> {
    let Ok(table) = assets.table("ini", "charactertemplateclient.ini") else {
        return Vec::new();
    };
    let Some(header) = table.section("") else {
        return fallback_models(assets, tid);
    };
    let col = |name: &str| {
        header
            .entries
            .iter()
            .find(|(_, v)| v.eq_ignore_ascii_case(name))
            .map(|(k, _)| k.clone())
    };
    let Some(sec) = table.section(&tid.to_string()) else {
        return fallback_models(assets, tid);
    };
    let mut out = Vec::new();
    for (column, prefix) in [("DFF", 'a'), ("DEFAULT_ARMOUR_DFF", 'b')] {
        let Some(key) = col(column) else { continue };
        if let Some(n) = sec.get(&key).and_then(|v| v.trim().parse::<u32>().ok()) {
            out.push(format!("{prefix}{n:07x}.ecl"));
        }
    }
    out
}

/// Sans client, la table est vide : on suppose que le numéro de DFF est le TID lui-même, ce qui
/// est vrai pour les modèles vérifiés (96 → `a0000060`, 584 → `a0000248`). Les patchs peuvent
/// ainsi viser `character/a{tid:07x}.ecl` en mode `--no-client`. Avec un client, un TID absent de
/// la table reste sans modèle.
fn fallback_models(assets: &Assets, tid: i32) -> Vec<String> {
    if assets.has_client() || tid <= 0 {
        return Vec::new();
    }
    vec![format!("a{tid:07x}.ecl")]
}

/// Charge (ou fabrique) les maillages d'une entité : corps, armure par défaut, puis visage et
/// cheveux par défaut. Les clumps de tête portent leur propre copie du squelette (88 frames,
/// 86 nœuds HAnim, peau à 86 os pour le TID 96) : ils se déforment avec le même clip que le corps.
pub fn entity_meshes(
    renderer: &mut Renderer,
    assets: &Assets,
    tid: i32,
    head: Head,
    fallback: &Arc<Mesh>,
) -> Result<Vec<(Arc<Mesh>, Mat4)>> {
    Ok(entity_pieces(renderer, assets, tid, Some(head), fallback)?.into_iter().map(|(m, t, _)| (m, t)).collect())
}

/// Comme `entity_meshes`, avec la partie du corps de chaque pièce (vêtements par défaut) ; sans tête pour `None`.
fn entity_pieces(
    renderer: &mut Renderer,
    assets: &Assets,
    tid: i32,
    head: Option<Head>,
    fallback: &Arc<Mesh>,
) -> Result<Vec<(Arc<Mesh>, Mat4, Option<i64>)>> {
    let files = models_for_tid(assets, tid);
    if files.is_empty() {
        log::warn!("TID {tid} : aucun modèle dans charactertemplateclient.ini");
    }
    let mut meshes = Vec::new();
    for file in &files {
        match model_pieces(renderer, assets, "character", file) {
            Ok(part) => {
                if part.is_empty() {
                    log::debug!("TID {tid} : {file} sans géométrie");
                }
                meshes.extend(part);
            }
            Err(e) if !assets.has_client() => log::debug!("TID {tid} : {file} : {e:#} → substitut"),
            Err(e) => log::warn!("TID {tid} : {file} : {e:#}"),
        }
    }
    if let (false, Some(head)) = (meshes.is_empty(), head) {
        for (dir, file) in default_head_files(assets, tid, head) {
            match model_pieces(renderer, assets, dir, &file) {
                Ok(part) => meshes.extend(part.into_iter().map(|(m, t, _)| (m, t, None))),
                Err(e) => log::warn!("TID {tid} : tête {dir}/{file} : {e:#}"),
            }
        }
    }
    if meshes.is_empty() {
        meshes.push((fallback.clone(), Mat4::IDENTITY, None));
    }
    Ok(meshes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apparence_des_objets_a_l_evolution() {
        // En-tête, ligne complète, case slayer vide (Xenon) : clé = colonne scion.
        let t = parse_evolution_table("slayer\torbiter\tscion\tsummoner\r\n6661\t6686\t7829\t\r\n\t6696\t7900\t8276\r\n");
        assert_eq!(t.len(), 2);
        assert_eq!(evolution_item(&t, 1, 7829), 6661, "Slayer");
        assert_eq!(evolution_item(&t, 2, 7829), 6686, "Orbiter");
        assert_eq!(evolution_item(&t, 4, 7829), 7829, "case vide : l'objet lui-même");
        assert_eq!(evolution_item(&t, 3, 7829), 7829, "Scion");
        assert_eq!(evolution_item(&t, 1, 7900), 7900, "Xenon sans apparence Slayer");
        assert_eq!(evolution_item(&t, 4, 7900), 8276);
        assert_eq!(evolution_item(&t, 1, 1), 1, "objet absent de la table");
    }

    #[test]
    fn vitesses_comme_archonia() {
        let mut s = Speeds::default();
        assert_eq!(s.units_per_second(), 1200.0, "course inconnue : 12000");
        s.fast = false;
        assert_eq!(s.units_per_second(), 400.0, "marche inconnue : 4000");
        s.learn(Some(2000), Some(5000));
        assert_eq!(s.units_per_second(), 200.0, "Movement=2000");
        s.fast = true;
        assert_eq!(s.units_per_second(), 500.0, "MovementFast=5000");
        s.learn(None, Some(90_000));
        assert_eq!(s.units_per_second(), 6000.0, "plafond 60000");
        s.riding = true;
        assert_eq!(s.units_per_second(), 9000.0, "à cheval : sans plafond");
    }

    #[test]
    fn turn_follows_archonia_convention() {
        // FUN_14016bf60 : atan2(dx, dz) en degrés ; l'avant du modèle (+Z) suit la destination.
        let o = Vec3::ZERO;
        assert_eq!(turn_towards(o, Vec3::new(0.0, 0.0, 10.0)), Some(0.0));
        assert_eq!(turn_towards(o, Vec3::new(10.0, 5.0, 0.0)), Some(90.0));
        assert_eq!(turn_towards(o, Vec3::new(-10.0, 0.0, 0.0)), Some(-90.0));
        assert_eq!(turn_towards(o, Vec3::new(0.0, 0.0, -10.0)), Some(180.0));
        assert_eq!(turn_towards(o, Vec3::new(0.0, 7.0, 0.0)), None);
        for to in [Vec3::new(3.0, 0.0, 4.0), Vec3::new(-5.0, 0.0, -1.0)] {
            let turn = turn_towards(o, to).unwrap();
            let forward = Mat4::from_rotation_y(turn.to_radians()).transform_vector3(Vec3::Z);
            assert!((forward - to.normalize()).length() < 1e-5);
        }
    }
}
