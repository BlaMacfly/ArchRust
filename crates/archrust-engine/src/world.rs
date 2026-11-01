//! Terrain : secteurs du monde chargés autour du joueur.
//!
//! Le monde est découpé en secteurs carrés de 6400 unités ; l'indice d'un secteur est sa
//! coordonnée divisée par 6400, décalée de 400 (vérifié : `D272,272` couvre x,z ∈ [-819200, -812800]).
//! Seize secteurs par axe forment une tuile `(sx/16)*100 + sy/16` :
//!
//! * `world/b%06dx.ma2` → `D{sx},{sy}.dff` : géométrie (1536 sommets, 512 triangles, 6 jeux d'UV),
//!   matériaux multi-couches (extension `0xfffff001`) dessinés par `terrain.wgsl` ;
//! * `world/a%06dx.ma2` → `R{sx},{sy}.dff` : dictionnaire de textures du secteur (128×128 DXT1) ;
//! * `world/a%06dx.ma1` → `C{sx},{sy}.amf` : grille d'attributs 16×16 (collision, zones).
//!
//! Un patch peut remplacer un secteur entier par un glTF (`[[models]] match = "world/D272,272.dff"`),
//! posé à l'origine du secteur ; la hauteur du sol est alors tirée de ses sommets.

use crate::assets::Assets;
use crate::render::{DrawItem, Mesh, Renderer, TerrainMesh, TerrainVertex, TERRAIN_STAGES};
use crate::scene::{ensure_texture, patched_model_meshes};
use archrust_crypto::{capi_crypt, KEY_TEXTURE};
use archrust_data::magpack::MagPack;
use archrust_data::rw::{Geometry, SectorGeometry, TextureDictionary};
use glam::{Mat4, Vec3};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub const SECTOR_SIZE: f32 = 6400.0;
pub const SECTOR_OFFSET: i32 = 400;

/// Indice de secteur d'une position monde.
pub fn sector_of(x: f32, z: f32) -> (i32, i32) {
    (
        (x / SECTOR_SIZE).floor() as i32 + SECTOR_OFFSET,
        (z / SECTOR_SIZE).floor() as i32 + SECTOR_OFFSET,
    )
}

/// Numéro de tuile d'un secteur.
pub fn tile_of(sx: i32, sy: i32) -> u32 {
    ((sx / 16) * 100 + sy / 16) as u32
}

/// Origine monde (coin bas-gauche) d'un secteur.
pub fn sector_origin(sx: i32, sy: i32) -> Vec3 {
    Vec3::new(
        (sx - SECTOR_OFFSET) as f32 * SECTOR_SIZE,
        0.0,
        (sy - SECTOR_OFFSET) as f32 * SECTOR_SIZE,
    )
}

/// Cellules par secteur et taille d'une cellule (6400 / 16).
pub const CELLS: usize = 16;
pub const CELL_SIZE: f32 = SECTOR_SIZE / CELLS as f32;

/// Hauteurs aux 17 × 17 coins de cellule d'un secteur, construites à partir des sommets de sa
/// géométrie (chaque sommet est un coin de cellule).
#[derive(Debug, Clone)]
pub struct HeightField {
    pub origin: Vec3,
    grid: Vec<f32>,
    filled: Vec<bool>,
    /// Triangles du sol par cellule (vide pour un glTF de patch) : hauteur exacte, comme Archonia
    /// qui lance un rayon vertical contre la géométrie du secteur (`FUN_140528f30`).
    cell_triangles: Vec<Vec<[Vec3; 3]>>,
}

/// Hauteur du triangle `t` à la verticale de (x, z), si le point est dans sa projection XZ.
pub fn triangle_height(t: &[Vec3; 3], x: f32, z: f32) -> Option<f32> {
    let [a, b, c] = t;
    let d = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
    if d.abs() < 1e-6 {
        return None;
    }
    let l1 = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / d;
    let l2 = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / d;
    let l3 = 1.0 - l1 - l2;
    const E: f32 = -1e-4;
    (l1 >= E && l2 >= E && l3 >= E).then(|| l1 * a.y + l2 * b.y + l3 * c.y)
}

