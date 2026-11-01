//! Jeux d'effets du client : `effect/ini/new/efffile_set.eff` (fiches) + `efffile_var.eff` (blocs).
//!
//! Format reconstitué sur les données et sur le chargeur d'Archonia (`AgcdEffSetMng`,
//! `efffile_set.eff` lu fiche par fiche en onze `fread` consécutifs) :
//!
//! ```text
//! efffile_set.eff : u32 nombre de fiches, puis fiches de 124 octets :
//!   u32 id (EffectIndex de skyset.ini, EFFECT des tables d'objets…)
//!   char[32] nom (CP949, terminé par 0)
//!   u32 ?, u32 ?, u32 ?, 16 octets, 16 octets, 24 octets (boîte, durée…),
//!   u32 nb_bases, u32 nb_effets, u32 ?,
//!   u32 décalage, u32 taille : bloc du jeu dans efffile_var.eff
//! efffile_var.eff : blocs mis bout à bout, chacun = enregistrements des bases puis des effets
//!   du jeu, écrits par la méthode virtuelle de chaque type d'effet (tailles propres au type).
//! ```
//!
//! Seul l'effet **son** (type 6, 196 octets) est décodé ici : `u32 type = 6`, 52 octets de
//! paramètres (observés : `1` en 0x2c, `1` en 0x32, `2` en 0x38), `char[128]` nom du fichier
//! (`A_CID_D.mp3`, joué par Archonia depuis `SOUND\STREAM\` en boucle), `f32` volume (1,1).
//! Un bloc qui n'est pas une suite d'effets son est balayé à la recherche de noms `*.mp3`
//! terminés par 0 (repli, signalé par [`EffectSet::scanned`]).
//!
//! Le décodage complet ([`EffectSets::decode`], [`decode_block`]) lit textures, **dépendances**
//! ([`EffDep`] : base enfant posée sur le repère de sa base parente ou sur un nœud HAnim de son
//! modèle, [`EffectSetData::attachments`]), bases (planches, particules, traînées, objets,
//! lumières [`LightParams`], sons, cadres mobiles [`MovingFrameParams`], planches au sol) et
//! animations, ainsi que les drapeaux du jeu ([`set_flag`]).

use std::collections::HashMap;

/// Taille d'une fiche de `efffile_set.eff`.
pub const SET_RECORD_LEN: usize = 124;
/// Taille d'un enregistrement d'effet son dans `efffile_var.eff`.
pub const SOUND_RECORD_LEN: usize = 196;
/// Type d'effet « son » (`AgcdEffSet`).
pub const EFFECT_TYPE_SOUND: u32 = 6;

/// Un jeu d'effets et les sons qu'il déclenche.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectSet {
    pub id: u32,
    pub name: String,
    /// Décalage et taille du bloc dans `efffile_var.eff`.
    pub offset: u32,
    pub len: u32,
    /// Noms des fichiers son (tels qu'écrits : `A_CID_D.mp3`), dans l'ordre du bloc.
    pub sounds: Vec<String>,
    /// `true` si les sons viennent d'un balayage du bloc (effets d'autres types mêlés), `false`
    /// si le bloc n'est fait que d'effets son décodés exactement.
    pub scanned: bool,
}

/// Table des jeux d'effets, indexée par `id`.
#[derive(Debug, Default, Clone)]
pub struct EffectSets {
    sets: HashMap<u32, EffectSet>,
    /// Fiches brutes (124 octets) par `id`, pour le décodage complet ([`EffectSets::decode`]).
    records: HashMap<u32, [u8; SET_RECORD_LEN]>,
    /// Fichier de blocs (`efffile_var.eff`).
    var: std::sync::Arc<Vec<u8>>,
}

// ---------------------------------------------------------------------------------------------
// Décodage complet d'un jeu (types d'effets et animations d'`AgcmEff2`).
//
// Relevé sur Archonia.exe : `AgcdEffSetMng::bMakeEffFile` (FUN_1408db680) écrit chaque jeu par
// FUN_1408da030 : textures (FUN_1408d8890, 0x84 octets), dépendances (12 octets), puis chaque base
// par sa méthode virtuelle +0x20. Les écrivains : en-tête commun FUN_1408f8610, rendu
// (`AgcdEffRenderBase`) FUN_1408f86f0, animations FUN_1408f8790 (méthode +0x38 de chaque animation,
// en-tête FUN_1408f46f0). Le format a été validé sur les 3 428 jeux du client (fin de bloc exacte).
// ---------------------------------------------------------------------------------------------

/// Types de base (`AgcdEffBase`, ordre de la table de noms en 0x140f485b8 et du `switch` de
/// FUN_1408e75a0).
pub mod base_kind {
    pub const BOARD: u32 = 0;
    pub const PSYS: u32 = 1;
    pub const PSYS_SBH: u32 = 2;
    pub const TAIL: u32 = 3;
    pub const OBJECT: u32 = 4;
    pub const LIGHT: u32 = 5;
    pub const SOUND: u32 = 6;
    pub const MOVING_FRAME: u32 = 7;
    pub const TERRAIN_BOARD: u32 = 8;
    pub const POST_FX: u32 = 9;
}

/// Types d'animation (`AgcdEffAnim_*`, ordre des classes : Colr, TuTv, Missile, Linear, Rev, Rot,
/// RpSpline, RtAnim, Scale, ParticlePosScale).
pub mod anim_kind {
    pub const COLOR: u32 = 0;
    pub const TUTV: u32 = 1;
    pub const MISSILE: u32 = 2;
    pub const LINEAR: u32 = 3;
    pub const REVOLUTION: u32 = 4;
    pub const ROTATION: u32 = 5;
    pub const RP_SPLINE: u32 = 6;
    pub const RT_ANIM: u32 = 7;
    pub const SCALE: u32 = 8;
    pub const PARTICLE_POS_SCALE: u32 = 9;
}

/// Taille de l'en-tête commun d'une base (u32 type, char[32] titre, u32 délai, u32 durée, u32 boucle,
/// u32 drapeaux, u32 nombre d'animations).
pub const BASE_HEADER_LEN: usize = 56;
/// Partie `AgcdEffRenderBase` : u32 mélange, i32 texture, vec3 position, vec3 rotation (degrés).
pub const RENDER_LEN: usize = 32;

/// Clés d'une animation, selon son type.
#[derive(Debug, Clone, PartialEq)]
pub enum AnimKeys {
    /// Couleur `RwRGBA` (octets r, g, b, a ; le contrôle de la planche la passe en ARGB à D3D,
    /// FUN_140839610).
    Color(Vec<(u32, [u8; 4])>),
    /// Coordonnées de texture (u0, v0, u1, v1) : image d'une planche de sprites.
    TuTv(Vec<(u32, [f32; 4])>),
    /// Position (déplacement linéaire) ou échelle : vec3 par clé.
    Vec3(Vec<(u32, [f32; 3])>),
    /// Rotation autour d'un axe : angle (degrés) par clé.
    Rotation { axis: [f32; 3], keys: Vec<(u32, f32)> },
    /// Révolution : deux vec3 puis clés vec3 (sens non relevé).
    Revolution { axis: [f32; 3], extra: [f32; 3], keys: Vec<(u32, [f32; 3])> },
    /// Projectile : 8 octets (vitesse, accélération : FUN_1408f0cb0).
    Missile { speed: f32, accel: f32 },
    /// Spline ou animation RenderWare : nom de fichier.
    File(String),
    /// Échelle des particules : flottant par clé.
    Scalar(Vec<(u32, f32)>),
}

