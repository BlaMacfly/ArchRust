# Diagnostic en jeu du point 17 et correction serveur du point 29

Complément local du 4 novembre 2026. Seul ArchRust est modifié. Aucun client
graphique, serveur, build release ou publication n'est lancé par ce travail.

## Activer les traces du point 17

Utiliser un exécutable **recompilé avec ce complément** : les exécutables déjà
présents ne sont pas copiés ni mis à jour ici. Depuis son dossier, dans PowerShell :

```powershell
$ancienFiltre = $env:RUST_LOG
$env:RUST_LOG = 'info,archrust_buffs=debug'
try {
    & '.\ArchRust.exe' play --game 'C:\Users\Marty\Desktop\HomeRPG ArchLord\client\ArchLord\Game' 2> '.\buffs-17.log'
} finally {
    $env:RUST_LOG = $ancienFiltre
}
```

Adapter uniquement le chemin de l'exécutable si son nom ou son emplacement diffère.
Le journal env_logger est sur la sortie d'erreur et comporte des horodatages à la
milliseconde. Le filtre dédié est commun au réseau et au moteur. Pour n'enregistrer
que ce diagnostic : `off,archrust_buffs=debug`. Pour extraire ensuite les lignes :

```powershell
Select-String -Path '.\buffs-17.log' -Pattern 'archrust_buffs'
```

Les nouvelles traces ne contiennent ni mot de passe, ni paquet brut. Elles exposent
les CID, identifiants/TID de compétences, durées, modèle du lanceur, noms de textures
et coordonnées du HUD. Le reste du journal `info` conserve les traces habituelles.

### Essai à réaliser

1. Se connecter et noter le sort choisi, l'heure du clic et son TID si connu.
2. Lancer un buff réellement appris, à l'arrêt, sans utiliser `/buff`.
3. Le renouveler, attendre une expiration ou le retirer ; observer la ligne des icônes.
4. Refaire avec un consommable dont l'icône s'affiche, puis avec un buff lancé en course.
5. Conserver le journal complet depuis la connexion jusqu'au problème, avec la version
   du client et le nom du sort. Les CID/TID et horodatages permettent la corrélation.

`/buff` sert seulement à isoler le HUD : il produit `hud_ajout` sans réception réseau
correspondante. Il ne démontre jamais une annonce du serveur. Une recharge affichée
sur la barre n'est pas non plus une preuve d'amélioration active.

### Étapes visibles dans le journal

| Trace | Ce qu'elle établit |
|---|---|
| `reseau_recu` | Paquet SKILL arrivé dans le traducteur : op 4/5/6/7, taille, présence du décodage général et nombre d'entrées de buff. En cas d'échec général, op inconnue et `paquet_decode=false`. |
| `reseau_decode op=4` | Ajout traduit : porteur, instance de compétence, TID, durée en ms, modèle du lanceur, nombre d'entrées et réussite du décodage de la première mini-entrée. |
| `reseau_decode op=6` | Liste traduite : entrées reçues/retenues et tuples `(TID, ms, modèle)` ; les mini-entrées invalides ou TID 0 sont filtrées par le comportement existant. |
| `reseau_decode op=5` | Retrait traduit : porteur et TID. |
| `reseau_non_traduit` | L'op concernée n'a pas produit d'événement reconnu : examiner décodage et champs manquants avant d'accuser le HUD. |
| `moteur_cast` / `moteur_refus` | Lancement ou refus, CID local et TID reçu/connu ; `annonce_buff=false` rappelle que l'op 7 n'ajoute aucun buff. |
| `moteur_ajout` / `moteur_liste` / `moteur_retrait` | Décision d'envoyer au HUD : CID porteur comparé au joueur local, `accepte_hud`, données reçues. Un autre porteur n'a normalement pas d'icône dans le HUD local. |
| `hud_ajout` | Entrée ajoutée ou renouvelée, durée et taille de liste précédente. Ce n'est pas une preuve du paquet brut. |
| `table_icones_illisible` / `icone_absente` | Table impossible à lire ou absence de `SMALL_SKILL_TEXTURE` pour ce TID. |
| `texture_chargee` / `texture_substitut` / `texture_illisible` | Nom résolu, dimensions et patch, ou problème de ressource. Le chargement n'est tracé qu'au remplissage du cache. |
| `hud_dessin` | Demande de dessin : TID, icône, texture disponible, rectangle, écran et intersection avec l'écran (`dans_ecran`). Cela ne prouve pas le résultat final sur le moniteur. |
| `hud_expiration` / `hud_retrait` / `hud_reset` | Suppression locale par échéance, retrait explicite ou remise à zéro de connexion. Une liste op 6 vide peut également vider le HUD, ce que montre `moteur_liste`. |