impl HeightField {
    pub fn from_vertices(sx: i32, sy: i32, vertices: &[Vec3]) -> Self {
        let origin = sector_origin(sx, sy);
        let n = CELLS + 1;
        let mut grid = vec![0f32; n * n];
        let mut filled = vec![false; n * n];
        for v in vertices {
            let i = ((v.x - origin.x) / CELL_SIZE)
                .round()
                .clamp(0.0, CELLS as f32) as usize;
            let j = ((v.z - origin.z) / CELL_SIZE)
                .round()
                .clamp(0.0, CELLS as f32) as usize;
            let k = j * n + i;
            if !filled[k] || v.y > grid[k] {
                grid[k] = v.y;
                filled[k] = true;
            }
        }
        // Les coins jamais visés prennent la hauteur d'un voisin rempli.
        let mut hf = HeightField {
            origin,
            grid,
            filled,
            cell_triangles: Vec::new(),
        };
        for k in 0..n * n {
            if !hf.filled[k] {
                let (i, j) = (k % n, k / n);
                let neighbours = [
                    (i.wrapping_sub(1), j),
                    (i + 1, j),
                    (i, j.wrapping_sub(1)),
                    (i, j + 1),
                ];
                if let Some(h) = neighbours
                    .iter()
                    .filter(|(a, b)| *a < n && *b < n)
                    .map(|(a, b)| b * n + a)
                    .find(|&m| hf.filled[m])
                    .map(|m| hf.grid[m])
                {
                    hf.grid[k] = h;
                }
            }
        }
        hf
    }

    fn corner(&self, i: usize, j: usize) -> f32 {
        self.grid[j.min(CELLS) * (CELLS + 1) + i.min(CELLS)]
    }

    /// Range les triangles du sol par cellule (celle de leur centre) : [`Self::height_at`] suit
    /// alors la géométrie dessinée au lieu d'interpoler entre les coins.
    pub fn set_triangles(&mut self, triangles: impl IntoIterator<Item = [Vec3; 3]>) {
        let mut cells = vec![Vec::new(); CELLS * CELLS];
        for t in triangles {
            let c = (t[0] + t[1] + t[2]) / 3.0;
            let i = ((c.x - self.origin.x) / CELL_SIZE).floor();
            let j = ((c.z - self.origin.z) / CELL_SIZE).floor();
            if (0.0..CELLS as f32).contains(&i) && (0.0..CELLS as f32).contains(&j) {
                cells[j as usize * CELLS + i as usize].push(t);
            }
        }
        self.cell_triangles = cells;
    }

    /// Hauteur du sol en un point du secteur : triangle de la cellule qui le contient, sinon
    /// interpolation bilinéaire des coins.
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let fx = ((x - self.origin.x) / CELL_SIZE).clamp(0.0, CELLS as f32 - 0.0001);
        let fz = ((z - self.origin.z) / CELL_SIZE).clamp(0.0, CELLS as f32 - 0.0001);
        let (i, j) = (fx.floor() as usize, fz.floor() as usize);
        if let Some(h) = self
            .cell_triangles
            .get(j * CELLS + i)
            .and_then(|ts| ts.iter().filter_map(|t| triangle_height(t, x, z)).reduce(f32::max))
        {
            return h;
        }
        let (tx, tz) = (fx - i as f32, fz - j as f32);
        let h00 = self.corner(i, j);
        let h10 = self.corner(i + 1, j);
        let h01 = self.corner(i, j + 1);
        let h11 = self.corner(i + 1, j + 1);
        let a = h00 + (h10 - h00) * tx;
        let b = h01 + (h11 - h01) * tx;
        a + (b - a) * tz
    }
}

/// Contenu affiché d'un secteur : sol du client (multi-couches) ou modèles d'un patch glTF.
enum SectorMeshes {
    Ground(Arc<TerrainMesh>),
    Models(Vec<(Arc<Mesh>, Mat4)>),
}

pub struct Terrain {
    pub radius: i32,
    sectors: HashMap<(i32, i32), SectorMeshes>,
    heights: HashMap<(i32, i32), HeightField>,
    failed: HashSet<(i32, i32)>,
    tiles_a: HashMap<u32, Option<Arc<MagPack>>>,
    tiles_b: HashMap<u32, Option<Arc<MagPack>>>,
    /// Archives d'attributs `a%06dx.ma1` (grilles `.amf`).
    tiles_attr: HashMap<u32, Option<Arc<MagPack>>>,
    /// Blocage des secteurs chargés (cellules et segments du `.amf`).
    blocking: HashMap<(i32, i32), SectorBlocking>,
    pub loads_per_frame: usize,
    pub last_center: Option<(i32, i32)>,
}

/// Blocage d'un secteur d'après son `C{sx},{sy}.amf` : cellules de 400 unités au sol bloqué et
/// segments à ne pas franchir.
#[derive(Debug, Clone, Default)]
pub struct SectorBlocking {
    pub cells: Vec<u32>,
    pub lines: Vec<[Vec3; 2]>,
}

/// Coordonnée de cellule globale (400 unités) d'une coordonnée monde, comme Archonia
/// (`x × 0,0025 + 6400` dans `FUN_140196ab0`) : secteur = cellule / 16, cellule locale = reste.
pub fn global_cell(v: f32) -> i32 {
    (v / CELL_SIZE + (SECTOR_OFFSET * CELLS as i32) as f32).floor() as i32
}

