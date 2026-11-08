//! Derniers octets réellement envoyés dans un tampon d'instance, indépendants du maillage.
//! Un aperçu hors écran et la scène doivent partager ce cache comme leur tampon GPU.

#[derive(Default)]
pub struct UploadCache {
    bytes: Option<[u8; 176]>,
}

impl UploadCache {
    /// Comparaison exacte : inclut matrice, teinte et paramètres temporels des lueurs.
    pub fn changed(&mut self, bytes: &[u8]) -> bool {
        let bytes: &[u8; 176] = bytes.try_into().expect("taille de l'uniforme d'instance");
        if self.bytes.as_ref() == Some(bytes) {
            return false;
        }
        self.bytes = Some(*bytes);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premiere_ecriture_et_tous_les_champs() {
        let mut cache = UploadCache::default();
        let mut bytes = [0; 176];
        assert!(cache.changed(&bytes));
        assert!(!cache.changed(&bytes));
        for i in 0..bytes.len() {
            bytes[i] ^= 1;
            assert!(cache.changed(&bytes), "octet {i}");
            assert!(!cache.changed(&bytes));
        }
    }

    #[test]
    fn apercu_reordonnancement_et_retour_scene() {
        let mut cache = UploadCache::default();
        let scene = [1; 176];
        let apercu = [2; 176];
        assert!(cache.changed(&scene));
        assert!(cache.changed(&apercu));
        assert!(cache.changed(&scene));
        assert!(!cache.changed(&scene));
    }
}