/// Une animation d'une base.
#[derive(Debug, Clone, PartialEq)]
pub struct EffAnim {
    pub kind: u32,
    pub flags: u32,
    /// Durée (ms) ; 0 = sans fin propre.
    pub life: u32,
    /// Mode de boucle (1 une fois, 2 en boucle : déduit des données, pas du code).
    pub loop_opt: u32,
    pub keys: AnimKeys,
}

/// Une base d'un jeu d'effets.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffBase {
    pub kind: u32,
    pub title: String,
    pub delay: u32,
    pub life: u32,
    pub loop_opt: u32,
    pub flags: u32,
    /// Mélange (`AgcdEffRenderBase`, `switch` de FUN_1408f8030) ; `None` sans partie de rendu.
    pub blend: Option<u32>,
    /// Indice dans les textures du jeu (-1 : aucune).
    pub texture: i32,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    /// Modèle d'un effet objet (`effect/clump/K*.ECL`) et sa couleur pré-éclairée.
    pub clump: Option<(String, u32)>,
    /// Effet son : fichier, fichier mono, nombre de boucles, volume.
    pub sound: Option<(u32, String, String, u32, f32)>,
    /// Octets propres au type (particules, traînée, lumière, cadre mobile), tels que lus.
    pub extra: Vec<u8>,
    /// Paramètres des systèmes de particules (types 1 et 2), décodés depuis `extra`.
    pub particles: Option<Particles>,
    /// Paramètres d'une traînée (type 3), décodés depuis `extra`.
    pub tail: Option<TailParams>,
    /// Lumière (type 5) et cadre mobile (type 7), décodés depuis `extra`.
    pub light: Option<LightParams>,
    pub moving_frame: Option<MovingFrameParams>,
    pub anims: Vec<EffAnim>,
}

/// Lumière (type 5, `AgcdEffLight`), 0x30 octets lus d'un bloc en mémoire +0x70 (lecteur
/// FUN_1409034a0, écrivain FUN_1409035d0, noms des champs : écrivain texte FUN_1408e7c60). Seuls
/// `light_type` et `cone_angle` sont lus à l'exécution (création FUN_140833eb0) ; position, rotation,
/// `surfaceProp` et couleur de matériau ne servent qu'aux écrivains.
#[derive(Debug, Clone, PartialEq)]
pub struct LightParams {
    /// 0x00 `LightType` : 0 point (`rpLIGHTPOINT`), 1 projecteur doux, 2 projecteur.
    pub light_type: u32,
    /// 0x04 `pos`, 0x10 `rotAngle` (inutilisés).
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    /// 0x1c `conAngle` (degrés) : `RpLightSetConeAngle`.
    pub cone_angle: f32,
    /// 0x20 `surfaceProp`, 0x2c `materialColr` RwRGBA (inutilisés).
    pub surface: [f32; 3],
    pub material: [u8; 4],
}

/// Rayon d'une lumière d'effet (DAT_140e4a14c, `RpLightSetRadius` à la création) ; l'animation
/// d'échelle le remplace par `échelle.x × 300` (FUN_1408f44e0).
pub const LIGHT_RADIUS: f32 = 300.0;

fn light_params(x: &[u8]) -> Option<LightParams> {
    Some(LightParams {
        light_type: u32_at(x, 0)?,
        pos: vec3_at(x, 4)?,
        rot: vec3_at(x, 0x10)?,
        cone_angle: f32_at(x, 0x1c)?,
        surface: vec3_at(x, 0x20)?,
        material: x.get(0x2c..0x30)?.try_into().ok()?,
    })
}

/// Cadre mobile (type 7, `AgcdEffMFrm`), six mots (lecteur FUN_140903780, écrivain texte
/// FUN_1408e7f30). **Jamais joué par Archonia** : la mise à jour (FUN_14083cbc0) ne traite que
/// `MFrmType == 1` (spline) et tue le contrôleur pour tout autre type ; or les 165 cadres mobiles
/// des données sont de type 0 (secousse), dont la fonction FUN_1409038c0 n'est appelée nulle part.
/// Décodé pour la documentation seulement.
#[derive(Debug, Clone, PartialEq)]
pub struct MovingFrameParams {
    /// 0x00 `MFrmType` : 0 secousse, 1 spline.
    pub mfrm_type: u32,
    /// 0x04 `ShakeFrm_Axis` : 0–2 axes du repère secoué, 3–5 axes du monde, 6–8 axes de la caméra,
    /// 9 direction au hasard dans le plan XZ.
    pub axis: u32,
    /// 0x08 `ShakeFrm_Whose` : 0 caméra, 1 porteur (redondant avec les drapeaux du jeu).
    pub whose: u32,
    /// 0x0c amplitude, 0x10 durée (ms), 0x14 nombre d'oscillations.
    pub amplitude: f32,
    pub duration: u32,
    pub cycle: f32,
}

fn moving_frame_params(x: &[u8]) -> Option<MovingFrameParams> {
    Some(MovingFrameParams { mfrm_type: u32_at(x, 0)?, axis: u32_at(x, 4)?, whose: u32_at(x, 8)?, amplitude: f32_at(x, 12)?, duration: u32_at(x, 16)?, cycle: f32_at(x, 20)? })
}

/// Dépendance entre deux bases d'un jeu (12 octets après les textures, rattachement
/// FUN_1408386d0) : la base enfant est posée sur le repère animé de la base parente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffDep {
    /// Indice de la base parente (16 bits bas du premier mot si `dep_flag::HANIM_NODE`).
    pub parent: u32,
    /// Avec `dep_flag::HANIM_NODE` : ID du nœud HAnim (16 bits hauts) du modèle de l'objet parent.
    pub node: Option<u32>,
    pub child: u32,
    pub flags: u32,
}

/// Drapeaux d'une dépendance (FUN_1408386d0).
pub mod dep_flag {
    /// Drapeaux de la base enfant `|= 0x80` (enfant qui n'est pas une traînée ; sens non relevé).
    pub const CHILD_0X80: u32 = 0x1;
    /// Rattachement à un nœud HAnim du modèle de l'objet parent.
    pub const HANIM_NODE: u32 = 0x2;
}

/// Drapeaux d'un jeu (fiche 0x2c, mémoire `AgcdEffSet` +0x8c, relevés dans FUN_140714b60).
pub mod set_flag {
    /// Jeu orienté vers la cible.
    pub const TO_TARGET: u32 = 0x4;
    /// Cadre mobile : repère inséré au-dessus du porteur (0x80), de la cible (0x100) ou de la
    /// caméra (0x200) ; sans effet visible dans Archonia (voir [`super::MovingFrameParams`]).
    pub const MOVING_FRAME: u32 = 0x40;
    /// Traînée à deux nœuds : bords sur les nœuds HAnim 702 (haut) et 701 (bas) du porteur.
    pub const TWO_NODE_TAIL: u32 = 0x400;
}

/// Nœuds HAnim des bords d'une traînée à deux nœuds (`RpHAnimIDGetIndex` 0x2be et 0x2bd dans
/// FUN_140714b60) : bord haut (A) et bord bas (B).
pub const TAIL_NODE_TOP: u32 = 702;
pub const TAIL_NODE_BOTTOM: u32 = 701;

/// Hauteurs d'une traînée indépendante d'un jeu sans [`set_flag::TWO_NODE_TAIL`], codées en dur
/// par FUN_140714b60 (0x428c0000, 0xc1a00000) à la place de celles des données (aucun cas livré).
pub const TAIL_DEFAULT_TOP: f32 = 70.0;
pub const TAIL_DEFAULT_BOTTOM: f32 = -20.0;