/// Recul d'Archonia devant une cellule bloquée : 0,05 cellule (`DAT_140e49bd0`).
const BLOCK_MARGIN: f32 = 0.05 * CELL_SIZE;

/// Arrêt d'un déplacement `from → to` (plan XZ) devant le segment `[a, b]` élargi de `radius`,
/// d'après `FUN_1401822a0` : le segment ne bloque que dans un sens (normale `(-dz, dx)` de
/// `a − b`) ; `None` s'il ne gêne pas.
fn clip_segment(a: Vec3, b: Vec3, radius: f32, from: Vec3, to: Vec3) -> Option<Vec3> {
    let (dx, dz) = (a.x - b.x, a.z - b.z);
    let len = (dx * dx + dz * dz).sqrt();
    if len < 1e-4 {
        return None;
    }
    let (nx, nz) = (-dz / len, dx / len);
    let (mx, mz) = (to.x - from.x, to.z - from.z);
    let dot = mx * nx + mz * nz;
    if dot > 0.0 {
        return None;
    }
    let inside = |px: f32, pz: f32| {
        (b.x - a.x) * (px - a.x) + (b.z - a.z) * (pz - a.z) >= 0.0 && (a.x - b.x) * (px - b.x) + (a.z - b.z) * (pz - b.z) >= 0.0
    };
    let dist = (from.x - a.x) * nx + (from.z - a.z) * nz;
    let t = if dot != 0.0 { -(dist - radius) / dot } else { f32::NEG_INFINITY };
    if t > 0.0 {
        if t > 1.0 {
            return None;
        }
        let (hx, hz) = (from.x + mx * t, from.z + mz * t);
        let travelled = ((hx - from.x).powi(2) + (hz - from.z).powi(2)).sqrt();
        if (mx * mx + mz * mz).sqrt() <= travelled || !inside(hx, hz) {
            return None;
        }
        Some(Vec3::new(hx, to.y, hz))
    } else {
        // Déjà contre le segment, du côté bloquant : on reste sur place si l'on est dans sa bande.
        if dist < 0.0 || dist > radius * 1.1 || !inside(from.x, from.z) {
            return None;
        }
        Some(from)
    }
}

/// Arrêt devant le cercle de rayon `radius` centré sur `c` (extrémité d'un segment,
/// `FUN_140182070`).
fn clip_circle(c: Vec3, radius: f32, from: Vec3, to: Vec3) -> Option<Vec3> {
    let (mx, mz) = (to.x - from.x, to.z - from.z);
    if (c.x - from.x) * mx + (c.z - from.z) * mz <= 0.0 {
        return None;
    }
    let (fx, fz) = (from.x - c.x, from.z - c.z);
    let a = mx * mx + mz * mz;
    let half_b = fx * mx + fz * mz;
    let disc = half_b * half_b - (fx * fx + fz * fz - radius * radius) * a;
    if disc < 0.0 || a < 1e-8 {
        return None;
    }
    let t = (-half_b - disc.sqrt()) / a;
    if t > 1.0 {
        return None;
    }
    if t < 0.0 {
        return Some(from);
    }
    Some(Vec3::new(from.x + mx * t, to.y, from.z + mz * t))
}

impl Default for Terrain {
    fn default() -> Self {
        Self::new(3)
    }
}

impl Terrain {
    pub fn new(radius: i32) -> Self {
        Self {
            radius,
            sectors: HashMap::new(),
            heights: HashMap::new(),
            failed: HashSet::new(),
            tiles_a: HashMap::new(),
            tiles_b: HashMap::new(),
            tiles_attr: HashMap::new(),
            blocking: HashMap::new(),
            loads_per_frame: 2,
            last_center: None,
        }
    }

    pub fn loaded_count(&self) -> usize {
        self.sectors.len()
    }

    fn tile(&mut self, assets: &Assets, prefix: char, tile: u32) -> Option<Arc<MagPack>> {
        let cache = if prefix == 'a' {
            &mut self.tiles_a
        } else {
            &mut self.tiles_b
        };
        if let Some(t) = cache.get(&tile) {
            return t.clone();
        }
        let loaded = match assets.magpack(prefix, tile, "ma2") {
            Ok(p) => Some(Arc::new(p)),
            Err(e) => {
                log::debug!("tuile {prefix}{tile:06}x.ma2 : {e}");
                None
            }
        };
        cache.insert(tile, loaded.clone());
        loaded
    }

    /// Géométrie `D{sx},{sy}.dff` d'un secteur du client.
    fn client_sector(&mut self, assets: &Assets, sx: i32, sy: i32) -> Option<SectorGeometry> {
        let b = self.tile(assets, 'b', tile_of(sx, sy))?;
        let name = format!("D{sx},{sy}.dff");
        let parsed = b
            .unpack_named(&name)?
            .map_err(anyhow::Error::from)
            .and_then(|plain| SectorGeometry::parse(&plain).map_err(anyhow::Error::from));
        parsed.inspect_err(|e| log::warn!("secteur {name} : {e}")).ok()
    }

