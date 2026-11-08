# Retour de bug n° 2 — personnages, buffs et performances

Travail local du 4 novembre 2026. Aucun changement à ArchCore, aucune publication,
aucun build release et aucun lancement du client graphique. Les autres modifications
présentes dans le dossier partagé sont conservées.

## État par point

| Point | Résultat | Preuve et limite |
|---|---|---|
| 15 | Animation corporelle de sort raccordée à la réception du lancement, pour le joueur et les autres personnages. | Avant : `Cast` ne renseignait que la recharge locale. Clip lu dans `SKILL_ANIMATION_NAME` au rang du nom dans `UsableSkillTName`, ensemble courant puis 0. Type -11, une seule lecture, redémarrage forcé. Voir rapport animation. |
| 16 | Effets des buffs raccordés aux ops 4/5/6, y compris pour les autres personnages. | Le modèle du lanceur vient du troisième champ de `SKILL_BUFF`, précédemment perdu ; `FUN_14056ad80` → `FUN_140572c00` → `FUN_1405748b0` : ensemble 0, condition 0x80000. Les lignes D1 ne partent plus lors du lancement (D=0). Contrôleurs possédés par (CID porteur, TID buff), renouvellement sans redémarrage et retrait ciblé. |
| 17 | Source de l'icône alignée et tests de texture/dessin ajoutés ; symptôme de la capture encore à reproduire. | `SMALL_SKILL_TEXTURE`, pack `texture/skill`, sans repli grande icône pour les buffs ; les autres usages gardent leur repli. Les fichiers de cinq sorts/consommables existent. Les temps sur la barre sont des recharges et ne prouvent pas la réception d'un buff. Aucun buff inventé depuis un lancement. Voir rapport icônes. |
| 19 | Pose rafraîchie à chaque image pour le joueur et les proches. | Le clip interpolait déjà ses clés ; le skinning était plafonné à 30 Hz. Le seuil devient nul à proximité, reste 30 Hz au loin. Mesure du nombre de poses, pas mesure FPS. |
| 24 | Chargement du modèle de butin corrigé. | `FUN_140644660` : FIELD_DFF → F, BASE_DFF → D. Le mauvais D chargeait une armure complète de Hunter ; F est un petit objet non skinné. Mort/coup et pierres : pas de confusion CID/TID trouvée dans les chemins relus. |
| 29 | Buff reçu pendant la course : arrêt local puis clip du sort. | `FUN_14069d250` arrête tout lanceur avant le clip ; pas de condition propre aux buffs. Un nouvel ordre de déplacement est différé localement pendant le blocage. Le serveur doit également figer son trajet : besoin serveur ci-dessous. |
| 30 | Optimisations CPU/rendu et mesures hors écran ; gain FPS à mesurer en jeu. | Cache des paramètres d'instance GPU identiques, rétention des animateurs indexée dès 128 entités et préparation des matrices partagée par pose/clump ; détails et mesures reproductibles dans le rapport performances. Aucun gain FPS à Tullan n'est affirmé. |

## Buffs : cycle de vie et limites

Les annonces ops 4 et 6 conservent maintenant le TID du modèle du lanceur, en plus
du TID de compétence et de la durée. L'op 6 remplace l'état du porteur. L'effet
attend que le modèle soit disponible ; une annonce renouvelée ne repart pas de
zéro. Un retrait, une échéance ou une sortie de vue retire ses contrôleurs sans
retirer les effets d'un autre personnage ni ceux d'une autre compétence.
Les effets permanents d'apparition, des armes et des colliers restent indépendants.
Les boucles sont celles des bases du fichier `.eff` : aucun cycle de relancement
arbitraire n'est ajouté. Les options de masquage sont respectées ; une réactivation
peut recréer les effets toujours actifs.

Exemples vérifiés avec les données Game, modèle 96 : Speed Attack (TID 4) → EID
3455, nœuds 101/102 ; Immortal Calling (8) → 3452 ; Resistance (130) et Spirit
Shield (227) → 3475 ; Sprint (229) → 3454 ; Lens Stone (457) → 3560.