/// Paramètres d'un système de particules.
#[derive(Debug, Clone, PartialEq)]
pub enum Particles {
    /// Type 1 (`AgcdEffParticleSystem`).
    System(PsysParams),
    /// Type 2 (`AgcdEffParticleSys_SimpleBlackHole`).
    BlackHole(SbhParams),
}

/// Drapeaux de base des particules (champ « drapeaux » de l'en-tête, relevés dans Archonia :
/// émission FUN_1408f9a30 / FUN_1408fb5f0, mise à jour FUN_14083dc30, rendu FUN_14083e4a0 ; trou
/// noir : FUN_1408f9110, FUN_14083dff0, FUN_1408397b0).
pub mod psys_flag {
    /// Couleur tirée au hasard à la naissance (alpha 255), puis alpha décroissant sur la vie ; les
    /// animations de couleur sont ignorées.
    pub const RANDOM_COLOR: u32 = 0x1_0000;
    /// Repère local : particules dans le repère de l'émetteur (suivent l'effet), orientées sur ses axes.
    pub const LOCAL_FRAME: u32 = 0x4_0000;
    /// Planche face à la caméra (matrice +0x80 de la caméra des effets : droite et haut de l'écran).
    pub const BILLBOARD: u32 = 0x8_0000;
    /// Planche verticale tournée vers la caméra (matrice +0xc0 : droite RenderWare de la caméra, haut = Y).
    pub const BILLBOARD_Y: u32 = 0x10_0000;
    /// Émission sur un anneau horizontal de rayon `offset` (au lieu du décalage le long de la direction).
    pub const RING: u32 = 0x20_0000;
    /// Avec `RING` : rayon tiré entre 0 et `offset`.
    pub const RING_RANDOM_RADIUS: u32 = 0x40_0000;
    /// Plafond strict du nombre de particules vivantes (sinon les plus anciennes sont remplacées).
    pub const CAPPED: u32 = 0x80_0000;
}

/// Drapeaux de direction d'émission (`dir_flags`, FUN_1408fad00).
pub mod psys_dir {
    /// Direction tirée au hasard sur la sphère.
    pub const RANDOM: u32 = 0x1;
    /// Direction tirée dans un cône d'axe Y de demi-angle `cone` (degrés), puis inclinée vers `dir`.
    pub const CONE: u32 = 0x2;
    /// Pas de rotation de l'émetteur appliquée à la direction.
    pub const NO_FRAME: u32 = 0x40;
    /// Aucune direction : vitesse et décalage nuls.
    pub const NONE: u32 = 0x80;
}

/// Système de particules (type 1), 204 octets après la partie de rendu (lecteur FUN_1408f8f30,
/// écrivain FUN_1408fa2a0 ; décalages mémoire 0x128 à 0x204 d'`AgcdEffParticleSystem`).
#[derive(Debug, Clone, PartialEq)]
pub struct PsysParams {
    /// 0x00 : nombre maximal de particules vivantes.
    pub max: u32,
    /// 0x04 : intervalle d'émission (ms ; 0 lu comme 1 ; 30 ms si nul à l'exécution).
    pub interval: u32,
    /// 0x08 : 64 octets non lus par l'émission ni le rendu (nom de l'émetteur ?).
    pub reserved: [u8; 64],
    /// 0x48, 0x4c : vitesse initiale (unités/s) et écart (tirage uniforme ±).
    pub speed: f32,
    pub speed_var: f32,
    /// 0x50, 0x54 : décalage de départ le long de la direction (ou rayon de l'anneau) et écart.
    pub offset: f32,
    pub offset_var: f32,
    /// 0x58, 0x5c : particules par émission + `count_var × hasard[0, 1)`.
    pub count: i32,
    pub count_var: u32,
    /// 0x60, 0x64 : vitesses angulaires (degrés/s) des deux angles d'émission ; 0x68–0x74 : bornes
    /// (min, max) de chaque angle (l'angle vaut `t × vitesse` borné, `min` si min = max).
    pub angle_rate: [f32; 2],
    pub angle1: [f32; 2],
    pub angle2: [f32; 2],
    /// 0x78 : direction d'émission ; 0x84 : axe du second angle.
    pub dir: [f32; 3],
    pub axis: [f32; 3],
    /// 0x90 : demi-angle du cône (degrés, avec [`psys_dir::CONE`]).
    pub cone: f32,
    /// 0x94 : drapeaux de direction ([`psys_dir`]).
    pub dir_flags: u32,
    /// 0x98 : forme de l'émetteur (0 point, 1 boîte ±dims, 2 cercle de rayon dims.x et hauteur ±dims.y,
    /// 3 sphère de rayon dims.x) ; 0x9c : dimensions.
    pub shape: u32,
    pub dims: [f32; 3],
    /// 0xa8, 0xac : vitesse de rotation propre (degrés/s) et écart.
    pub spin: f32,
    pub spin_var: f32,
    /// 0xb0, 0xb4 : vie d'une particule (ms, 0 = sans fin) + écart × hasard[0, 1).
    pub life: u32,
    pub life_var: u32,
    /// 0xb8, 0xbc : coefficients de gravité (× (0, −980, 0)) et de vent (× (1, 0, 0)) ; 0xc0, 0xc4 : écarts.
    pub gravity: f32,
    pub wind: f32,
    pub gravity_var: f32,
    pub wind_var: f32,
    /// 0xc8 : non utilisé par la simulation.
    pub reserved2: u32,
}

/// Particules « trou noir simple » (type 2), onze mots (lecteur FUN_1408f8d50).
#[derive(Debug, Clone, PartialEq)]
pub struct SbhParams {
    /// Nombre maximal, particules par émission (+ écart × hasard), intervalle (ms), vie (ms).
    pub max: u32,
    pub count: i32,
    pub count_var: i32,
    pub interval: u32,
    pub life: u32,
    /// Vitesse et écart (±).
    pub speed: f32,
    pub speed_var: f32,
    /// Angle d'émission (degrés) tiré entre `angle_min` et `angle_max`.
    pub angle_min: f32,
    pub angle_max: f32,
    /// Rayon de l'anneau de départ (drapeau 0x800000).
    pub ring: f32,
}

/// Drapeaux de base du trou noir (FUN_1408f9110, FUN_1408397b0).
pub mod sbh_flag {
    pub const RANDOM_COLOR: u32 = 0x1_0000;
    /// Avec `SCREEN` : positions relatives à l'émetteur, qui les emmène (sinon fixées dans le monde).
    pub const LOCAL_FRAME: u32 = 0x4_0000;
    /// Départ à `−vitesse × vie` : la particule arrive au centre à sa mort (« trou noir »).
    pub const INWARD: u32 = 0x8_0000;
    /// Angles successifs (pas = (max − min) / nombre maximal) au lieu d'un tirage.
    pub const SEQUENTIAL: u32 = 0x10_0000;
    pub const CAPPED: u32 = 0x20_0000;
    /// Vitesse dans le plan de l'écran, positions dans le monde ; planches selon les drapeaux 1 et 2.
    pub const SCREEN: u32 = 0x40_0000;
    /// Départ sur un anneau horizontal de rayon `ring`.
    pub const RING: u32 = 0x80_0000;
    pub const RING_RANDOM_RADIUS: u32 = 0x100_0000;
    /// Pas d'angle : vitesse le long de Y.
    pub const NO_ANGLE: u32 = 0x200_0000;
}