    /// Sol et blocage d'un secteur du client, sans maillage ni texture (outils, tests).
    pub fn load_ground(&mut self, assets: &Assets, sx: i32, sy: i32) -> bool {
        self.load_blocking(assets, sx, sy);
        let Some(sector) = self.client_sector(assets, sx, sy) else { return false };
        self.heights.insert((sx, sy), ground_field(sx, sy, &sector.geometry));
        true
    }

    fn load_sector(
        &mut self,
        renderer: &mut Renderer,
        assets: &Assets,
        sx: i32,
        sy: i32,
    ) -> Option<SectorMeshes> {
        // 0. Blocage du secteur (`C{sx},{sy}.amf`), quel que soit le sol dessiné.
        self.load_blocking(assets, sx, sy);
        // 1. Un patch fournit un glTF pour ce secteur : posé à l'origine du secteur, hauteurs tirées de ses sommets.
        let key = format!("world/D{sx},{sy}.dff");
        if let Some(meshes) = patched_model_meshes(renderer, assets, &key) {
            let root = Mat4::from_translation(sector_origin(sx, sy));
            if let Some(Ok(pm)) = assets.patched_model(&key) {
                let pts = pm.model.world_positions(
                    root * Mat4::from_scale(Vec3::splat(if pm.scale > 0.0 {
                        pm.scale
                    } else {
                        1.0
                    })),
                );
                self.heights
                    .insert((sx, sy), HeightField::from_vertices(sx, sy, &pts));
            }
            return Some(SectorMeshes::Models(
                meshes
                    .into_iter()
                    .map(|(m, local)| (m, root * local))
                    .collect(),
            ));
        }
        // 2. Géométrie du client.
        let tile = tile_of(sx, sy);
        let sector = self.client_sector(assets, sx, sy)?;
        // Texture propre au secteur (dictionnaire R) : téléversée sous le nom du secteur.
        let r_name = format!("R{sx},{sy}");
        if renderer.texture(&r_name).is_none() {
            if let Some(a) = self.tile(assets, 'a', tile) {
                if let Some(Ok(txd_plain)) = a.unpack_named(&format!("{r_name}.dff")) {
                    let txd_plain = if txd_plain.len() > 4 && txd_plain[0] == 0x16 {
                        txd_plain
                    } else {
                        capi_crypt(KEY_TEXTURE, &txd_plain)
                    };
                    if let Ok(dict) = TextureDictionary::parse(&txd_plain) {
                        if let Some(t) = dict.textures.first() {
                            if let Ok(rgba) = t.to_rgba8() {
                                renderer.upload_texture(
                                    &r_name,
                                    t.mips[0].width,
                                    t.mips[0].height,
                                    &rgba,
                                );
                            }
                        }
                    }
                }
            }
        }
        let mut geometry = sector.geometry;
        self.heights.insert((sx, sy), ground_field(sx, sy, &geometry));
        // Un matériau sans texture de base (aucun observé depuis la lecture de l'extension
        // 0xfffff001) reçoit la texture du secteur. Elle couvre tout le secteur alors que les UV
        // sont par cellule : ce n'est qu'un repli.
        for m in geometry.materials.iter_mut() {
            if m.texture.is_none() && renderer.texture(&r_name).is_some() {
                m.texture = Some(archrust_data::rw::Texture {
                    name: r_name.clone(),
                    ..Default::default()
                });
            }
        }
        Some(SectorMeshes::Ground(Arc::new(build_terrain_mesh(
            renderer, assets, &geometry,
        ))))
    }

    /// Charge progressivement les secteurs autour de `center`, décharge les lointains.
    /// Oublie les maillages des secteurs (leurs textures sont liées au GPU à la construction) : ils se
    /// reconstruisent au fil des images suivantes, avec les textures des packs rechargés.
    pub fn rebuild_meshes(&mut self) {
        self.sectors.clear();
        self.failed.clear();
    }

