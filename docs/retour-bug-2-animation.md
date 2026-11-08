# Points 15 et 29 — lancement des sorts et arrêt de course

## Coordination

Fichiers possédés : `animation.rs` et `app_skills.rs`. Une interception ciblée
du clic au sol dans `app.rs` permet de différer le déplacement local.
Aucun changement dans
`app_effects.rs`, `worn.rs`, ArchCore ou le serveur. L'interpolation et les
optimisations restent à l'autre agent ; aucun changement dans `Clip::sample`,
la déformation ou la cadence de mise à jour.

Le protocole Buffs `(tid, ms, caster_tid)` est accepté dans `app_skills.rs` ;
le troisième champ ne sert pas à l'affichage de la barre.

## Cause et correction

`on_skill_event` ne jouait aucun clip : `Cast` local ne mettait à jour que la
recharge et `Cast` distant était ignoré. Le chemin ajouté arrête la destination
du lanceur avant toute recherche de clip, indépendamment du type de sort.
Le rang vient de `UsableSkillTName` et le choix de clip de
`SKILL_ANIMATION_NAME`, avec ensemble d'arme courant puis ensemble 0 uniquement.
Les animateurs absents ou périmés sont reconstruits au lancement.
L'ensemble est recalculé avec `worn::anim_equip_for` sur l'équipement actuel,
sans attendre la prochaine passe d'animation : un changement d'arme récent
est donc pris en compte. Le TID absent du Cast local peut être retrouvé dans
les compétences connues par identifiant ; aucun emprunt de cette liste pour
un lanceur distant.

Le type de lancement -11 est distinct de l'attaque et des actions sociales.
Le même clip redémarre à chaque lancement ; un touché ne le remplace pas.
La monture reçoit l'attente, jamais le type négatif du cavalier.
Le lanceur se tourne vers la position connue de `Cast.target` si la cible
est différente de lui. L'ancien trajet est abandonné. Un clic au sol pendant
le blocage local attend son expiration ; le dernier clic remplace le précédent.
Sans nouvel ordre, le personnage reste à l'attente. Un autre sort demandé
pendant ce blocage n'est pas envoyé.

## Vitesse et limites explicitement connues

La vitesse du clip local utilise le facteur `(9, 3)` : `1 + m / 100`.
La durée de blocage suit la formule RE distincte :
`durée du clip × max(0, 1 − m / 100)`. `cast_ms` ne pilote ni le clip ni
ce blocage ; il reste utilisé avec `recast_ms` pour la recharge existante.
Le plafond global et ses réductions régionales de `FUN_1402fb110` ne sont
pas identifiés : aucune constante de plafond n'a été inventée. Le facteur
local non négatif est utilisé directement. Le `max(0)` appliqué au facteur est
un choix défensif de l'implémentation, pas une règle prouvée par la RE.
Les facteurs des autres personnages
ne sont pas conservés dans `Entity` : vitesse nominale pour ces lanceurs.
Ces deux limites restent à compléter pour une fidélité intégrale de la vitesse.
L'exception de ruée hors portée, les points d'impact et les drapeaux rares de
fondu/enchaînement ne sont pas ajoutés dans ce correctif.

## Serveur — intervention nécessaire, sans modification ici

Les notes `re-anim-sort.md` observent dans `FUN_14069d250` un arrêt avant le
clip, et dans `FUN_1404bce90` qu'un nouveau déplacement peut remplacer -11.
Le serveur doit donc arrêter le trajet du lanceur au lancement effectif et ne
pas continuer à diffuser son ancien déplacement. Le client ne doit pas masquer
indéfiniment les nouveaux mouvements réseau : Archonia les applique.
Aucun `Command::Stop` n'est envoyé au Cast : `presence::on_move` annulerait
la compétence côté serveur. La correction serveur doit immobiliser le lanceur
dans son propre chemin de lancement, sans passer par une annulation de mouvement
venant du client. Le serveur n'a pas été modifié.

## Validation

Premier contrôle après adaptation Buffs : réussi.
Contrôle après ajout du chemin de lancement : réussi avec
`cargo +1.98.1 check -p archrust-engine`.
L'inspection avec `ARCHRUST_GAME_DIR` réel a confirmé les noms de fichiers :
TID 96, rang 0, ensemble 1 → `d0006001.ean`, ensemble 3 → `d0006003.ean`.
Il n'existe pas d'ensemble 0 pour ce rang : à mains nues ou ensemble 7,
aucun clip n'est choisi, conformément à Archonia. Le rang 38 fournit
`d0060260.ean` dans l'ensemble 0, utilisable en repli depuis l'ensemble 7.
Les cinq tests `animation::tests` avec les données réelles ont réussi :
chargement, durée/pistes, échantillonnage et redémarrage du vrai clip,
protection contre le touché, absence de lancement sur un mort et multiplicateur
1,25 pour un facteur 25. Mesures chevalier : `d0006001.ean` 1,266667 s,
`d0006003.ean` 1,4 s, `d0060260.ean` 6 s, `d0060320.ean` 5 s ; 86 pistes chacun.
Le test réel est ensuite étendu à Mage et Hunter par recherche de `Name` et
contrôle de `Class` dans `CharacterDataTable.txt`, sans supposer de TID.
Lecture réelle vérifiée : Mage TID 6, classe 4 ; Hunter TID 8, classe 2.
Attention : la vraie table donne 98 = GM, 102 = Hen.
Dernier contrôle après cette extension :
`cargo +1.98.1 check -p archrust-engine` réussi.
Test ciblé `animation::tests::clips_sorts_reels` avec `ARCHRUST_GAME_DIR`
réel : réussi, après extension Mage/Hunter. Mage : rang 0, ensemble 1,
`d0000601.ean`, 2,466667 s, 90 pistes. Hunter : rang 0, ensemble 1,
`d0000801.ean`, 2,266667 s, 101 pistes. Les cinq tests d'animation avaient
également tous réussi avant cette extension du seul test de données.
Les avertissements observés concernent les liens physiques du cache incrémental ;
aucune erreur de compilation ni de test. Aucun Cargo encore en cours de mon côté.

Une erreur transitoire d'encodage introduite par le script d'édition a été
corrigée. Vérification explicite des trois fichiers touchés : UTF-8 valide,
zéro CRCRLF, lignes vides doublées retirées sans reformatage ni `cargo fmt`.
Aucun client graphique ni release lancé.


## Validation d'intégration finale du coordinateur

Le moteur compile et `cargo +1.98.1 test --workspace` réussit après intégration :
547 tests réussis, 0 échec, 3 tests d'assets ignorés par défaut. Les deux tests
réels d'icônes de buffs ont été exécutés explicitement et réussissent aussi.
Voir [le bilan commun](retour-bug-2-personnages.md) et
[le journal final](retour-bug-2-tests-workspace.txt) pour les conditions et limites.

Le complément de diagnostic et l'emplacement précis de l'arrêt serveur sont dans
[le guide 17/29](retour-bug-2-diagnostic-17-et-serveur-29.md). Il précise aussi le
risque d'écho STOP du client actuel si l'arrêt et le Cast sont traités dans deux
images différentes ; le helper serveur ne doit pas être appliqué aveuglément.