/// Spline RenderWare (`RpSpline`, fichiers `effect/animation/*.ESP` de l'animation 6) : morceau
/// 0x0C = char[32] nom, i32 nombre de points, i32 type (1 ouverte, 2 fermée), points vec3.
#[derive(Debug, Clone, PartialEq)]
pub struct Spline {
    pub points: Vec<[f32; 3]>,
    pub closed: bool,
}

impl Spline {
    /// Lit le premier morceau 0x0C d'un flux RenderWare (morceaux de tête sautés).
    pub fn parse(b: &[u8]) -> Option<Spline> {
        let mut o = 0;
        while o + 12 <= b.len() {
            let (kind, size) = (u32_at(b, o)?, u32_at(b, o + 4)? as usize);
            if kind == 0x0c {
                let d = b.get(o + 12..o + 12 + size)?;
                let n = u32_at(d, 32)? as usize;
                let closed = u32_at(d, 36)? == 2;
                let points = (0..n).map(|i| vec3_at(d, 40 + i * 12)).collect::<Option<Vec<_>>>()?;
                return (n >= 4).then_some(Spline { points, closed });
            }
            o += 12 + size;
        }
        None
    }

    /// Position au paramètre `w` ∈ [0, 1] sur le chemin « lisse » (RpSplineFindMatrix, chemin 10,
    /// FUN_1403734c0) : segment `⌊(n − 3)·w⌋`, B-spline cubique uniforme sur quatre points (bases
    /// tabulées par Archonia, supposées être les bases uniformes classiques).
    pub fn position(&self, w: f32) -> [f32; 3] {
        let n = self.points.len();
        let mut w = w.abs();
        if self.closed && w > 1.0 {
            w -= w.trunc();
        }
        let (seg, u) = if w >= 1.0 {
            (n - 4, 1.0)
        } else {
            let x = (n - 3) as f32 * w;
            ((x as usize).min(n - 4), x.fract())
        };
        let (u2, u3) = (u * u, u * u * u);
        let b = [(1.0 - u).powi(3) / 6.0, (3.0 * u3 - 6.0 * u2 + 4.0) / 6.0, (-3.0 * u3 + 3.0 * u2 + 3.0 * u + 1.0) / 6.0, u3 / 6.0];
        let mut out = [0.0; 3];
        for (k, bk) in b.iter().enumerate() {
            let p = self.points[(seg + k) % n];
            for (o, c) in out.iter_mut().zip(p) {
                *o += bk * c;
            }
        }
        out
    }
}

fn psys_params(x: &[u8]) -> Option<PsysParams> {
    let f = |o: usize| f32_at(x, o);
    let u = |o: usize| u32_at(x, o);
    let v3 = |o: usize| vec3_at(x, o);
    Some(PsysParams {
        max: u(0)?,
        // Le lecteur remplace un intervalle nul par 1 (FUN_1408f8f30).
        interval: u(4)?.max(1),
        reserved: x.get(8..72)?.try_into().ok()?,
        speed: f(0x48)?,
        speed_var: f(0x4c)?,
        offset: f(0x50)?,
        offset_var: f(0x54)?,
        count: u(0x58)? as i32,
        count_var: u(0x5c)?,
        angle_rate: [f(0x60)?, f(0x64)?],
        angle1: [f(0x68)?, f(0x6c)?],
        angle2: [f(0x70)?, f(0x74)?],
        dir: v3(0x78)?,
        axis: v3(0x84)?,
        cone: f(0x90)?,
        dir_flags: u(0x94)?,
        shape: u(0x98)?,
        dims: v3(0x9c)?,
        spin: f(0xa8)?,
        spin_var: f(0xac)?,
        life: u(0xb0)?,
        life_var: u(0xb4)?,
        gravity: f(0xb8)?,
        wind: f(0xbc)?,
        gravity_var: f(0xc0)?,
        wind_var: f(0xc4)?,
        reserved2: u(0xc8)?,
    })
}

/// Traînée (type 3), cinq mots (lecteur FUN_140903e30, ajout des points FUN_14083f2f0, bords
/// FUN_14083d8b0).
#[derive(Debug, Clone, PartialEq)]
pub struct TailParams {
    /// Nombre maximal de points (Archonia en garde au plus 5 pour la courbe).
    pub max: u32,
    /// Intervalle d'ajout d'un point (ms) et vie d'un point (ms).
    pub interval: u32,
    pub life: u32,
    /// Distances des deux bords le long de l'axe de l'émetteur (haut, bas).
    pub top: f32,
    pub bottom: f32,
}

/// Drapeaux de base de la traînée (FUN_14083d8b0) : axe des bords = axe Y de l'émetteur, tourné de
/// 45° (0x10000) ou 135° (0x40000) autour de son axe Z, ou son axe X (0x20000).
pub mod tail_flag {
    pub const TILT_45: u32 = 0x1_0000;
    pub const AXIS_X: u32 = 0x2_0000;
    pub const TILT_135: u32 = 0x4_0000;
}

fn tail_params(x: &[u8]) -> Option<TailParams> {
    Some(TailParams { max: u32_at(x, 0)?, interval: u32_at(x, 4)?, life: u32_at(x, 8)?, top: f32_at(x, 12)?, bottom: f32_at(x, 16)? })
}

fn sbh_params(x: &[u8]) -> Option<SbhParams> {
    Some(SbhParams {
        max: u32_at(x, 0)?,
        count: u32_at(x, 4)? as i32,
        count_var: u32_at(x, 8)? as i32,
        // Le lecteur remplace un intervalle nul par 1 (FUN_1408f8d50).
        interval: u32_at(x, 12)?.max(1),
        life: u32_at(x, 16)?,
        speed: f32_at(x, 20)?,
        speed_var: f32_at(x, 24)?,
        angle_min: f32_at(x, 28)?,
        angle_max: f32_at(x, 32)?,
        ring: f32_at(x, 40)?,
    })
}

/// Jeu d'effets décodé.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectSetData {
    pub id: u32,
    pub name: String,
    /// Durée du jeu (ms, premier champ après le nom).
    pub life: u32,
    /// Drapeaux du jeu (fiche 0x2c, [`set_flag`]).
    pub flags: u32,
    /// Textures : (indice, nom, masque).
    pub textures: Vec<(u32, String, String)>,
    /// Dépendances (base parente → base enfant), dans l'ordre du bloc.
    pub deps: Vec<EffDep>,
    pub bases: Vec<EffBase>,
}

/// Rattachement d'une base : à la base parente `parent` (et à son nœud HAnim `node`), sinon au
/// repère du jeu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attach {
    pub parent: usize,
    pub node: Option<u32>,
}

impl EffectSetData {
    /// Parent de chaque base d'après les dépendances, comme FUN_1408386d0 ; `None` si Archonia
    /// arrête le jeu : indice hors des bases, traînée enfant d'une traînée, d'un son ou d'un cadre
    /// mobile (types 3, 6, 7), base parente d'elle-même ou boucle de rattachement. Une base
    /// rattachée deux fois garde la dernière dépendance (`RwFrameAddChild` la déplace). Le nœud
    /// HAnim n'est gardé que pour un parent objet (seul à avoir un modèle, FUN_1404db0b0).
    pub fn attachments(&self) -> Option<Vec<Option<Attach>>> {
        let n = self.bases.len();
        let mut out: Vec<Option<Attach>> = vec![None; n];
        for d in &self.deps {
            let (p, c) = (d.parent as usize, d.child as usize);
            let (parent, child) = (self.bases.get(p)?, self.bases.get(c)?);
            if p == c || (child.kind == base_kind::TAIL && matches!(parent.kind, base_kind::TAIL | base_kind::SOUND | base_kind::MOVING_FRAME)) {
                return None;
            }
            out[c] = Some(Attach { parent: p, node: d.node.filter(|_| parent.kind == base_kind::OBJECT) });
        }
        // Pas de boucle : chaque chaîne de parents se termine en au plus `n` pas.
        for i in 0..n {
            let (mut j, mut steps) = (i, 0);
            while let Some(a) = out[j] {
                j = a.parent;
                steps += 1;
                if steps > n {
                    return None;
                }
            }
        }
        Some(out)
    }
}