    pub fn update(&mut self, renderer: &mut Renderer, assets: &Assets, center: Vec3) {
        let (cx, cy) = sector_of(center.x, center.z);
        if self.last_center != Some((cx, cy)) {
            self.last_center = Some((cx, cy));
            let r = self.radius + 2;
            self.sectors
                .retain(|&(sx, sy), _| (sx - cx).abs() <= r && (sy - cy).abs() <= r);
            self.heights
                .retain(|&(sx, sy), _| (sx - cx).abs() <= r && (sy - cy).abs() <= r);
            self.blocking
                .retain(|&(sx, sy), _| (sx - cx).abs() <= r && (sy - cy).abs() <= r);
            self.failed
                .retain(|&(sx, sy)| (sx - cx).abs() <= r && (sy - cy).abs() <= r);
        }
        let mut budget = self.loads_per_frame;
        // Du plus proche au plus lointain.
        let mut wanted: Vec<(i32, i32)> = Vec::new();
        for dy in -self.radius..=self.radius {
            for dx in -self.radius..=self.radius {
                wanted.push((cx + dx, cy + dy));
            }
        }
        wanted.sort_by_key(|&(sx, sy)| (sx - cx).abs().max((sy - cy).abs()));
        for (sx, sy) in wanted {
            if budget == 0 {
                break;
            }
            if self.sectors.contains_key(&(sx, sy)) || self.failed.contains(&(sx, sy)) {
                continue;
            }
            match self.load_sector(renderer, assets, sx, sy) {
                Some(meshes) => {
                    self.sectors.insert((sx, sy), meshes);
                }
                None => {
                    self.failed.insert((sx, sy));
                }
            }
            budget -= 1;
        }
    }

    pub fn draw_items(&self) -> Vec<DrawItem> {
        self.sectors
            .values()
            .filter_map(|s| match s {
                SectorMeshes::Models(m) => Some(m),
                SectorMeshes::Ground(_) => None,
            })
            .flatten()
            .map(|(m, model)| DrawItem {
                mesh: m.clone(),
                model: *model,
                tint: [1.0; 4],
                character: false,
            })
            .collect()
    }

    /// Secteurs de sol du client, dessinés par le pipeline multi-couches.
    pub fn ground_meshes(&self) -> Vec<Arc<TerrainMesh>> {
        self.sectors
            .values()
            .filter_map(|s| match s {
                SectorMeshes::Ground(g) => Some(g.clone()),
                SectorMeshes::Models(_) => None,
            })
            .collect()
    }

    /// Hauteur du terrain sous (x, z), si le secteur est chargé.
    pub fn height_at(&self, x: f32, z: f32) -> Option<f32> {
        self.heights
            .get(&sector_of(x, z))
            .map(|h| h.height_at(x, z))
    }

    /// Lit le blocage d'un secteur (`C{sx},{sy}.amf` de `a%06dx.ma1`), une fois.
    pub fn load_blocking(&mut self, assets: &Assets, sx: i32, sy: i32) {
        if self.blocking.contains_key(&(sx, sy)) {
            return;
        }
        let tile = tile_of(sx, sy);
        let pack = self
            .tiles_attr
            .entry(tile)
            .or_insert_with(|| assets.magpack('a', tile, "ma1").ok().map(Arc::new))
            .clone();
        let blocking = pack
            .and_then(|p| p.unpack_named(&format!("C{sx},{sy}.amf"))?.ok())
            .map(|bytes| SectorBlocking {
                cells: crate::zones::parse_amf(&bytes).unwrap_or_default(),
                lines: crate::zones::parse_amf_lines(&bytes),
            })
            .unwrap_or_default();
        self.blocking.insert((sx, sy), blocking);
    }

    /// Blocage déjà connu d'un secteur (tests).
    pub fn set_blocking(&mut self, sx: i32, sy: i32, blocking: SectorBlocking) {
        self.blocking.insert((sx, sy), blocking);
    }

    /// Cellule globale (voir [`global_cell`]) au sol bloqué ; secteur inconnu = libre.
    fn cell_blocked(&self, gx: i32, gz: i32) -> bool {
        let n = CELLS as i32;
        self.blocking
            .get(&(gx.div_euclid(n), gz.div_euclid(n)))
            .and_then(|b| b.cells.get((gz.rem_euclid(n) * n + gx.rem_euclid(n)) as usize))
            .is_some_and(|&c| crate::zones::cell_ground_blocked(c))
    }

    /// Vrai si la cellule de (x, z) est au sol bloqué : Archonia refuse alors le déplacement
    /// (`FUN_140183bd0` dans `FUN_140168070`).
    pub fn blocked(&self, x: f32, z: f32) -> bool {
        self.cell_blocked(global_cell(x), global_cell(z))
    }

