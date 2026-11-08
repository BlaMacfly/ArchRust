//! Retrait des animateurs absents : parcours linéaire pour les petites scènes,
//! index temporaire pour éviter un parcours quadratique dans les scènes peuplées.
use std::collections::{HashMap, HashSet};

pub(super) fn retain_present<T, E>(animators: &mut HashMap<i32, T>, entities: &[E], cid: impl Fn(&E) -> i32) {
    if entities.len() < 128 {
        animators.retain(|id, _| entities.iter().any(|e| cid(e) == *id));
    } else {
        let present: HashSet<i32> = entities.iter().map(cid).collect();
        animators.retain(|id, _| present.contains(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branches_et_seuil_identiques_a_la_recherche_lineaire() {
        for count in [0, 1, 32, 127, 128, 129, 256, 2000] {
            let mut entities: Vec<i32> = (0..count).map(|i| i * 3 - 7).collect();
            if count > 0 { entities[count as usize - 1] = entities[0]; }
            let source: HashMap<i32, String> = (-10..count * 3 + 10).map(|id| (id, format!("pose {id}"))).collect();
            let mut expected = source.clone();
            expected.retain(|id, _| entities.contains(id));
            let mut actual = source;
            retain_present(&mut actual, &entities, |id| *id);
            assert_eq!(actual, expected, "nombre d'entités : {count}");
        }
    }

    #[test]
    fn departs_et_retours_preservent_les_valeurs_et_najoutent_rien() {
        let mut slots = HashMap::from([(1, Some("action en cours")), (2, None), (3, Some("course"))]);
        retain_present(&mut slots, &[3, 1, 99], |id| *id);
        assert_eq!(slots, HashMap::from([(1, Some("action en cours")), (3, Some("course"))]));
        retain_present(&mut slots, &[2, 3], |id| *id);
        assert_eq!(slots, HashMap::from([(3, Some("course"))]));
        retain_present(&mut slots, &[] as &[i32], |id| *id);
        assert!(slots.is_empty());
    }
}
