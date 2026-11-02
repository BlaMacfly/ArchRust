//! Animation squelettique des personnages, d'après le client Archonia.
//!
//! **Choix des animations** — `ini/charactertemplateanimation.ini`, lu par Archonia.exe dans
//! `FUN_14050b1b0` (sha256 e9c7609b…) : chaque section `[TID]` est positionnelle (l'en-tête `[]`
//! nomme les colonnes, ex. `2=ANIMATION_NAME000`) et chaque valeur `ANIMATION_NAME…` vaut
//! `type:équipement:variante:fichier` (`sscanf "%d:%d:%d:%s"`). Le client range le fichier dans
//! une table `[type][équipement]`, la variante désignant une entrée de la liste ;
//! `ANIMATION_ACTIVE_RATE…` (`%d:%d:%d:%d`) donne le taux de tirage d'une variante. L'équipement est
//! converti par `FUN_1404addf0` selon `ANIMATION_TYPE2` ; 0 = sans arme. Les types 1 et 2 reçoivent
//! d'office le drapeau 1 (boucle).
//!
//! Sens des types, **déduit des données** (amplitude des clips de TID 96) : 0 = attente
//! (`a0060000` respire, 4° au plus ; `a0060002`, tiré à 20 %, est une attente animée de 7 s),
//! 1 = marche, 2 = course (`a0060200`, cycle de 0,8 s, membres jusqu'à 100°), 3 = attaque
//! (avec `ANIMATION_POINT`), 4 = coup reçu, 5 = mort.
//!
//! **Pose** — un clip `.ean` contient des clés HAnim standard (préfixe : version 0x100, type 1,
//! nombre de clés, drapeaux, durée en secondes). Les `n` premières clés ouvrent les pistes des `n`
//! nœuds de la hiérarchie, dans l'ordre du tableau de nœuds ; chaque clé suivante désigne la
//! précédente de sa piste par décalage en octets (`indice × 36`). Le quaternion s'applique tel quel
//! (convention glam), vérifié par la déformation : le conjugué écarte le torse de 140 unités.
//! La déformation suit RpSkin : voir `Clump::skin_matrices`.