Les relevés d'Archonia utilisés se trouvent dans les notes `re-buffs.md`,
`re-anim-sort.md` et `re-objet-au-sol.md` de la session initiale. Binaire de référence :
`reference-archonia/Archonia.exe`, SHA-256
`e9c7609b02c30a17959aac37321750f6785e64f5b582f660a7c5bfef6cddc98f`.
Les adresses ci-dessus sont des observations statiques, pas un essai d'exécution.
Lors de cette première passe, les demandes REA de `FUN_140572c00` et `FUN_140574420` ont
chacune expiré après 300 s, sans repli Ghidra externe ; la session a ensuite été fermée.
La reprise de décompilation du 4 novembre 2026 a depuis établi le sélecteur D par les
instructions du PE et une décompilation Ghidra ciblée : le buff exige D=1 ; le lancement
accepte D≤0, D absent valant −1. Le parseur d'origine conserve la dernière occurrence.
M1 déclenche une mise à jour de la représentation et des objets portés ; N teste une
donnée attachée dont l'interprétation complète reste à établir. Voir `FORMATS.md`.
Le filtre Rust suit cette règle depuis le 8 novembre : buff D=1 exigé, lancement D≤0
(absent = −1), dernière occurrence retenue (`cust_d`, test `selecteur_d_des_donnees_d_effet`). Ces nouveaux relevés
ne constituent ni une modification du client installé ni une validation en jeu.

## Besoin serveur (lecture seule)

À la réception d'un lancement effectif, Archonia arrête le déplacement local.
Le serveur doit figer la position du lanceur et annuler son ancienne destination
avant de poursuivre la simulation/diffusion des déplacements. Le client ne doit
pas envoyer un nouveau paquet STOP après réception du cast : dans le serveur
consulté, `presence::on_move` annule aussi l'incantation. Aucun correctif serveur
n'est réalisé ici. Essayer notamment un buff sur soi lancé en pleine course,
observé simultanément par un autre joueur.

Pour le point 17, une cause serveur n'est pas prouvée. Les sources consultées
contiennent bien les constructeurs et envois ops 4/6. Relever le TID exact du sort
et le journal de réception permet de trancher : une recharge n'est pas une annonce
de buff. Le rapport icônes détaille la vérification du HUD sans fenêtre.

## À essayer en jeu

- Chevalier et Mage, plusieurs armes puis sans arme : sort offensif et buff ;
  corps animé, orientation, effet et icône, puis retour à l'attente.
- Buff en course : arrêt au lancement, dernier clic repris après blocage ;
  observer la scène depuis un second joueur et vérifier la position serveur.
- Renouveler Speed Attack, empiler des buffs de même EID, retirer un seul buff,
  laisser expirer, sortir/rentrer de vue et se reconnecter : aucun doublon persistant.
- Masquer/réactiver les effets, y compris ceux des autres joueurs.
- Faire tomber Shadowdash Boots (TID 123) après une élimination : modèle F au sol,
  aucun torse/bras de Hunter ; vérifier aussi or, arme et consommable.
- Marche/course à 60 et 144 Hz, monture comprise ; contrôler les proches et les
  changements d'équipement, sans attendre du plafond lointain la fluidité proche.
- À Tullan : même position, caméra et réglages, relever médianes et images lentes,
  temps CPU/GPU et nombre d'appels de dessin. Comparer objets, skinning, herbe,
  ombres, lumières, effets et plaques de noms séparément. Les mesures synthétiques
  hors écran ne remplacent pas cette comparaison graphique.

## Validation

Résultats finaux consignés à la fin du présent document après stabilisation des
modifications concurrentes. Commande finale imposée : `cargo +1.98.1 test --workspace`,
avec `ARCHRUST_GAME_DIR` pointant sur les données Game et `CARGO_INCREMENTAL=0`.


## Mesures CPU et décisions du point 30

