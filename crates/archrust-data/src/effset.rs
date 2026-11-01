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
    /// Octets propres au type non décodés (particules, traînée, lumière, cadre mobile).
    pub extra: Vec<u8>,
    pub anims: Vec<EffAnim>,
}

/// Jeu d'effets décodé.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectSetData {
    pub id: u32,
    pub name: String,
    /// Durée du jeu (ms, premier champ après le nom).
    pub life: u32,
    /// Textures : (indice, nom, masque).
    pub textures: Vec<(u32, String, String)>,
    pub bases: Vec<EffBase>,
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

/// Décode le bloc d'un jeu (`textures`, `bases`, `deps` : compteurs de la fiche).
pub fn decode_block(block: &[u8], textures: usize, bases: usize, deps: usize) -> Option<(Vec<(u32, String, String)>, Vec<EffBase>)> {
    let mut o = 0;
    let mut tex = Vec::with_capacity(textures);
    for _ in 0..textures {
        tex.push((u32_at(block, o)?, text_at(block, o + 4, 64)?, text_at(block, o + 68, 64)?));
        o += 0x84;
    }
    o += deps * 12;
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
            _ => base.extra = extra.to_vec(),
        }
        o += extra_len;
        for _ in 0..anims {
            let (a, next) = read_anim(block, o)?;
            base.anims.push(a);
            o = next;
        }
        out.push(base);
    }
    (o == block.len()).then_some((tex, out))
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

    /// Jeu complet : textures, bases et animations. Fiche : 0x24 durée, 0x68 nombre de textures,
    /// 0x6c nombre de bases, 0x70 nombre de dépendances, 0x74/0x78 bloc.
    pub fn decode(&self, id: u32) -> Option<EffectSetData> {
        let r = self.records.get(&id)?;
        let (offset, len) = (u32_at(r, 0x74)? as usize, u32_at(r, 0x78)? as usize);
        let block = self.var.get(offset..offset + len)?;
        let (textures, bases) = decode_block(block, u32_at(r, 0x68)? as usize, u32_at(r, 0x6c)? as usize, u32_at(r, 0x70)? as usize)?;
        Some(EffectSetData { id, name: crate::ini::decode_cp949(cstr(&r[4..36])), life: u32_at(r, 0x24)?, textures, bases })
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
        let (tex, bases) = decode_block(&b, 1, 3, 0).expect("bloc décodé");
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