    /// Point où s'arrête un déplacement `from → to` : avant la première cellule bloquée traversée
    /// (recul de 0,05 cellule, `FUN_140196ab0`), puis devant les segments des secteurs traversés
    /// élargis de `radius` (`FUN_140197780`). Départ dans une cellule bloquée : pas de mouvement.
    /// La hauteur rendue est celle de `to` ; à recalculer au point d'arrêt.
    pub fn clip_move(&self, from: Vec3, to: Vec3, radius: f32) -> Vec3 {
        let (gx0, gz0) = (global_cell(from.x), global_cell(from.z));
        if self.cell_blocked(gx0, gz0) {
            return from;
        }
        let d = Vec3::new(to.x - from.x, 0.0, to.z - from.z);
        let len = d.length();
        let mut dest = to;
        if len > 1e-3 {
            // Parcours des cellules le long du segment (Amanatides–Woo).
            let (mut gx, mut gz) = (gx0, gz0);
            let (gx1, gz1) = (global_cell(to.x), global_cell(to.z));
            let step_x = if d.x > 0.0 { 1 } else { -1 };
            let step_z = if d.z > 0.0 { 1 } else { -1 };
            let base = (SECTOR_OFFSET * CELLS as i32) as f32 * CELL_SIZE;
            let boundary = |g: i32, s: i32| (g + (s > 0) as i32) as f32 * CELL_SIZE - base;
            let t_of = |p: f32, v: f32, b: f32| if v.abs() < 1e-6 { f32::INFINITY } else { (b - p) / v };
            let mut t_x = t_of(from.x, d.x, boundary(gx, step_x));
            let mut t_z = t_of(from.z, d.z, boundary(gz, step_z));
            let dt_x = if d.x.abs() < 1e-6 { f32::INFINITY } else { CELL_SIZE / d.x.abs() };
            let dt_z = if d.z.abs() < 1e-6 { f32::INFINITY } else { CELL_SIZE / d.z.abs() };
            let margin = BLOCK_MARGIN / len;
            while (gx, gz) != (gx1, gz1) {
                let t = if t_x < t_z {
                    gx += step_x;
                    let t = t_x;
                    t_x += dt_x;
                    t
                } else {
                    gz += step_z;
                    let t = t_z;
                    t_z += dt_z;
                    t
                };
                if t > 1.0 {
                    break;
                }
                if self.cell_blocked(gx, gz) {
                    let t = (t - margin).max(0.0);
                    dest = Vec3::new(from.x + d.x * t, to.y, from.z + d.z * t);
                    break;
                }
            }
        }
        // Segments des secteurs couverts par le déplacement.
        let (a, b) = (sector_of(from.x, from.z), sector_of(dest.x, dest.z));
        for sx in a.0.min(b.0)..=a.0.max(b.0) {
            for sy in a.1.min(b.1)..=a.1.max(b.1) {
                let Some(sector) = self.blocking.get(&(sx, sy)) else { continue };
                for [p, q] in &sector.lines {
                    if let Some(hit) = clip_segment(*p, *q, radius, from, dest) {
                        dest = hit;
                    }
                    for c in [p, q] {
                        if let Some(hit) = clip_circle(*c, radius, from, dest) {
                            dest = hit;
                        }
                    }
                }
            }
        }
        dest
    }

    /// Premier point du terrain touché par un rayon (marche à pas fixe puis dichotomie).
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<Vec3> {
        let dir = dir.normalize_or_zero();
        if dir == Vec3::ZERO {
            return None;
        }
        let step = 100.0;
        let mut t = 0.0;
        let mut prev_above: Option<(f32, f32)> = None; // (t, écart au sol)
        while t <= max_dist {
            let p = origin + dir * t;
            if let Some(h) = self.height_at(p.x, p.z) {
                let gap = p.y - h;
                if gap <= 0.0 {
                    let (mut lo, mut hi) = (prev_above.map(|(t0, _)| t0).unwrap_or(t), t);
                    for _ in 0..16 {
                        let mid = (lo + hi) / 2.0;
                        let q = origin + dir * mid;
                        match self.height_at(q.x, q.z) {
                            Some(hm) if q.y - hm <= 0.0 => hi = mid,
                            _ => lo = mid,
                        }
                    }
                    let q = origin + dir * hi;
                    return Some(Vec3::new(q.x, self.height_at(q.x, q.z).unwrap_or(q.y), q.z));
                }
                prev_above = Some((t, gap));
            } else {
                prev_above = None;
            }
            t += step;
        }
        None
    }
}

/// Hauteurs d'un secteur du client : coins de cellule et triangles de sa géométrie.
fn ground_field(sx: i32, sy: i32, geometry: &Geometry) -> HeightField {
    let mut field = HeightField::from_vertices(sx, sy, &geometry.vertices);
    let vs = &geometry.vertices;
    field.set_triangles(geometry.triangles.iter().filter_map(|t| {
        let [i, j, k] = t.v.map(|i| i as usize);
        Some([*vs.get(i)?, *vs.get(j)?, *vs.get(k)?])
    }));
    field
}