- Corps réel du chevalier : 60 poses de course, 8 pièces, 1 237 sommets.
  Préparation du squelette commune aux pièces d'un même clump, strictement dans
  une même pose et pour un seul personnage. Les sommets restent individuels.
  Positions, normales, UV et couleurs identiques bit à bit à la référence sur
  ces 60 poses. Le chemin des peaux liées à un porteur reste inchangé.
- Pour 600 poses complètes, médiane CPU : **51,44 ms avant, 21,35 ms après**
  (environ −58,5 % pour ce traitement), préparation et allocations incluses.
  Cela n'est ni un gain de FPS ni une mesure de transferts GPU.
- Rétention des animateurs, 2 000 entités et 100 passages : **180,78 ms avant,
  10,33 ms après**, index temporaire compris. Sous 128 entités, parcours linéaire
  conservé. La proposition d'un HashMap d'os reconstruit par appel a été écartée
  parce qu'elle était plus lente sur le squelette réel mesuré.
- Paramètres GPU identiques : à 4 096 instances statiques et 240 images, la
  simulation passe de 983 040 copies à 4 096. La comparaison ajoute un coût CPU
  dans les cas dynamiques ; aucun gain global n'est prétendu sans mesure wgpu.
- Herbe, ombres, lumières, effets et noms : chemins relus et limites recensées
  dans le rapport performances ; pas de suppression de contenu visible ni de
  nouveaux seuils de qualité. Leur temps GPU et le gain à Tullan restent à mesurer.

Les mesures brutes et les commandes de reproduction sont conservées dans les
rapports associés. La machine était partagée avec des compilations concurrentes ;
les temps absolus et leur dispersion doivent être interprétés dans ce contexte.


## Validation finale — réussie