use crate::assets::Assets;
use crate::render::{Mesh, Renderer, Vertex};
use archrust_data::ean::{EanFile, HAnimSample};
use archrust_data::rw::Clump;
use glam::{Mat3, Quat, Vec3};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Type d'animation d'Archonia (premier nombre de `ANIMATION_NAME`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimType {
    Wait = 0,
    Walk = 1,
    Run = 2,
    Attack = 3,
    Hit = 4,
    Death = 5,
}

/// Type interne d'un clip d'action sociale (hors des types d'Archonia : une monture ne le suit pas).
pub const SOCIAL_KIND: u32 = 100;

/// Géométrie skinnée d'un clump du client, liée à son maillage GPU.
pub struct SkinBinding {
    pub clump: Arc<Clump>,
    pub geometry: usize,
    /// Sommets de la pose de bind (espace de la frame de l'atomic).
    pub base: Vec<Vertex>,
    /// Squelette du porteur auquel la peau est liée par ID de nœud (arme skinnée : Xenon, voir
    /// `Clump::skin_matrices_on`) ; `None` : la hiérarchie du clump lui-même (corps, armures, têtes).
    pub wearer: Option<Arc<Clump>>,
}

/// Clip `.ean` prêt à échantillonner : pistes triées par temps, une par nœud HAnim.
pub struct Clip {
    pub name: String,
    pub duration: f32,
    tracks: Vec<Vec<(f32, Quat, Vec3)>>,
}

impl Clip {
    pub fn from_ean(name: &str, ean: &EanFile) -> Option<Clip> {
        let chunk = ean.chunks.iter().find(|c| c.id == 0x1b)?;
        let layout = chunk.animation_layout()?;
        let keys: Vec<_> = chunk.h_anim_keys()?.collect();
        let mut tracks: Vec<Vec<(f32, Quat, Vec3)>> = chunk
            .linked_tracks()?
            .into_iter()
            .map(|t| {
                t.into_iter()
                    .map(|i| {
                        let k = keys[i];
                        let q = Quat::from_array(k.rotation);
                        let q = if q.is_finite() && q.length_squared() > 1e-12 {
                            q.normalize()
                        } else {
                            Quat::IDENTITY
                        };
                        (k.time, q, Vec3::from_array(k.translation))
                    })
                    .collect()
            })
            .collect();
        for t in &mut tracks {
            t.sort_by(|a, b| a.0.total_cmp(&b.0));
        }
        let declared = f32::from_bits(layout.prefix[4]);
        let last = keys
            .iter()
            .map(|k| k.time)
            .filter(|t| t.is_finite())
            .fold(0.0, f32::max);
        let duration = if declared.is_finite() && declared > 0.0 {
            declared
        } else {
            last
        };
        Some(Clip {
            name: name.to_string(),
            duration: duration.max(1e-3),
            tracks,
        })
    }

    /// Pose à l'instant `time` (secondes, ramené dans la durée du clip).
    pub fn sample(&self, time: f32) -> Vec<HAnimSample> {
        let t = time.rem_euclid(self.duration);
        self.tracks
            .iter()
            .map(|track| {
                let upper = track.partition_point(|k| k.0 <= t);
                let (q, p) = if upper == 0 {
                    (track[0].1, track[0].2)
                } else if upper == track.len() {
                    (track[upper - 1].1, track[upper - 1].2)
                } else {
                    let (a, b) = (track[upper - 1], track[upper]);
                    let span = b.0 - a.0;
                    let f = if span > 0.0 {
                        ((t - a.0) / span).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    (a.1.slerp(b.1, f).normalize(), a.2.lerp(b.2, f))
                };
                HAnimSample {
                    rotation: q.to_array(),
                    translation: p.to_array(),
                }
            })
            .collect()
    }
}

/// Variante d'animation : fichier et taux de tirage (`ANIMATION_ACTIVE_RATE`, 0 si absent).
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub file: String,
    pub rate: u32,
}

/// Animations d'un TID pour un ensemble d'arme (`AgcdCharacter+0xfc`, voir `worn::anim_equip`),
/// par type, avec le repli d'Archonia déjà appliqué (voir [`AnimationSet::for_tid`]).
#[derive(Debug, Clone, Default)]
pub struct AnimationSet {
    pub by_type: HashMap<u32, Vec<Variant>>,
}

impl AnimationSet {
    /// Lit la section `[tid]` de `charactertemplateanimation.ini` pour l'ensemble `equip`, avec le
    /// repli de `FUN_1404be330` : clip `[type][ensemble]` ; absent, un joueur (`player`) qui marche
    /// (type 1) prend `[2][ensemble]` puis `[2][0]`, sinon `[type][0]` ; absent, rien (l'animation
    /// courante continue). Un ensemble négatif (−1) ne lance aucune animation : table vide.
    pub fn for_tid(assets: &Assets, tid: i32, equip: i32, player: bool) -> AnimationSet {
        if equip < 0 {
            return AnimationSet::default();
        }
        let own = Self::for_equip(assets, tid, equip as u32);
        let base = if equip == 0 { own.clone() } else { Self::for_equip(assets, tid, 0) };
        Self::resolve(&own, &base, player)
    }

    /// Repli de `FUN_1404be330` entre l'ensemble choisi (`own`) et l'ensemble 0 (`base`).
    pub fn resolve(own: &AnimationSet, base: &AnimationSet, player: bool) -> AnimationSet {
        let mut types: Vec<u32> = own.by_type.keys().chain(base.by_type.keys()).copied().collect();
        if player {
            types.push(AnimType::Walk as u32);
        }
        types.sort();
        types.dedup();
        let mut set = AnimationSet::default();
        for t in types {
            let walk = player && t == AnimType::Walk as u32;
            let run = AnimType::Run as u32;
            let found = own.by_type.get(&t).or_else(|| if walk { own.by_type.get(&run).or_else(|| base.by_type.get(&run)) } else { base.by_type.get(&t) });
            if let Some(list) = found {
                set.by_type.insert(t, list.clone());
            }
        }
        set
    }

    fn for_equip(assets: &Assets, tid: i32, equip: u32) -> AnimationSet {
        let mut set = AnimationSet::default();
        let Ok(table) = assets.table("ini", "charactertemplateanimation.ini") else {
            return set;
        };
        let (Some(header), Some(section)) = (table.section(""), table.section(&tid.to_string()))
        else {
            return set;
        };
        let column = |key: &str| header.get(key).unwrap_or("");
        let mut rates: HashMap<(u32, u32), u32> = HashMap::new();
        let mut names: Vec<(u32, u32, String)> = Vec::new();
        for (key, value) in &section.entries {
            let col = column(key);
            let mut it = value.splitn(4, ':');
            let (Some(t), Some(e), Some(v), Some(rest)) =
                (it.next(), it.next(), it.next(), it.next())
            else {
                continue;
            };
            let (Ok(t), Ok(e), Ok(v)) = (
                t.trim().parse::<u32>(),
                e.trim().parse::<u32>(),
                v.trim().parse::<u32>(),
            ) else {
                continue;
            };
            if e != equip {
                continue;
            }
            if col.starts_with("ANIMATION_NAME") {
                if !rest.trim().is_empty() && rest.trim() != "0" {
                    names.push((t, v, rest.trim().to_ascii_lowercase()));
                }
            } else if col.starts_with("ANIMATION_ACTIVE_RATE") {
                if let Ok(r) = rest.trim().parse::<u32>() {
                    rates.insert((t, v), r);
                }
            }
        }
        names.sort();
        for (t, v, file) in names {
            let rate = rates.get(&(t, v)).copied().unwrap_or(0);
            set.by_type.entry(t).or_default().push(Variant {
                file: format!("{file}.ean"),
                rate,
            });
        }
        set
    }

    /// Variante à jouer pour un tirage `roll` ∈ [0, 1) : pondérée par les taux, sinon la première.
    pub fn pick(&self, kind: u32, roll: f32) -> Option<&Variant> {
        let list = self.by_type.get(&kind)?;
        let total: u32 = list.iter().map(|v| v.rate).sum();
        if total == 0 {
            return list.first();
        }
        let mut target = roll.clamp(0.0, 0.999_999) * total as f32;
        for v in list {
            if v.rate == 0 {
                continue;
            }
            if target < v.rate as f32 {
                return Some(v);
            }
            target -= v.rate as f32;
        }
        list.iter().rev().find(|v| v.rate > 0)
    }
}

fn clip_cache() -> &'static Mutex<HashMap<String, Option<Arc<Clip>>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Arc<Clip>>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Clip `character\animation\<file>`, analysé une fois puis gardé en cache.
pub fn load_clip(assets: &Assets, file: &str) -> Option<Arc<Clip>> {
    if let Some(c) = clip_cache().lock().unwrap().get(file) {
        return c.clone();
    }
    let clip = match assets
        .read_packed("character/animation", file)
        .map_err(anyhow::Error::from)
        .and_then(|b| EanFile::parse(&b).map_err(anyhow::Error::from))
    {
        Ok(ean) => Clip::from_ean(file, &ean).map(Arc::new),
        Err(e) => {
            log::debug!("animation {file} : {e:#}");
            None
        }
    };
    clip_cache()
        .lock()
        .unwrap()
        .insert(file.to_string(), clip.clone());
    clip
}

/// Animation d'une entité : maillages skinnés, clips du TID et état courant.
pub struct Animator {
    parts: Vec<(Arc<Mesh>, Arc<SkinBinding>)>,
    set: AnimationSet,
    kind: u32,
    /// Type demandé en dernier (attente, marche, course), même sans clip pour lui : c'est le type que
    /// suit la monture (`FUN_1404be4f0`, voir [`Animator::follow`]).
    requested: u32,
    /// Dernier type reçu par [`Animator::follow`].
    followed: u32,
    clip: Option<Arc<Clip>>,
    time: f32,
    since_update: f32,
    seed: u32,
    /// Clip ponctuel en cours (attaque, coup reçu) : joué une fois, puis retour à l'attente ou à la
    /// course.
    action: bool,
    /// Mort : le clip de mort est joué une fois puis sa dernière pose est gardée.
    dead: bool,
    /// Mondes des cadres du squelette (sans cadre 0) dans la dernière pose écrite : mains des objets
    /// portés, selle des montures, nœuds des effets, à la même cadence que la peau.
    world: Option<Vec<glam::Mat4>>,
    /// Identité des maillages animés, pour reconstruire l'animateur s'ils changent.
    pub signature: usize,
    /// Ensemble d'animations (`AgcdCharacter+0xfc`, voir `worn::anim_equip`) ; −1 : aucune.
    pub equip: i32,
}

impl Animator {
    /// Animateur d'une entité, ou `None` si aucun de ses maillages n'est skinné ; `equip` est
    /// l'ensemble d'animations (`worn::anim_equip`), `player` choisit le repli des joueurs
    /// ([`AnimationSet::for_tid`]). Sans clip d'attente, le modèle reste en pose de liaison.
    pub fn new(
        assets: &Assets,
        tid: i32,
        meshes: &[(Arc<Mesh>, glam::Mat4)],
        seed: u32,
        equip: i32,
        player: bool,
    ) -> Option<Animator> {
        let parts: Vec<_> = meshes
            .iter()
            .filter_map(|(m, _)| m.skin.clone().map(|s| (m.clone(), s)))
            .collect();
        if parts.is_empty() {
            return None;
        }
        let set = AnimationSet::for_tid(assets, tid, equip, player);
        let signature = signature(meshes);
        let mut a = Animator {
            parts,
            set,
            kind: u32::MAX,
            requested: AnimType::Wait as u32,
            followed: u32::MAX,
            clip: None,
            time: 0.0,
            since_update: f32::MAX,
            seed: seed.max(1),
            action: false,
            dead: false,
            world: None,
            signature,
            equip,
        };
        a.switch(assets, AnimType::Wait as u32);
        Some(a)
    }

    fn roll(&mut self) -> f32 {
        // xorshift32 : tirage des variantes, déterministe par entité.
        let mut x = self.seed;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Lance le type `kind` (repli déjà fait par [`AnimationSet::for_tid`]) ; sans clip pour lui,
    /// l'animation courante continue (`FUN_1404be330`).
    fn switch(&mut self, assets: &Assets, kind: u32) {
        self.requested = kind;
        if kind == self.kind || !self.set.by_type.contains_key(&kind) {
            return;
        }
        self.kind = kind;
        let roll = self.roll();
        self.clip = self
            .set
            .pick(kind, roll)
            .and_then(|v| load_clip(assets, &v.file));
        self.time = 0.0;
        self.since_update = f32::MAX;
    }

    /// Avance l'animation de `dt` et réécrit les sommets si nécessaire (au plus `max_hz` fois par
    /// seconde). `moving` choisit le déplacement (la course si `fast`, drapeau 0x10, sinon la marche),
    /// sinon l'attente.
    pub fn update(
        &mut self,
        assets: &Assets,
        renderer: &Renderer,
        dt: f32,
        moving: bool,
        fast: bool,
        max_hz: f32,
    ) {
        let wanted = match (moving, fast) {
            (true, true) => AnimType::Run,
            (true, false) => AnimType::Walk,
            (false, _) => AnimType::Wait,
        } as u32;
        // Un clip ponctuel se termine à sa fin, ou dès que le personnage se remet en marche.
        if self.action
            && !self.dead
            && (moving || self.clip.as_ref().is_none_or(|c| self.time >= c.duration - 2e-3))
        {
            self.action = false;
            self.kind = u32::MAX;
        }
        if !self.action && !self.dead {
            self.switch(assets, wanted);
        }
        self.advance(assets, renderer, dt, max_hz);
    }

    /// Monture : à chaque animation lancée sur le cavalier (type `kind`, voir
    /// [`Animator::current_type`]), la même est lancée sur la monture (`FUN_1404be4f0`), sauf si elle
    /// joue déjà ce type pour l'attente, la marche ou la course ; la monture n'étant pas un joueur, un
    /// type sans clip `[type][0]` la laisse sur son clip. Puis l'animation avance comme dans `update`.
    pub fn follow(&mut self, assets: &Assets, renderer: &Renderer, dt: f32, kind: u32, max_hz: f32) {
        if kind != self.followed {
            self.followed = kind;
            let looping = kind <= AnimType::Run as u32;
            let playing = looping && self.kind == kind && !self.action && !self.dead;
            if !playing && self.set.by_type.contains_key(&kind) {
                if kind == AnimType::Death as u32 {
                    self.set_dead(assets, true, false);
                } else if looping {
                    self.dead = false;
                    self.action = false;
                    self.kind = u32::MAX;
                    self.switch(assets, kind);
                } else {
                    self.dead = false;
                    let once = if kind == AnimType::Attack as u32 { AnimType::Attack } else { AnimType::Hit };
                    let _ = self.play_once(assets, once);
                }
            }
        }
        self.advance(assets, renderer, dt, max_hz);
    }

    /// Type d'animation en cours ou demandé : mort (5), clip ponctuel (3, 4), sinon le dernier type
    /// demandé (0 attente, 1 marche, 2 course), qu'il ait un clip ou non.
    pub fn current_type(&self) -> u32 {
        if self.dead {
            AnimType::Death as u32
        } else if self.action {
            self.kind
        } else {
            self.requested
        }
    }

    fn advance(&mut self, assets: &Assets, renderer: &Renderer, dt: f32, max_hz: f32) {
        let Some(clip) = self.clip.clone() else {
            return;
        };
        self.time += dt;
        if self.dead || self.action {
            // Pose finale gardée (mort) ou clip joué une seule fois (attaque, coup reçu).
            self.time = self.time.min(clip.duration - 1e-3).max(0.0);
        }
        if self.time >= clip.duration && self.kind == AnimType::Wait as u32 {
            // Fin d'un cycle d'attente : nouveau tirage parmi les variantes, comme les taux le prévoient.
            self.time -= clip.duration;
            let roll = self.roll();
            if let Some(next) = self
                .set
                .pick(self.kind, roll)
                .and_then(|v| load_clip(assets, &v.file))
            {
                self.clip = Some(next);
            }
        }
        self.since_update += dt;
        if self.since_update < 1.0 / max_hz {
            return;
        }
        self.since_update = 0.0;
        let Some(clip) = self.clip.clone() else {
            return;
        };
        let pose = clip.sample(self.time);
        for (mesh, binding) in &self.parts {
            if let Some(vertices) = deform(binding, &pose) {
                renderer.write_vertices(mesh, &vertices);
            }
        }
        self.world = self.parts.first().map(|(_, b)| b.clump.world_matrices_with_pose_rooted(&pose));
    }

    /// Joue une fois un clip du type `kind` (3 attaque, 4 coup reçu), s'il existe pour ce modèle.
    /// Un coup reçu n'interrompt pas une attaque en cours ; rien ne se joue sur un mort.
    pub fn play_once(&mut self, assets: &Assets, kind: AnimType) -> bool {
        let kind = kind as u32;
        if self.dead || !self.set.by_type.contains_key(&kind) {
            return false;
        }
        if kind == AnimType::Hit as u32 && self.action && self.kind == AnimType::Attack as u32 {
            return false;
        }
        let roll = self.roll();
        let Some(clip) = self.set.pick(kind, roll).and_then(|v| load_clip(assets, &v.file)) else {
            return false;
        };
        // Plusieurs variantes d'attaque sans taux : on les alterne d'un coup à l'autre.
        let clip = match self.set.by_type.get(&kind) {
            Some(list) if list.len() > 1 && list.iter().all(|v| v.rate == 0) => list
                .get((roll * list.len() as f32) as usize % list.len())
                .and_then(|v| load_clip(assets, &v.file))
                .unwrap_or(clip),
            _ => clip,
        };
        self.kind = kind;
        self.clip = Some(clip);
        self.time = 0.0;
        self.since_update = f32::MAX;
        self.action = true;
        true
    }

    /// Joue une fois le clip `file` du pack `character/animation` (action sociale : colonne ANIMATION de
    /// `INI\SOCIAL_PC.txt`, fichier `.rws` au même format que les `.ean`), comme un clip ponctuel : retour à
    /// l'attente à sa fin ou dès que le personnage se déplace. Rien sur un mort ni sans le clip.
    pub fn play_file_once(&mut self, assets: &Assets, file: &str) -> bool {
        if self.dead {
            return false;
        }
        let Some(clip) = load_clip(assets, &file.trim().to_ascii_lowercase()) else {
            return false;
        };
        self.kind = SOCIAL_KIND;
        self.clip = Some(clip);
        self.time = 0.0;
        self.since_update = f32::MAX;
        self.action = true;
        true
    }

    /// Mort (clip de type 5 joué une fois, dernière pose gardée) ou retour à la vie. `at_end` pose
    /// directement la fin du clip (cadavre déjà à terre quand il entre dans la vue).
    pub fn set_dead(&mut self, assets: &Assets, dead: bool, at_end: bool) {
        if dead == self.dead {
            return;
        }
        self.dead = dead;
        self.action = false;
        self.since_update = f32::MAX;
        if dead {
            let kind = AnimType::Death as u32;
            let roll = self.roll();
            if let Some(clip) = self.set.pick(kind, roll).and_then(|v| load_clip(assets, &v.file)) {
                self.time = if at_end { clip.duration } else { 0.0 };
                self.clip = Some(clip);
                self.kind = kind;
            }
        } else {
            self.kind = u32::MAX;
            self.switch(assets, AnimType::Wait as u32);
        }
    }

    pub fn is_dead(&self) -> bool {
        self.dead
    }

    pub fn clip_name(&self) -> Option<&str> {
        self.clip.as_ref().map(|c| c.name.as_str())
    }

    /// Squelette de l'entité (clump de la première pièce skinnée : le corps).
    pub fn skeleton(&self) -> Option<&Arc<Clump>> {
        self.parts.first().map(|(_, b)| &b.clump)
    }

    /// Matrice du nœud HAnim `node_id` (repère du clump, cadre 0 à l'identité) dans la dernière pose
    /// écrite, la pose de liaison avant la première : point d'attache des effets (`NODE_ID` des
    /// données d'effet, voir `app_effects.rs`), des objets portés (101, 102) et du cavalier (20).
    pub fn node_matrix(&self, node_id: u32) -> Option<glam::Mat4> {
        let clump = self.skeleton()?;
        let frame = clump.frames.iter().position(|f| f.hanim.as_ref().is_some_and(|h| h.node_id == node_id))?;
        match &self.world {
            Some(world) => world.get(frame).copied(),
            None => clump.world_matrices_rooted().get(frame).copied(),
        }
    }
}

/// Identité d'un ensemble de maillages (adresses des `Arc`).
pub fn signature(meshes: &[(Arc<Mesh>, glam::Mat4)]) -> usize {
    meshes.iter().fold(meshes.len(), |acc, (m, _)| {
        acc.wrapping_mul(31).wrapping_add(Arc::as_ptr(m) as usize)
    })
}

/// Sommets d'une géométrie skinnée dans une pose : positions et normales mélangées par os.
pub fn deform(binding: &SkinBinding, pose: &[HAnimSample]) -> Option<Vec<Vertex>> {
    let geometry = binding.clump.geometries.get(binding.geometry)?;
    let skin = geometry.skin.as_ref()?;
    let matrices = match &binding.wearer {
        Some(wearer) => binding.clump.skin_matrices_on(binding.geometry, wearer, pose).ok()?,
        None => binding.clump.skin_matrices(binding.geometry, pose).ok()?,
    };
    if skin.indices.len() != binding.base.len() {
        return None;
    }
    let normals: Vec<Mat3> = matrices.iter().map(|m| Mat3::from_mat4(*m)).collect();
    let mut out = binding.base.clone();
    for (i, v) in out.iter_mut().enumerate() {
        let p = Vec3::from_array(binding.base[i].position);
        let n = Vec3::from_array(binding.base[i].normal);
        let (mut pp, mut nn) = (Vec3::ZERO, Vec3::ZERO);
        let mut total = 0.0;
        for lane in 0..4 {
            let w = skin.weights[i][lane];
            if w == 0.0 {
                continue;
            }
            let b = skin.indices[i][lane] as usize;
            let (Some(m), Some(r)) = (matrices.get(b), normals.get(b)) else {
                continue;
            };
            pp += m.transform_point3(p) * w;
            nn += (*r * n) * w;
            total += w;
        }
        // Poids renormalisés : 0,27 % des sommets du pack character ont une somme différente de 1
        // (jusqu'à 1,5), ce qui étirait la pièce vers l'origine ou au-delà. Une normale
        // dégénérée garde celle de la pose de bind (une normale nulle donnerait du noir).
        if total > 0.0 {
            v.position = (pp / total).to_array();
            let nn = nn.normalize_or_zero();
            if nn != Vec3::ZERO {
                v.normal = nn.to_array();
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : le chevalier (TID 96) à l'épée courte
    /// (ensemble 1) frappe avec `a0060310`, à mains nues (ensemble 0) avec `a0060300` ; la mort
    /// manquante de l'ensemble 1 vient de l'ensemble 0 ; le chevalier n'a pas de marche : un joueur
    /// prend la course, un personnage non joueur rien ; ensemble −1 : aucune animation.
    #[test]
    fn animations_selon_l_arme() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let assets = Assets::new(Some(dir.into()), &[]);
        let attack = |e| AnimationSet::for_tid(&assets, 96, e, true).pick(AnimType::Attack as u32, 0.0).map(|v| v.file.clone());
        assert_eq!(attack(0).as_deref(), Some("a0060300.ean"));
        assert_eq!(attack(1).as_deref(), Some("a0060310.ean"));
        assert!(AnimationSet::for_tid(&assets, 96, 1, true).by_type.contains_key(&(AnimType::Death as u32)));
        let walk = |player| AnimationSet::for_tid(&assets, 96, 7, player).by_type.get(&(AnimType::Walk as u32)).and_then(|v| v.first()).map(|v| v.file.clone());
        let run = AnimationSet::for_tid(&assets, 96, 7, true).by_type.get(&(AnimType::Run as u32)).and_then(|v| v.first()).map(|v| v.file.clone());
        assert!(run.is_some(), "course à cheval (ensemble 7)");
        assert_eq!(walk(true), run, "joueur à cheval qui marche : la course");
        assert_eq!(walk(false), None);
        assert!(AnimationSet::for_tid(&assets, 96, -1, true).by_type.is_empty());
    }

    /// `FUN_1404be330` : `[type][ensemble]`, puis pour un joueur qui marche `[2][ensemble]` et `[2][0]`,
    /// sinon `[type][0]`.
    #[test]
    fn repli_des_clips_comme_archonia() {
        let v = |f: &str| vec![Variant { file: f.into(), rate: 0 }];
        let mut own = AnimationSet::default();
        own.by_type.insert(3, v("e3"));
        let mut base = AnimationSet::default();
        for (t, f) in [(0, "b0"), (1, "b1"), (2, "b2"), (3, "b3")] {
            base.by_type.insert(t, v(f));
        }
        let first = |s: &AnimationSet, t: u32| s.by_type.get(&t).map(|l| l[0].file.clone());
        let npc = AnimationSet::resolve(&own, &base, false);
        assert_eq!((first(&npc, 0).as_deref(), first(&npc, 1).as_deref(), first(&npc, 3).as_deref()), (Some("b0"), Some("b1"), Some("e3")));
        let pc = AnimationSet::resolve(&own, &base, true);
        assert_eq!(first(&pc, 1).as_deref(), Some("b2"), "joueur : la marche manquante de l'ensemble prend la course de l'ensemble 0");
        own.by_type.insert(2, v("e2"));
        assert_eq!(first(&AnimationSet::resolve(&own, &base, true), 1).as_deref(), Some("e2"));
        base.by_type.remove(&1);
        assert_eq!(first(&AnimationSet::resolve(&own, &base, false), 1), None, "non joueur : rien");
    }

    #[test]
    fn variants_are_weighted_by_rate() {
        let mut set = AnimationSet::default();
        set.by_type.insert(
            0,
            vec![
                Variant {
                    file: "a0060000.ean".into(),
                    rate: 80,
                },
                Variant {
                    file: "a0060001.ean".into(),
                    rate: 0,
                },
                Variant {
                    file: "a0060002.ean".into(),
                    rate: 20,
                },
            ],
        );
        assert_eq!(set.pick(0, 0.0).unwrap().file, "a0060000.ean");
        assert_eq!(set.pick(0, 0.79).unwrap().file, "a0060000.ean");
        assert_eq!(set.pick(0, 0.81).unwrap().file, "a0060002.ean");
        assert!(set.pick(2, 0.5).is_none());
    }
}