/// Textures des étages d'un matériau de sol : base, puis les entrées de l'extension `0xfffff001`
/// dans l'ordre du fichier (paires masque de type 3 / couche de type 1). L'étage n lit le jeu d'UV n
/// (vérifié sur `D325,389` : les cellules à une couche ont un UV1 en pas de 0,25 — case de l'atlas
/// du masque — et un UV2 de 0 à 1 ; celles à deux couches ont en plus UV3 en pas de 0,25 et UV4 de
/// 0 à 1 ; les étages inutilisés gardent des UV par défaut en pas de 0,5).
pub fn stage_textures(material: &archrust_data::rw::Material) -> [Option<String>; TERRAIN_STAGES] {
    let mut stages: [Option<String>; TERRAIN_STAGES] = Default::default();
    stages[0] = material.texture.as_ref().map(|t| t.name.clone());
    for (i, layer) in material.layers.iter().enumerate() {
        if i + 1 >= TERRAIN_STAGES {
            log::debug!(
                "matériau de sol : {} couches, au-delà de 2 ignorées",
                material.layers.len()
            );
            break;
        }
        stages[i + 1] = layer.texture.as_ref().map(|t| t.name.clone());
    }
    stages
}

/// Maillage GPU d'un secteur du client : sommets en coordonnées monde, cinq jeux d'UV, triangles
/// regroupés par matériau (Bin Mesh s'il est présent, sinon la liste de triangles).
fn build_terrain_mesh(renderer: &mut Renderer, assets: &Assets, g: &Geometry) -> TerrainMesh {
    let n = g.vertices.len();
    let vertices: Vec<TerrainVertex> = (0..n)
        .map(|i| {
            let mut uv = [[0.0; 2]; TERRAIN_STAGES];
            for (k, set) in g.uv_sets.iter().take(TERRAIN_STAGES).enumerate() {
                uv[k] = set.get(i).copied().unwrap_or([0.0, 0.0]);
            }
            TerrainVertex {
                position: g.vertices[i].to_array(),
                normal: g.normals.get(i).copied().unwrap_or(Vec3::Y).to_array(),
                color: g
                    .colors
                    .get(i)
                    .map(|c| c.map(|v| v as f32 / 255.0))
                    .unwrap_or([1.0; 4]),
                uv,
            }
        })
        .collect();
    let mut by_material: HashMap<u32, Vec<u32>> = HashMap::new();
    if !g.meshes.is_empty() && !g.tristrip {
        for m in &g.meshes {
            by_material
                .entry(m.material)
                .or_default()
                .extend(m.indices.iter().copied().filter(|&i| (i as usize) < n));
        }
    } else {
        for t in &g.triangles {
            if t.v.iter().all(|&i| (i as usize) < n) {
                by_material
                    .entry(t.material as u32)
                    .or_default()
                    .extend(t.v.iter().map(|&i| i as u32));
            }
        }
    }
    let mut materials: Vec<_> = by_material.into_iter().collect();
    materials.sort_by_key(|(m, _)| *m);
    let mut indices = Vec::new();
    let mut parts = Vec::new();
    for (material, list) in materials {
        let stages = g
            .materials
            .get(material as usize)
            .map(stage_textures)
            .unwrap_or_default();
        for name in stages.iter().flatten() {
            ensure_texture(renderer, assets, name);
        }
        parts.push((indices.len() as u32, list.len() as u32, stages));
        indices.extend(list);
    }
    renderer.create_terrain_mesh(&vertices, &indices, parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heightfield_bilinear_and_raycast() {
        // Plan incliné : y = x - origine.x (sur 6400 unités).
        let origin = sector_origin(272, 272);
        let mut verts = Vec::new();
        for j in 0..=CELLS {
            for i in 0..=CELLS {
                verts.push(Vec3::new(
                    origin.x + i as f32 * CELL_SIZE,
                    i as f32 * CELL_SIZE,
                    origin.z + j as f32 * CELL_SIZE,
                ));
            }
        }
        let hf = HeightField::from_vertices(272, 272, &verts);
        assert!((hf.height_at(origin.x + 1000.0, origin.z + 3000.0) - 1000.0).abs() < 0.01);
        let mut t = Terrain::new(1);
        t.heights.insert((272, 272), hf);
        let hit = t
            .raycast(
                Vec3::new(origin.x + 2000.0, 5000.0, origin.z + 2000.0),
                Vec3::new(0.0, -1.0, 0.0),
                10000.0,
            )
            .unwrap();
        assert!((hit.y - 2000.0).abs() < 1.0, "{hit:?}");
        assert!(t
            .raycast(Vec3::new(0.0, 100.0, 0.0), Vec3::Y, 1000.0)
            .is_none());
    }

    #[test]
    fn mapping() {
        assert_eq!(sector_of(-819200.0, -819200.0), (272, 272));
        assert_eq!(sector_of(-812800.1, -812800.1), (272, 272));
        assert_eq!(sector_of(-475955.0, -66219.0), (325, 389));
        assert_eq!(tile_of(325, 389), 2024);
        assert_eq!(
            sector_origin(272, 272),
            Vec3::new(-819200.0, 0.0, -819200.0)
        );
    }

    #[test]
    fn triangle_heights_follow_the_drawn_ground() {
        // Une cellule pliée le long de sa diagonale : le bilinéaire se tromperait au centre.
        let o = sector_origin(272, 272);
        let mut hf = HeightField::from_vertices(272, 272, &[o]);
        let (a, b, c, d) = (o, o + Vec3::new(400.0, 0.0, 0.0), o + Vec3::new(0.0, 0.0, 400.0), o + Vec3::new(400.0, 400.0, 400.0));
        hf.set_triangles([[a, b, d], [a, d, c]]);
        assert!((hf.height_at(o.x + 300.0, o.z + 100.0) - 100.0).abs() < 1e-3);
        assert!((hf.height_at(o.x + 100.0, o.z + 300.0) - 100.0).abs() < 1e-3);
        assert!((hf.height_at(o.x + 200.0, o.z + 200.0) - 200.0).abs() < 1e-3);
    }

    /// Un pas vers une cellule au sol bloqué s'arrête 20 unités avant elle ; un départ bloqué ne
    /// bouge pas ; un segment ne bloque que de son côté.
    #[test]
    fn moves_stop_before_blocked_cells_and_lines() {
        let mut t = Terrain::new(1);
        let mut cells = vec![0x0200u32; 256];
        cells[5 * 16 + 7] = 0x0301; // cellule (7, 5) : sol bloqué
        let o = sector_origin(300, 300);
        let wall_a = o + Vec3::new(400.0, 0.0, 3000.0);
        let wall_b = o + Vec3::new(400.0, 0.0, 4000.0);
        t.set_blocking(300, 300, SectorBlocking { cells, lines: vec![[wall_a, wall_b]] });
        let from = o + Vec3::new(5.5 * 400.0, 0.0, 5.5 * 400.0);
        let to = o + Vec3::new(9.5 * 400.0, 0.0, 5.5 * 400.0);
        assert!(t.blocked(o.x + 7.5 * 400.0, o.z + 5.5 * 400.0));
        assert!(!t.blocked(from.x, from.z));
        let stop = t.clip_move(from, to, 0.0);
        assert!((stop.x - (o.x + 7.0 * 400.0 - 20.0)).abs() < 0.01, "{stop:?}");
        assert!(!t.blocked(stop.x, stop.z));
        // Trajet qui passe à côté : intact.
        let by = o + Vec3::new(9.5 * 400.0, 0.0, 6.5 * 400.0);
        assert_eq!(t.clip_move(o + Vec3::new(5.5 * 400.0, 0.0, 6.5 * 400.0), by, 0.0), by);
        // Départ dans la cellule bloquée : pas de mouvement.
        let inside = o + Vec3::new(7.5 * 400.0, 0.0, 5.5 * 400.0);
        assert_eq!(t.clip_move(inside, to, 0.0), inside);
        // Segment x = 400 (z 3000..4000), normale (-dz, dx) de a − b vers +x : arrêt en venant de +x…
        let east = o + Vec3::new(800.0, 0.0, 3500.0);
        let west = o + Vec3::new(0.0, 0.0, 3500.0);
        let hit = t.clip_move(east, west, 0.0);
        assert!((hit.x - (o.x + 400.0)).abs() < 0.01, "{hit:?}");
        // … libre en venant de -x, et au-delà de ses extrémités.
        assert_eq!(t.clip_move(west, east, 0.0), east);
        let south = o + Vec3::new(0.0, 0.0, 2000.0);
        assert_eq!(t.clip_move(o + Vec3::new(800.0, 0.0, 2000.0), south, 0.0), south);
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : autour de Tullan, un pas vers une
    /// cellule bloquée du `.amf` (rempart) s'arrête avant elle.
    #[test]
    fn tullan_blocked_cells_stop_the_walk() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let mut t = Terrain::new(1);
        assert!(t.load_ground(&assets, 343, 412));
        let o = sector_origin(343, 412);
        let cell = |i: i32, j: i32| o + Vec3::new((i as f32 + 0.5) * CELL_SIZE, 0.0, (j as f32 + 0.5) * CELL_SIZE);
        let (i, j) = (1..16)
            .flat_map(|j| (1..16).map(move |i| (i, j)))
            .find(|&(i, j)| t.blocked(cell(i, j).x, cell(i, j).z) && !t.blocked(cell(i - 1, j).x, cell(i - 1, j).z))
            .expect("une cellule bloquée à côté d'une libre");
        let stop = t.clip_move(cell(i - 1, j), cell(i, j), 0.0);
        assert!((stop.x - (o.x + i as f32 * CELL_SIZE - 20.0)).abs() < 0.01, "{stop:?}");
        assert!(!t.blocked(stop.x, stop.z));
    }
}