fn f32_at(b: &[u8], o: usize) -> Option<f32> {
    u32_at(b, o).map(f32::from_bits)
}

fn vec3_at(b: &[u8], o: usize) -> Option<[f32; 3]> {
    Some([f32_at(b, o)?, f32_at(b, o + 4)?, f32_at(b, o + 8)?])
}

fn text_at(b: &[u8], o: usize, len: usize) -> Option<String> {
    Some(crate::ini::decode_cp949(cstr(b.get(o..o + len)?)))
}

/// Lit une animation en `o` ; rend l'animation et la position suivante.
fn read_anim(b: &[u8], o: usize) -> Option<(EffAnim, usize)> {
    let kind = u32_at(b, o)?;
    let (flags, life, loop_opt) = (u32_at(b, o + 4)?, u32_at(b, o + 8)?, u32_at(b, o + 12)?);
    let mut p = o + 16;
    let count = |p: usize| u32_at(b, p).map(|n| n as usize).filter(|&n| n < 10_000);
    let keys = match kind {
        anim_kind::COLOR => {
            let n = count(p)?;
            p += 4;
            let k = (0..n).map(|i| Some((u32_at(b, p + i * 8)?, b.get(p + i * 8 + 4..p + i * 8 + 8)?.try_into().ok()?))).collect::<Option<Vec<_>>>()?;
            p += n * 8;
            AnimKeys::Color(k)
        }
        anim_kind::PARTICLE_POS_SCALE => {
            let n = count(p)?;
            p += 4;
            let k = (0..n).map(|i| Some((u32_at(b, p + i * 8)?, f32_at(b, p + i * 8 + 4)?))).collect::<Option<Vec<_>>>()?;
            p += n * 8;
            AnimKeys::Scalar(k)
        }
        anim_kind::TUTV => {
            let n = count(p)?;
            p += 4;
            let k = (0..n)
                .map(|i| {
                    let q = p + i * 20;
                    Some((u32_at(b, q)?, [f32_at(b, q + 4)?, f32_at(b, q + 8)?, f32_at(b, q + 12)?, f32_at(b, q + 16)?]))
                })
                .collect::<Option<Vec<_>>>()?;
            p += n * 20;
            AnimKeys::TuTv(k)
        }
        anim_kind::MISSILE => {
            let k = AnimKeys::Missile { speed: f32_at(b, p)?, accel: f32_at(b, p + 4)? };
            p += 8;
            k
        }
        anim_kind::LINEAR | anim_kind::SCALE => {
            let n = count(p)?;
            p += 4;
            let k = (0..n).map(|i| Some((u32_at(b, p + i * 16)?, vec3_at(b, p + i * 16 + 4)?))).collect::<Option<Vec<_>>>()?;
            p += n * 16;
            AnimKeys::Vec3(k)
        }
        anim_kind::REVOLUTION => {
            let (axis, extra) = (vec3_at(b, p)?, vec3_at(b, p + 12)?);
            let n = count(p + 24)?;
            p += 28;
            let k = (0..n).map(|i| Some((u32_at(b, p + i * 16)?, vec3_at(b, p + i * 16 + 4)?))).collect::<Option<Vec<_>>>()?;
            p += n * 16;
            AnimKeys::Revolution { axis, extra, keys: k }
        }
        anim_kind::ROTATION => {
            let axis = vec3_at(b, p)?;
            let n = count(p + 12)?;
            p += 16;
            let k = (0..n).map(|i| Some((u32_at(b, p + i * 8)?, f32_at(b, p + i * 8 + 4)?))).collect::<Option<Vec<_>>>()?;
            p += n * 8;
            AnimKeys::Rotation { axis, keys: k }
        }
        anim_kind::RP_SPLINE | anim_kind::RT_ANIM => {
            let k = AnimKeys::File(text_at(b, p, 64)?);
            p += 64;
            k
        }
        _ => return None,
    };
    Some((EffAnim { kind, flags, life, loop_opt, keys }, p))
}

/// Taille propre au type après l'en-tête (et la partie de rendu s'il y en a une) ; `None` pour un
/// type inconnu (PostFX, caméra : absents des données).
fn base_layout(kind: u32) -> Option<(bool, usize)> {
    Some(match kind {
        base_kind::BOARD | base_kind::TERRAIN_BOARD => (true, 0),
        // u32, u32, 64 octets (émetteur), 96 octets (particules), 36 octets : FUN_1408fa2a0.
        base_kind::PSYS => (true, 204),
        // onze u32 : FUN_1408fa120.
        base_kind::PSYS_SBH => (true, 44),
        // cinq u32 : FUN_140903f60.
        base_kind::TAIL => (true, 20),
        // char[64] modèle + u32 couleur pré-éclairée : FUN_1408dc690.
        base_kind::OBJECT => (true, 68),
        // 0x30 octets : FUN_1409035d0.
        base_kind::LIGHT => (false, 0x30),
        // 0x8c octets : FUN_1408fc220.
        base_kind::SOUND => (false, 0x8c),
        // six u32 : FUN_140903bd0.
        base_kind::MOVING_FRAME => (false, 24),
        _ => return None,
    })
}

/// Lit une dépendance (12 octets) : `u32 parent` (drapeau 2 : 16 bits bas = indice, 16 bits hauts =
/// ID de nœud HAnim), `u32 enfant`, `u32 drapeaux`.
fn read_dep(b: &[u8], o: usize) -> Option<EffDep> {
    let (a, child, flags) = (u32_at(b, o)?, u32_at(b, o + 4)?, u32_at(b, o + 8)?);
    Some(if flags & dep_flag::HANIM_NODE != 0 { EffDep { parent: a & 0xffff, node: Some(a >> 16), child, flags } } else { EffDep { parent: a, node: None, child, flags } })
}

/// Bloc d'un jeu décodé : textures, dépendances, bases.
pub type DecodedBlock = (Vec<(u32, String, String)>, Vec<EffDep>, Vec<EffBase>);