- `cargo +1.98.1 check -p archrust-engine` : réussi après intégration de toutes les corrections.
- `cargo +1.98.1 test --workspace` : **547 réussis, 0 échec, 3 ignorés par défaut**, tests de documentation compris ; code de sortie 0.
- Environnement : `ARCHRUST_GAME_DIR=C:/Users/Marty/Desktop/HomeRPG ArchLord/client/ArchLord/Game`, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2` pour la dernière passe.
- Les deux tests ignorés d'icônes des buffs ont aussi été exécutés explicitement avec les données réelles : **2/2 réussis** (pixels sans substitut et formes egui du HUD sans fenêtre native). Le troisième test ignoré par défaut concerne l'icône Karkaen, hors des sept points de ce lot.
- Le workspace valide les tests des nouveaux contrôleurs de buffs, les clips réels chevalier/Mage/Hunter, les modèles F/D du butin, le cache des uniformes et les deux tests de préparation des poses et de rétention des animateurs.
- Trois tests d'intégration réseau avaient initialement été bloqués par l'erreur Windows 10013 sur les sockets. Ils passent dans la relance avec accès aux connexions locales autorisé. Aucun test n'a été désactivé pour contourner cet échec.
- Un ancien initialiseur du test `archrust-addons/tests/item_catalog.rs` a été complété par `..Default::default()` pour accepter les champs du catalogue ajoutés par les autres travaux. Aucun changement de sa logique ni des champs existants.

Journal complet de la dernière passe : [retour-bug-2-tests-workspace.txt](retour-bug-2-tests-workspace.txt).

Rapports détaillés : [animation et course](retour-bug-2-animation.md),
[icônes](retour-bug-2-icones.md), [butin](retour-bug-2-objets-au-sol.md),
[performances et mesures](retour-bug-2-performances.md).
Le format et les règles de lecture sont décrits dans [FORMATS.md](FORMATS.md).

Le point 17 n'est pas présenté comme reproduit puis résolu en jeu : le chemin actuel
texture/dessin est validé, mais la réception des buffs de la capture demeure à tracer.
L'arrêt serveur du point 29 et la mesure FPS à Tullan restent les suites nécessaires,
sans modification du serveur ni lancement graphique dans ce travail.

## Complément : diagnostic en jeu et intervention serveur précise

Des traces opt-in `archrust_buffs` suivent maintenant la réception, le décodage,
la décision de porteur local, la durée/expiration, les ressources et la demande
de dessin des icônes, sans journaliser chaque image stable.
Le helper serveur approprié et l'emplacement d'intégration sont précisés dans
[le guide du diagnostic 17 et de la correction serveur 29](retour-bug-2-diagnostic-17-et-serveur-29.md).
Aucune modification d'ArchCore n'est réalisée.


Validation du complément de traces : check moteur réussi ; workspace **547 réussis,
0 échec, 3 ignorés par défaut**, et tests réels d'icônes **2/2 réussis** après les
modifications. Voir les journaux `retour-bug-2-tests-traces.txt` et
`retour-bug-2-tests-traces-icones.txt`. Le guide 17/29 propose l'arrêt serveur sans
STOP préalable au lanceur pour éviter l'écho du client actuel ; ce code serveur
reste une proposition non appliquée et non testée côté ArchCore.

## Complément du 8 novembre 2026 : champ de la caméra, cadence des poses, piste du point 17

- **Champ de la caméra** (`render_frustum.rs`) : avant l'envoi au GPU, les pièces dont la
  sphère englobante sort d'un des quatre plans latéraux ne sont ni écrites ni dessinées
  (pièces animées : rayon doublé, 150 unités au moins). Mesure sur les placements réels
  de Tullan (`examples/frustum_audit.rs`, caméra par défaut, 8 directions, objets comptés
  pour des sphères de 500 unités) : **26 % des objets à portée restent à dessiner**, soit
  environ 74 % d'appels de dessin d'objets en moins. Gain en images par seconde : à mesurer en jeu.
- **Pose hors du champ** : le temps du clip avance, la pose n'est pas réécrite ; elle l'est
  dès le retour dans le champ. Les ombres projetées ne portent que sur le décor : rien de visible.
- **Cadence à 30 Hz** (personnages éloignés) : le reste de la période est reporté au lieu
  d'être remis à zéro. Simulation de 6 000 images à 60 ips avec ±0,5 à ±2 ms de gigue :
  **24 poses/s avant, 30 après**, à intervalles réguliers (point 19, au loin).
- **Boucle de dessin** : noms de textures des pièces mis en minuscules à la création du
  maillage (plus d'allocation par pièce et par image) ; tampons et texture gardés quand la
  pièce suivante utilise le même maillage ou la même texture. Ordre de dessin inchangé.
- **Point 17, piste serveur (lecture seule)** : ArchCore n'envoie l'op 4 d'un buff de joueur
  que si `SkillLevel::leaves_effect` est vrai (`skills.rs`, `lasting()` : durée > 0 et
  modificateurs, dégâts périodiques ou purge reconnus, ou un état posé). Un sort de durée
  non nulle dont les colonnes de `SKILL_CONST` ne sont pas reprises par le serveur n'est
  donc jamais annoncé : ni icône ni effet chez le client. Non prouvé pour les sorts de la
  capture : à vérifier avec les traces `archrust_buffs` (une ligne `moteur_cast` sans
  `moteur_ajout` du même TID désigne ce cas).

Validation de ce complément : voir la fin du présent document.

Validation du complément (8 novembre) : `cargo +1.98.1 check -p archrust-engine` réussi ;
`cargo +1.98.1 test --workspace` (`ARCHRUST_GAME_DIR` sur les données Game, `CARGO_INCREMENTAL=0`) :
**551 réussis, 0 échec, 3 ignorés par défaut**, code de sortie 0. Nouveaux tests : `champ_de_la_camera`,
`sphere_d_une_piece`, `selecteur_d_des_donnees_d_effet`. Journal :
[retour-bug-2-tests-8-novembre.txt](retour-bug-2-tests-8-novembre.txt). Toujours ni build release, ni
lancement graphique, ni modification d'ArchCore.