Le dessin est tracé à la première apparition et quand sa disponibilité ou sa zone
change, pas à chaque image stable. Un renouvellement autorise une nouvelle ligne de
dessin même si la texture est déjà en cache. Les entrées de suivi sont supprimées
au retrait ou à l'expiration ; aucune décision d'affichage ne dépend de ce suivi.
La texture peut donc ne pas produire une nouvelle ligne `texture_chargee` après
chaque ajout : rechercher la première occurrence de son nom plus haut dans le journal.

### Interpréter le premier maillon manquant

- `moteur_cast` sans op 4 ni entrée dans une op 6 : aucune annonce de buff constatée
  dans cette fenêtre du journal. Cela oriente vers l'application/émission serveur ou
  la réception en amont ; ce n'est pas à lui seul une preuve de défaut serveur.
- `reseau_recu` sans traduction : contrôler `paquet_decode`, `reseau_non_traduit`,
  les entrées invalides et les champs de base du porteur.
- Traduction correcte mais `accepte_hud=false` : contrôler le CID du porteur et le
  chargement du joueur (`joueur=None` sur les ajouts reçus trop tôt).
- `hud_ajout` suivi d'une liste vide, d'un retrait ou d'une expiration : identifier
  l'événement qui efface l'entrée et comparer la durée réseau en millisecondes.
- `hud_ajout` sans `hud_dessin` : vérifier que le HUD du monde est effectivement
  dessiné et qu'aucun événement de suppression ne s'intercale.
- `texture_substitut`, `texture_illisible` ou `icone_absente` : examiner la table,
  les packs, les patchs et le chemin Game de ce client.
- `hud_dessin` avec texture et zone correctes, mais icône absente à l'écran : le
  chemin testé a demandé l'image ; poursuivre la composition, les superpositions
  et le rendu, avec une capture prise au même moment.

Limite préexistante désormais observable : l'op 4 ne traduit que la première
mini-entrée du champ 8. Si `entrees` est supérieur à 1, les suivantes ne produisent
pas d'ajouts distincts sur ce chemin. L'op 6 parcourt toute sa liste. Aucun
changement de protocole ou de comportement n'est introduit pour masquer ce cas.

## Point 29 : correction serveur requise, non appliquée

### Constat en lecture seule

Dans `ArchCore-core/crates/archcore/src/world/skills.rs`, `begin` passe de
`Stage::Approach` à `Stage::Casting`, débite les coûts et diffuse `cast_packet`
sans supprimer `p.dest`. `presence::move_step`, appelé par `combat::step` avant
`skills::step` à chaque tour, continue donc à avancer cette ancienne destination
aux tours suivants.

Un helper existe déjà : **`presence::stop(st, sid)`**, dans `world/presence.rs`.
Il avance la marche jusqu'à l'instant d'arrêt, met `p.dest = None`, marque le
joueur modifié et diffuse `OPT_CHAR_MOVE` au lanceur et aux observateurs.
`move_fields` encode alors position = destination, avec STOP | SYNC. Ce helper
n'annule pas `p.skills.cast`.