/// Décode le bloc d'un jeu (`textures`, `bases`, `deps` : compteurs de la fiche).
pub fn decode_block(block: &[u8], textures: usize, bases: usize, deps: usize) -> Option<DecodedBlock> {
    let mut o = 0;
    let mut tex = Vec::with_capacity(textures);
    for _ in 0..textures {
        tex.push((u32_at(block, o)?, text_at(block, o + 4, 64)?, text_at(block, o + 68, 64)?));
        o += 0x84;
    }
    let mut dep_list = Vec::with_capacity(deps.min(1024));
    for _ in 0..deps {
        dep_list.push(read_dep(block, o)?);
        o += 12;
    }
    let mut out = Vec::with_capacity(bases);
    for _ in 0..bases {
        let kind = u32_at(block, o)?;
        let (render, extra_len) = base_layout(kind)?;
        let mut base = EffBase {
            kind,
            title: text_at(block, o + 4, 32)?,
            delay: u32_at(block, o + 0x24)?,
            life: u32_at(block, o + 0x28)?,
            loop_opt: u32_at(block, o + 0x2c)?,
            flags: u32_at(block, o + 0x30)?,
            texture: -1,
            ..Default::default()
        };
        let anims = u32_at(block, o + 0x34)? as usize;
        o += BASE_HEADER_LEN;
        if render {
            base.blend = Some(u32_at(block, o)?);
            base.texture = u32_at(block, o + 4)? as i32;
            base.position = vec3_at(block, o + 8)?;
            base.rotation = vec3_at(block, o + 20)?;
            o += RENDER_LEN;
        }
        let extra = block.get(o..o + extra_len)?;
        match kind {
            base_kind::OBJECT => base.clump = Some((crate::ini::decode_cp949(cstr(&extra[..64])), u32_at(extra, 64)?)),
            base_kind::SOUND => {
                base.sound = Some((u32_at(extra, 0)?, text_at(extra, 4, 64)?, text_at(extra, 68, 64)?, u32_at(extra, 132)?, f32_at(extra, 136)?))
            }
            _ => {
                base.extra = extra.to_vec();
                base.particles = match kind {
                    base_kind::PSYS => psys_params(extra).map(Particles::System),
                    base_kind::PSYS_SBH => sbh_params(extra).map(Particles::BlackHole),
                    _ => None,
                };
                match kind {
                    base_kind::TAIL => base.tail = tail_params(extra),
                    base_kind::LIGHT => base.light = light_params(extra),
                    base_kind::MOVING_FRAME => base.moving_frame = moving_frame_params(extra),
                    _ => {}
                }
            }
        }
        o += extra_len;
        for _ in 0..anims {
            let (a, next) = read_anim(block, o)?;
            base.anims.push(a);
            o = next;
        }
        out.push(base);
    }
    (o == block.len()).then_some((tex, dep_list, out))
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|c| u32::from_le_bytes(c.try_into().unwrap()))
}

fn cstr(b: &[u8]) -> &[u8] {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    &b[..end]
}

/// Sons d'un bloc : suite d'effets son décodés, sinon balayage des `*.mp3`.
pub fn block_sounds(block: &[u8]) -> (Vec<String>, bool) {
    if !block.is_empty()
        && block.len().is_multiple_of(SOUND_RECORD_LEN)
        && block
            .chunks_exact(SOUND_RECORD_LEN)
            .all(|r| u32_at(r, 0) == Some(EFFECT_TYPE_SOUND))
    {
        let sounds = block
            .chunks_exact(SOUND_RECORD_LEN)
            .map(|r| String::from_utf8_lossy(cstr(&r[0x3c..0xbc])).into_owned())
            .filter(|s| !s.is_empty())
            .collect();
        return (sounds, false);
    }
    // Repli : tout nom `….mp3` terminé par 0 dont le début suit un 0.
    let mut sounds = Vec::new();
    let lower: Vec<u8> = block.iter().map(|b| b.to_ascii_lowercase()).collect();
    let mut i = 0;
    while i + 5 <= lower.len() {
        if &lower[i..i + 4] == b".mp3" && lower.get(i + 4) == Some(&0) {
            let start = block[..i].iter().rposition(|&c| c == 0).map_or(0, |p| p + 1);
            let name = &block[start..i + 4];
            if name.iter().all(|c| c.is_ascii_graphic()) {
                sounds.push(String::from_utf8_lossy(name).into_owned());
            }
            i += 5;
        } else {
            i += 1;
        }
    }
    (sounds, true)
}

impl EffectSets {
    /// Lit les deux fichiers ; les fiches tronquées ou hors du fichier de blocs sont ignorées.
    pub fn parse(set_file: &[u8], var_file: &[u8]) -> Self {
        let mut sets = HashMap::new();
        let mut records = HashMap::new();
        let Some(count) = u32_at(set_file, 0) else {
            return Self { sets, ..Default::default() };
        };
        for i in 0..count as usize {
            let Some(r) = set_file.get(4 + i * SET_RECORD_LEN..4 + (i + 1) * SET_RECORD_LEN) else {
                break;
            };
            let id = u32_at(r, 0).unwrap();
            records.insert(id, r.try_into().unwrap());
            let name = crate::ini::decode_cp949(cstr(&r[4..36]));
            let offset = u32_at(r, 0x74).unwrap();
            let len = u32_at(r, 0x78).unwrap();
            let (sounds, scanned) = match var_file.get(offset as usize..(offset + len) as usize) {
                Some(block) if len > 0 => block_sounds(block),
                _ => (Vec::new(), false),
            };
            sets.insert(
                id,
                EffectSet {
                    id,
                    name,
                    offset,
                    len,
                    sounds,
                    scanned,
                },
            );
        }
        Self { sets, records, var: std::sync::Arc::new(var_file.to_vec()) }
    }

    /// Jeu complet : textures, dépendances, bases et animations. Fiche : 0x24 durée, 0x2c drapeaux,
    /// 0x68 nombre de textures, 0x6c nombre de bases, 0x70 nombre de dépendances, 0x74/0x78 bloc.
    pub fn decode(&self, id: u32) -> Option<EffectSetData> {
        let r = self.records.get(&id)?;
        let (offset, len) = (u32_at(r, 0x74)? as usize, u32_at(r, 0x78)? as usize);
        let block = self.var.get(offset..offset + len)?;
        let (textures, deps, bases) = decode_block(block, u32_at(r, 0x68)? as usize, u32_at(r, 0x6c)? as usize, u32_at(r, 0x70)? as usize)?;
        Some(EffectSetData { id, name: crate::ini::decode_cp949(cstr(&r[4..36])), life: u32_at(r, 0x24)?, flags: u32_at(r, 0x2c)?, textures, deps, bases })
    }

    pub fn get(&self, id: u32) -> Option<&EffectSet> {
        self.sets.get(&id)
    }

    pub fn len(&self) -> usize {
        self.sets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }

    /// Jeux triés par `id`.
    pub fn iter(&self) -> impl Iterator<Item = &EffectSet> {
        let mut v: Vec<&EffectSet> = self.sets.values().collect();
        v.sort_by_key(|s| s.id);
        v.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sound_record(name: &str) -> Vec<u8> {
        let mut r = vec![0u8; SOUND_RECORD_LEN];
        r[0] = EFFECT_TYPE_SOUND as u8;
        r[0x2c] = 1;
        r[0x32] = 1;
        r[0x38] = 2;
        r[0x3c..0x3c + name.len()].copy_from_slice(name.as_bytes());
        r[0xc0..0xc4].copy_from_slice(&1.1f32.to_le_bytes());
        r
    }

    fn set_record(id: u32, name: &str, offset: u32, len: u32) -> Vec<u8> {
        let mut r = vec![0u8; SET_RECORD_LEN];
        r[0..4].copy_from_slice(&id.to_le_bytes());
        r[4..4 + name.len()].copy_from_slice(name.as_bytes());
        r[0x74..0x78].copy_from_slice(&offset.to_le_bytes());
        r[0x78..0x7c].copy_from_slice(&len.to_le_bytes());
        r
    }

    fn put(v: &mut Vec<u8>, words: &[u32]) {
        for w in words {
            v.extend(w.to_le_bytes());
        }
    }

    fn header(v: &mut Vec<u8>, kind: u32, delay: u32, life: u32, flags: u32, anims: u32) {
        put(v, &[kind]);
        v.extend([0u8; 32]);
        put(v, &[delay, life, 1, flags, anims]);
    }

    #[test]
    fn full_block_decodes_textures_bases_and_anims() {
        let mut b = Vec::new();
        put(&mut b, &[0]);
        let mut name = b"S0000102".to_vec();
        name.resize(128, 0);
        b.extend(&name);
        // Planche : mélange 3, texture 0, position (0, 150, 0) ; échelle 2 clés, couleur 1 clé, rotation.
        header(&mut b, base_kind::BOARD, 500, 1000, 1, 3);
        put(&mut b, &[3, 0, 0, 150f32.to_bits(), 0, 0, 0, 0]);
        put(&mut b, &[anim_kind::SCALE, 0, 1000, 1, 2, 0, 1f32.to_bits(), 1f32.to_bits(), 1f32.to_bits(), 1000, 20f32.to_bits(), 20f32.to_bits(), 20f32.to_bits()]);
        put(&mut b, &[anim_kind::COLOR, 0, 500, 1, 1, 0, 0x00ff_ffff]);
        put(&mut b, &[anim_kind::ROTATION, 0, 0, 1, 0, 1f32.to_bits(), 0, 1, 0, 90f32.to_bits()]);
        // Objet : modèle K0004300.ECL.
        header(&mut b, base_kind::OBJECT, 0, 0, 0, 0);
        put(&mut b, &[2, u32::MAX, 0, 0, 0, 0, 0, 0]);
        let mut clump = b"K0004300.ECL".to_vec();
        clump.resize(64, 0);
        b.extend(&clump);
        put(&mut b, &[u32::MAX]);
        // Son.
        header(&mut b, base_kind::SOUND, 0, 0, 0x20000, 0);
        let mut s = vec![0u8; 0x8c];
        s[0] = 1;
        s[4..14].copy_from_slice(b"EA0044.WAV");
        s[136..140].copy_from_slice(&1.1f32.to_le_bytes());
        b.extend(&s);
        let (tex, deps, bases) = decode_block(&b, 1, 3, 0).expect("bloc décodé");
        assert!(deps.is_empty());
        assert_eq!(tex, vec![(0, "S0000102".to_string(), String::new())]);
        assert_eq!(bases.len(), 3);
        let board = &bases[0];
        assert_eq!((board.kind, board.delay, board.life, board.blend, board.texture), (base_kind::BOARD, 500, 1000, Some(3), 0));
        assert_eq!(board.position, [0.0, 150.0, 0.0]);
        assert_eq!(board.anims.len(), 3);
        assert_eq!(board.anims[0].keys, AnimKeys::Vec3(vec![(0, [1.0; 3]), (1000, [20.0; 3])]));
        assert_eq!(board.anims[1].keys, AnimKeys::Color(vec![(0, [0xff, 0xff, 0xff, 0])]));
        assert_eq!(board.anims[2].keys, AnimKeys::Rotation { axis: [0.0, 1.0, 0.0], keys: vec![(0, 90.0)] });
        assert_eq!(bases[1].clump, Some(("K0004300.ECL".to_string(), u32::MAX)));
        assert_eq!(bases[2].sound.as_ref().map(|s| s.1.as_str()), Some("EA0044.WAV"));
        // Bloc tronqué ou trop long : refusé.
        assert!(decode_block(&b[..b.len() - 1], 1, 3, 0).is_none());
        let mut long = b.clone();
        long.push(0);
        assert!(decode_block(&long, 1, 3, 0).is_none());
    }

    /// Dépendances (12 o après les textures), lumière (0x30 o) et cadre mobile (24 o).
    #[test]
    fn deps_light_and_moving_frame_decode() {
        let mut b = Vec::new();
        // Deux dépendances : objet 0 → traînée 1 ; nœud HAnim 5 de l'objet 0 → lumière 2.
        put(&mut b, &[0, 1, 0]);
        put(&mut b, &[(5 << 16), 2, dep_flag::HANIM_NODE | dep_flag::CHILD_0X80]);
        header(&mut b, base_kind::OBJECT, 0, 0, 0, 0);
        put(&mut b, &[2, u32::MAX, 0, 0, 0, 0, 0, 0]);
        b.extend([0u8; 64]);
        put(&mut b, &[0]);
        header(&mut b, base_kind::TAIL, 0, 1000, 0, 0);
        put(&mut b, &[3, u32::MAX, 0, 0, 0, 0, 0, 0]);
        put(&mut b, &[20, 50, 1000, 700f32.to_bits(), (-700f32).to_bits()]);
        // Lumière de l'EID 51 : point, cône 30°, surfaceProp −1, matériau blanc ; échelle 2 → 0.
        header(&mut b, base_kind::LIGHT, 0, 75, 0x4, 1);
        put(&mut b, &[0, 0, 0, 0, 0, 0, 0, 30f32.to_bits()]);
        put(&mut b, &[(-1f32).to_bits(); 3]);
        put(&mut b, &[u32::MAX]);
        put(&mut b, &[anim_kind::SCALE, 0, 75, 1, 2, 0, 2f32.to_bits(), 2f32.to_bits(), 2f32.to_bits(), 75, 0, 0, 0]);
        // Cadre mobile de l'EID 3622 : secousse, axe Y du monde, caméra, 15, 1000 ms, 10.
        header(&mut b, base_kind::MOVING_FRAME, 0, 1000, 0, 0);
        put(&mut b, &[0, 4, 0, 15f32.to_bits(), 1000, 10f32.to_bits()]);
        let (_, deps, bases) = decode_block(&b, 0, 4, 2).expect("bloc décodé");
        assert_eq!(deps[0], EffDep { parent: 0, node: None, child: 1, flags: 0 });
        assert_eq!(deps[1], EffDep { parent: 0, node: Some(5), child: 2, flags: 3 });
        let l = bases[2].light.as_ref().expect("lumière");
        assert_eq!((l.light_type, l.cone_angle, l.surface, l.material), (0, 30.0, [-1.0; 3], [0xff; 4]));
        assert_eq!(bases[2].anims[0].keys, AnimKeys::Vec3(vec![(0, [2.0; 3]), (75, [0.0; 3])]));
        let m = bases[3].moving_frame.as_ref().expect("cadre mobile");
        assert_eq!((m.mfrm_type, m.axis, m.whose, m.amplitude, m.duration, m.cycle), (0, 4, 0, 15.0, 1000, 10.0));
        // Rattachements : traînée sur l'objet, lumière sur le nœud 5 de l'objet.
        let set = EffectSetData { deps: deps.clone(), bases: bases.clone(), ..Default::default() };
        let at = set.attachments().expect("jeu jouable");
        assert_eq!(at, vec![None, Some(Attach { parent: 0, node: None }), Some(Attach { parent: 0, node: Some(5) }), None]);
        // Traînée enfant d'un cadre mobile, indice hors des bases, boucle : jeu arrêté.
        let bad = |deps: Vec<EffDep>| EffectSetData { deps, bases: bases.clone(), ..Default::default() }.attachments();
        assert!(bad(vec![EffDep { parent: 3, node: None, child: 1, flags: 0 }]).is_none());
        assert!(bad(vec![EffDep { parent: 9, node: None, child: 1, flags: 0 }]).is_none());
        assert!(bad(vec![EffDep { parent: 0, node: None, child: 2, flags: 0 }, EffDep { parent: 2, node: None, child: 0, flags: 0 }]).is_none());
        // Compteur de dépendances faux : fin de bloc décalée, refusé.
        assert!(decode_block(&b, 0, 4, 1).is_none());
    }

    #[test]
    fn spline_chunk_and_bspline_position() {
        // Morceau de tête 0x24 (sauté), puis morceau 0x0C : nom, 5 points, type 1 (ouverte).
        let mut b = Vec::new();
        put(&mut b, &[0x24, 4, 0x1803_ffff, 0]);
        let pts = [[0.0, 0.0, 0.0], [0.0, 60.0, 0.0], [0.0, 120.0, 0.0], [0.0, 180.0, 0.0], [0.0, 240.0, 0.0]];
        put(&mut b, &[0x0c, 32 + 8 + 60, 0x1803_ffff]);
        b.extend([0u8; 32]);
        put(&mut b, &[5, 1]);
        for p in pts {
            put(&mut b, &p.map(f32::to_bits));
        }
        let s = Spline::parse(&b).expect("spline");
        assert_eq!((s.points.len(), s.closed), (5, false));
        // Points alignés et réguliers : la B-spline uniforme va de 60 à 180 linéairement.
        assert!((s.position(0.0)[1] - 60.0).abs() < 1e-3);
        assert!((s.position(0.5)[1] - 120.0).abs() < 1e-3);
        assert!((s.position(1.0)[1] - 180.0).abs() < 1e-3);
        assert!(Spline::parse(&b[..b.len() - 1]).is_none());
    }

    /// Avec les données du client (`ARCHRUST_GAME_DIR`) : tous les jeux se décodent exactement.
    #[test]
    fn all_client_sets_decode() {
        let Some(dir) = std::env::var_os("ARCHRUST_GAME_DIR") else { return };
        let dir = std::path::Path::new(&dir).join("effect/ini/new");
        let (Ok(set), Ok(var)) = (std::fs::read(dir.join("efffile_set.eff")), std::fs::read(dir.join("efffile_var.eff"))) else { return };
        let sets = EffectSets::parse(&set, &var);
        let failed: Vec<u32> = sets.iter().map(|s| s.id).filter(|&id| sets.decode(id).is_none()).collect();
        assert!(failed.is_empty(), "jeux non décodés : {failed:?}");
        let lv = sets.decode(3610).unwrap();
        assert_eq!(lv.name, "LevelUp(Main)");
        assert!(lv.bases.iter().any(|b| b.clump.as_ref().is_some_and(|c| c.0 == "K0004300.ECL")));
        // Dépendances et traînées (relevé de la spec) : 713 traînées, dont 710 enfants d'une base
        // (objet 429, planche 264, trou noir 16, particules 1) ; drapeau de dépendance toujours 0.
        let all: Vec<EffectSetData> = sets.iter().filter_map(|s| sets.decode(s.id)).collect();
        let mut tails = 0;
        let mut parents: HashMap<u32, usize> = HashMap::new();
        let mut dep_flags: HashMap<u32, usize> = HashMap::new();
        let mut refused = Vec::new();
        for s in &all {
            for d in &s.deps {
                *dep_flags.entry(d.flags).or_default() += 1;
                // Nœuds HAnim : toujours sur un objet (ex. jeu 14, K0000e00.ECL, nœuds 990 à 993).
                if d.node.is_some() {
                    assert_eq!(s.bases[d.parent as usize].kind, base_kind::OBJECT, "jeu {}", s.id);
                }
            }
            let Some(at) = s.attachments() else {
                refused.push(s.id);
                continue;
            };
            for (i, b) in s.bases.iter().enumerate().filter(|(_, b)| b.kind == base_kind::TAIL) {
                tails += 1;
                if let Some(a) = at[i] {
                    *parents.entry(s.bases[a.parent].kind).or_default() += 1;
                    let t = b.tail.as_ref().unwrap();
                    assert!(t.top != 0.0 || t.bottom != 0.0, "jeu {} : traînée sans hauteurs", s.id);
                }
            }
        }
        assert!(refused.is_empty(), "jeux refusés : {refused:?}");
        // 5 207 dépendances : 5 191 sans drapeau, 3 avec le drapeau 1, 13 sur un nœud HAnim.
        assert_eq!((dep_flags[&0], dep_flags[&1], dep_flags[&2]), (5191, 3, 13));
        assert_eq!(tails, 713);
        assert_eq!(parents.values().sum::<usize>(), 710);
        assert_eq!((parents[&base_kind::OBJECT], parents[&base_kind::BOARD], parents[&base_kind::PSYS_SBH], parents[&base_kind::PSYS]), (429, 264, 16, 1));
        // Traînées indépendantes : seulement dans les trois jeux à deux nœuds (drapeau 0x400).
        let two: Vec<u32> = all.iter().filter(|s| s.flags & set_flag::TWO_NODE_TAIL != 0).map(|s| s.id).collect();
        assert_eq!(two, vec![2627, 2960, 6082]);
        // Une seule lumière (EID 51), cadres mobiles tous de type 0 (secousse).
        let lights: Vec<(u32, &EffBase)> = all.iter().flat_map(|s| s.bases.iter().map(move |b| (s.id, b))).filter(|(_, b)| b.kind == base_kind::LIGHT).collect();
        assert_eq!(lights.len(), 1);
        assert_eq!(lights[0].0, 51);
        let l = lights[0].1.light.as_ref().unwrap();
        assert_eq!((l.light_type, l.cone_angle), (0, 30.0));
        let mfrm: Vec<&MovingFrameParams> = all.iter().flat_map(|s| s.bases.iter()).filter_map(|b| b.moving_frame.as_ref()).collect();
        assert_eq!(mfrm.len(), 165);
        assert!(mfrm.iter().all(|m| m.mfrm_type == 0));
    }

    #[test]
    fn sound_sets_are_decoded_exactly_and_mixed_blocks_are_scanned() {
        let mut var = sound_record("A_GRN_M.mp3");
        var.extend(sound_record("OE0000.mp3"));
        let mixed_at = var.len() as u32;
        // Bloc mêlé : un effet d'un autre type (type 3, 40 octets inventés) puis un son.
        let mut mixed = vec![0u8; 40];
        mixed[0] = 3;
        mixed.extend(b"\0WATER_FALL.mp3\0suite");
        var.extend(&mixed);
        let mut set = 3u32.to_le_bytes().to_vec();
        set.extend(set_record(3828, "field-dawn", 0, SOUND_RECORD_LEN as u32));
        set.extend(set_record(3806, "Water_Small1", SOUND_RECORD_LEN as u32, SOUND_RECORD_LEN as u32));
        set.extend(set_record(7, "Fall", mixed_at, mixed.len() as u32));
        let sets = EffectSets::parse(&set, &var);
        assert_eq!(sets.len(), 3);
        let s = sets.get(3828).unwrap();
        assert_eq!(s.name, "field-dawn");
        assert_eq!(s.sounds, vec!["A_GRN_M.mp3".to_string()]);
        assert!(!s.scanned);
        assert_eq!(sets.get(3806).unwrap().sounds, vec!["OE0000.mp3".to_string()]);
        let fall = sets.get(7).unwrap();
        assert_eq!(fall.sounds, vec!["WATER_FALL.mp3".to_string()]);
        assert!(fall.scanned);
        assert_eq!(sets.iter().map(|s| s.id).collect::<Vec<_>>(), vec![7, 3806, 3828]);
        assert!(sets.get(1).is_none());
        // Fiche hors du fichier de blocs : jeu sans son.
        let mut short = 1u32.to_le_bytes().to_vec();
        short.extend(set_record(9, "x", 10_000, 196));
        assert!(EffectSets::parse(&short, &var).get(9).unwrap().sounds.is_empty());
        assert!(EffectSets::parse(&[], &[]).is_empty());
    }
}
