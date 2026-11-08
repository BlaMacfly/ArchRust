# Retour bug 2 — point 24 : objets au sol

## Correction

Seul fichier source modifié : `crates/archrust-engine/src/app_combat.rs`, tests inclus.
`FIELD_DFF` positif sélectionne désormais `f{id:07x}.ecl`. Suppression du repli
sur `BASE_DFF` : champ absent, nul, invalide ou négatif → aucun modèle de table.
Le commentaire de l'or désigne maintenant `f000010c.ecl`.

Preuve utilisée : notes existantes `re-objet-au-sol.md`, chargement observé dans
`FUN_140644660` : BASE = D, SECOND = E, FIELD = F, PICK = G ; aucun repli BASE
pour le clump FIELD. Aucune nouvelle analyse du binaire effectuée.

Pas de changement du placement, des transformations ni du traitement des atomics.
Le cube de diagnostic préexistant reste utilisé si le modèle manque ou ne charge pas :
ce n'est pas un repli BASE. Le dessin original sans clump F et l'utilisateur de
`FIELD_TRANSFORM` restent inconnus selon les notes RE.

## Vérification CID/TID en lecture seule

- `app_effects.rs` : mort → `effects_event(cid, COND_DEAD)` ; coup réussi/critique
  → `effects_event(target_cid, COND_STRUCK)`. `effects_event` résout le CID via
  `effects_tid_of`, lit les données et le cache par TID, puis ancre par CID.
- `app_stones.rs` : `stones_on_hit(attacker, target)` trouve le porteur par CID,
  lit le rang avec le TID de son arme, les sertissages avec l'IID de l'arme,
  puis joue l'effet sur le CID cible.

Aucune confusion CID/TID repérée dans ces chemins ; aucune modification nécessaire
signalée pour ce motif. Cette vérification statique ne valide pas l'arrivée des
événements ni la présence du personnage au moment du traitement.

## Validation

- `cargo +1.98.1 check -p archrust-engine` : réussi.
- `ARCHRUST_GAME_DIR` défini vers le dossier `client/ArchLord/Game`, puis
  `cargo +1.98.1 test -p archrust-engine --lib modele_au_sol -- --nocapture` :
  **2 tests réussis, 0 échec**, aucun test ignoré (310 autres filtrés).
- Diagnostic : TID 123 sélectionne `f0000076.ecl` ; `f0000076.ecl`,
  `f000002e.ecl` et `f000010c.ecl` possèdent chacun 1 atomic et 1 géométrie.
  Le témoin porté `d0000076.ecl` possède 8 atomics.
- Seul avertissement de compilation relevé : liens physiques du cache incrémental
  indisponibles, remplacement automatique par copie. Aucun échec de compilation.

Aucun release, lancement graphique, fmt, git ou publication. Aucun changement
à ArchCore, aux autres fichiers source ou au document FORMATS partagé.


## Validation d'intégration finale du coordinateur

Le moteur compile et `cargo +1.98.1 test --workspace` réussit après intégration :
547 tests réussis, 0 échec, 3 tests d'assets ignorés par défaut. Les deux tests
réels d'icônes de buffs ont été exécutés explicitement et réussissent aussi.
Voir [le bilan commun](retour-bug-2-personnages.md) et
[le journal final](retour-bug-2-tests-workspace.txt) pour les conditions et limites.