À distinguer de **`skills::stop`**, qui annule le lancement, et de
**`presence::on_move`**, qui traite un ordre client et efface notamment
`p.skills.cast`. Un paquet STOP envoyé par le client n'est donc pas la solution.

### Emplacement et ordre à respecter

Intervenir dans `skills::begin`, **au lancement effectif**, après les contrôles
qui peuvent encore refuser le sort (portée, état, ressources, munitions), et après
les branches qui conservent `Stage::Approach` pour marcher jusqu'à portée.
Ne pas immobiliser dès la demande initiale ni chaque fois qu'une approche est en cours.

Juste avant la prise mutable qui débite les ressources et écrit `Stage::Casting`
(repère actuel : `let name = t.name.clone();` puis `let p = st.players.get_mut…`) :

```rust
// Proposition pour ArchCore, NON appliquée dans ce travail.
let name = t.name.clone();
// Nouveau helper proposé ci-dessous : arrêt serveur et observateurs seulement.
super::presence::stop_for_cast(st, sid, now);
let p = st.players.get_mut(&sid).unwrap();
// Conserver ensuite les coûts, ready_at, Stage::Casting et les notifications existants.
```

C'est une proposition d'intégration fondée sur les helpers consultés, pas un patch
serveur compilé ni testé ici. Utiliser la position arrêtée pour les calculs qui
suivent : notamment, pour un sort sur soi sans point au sol, le repli de
`target_pos` doit venir de `p.ch.pos()` après l'arrêt plutôt que d'un ancien `me`.
Ne jamais reprendre l'ancienne destination comme position du lanceur pendant le cast.

### Attention au STOP renvoyé par le client actuel

Le helper existant diffuse aussi l'arrêt au lanceur. Dans ArchRust actuel,
`app.rs` applique `CharacterMoved` sans remettre `walking` à faux ; `simulate`
envoie ensuite `Command::Stop` s'il constate une arrivée avec `walking` encore
vrai. Si STOP et Cast sont traités dans deux images différentes, cet écho peut
atteindre `presence::on_move` et annuler le cast. C'est un risque déduit de ces
chemins sources ; aucun essai réseau graphique ne l'a encore reproduit ici.

**Ne pas remplacer le helper proposé `stop_for_cast` par l'actuel `presence::stop`
sans traiter cet écho.** Pour une correction limitée au serveur,
réutiliser sa mise à jour de position et `dest = None`, mais adapter la diffusion
d'arrêt du cast aux **observateurs seulement** ; le lanceur reçoit le Cast et
l'arrête déjà localement. Vérifier alors sa resynchronisation de position.
L'autre possibilité exige une correction cliente distincte : un arrêt imposé
remet `walking` à faux sans émettre de STOP en retour ; le helper complet peut
alors être conservé. Aucune de ces modifications fonctionnelles n'est appliquée
par ce complément de traces.

Ne pas supposer que deux paquets envoyés ensemble seront traités dans la même
image, et ne pas introduire un filtrage serveur heuristique qui ignorerait les
vrais nouveaux ordres de déplacement.

### Proposition concrète du helper serveur seul

À ajouter ultérieurement **dans `world/presence.rs` côté serveur**, pour pouvoir
réutiliser son `advance` privé. Ce code est uniquement un exemple d'intégration
non compilé ici ; il ne figure dans aucun fichier d'ArchCore modifié.

```rust
pub fn stop_for_cast(st: &mut WorldState, sid: u32, now: Instant) {
    let Some(p) = st.players.get_mut(&sid) else { return };
    advance(p, now);
    p.dest = None;
    p.follow_target = 0;
    p.follow_distance = 0;
    p.dirty = true;
    let cid = p.ch.id;
    let fields = move_fields(st, sid); // position = destination, STOP | SYNC
    for (other, o) in &st.players {
        if *other != sid && o.seen_players.contains(&cid) {
            o.send(&schema::OPT_CHAR_MOVE, fields.clone());
        }
    }
}
```

