//! Préparation éphémère d'une pose par clump ; aucune réutilisation entre images/personnages.
use super::{deform, SkinBinding, Vertex};
use archrust_data::ean::HAnimSample;
use archrust_data::rw::Clump;
use glam::{Mat3, Mat4, Vec3};

struct PreparedSkinPose<'a> {
    clump: &'a Clump,
    world: Vec<Mat4>,
    frame_by_index: Vec<Option<usize>>,
}

impl<'a> PreparedSkinPose<'a> {
    fn new(clump: &'a Clump, pose: &[HAnimSample]) -> Option<Self> {
        let hierarchy = clump.frames.iter().filter_map(|f| f.hanim.as_ref()).find(|h| !h.nodes.is_empty())?;
        let world = clump.world_matrices_with_pose(pose);
        let mut frame_by_index = vec![None; hierarchy.nodes.len()];
        for node in &hierarchy.nodes {
            if let Some(slot) = frame_by_index.get_mut(node.index as usize) {
                *slot = clump.frames.iter().position(|f| f.hanim.as_ref().is_some_and(|h| h.node_id == node.id));
            }
        }
        Some(Self { clump, world, frame_by_index })
    }

    fn deform(&self, binding: &SkinBinding) -> Option<Vec<Vertex>> {
        let geometry = self.clump.geometries.get(binding.geometry)?;
        let skin = geometry.skin.as_ref()?;
        let atomic_world_inv = self.clump.atomics.iter()
            .find(|a| a.geometry as usize == binding.geometry)
            .and_then(|a| self.world.get(a.frame as usize))
            .map_or(Mat4::IDENTITY, |m| m.inverse());
        // Même ordre d'opérations flottantes que Clump::skin_matrices, par géométrie.
        let matrices: Vec<Mat4> = skin.inverse_bind.iter().enumerate().map(|(index, bind)| {
            self.frame_by_index.get(index).copied().flatten()
                .map(|frame| atomic_world_inv * self.world[frame] * *bind)
                .unwrap_or(Mat4::IDENTITY)
        }).collect();
        if skin.indices.len() != binding.base.len() { return None; }
        let normals: Vec<Mat3> = matrices.iter().map(|m| Mat3::from_mat4(*m)).collect();
        let mut out = binding.base.clone();
        for (i, v) in out.iter_mut().enumerate() {
            let p = Vec3::from_array(binding.base[i].position);
            let n = Vec3::from_array(binding.base[i].normal);
            let (mut pp, mut nn) = (Vec3::ZERO, Vec3::ZERO);
            let mut total = 0.0;
            for lane in 0..4 {
                let w = skin.weights[i][lane];
                if w == 0.0 { continue; }
                let b = skin.indices[i][lane] as usize;
                let (Some(m), Some(r)) = (matrices.get(b), normals.get(b)) else { continue };
                pp += m.transform_point3(p) * w;
                nn += (*r * n) * w;
                total += w;
            }
            if total > 0.0 {
                v.position = (pp / total).to_array();
                let nn = nn.normalize_or_zero();
                if nn != Vec3::ZERO { v.normal = nn.to_array(); }
            }
        }
        Some(out)
    }
}

/// Lot CPU d'une seule pose. Les clumps sont distingués par identité, pas par nom de modèle.
/// Le chemin des peaux liées au squelette d'un porteur reste celui de `deform`.
pub struct SkinPoseBatch<'a> {
    pose: &'a [HAnimSample],
    clumps: Vec<(&'a Clump, Option<PreparedSkinPose<'a>>)>,
}

impl<'a> SkinPoseBatch<'a> {
    pub fn new(pose: &'a [HAnimSample]) -> Self { Self { pose, clumps: Vec::new() } }

    pub fn deform(&mut self, binding: &'a SkinBinding) -> Option<Vec<Vertex>> {
        if binding.wearer.is_some() { return deform(binding, self.pose); }
        let clump = binding.clump.as_ref();
        let index = self.clumps.iter().position(|(c, _)| std::ptr::eq(*c, clump)).unwrap_or_else(|| {
            self.clumps.push((clump, PreparedSkinPose::new(clump, self.pose)));
            self.clumps.len() - 1
        });
        self.clumps[index].1.as_ref()?.deform(binding)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archrust_data::rw::{Atomic, Frame, Geometry, HAnim, HAnimNode, Skin};
    use std::sync::Arc;

    fn binding(offset: f32) -> SkinBinding {
        let frame = |id, nodes, parent, x| Frame {
            rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            position: Vec3::new(x, 0.0, 0.0), parent, flags: 0, name: String::new(),
            hanim: Some(HAnim { node_id: id, nodes, ..Default::default() }),
        };
        let clump = Clump {
            frames: vec![frame(10, vec![HAnimNode { id: 20, index: 0, flags: 0 }], -1, offset), frame(20, vec![], 0, 2.0)],
            geometries: vec![Geometry {
                skin: Some(Skin { bone_count: 1, used_bones: vec![0], indices: vec![[0; 4]], weights: vec![[0.75, 0.0, 0.0, 0.0]], inverse_bind: vec![Mat4::IDENTITY] }),
                ..Default::default()
            }],
            atomics: vec![Atomic { frame: 0, geometry: 0, flags: 0 }],
        };
        SkinBinding { clump: Arc::new(clump), geometry: 0, wearer: None, base: vec![Vertex {
            position: [1.0, 2.0, 3.0], normal: [0.0, 1.0, 0.0], uv: [0.25, 0.75], color: [1.0; 4],
        }] }
    }

    fn equal(a: &[Vertex], b: &[Vertex]) {
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(b) {
            assert!(a.position.into_iter().chain(a.normal).chain(a.uv).chain(a.color).map(f32::to_bits)
                .eq(b.position.into_iter().chain(b.normal).chain(b.uv).chain(b.color).map(f32::to_bits)));
        }
    }

    #[test]
    fn partage_par_identite_et_pose_ephemere() {
        let a = binding(3.0);
        let mut b = binding(-8.0);
        // L'atomic différent doit garder sa propre inverse, même avec les mêmes ID d'os.
        Arc::make_mut(&mut b.clump).atomics[0].frame = 1;
        for x in [0.0, 7.0, -2.5] {
            let pose = [HAnimSample { rotation: [0.0, 0.0, 0.0, 1.0], translation: [x, 4.0, 1.0] }];
            let mut batch = SkinPoseBatch::new(&pose);
            for binding in [&a, &a, &b, &a] { equal(&deform(binding, &pose).unwrap(), &batch.deform(binding).unwrap()); }
            assert_eq!(batch.clumps.len(), 2);
        }
    }

    #[test]
    fn wearer_et_absences_gardent_le_chemin_reference() {
        let mut a = binding(3.0);
        a.wearer = Some(binding(-9.0).clump);
        let mut batch = SkinPoseBatch::new(&[]);
        equal(&deform(&a, &[]).unwrap(), &batch.deform(&a).unwrap());
        assert!(batch.clumps.is_empty());
        a.wearer = None;
        Arc::make_mut(&mut a.clump).frames[0].hanim.as_mut().unwrap().nodes.clear();
        let mut batch = SkinPoseBatch::new(&[]);
        assert!(deform(&a, &[]).is_none());
        assert!(batch.deform(&a).is_none());
    }
}