Ce helper ne touche ni `p.skills.cast`, ni les ressources, ni les recharges.
Contrairement à `broadcast_move` actuel, sa boucle exclut le lanceur pour ne pas
provoquer l'écho décrit plus haut. Le lanceur continue à recevoir l'op 7 par
`combat::send_around`, qui l'inclut déjà. Ne pas remplacer les autres usages du
helper `presence::stop` : cette variante concerne le lancement effectif seulement.

L'ordre réseau attendu sur la même connexion est :

1. `OPT_CHAR_MOVE` d'arrêt synchronisé, position et destination égales, aux observateurs ; au lanceur seulement après garantie de non-écho décrite ci-dessus ;
2. notifications de facteurs existantes, puis AGPMSKILL op 7 annonçant le cast ;
3. effets/résultat aux échéances existantes, sans nouvelle diffusion de l'ancien trajet.

Ne pas effacer `p.skills.cast` pour arrêter la marche, ne pas rappeler `on_move`,
ne pas utiliser la fonction d'annulation `skills::stop`, et ne pas ressusciter
l'ancienne destination à la fin. Un nouvel ordre de mouvement suit ensuite les
règles existantes d'interruption ; ce correctif ne doit pas réécrire la durée du
sort, sa recharge ou son coût. L'inhibition actuelle de l'attaque normale jusqu'à
`complete_at` reste à préserver : vérifier qu'aucune autre approche automatique
ne remet une destination pendant l'incantation.

### Tests d'acceptation à ajouter côté serveur ultérieurement

- Buff sur soi pendant une course : après `begin`, destination absente, cast encore
  présent en `Stage::Casting`, ressources/recharge appliquées une seule fois.
- Au tour suivant, sans nouvel ordre, la position ne continue pas vers l'ancien but.
- Un second joueur reçoit l'arrêt avant le cast ; aucune reprise de MOVE_FAST vers l'ancien but n'arrive pendant le clip. Tester STOP et Cast traités dans des images distinctes : aucun écho client ne doit annuler l'incantation. Pour la variante serveur seul, le lanceur reçoit le Cast sans STOP préalable, avec vérification de sa position.
- Sort offensif ou au sol hors portée : l'approche demeure possible, puis l'arrêt
  intervient seulement quand le lancement commence réellement.
- Sort refusé : ni coût ni op 7 ; pas d'arrêt ajouté prématurément par cette correction.
- Nouveau mouvement explicite : comportement d'interruption existant conservé ;
  sans nouvel ordre, aucun retour automatique à l'ancienne destination après le buff.

Aucun fichier d'ArchCore n'a été modifié et aucun de ces tests serveur n'a été
exécuté ici. La règle client provient des relevés existants d'Archonia
`FUN_14069d250` (arrêt avant le clip) et `FUN_1404bce90` (nouveau mouvement accepté),
déjà cités dans le rapport d'animation.


## Validation du complément client

- `cargo +1.98.1 check -p archrust-engine` : réussi après les blocs de traces.
- `cargo +1.98.1 test --workspace` : **547 réussis, 0 échec, 3 ignorés par défaut**, code de sortie 0.
- Tests réels d'icônes exécutés explicitement après ajout des traces : **2/2 réussis** (pixels des packs et dessin egui sans fenêtre native).
- Données : `ARCHRUST_GAME_DIR=C:/Users/Marty/Desktop/HomeRPG ArchLord/client/ArchLord/Game` ; `CARGO_INCREMENTAL=0`, chaîne 1.98.1.
- Journaux : [workspace](retour-bug-2-tests-traces.txt) et [icônes](retour-bug-2-tests-traces-icones.txt).

Ces contrôles valident le client, pas la proposition de code serveur ni le symptôme
en jeu. Aucun fichier d'ArchCore, exécutable de distribution ou interface originale
n'est modifié ; aucune publication ni exécution du client graphique.
