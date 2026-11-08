# Retour bug 2 — performances et animation fluide (points 19 et 30)

Travail du 4 novembre 2026, directement dans ArchRust partagé, sans git. Règles client lues ;
notes existantes `re-anim-sort.md` consultées en lecture seule. Aucun changement d'ArchCore,
de FORMATS.md, de free-assets ou de REFERENCE-BENILLA.md. Animation.rs, initialement exclu,
a ensuite été modifié avec l'autorisation explicite du parent pour le seul cache par pose. Aucun lancement
graphique, release, fmt ou publication.

## Changements effectifs

- `crates/archrust-engine/src/render.rs` : les tampons d'instance gardent les derniers octets
  envoyés, via `render_upload_cache.rs`. Une écriture `Queue::write_buffer` n'est faite que
  si ces octets changent. Cache attaché au tampon, partagé entre scène et aperçu hors écran :
  pas de cache par pointeur de maillage, donc pas de confusion entre deux instances du même modèle.
  Matrice, teinte et les six vecteurs de lueur sont comparés exactement, sans seuil flottant.
  Les changements de temps des lueurs, d'ordre des objets, d'équipement et de scène sont couverts
  dès qu'ils modifient le contenu à envoyer. Les sommets skinnés restent réécrits indépendamment.
- `crates/archrust-engine/src/app.rs`, uniquement `animate` : joueur local à chaque image ;
  autres personnages à chaque image s'ils sont à moins de 2 000 unités du joueur OU du point
  visé par la caméra. Au-delà : plafond préexistant de 30 Hz dans le rayon de 8 000 unités du
  point visé. Les proches du joueur restent animés même si une caméra très éloignée vise ailleurs.
  Distances au carré pour ces décisions. `f32::INFINITY` donne un seuil de pose nul dans
  l'API existante (`1 / max_hz`), sans accumulation puis remise à zéro d'une période arrondie.
  Aucun changement de sélection des clips, des sorts, des variantes ou de leur chronologie.
- `examples/performance_cpu.rs` : benchmark autonome CPU, sans fenêtre, GPU ou assets externes.
  Il importe le cache réellement utilisé par le rendu. Sortie brute conservée dans
  `docs/performance-cpu-mesures.txt`.
- Suite demandée par le parent : `app_animator_retention.rs`, appelé dans `app.rs::animate`,
  conserve la recherche linéaire sous 128 entités et construit un HashSet des CID à partir de
  128 entités. Les valeurs des animateurs conservés ne sont ni remplacées ni réinitialisées.
  L'index est temporaire, sans cache persistant. App.rs a été relu immédiatement avant le patch ;
  seuls la déclaration du module et l'appel de rétention ont été ajoutés dans cette suite.
  Les sorties de cast concurrentes ne sont pas modifiées.
- Dernière suite autorisée, agent animation terminé : `animation_prepared.rs` prépare matrices
  monde et correspondance des os une seule fois par clump et par pose. `animation.rs::advance`
  crée un `SkinPoseBatch` après échantillonnage du clip et l'utilise pour les pièces. `deform`
  public reste la référence originale ; les peaux avec `wearer` passent toujours directement
  par cette référence. Les inverses d'atomic, palettes et normales restent propres à chaque
  géométrie. Ni rw.rs, ni skill_speed, ni les règles/tests de sélection des clips ne sont modifiés.

Le cache des instances évite des envois redondants. Le dernier cache CPU est distinct : il ne vit
que pendant une pose, sans réutilisation inter-image. Les poses ne sont pas mutualisées entre personnages : équipements, temps, variantes et sorts
peuvent différer. Aucun nouveau culling n'a été ajouté, afin de ne pas supprimer de géométrie
visible ni de dépendances d'ombre/effet. Les distances de rendu et niveaux de détail des objets
restent ceux du client actuel.

## Mesures reproductibles

Depuis la racine ArchRust, sans Cargo et sans verrouiller son dossier de compilation :

```powershell
rustup run 1.98.1 rustc --edition=2021 -C opt-level=1 crates/archrust-engine/examples/performance_cpu.rs -o target/performance_cpu.exe
& ./target/performance_cpu.exe
```

Compilateur constaté : `rustc 1.98.1 (48a229cea 2026-09-01)`, Windows x64.
Optimisation de développement `opt-level=1`, aucun profil release. Machine partagée avec
d'autres agents en compilation : dispersion importante. Une paire d'échauffement exclue,
quatre paires conservées, ordre avant/après alterné. Pas de conclusion statistique de FPS.

### Copies d'instances : avant/après synthétiques

4 096 instances, 240 images, 176 octets par instance. Avant = copie systématique ; après = même
copie uniquement si le cache de production signale un changement. Le stockage est un tableau
CPU préalloué. **Ce test ne mesure ni wgpu, ni les transferts réels, ni le GPU, ni une scène de jeu.**

| Paramètres variables | Copies avant | Copies après | Médiane CPU avant (µs) | Médiane CPU après (µs) |
|---|---:|---:|---:|---:|
| 0 % | 983 040 | 4 096 | 16 451,5 | 14 999,5 |
| environ 10 % | 983 040 | 102 086 | 11 352,5 | 27 309 |
| 100 % | 983 040 | 983 040 | 17 383,5 | 29 320 |

Le cas statique supprime 239 copies sur 240 après initialisation. Le cas mixte supprime environ
89,6 % des copies. Quand tout change, aucune copie n'est évitée et le cache ajoute un coût CPU.
Le cas mixte est aussi plus lent ici : la comparaison de 176 octets coûte davantage qu'une simple
copie préallouée. **Aucune accélération CPU générale n'est démontrée pour ce cache** ; son intérêt
est la suppression d'appels wgpu de staging, dont le coût n'a pas été mesuré. Il consomme environ
180 octets supplémentaires par emplacement conservé dans le pool, soit environ 720 Kio pour
4 096 emplacements. Un déplacement ou réordonnancement massif réduit son efficacité.

### Recherche des animateurs absents : APPLIQUÉE avec seuil 128

Nouvelle mesure dans `performance-cpu-retention-128.txt` : le benchmark importe la fonction
effectivement appelée dans app.rs et compare l'ancien parcours linéaire à la stratégie avec seuil.
Construction du HashSet comprise, 10 % d'animateurs périmés, 100 passages. L'accumulation est
maintenant en nanosecondes, convertie une seule fois à la fin : plus de zéros par troncature de
chaque passage. Les anciennes mesures brutes sont conservées comme historique séparé.

| Entités | Médiane avant, 100 passages (µs) | Médiane après, 100 passages (µs) | Branche après |
|---|---:|---:|---|
| 32 | 43,5 | 44,5 | Linéaire |
| 127 | 871,5 | 594 | Linéaire |
| 128 | 655,5 | 310 | HashSet |
| 256 | 3 081,5 | 1 278,5 | HashSet |
| 2 000 | 180 777,5 | 10 330,5 | HashSet |

À 128 et 2 000 entités, les quatre essais sont plus rapides avec index. Pour 32 et 127 entités,
le parcours reste linéaire : l'écart observé n'est pas un gain algorithmique et reflète aussi
la dispersion de la machine partagée et les différences d'inlining. Les comparaisons de clés
passent de O(animateurs × entités) à un coût moyen O(animateurs + entités) au-delà du seuil.
L'allocation temporaire du HashSet est incluse, sans nouveau dossier de cache ni Cargo.

Deux tests supplémentaires exercent le code de production : égalité complète des HashMap avec
la référence linéaire pour 0, 1, 32, 127, 128, 129, 256 et 2 000 entrées, CID négatifs, doublons,
départs, retours et préservation des valeurs `Some`/`None`. Ils n'ajoutent pas d'animateur manquant.

### Fréquence des poses

Simulation du seuil temporel actuel, sans déformation de géométrie : sur 60 images en une seconde,
30 déclenchements avant, 60 avec seuil nul ; sur 144 images, 29 avant, 144 avec seuil nul.
Ce sont des nombres d'appels permis par le seuil, **pas des FPS observés**, ni un profil du skinning.
Le surcoût de déformation CPU augmente pour les proches ; il n'a pas été mesuré sur de vrais
personnages. Aucun gain FPS n'est annoncé. Montures et décor animé ont leur propre cadence,
inchangée ici : la fluidité de ces éléments n'est pas couverte par ce bloc.

### Profil réel du skinning TID 96 — avant toute optimisation de rw.rs

`examples/skinning_cpu.rs` mesure le même corps `b0000060.ecl`, le squelette `a0000060.ecl`
et la course `a0060200.ean` que l'audit existant. Corps : 95 frames, 8 pièces skinnées,
1 237 sommets ; squelette : 87 frames, aucune géométrie skinnée. Les packs sont lus sans écriture.
60 poses précalculées réparties sur le cycle, 600 appels par mesure, 16 appels de chauffe,
première série exclue puis quatre séries gardées. Chargement, construction des sommets de bind
et précalcul des poses sont exclus des boucles de matrices et de déformation.

| Traitement | Médiane des 600 appels (µs) |
|---|---:|
| Échantillonnage du clip | 1 697,5 |
| Matrices monde, un clump par pose | 3 190 |
| Matrices de skin, ensemble des 8 pièces par pose | 43 951,5 |
| Déformation positions/normales, ensemble des 8 pièces par pose | 55 020 |
| Associations des ID, recherche linéaire | 1 502,5 |
| Associations des ID, HashMap reconstruite à chaque appel | 2 360 |

Les matrices occupent un ordre de grandeur important du coût total ; les mesures sont séparées,
elles ne constituent pas une décomposition instrumentée d'un même appel. La construction d'un
HashMap des ID est **plus lente dans chacun des quatre essais** sur ce squelette. L'association
obtenue est testée identique à la recherche linéaire, y compris la conservation de la première
frame pour un ID dupliqué (`entry.or_insert`). Mais aucune matrice ni déformation optimisée
n'est produite par cette expérimentation : elle ne prouve donc pas une égalité de poses après
optimisation. **rw.rs est laissé intact**, conformément à la condition de gain prouvé. Un index
préconstruit lors du chargement, avec invalidation si frames publiques modifiées, serait une autre
approche à mesurer ; elle n'est pas appliquée ici.

Reproduction sans Cargo ni nouveau cache de compilation :

```powershell
./tools/profile_skinning_cpu.ps1 -Game 'C:/Users/Marty/Desktop/HomeRPG ArchLord/client/ArchLord/Game'
```

Le script extrait textuellement `SkinBinding`, `Clip`, `deform`, `geometry_vertices` et
`compute_normals` des sources courantes, sans modifier animation.rs ; il ajoute un petit chargeur
direct de packs limité aux noms vérifiés par l'audit. Les champs CPU de `Vertex` sont repris sans
la partie GPU. Les fonctions `Clump` proviennent de la bibliothèque de développement déjà présente
`libarchrust_data-67faac3061927a0f.rlib` (2 novembre), avec `libglam-333f782367142cf9.rlib`.
**Limite : rw.rs n'est pas recompilé par ce script.** Son hash source consigné est un repère,
pas une preuve de correspondance binaire. Les empreintes animation/rw et résultats sont dans
`performance-skinning-96-avant.txt`. Les noms de bibliothèques sont propres au dossier target
actuel ; il faut les adapter si les dépendances sont reconstruites avec d'autres identifiants.
Cette voie a été utilisée car la bibliothèque moteur existante n'était plus réutilisable seule
(dépendances AddOns reconstruites entre-temps), et le parent a demandé de ne pas lancer Cargo.
Le mode normal de l'exemple utilise directement le moteur après sa prochaine compilation complète.

L'audit existant `target/x86_64-pc-windows-msvc/debug/examples/anim_audit.exe` a aussi été exécuté
sur le TID 96 ; sa sortie est dans `performance-anim-audit-96.txt`. Il ne signale pas de normale
nulle/NaN ni de position NaN dans ces poses. Son heuristique de distance signale déjà des sommets
au-delà de quatre rayons de certaines petites pièces : ces alertes de référence sont conservées,
sans les présenter comme une régression ou comme une validation graphique. Cet exécutable
préexistant ne certifie pas les changements concurrents actuels de l'agent animation.

## Vérification de préservation du rendu

Vérification par lecture des chemins et tests CPU, sans observation graphique :

| Famille | Vérification et limite |
|---|---|
| Objets statiques et LOD | Listes, matrices, distances, textures et ordre de dessin conservés ; seules les écritures identiques sont omises. |
| Peaux et équipements | `write_vertices` indépendant du cache ; pas de partage des sommets entre personnages ajouté ; teintes et matrices incluses. |
| Herbe et eau | `world_renderer.prepare`, `draw_grass`, `draw_water` inchangés ; leur animation ne dépend pas du cache ajouté. |
| Ombres | `shadow_request` et `shadow.encode` inchangés ; aucune nouvelle exclusion de géométrie. |
| Lumières | Les globales sont toujours réécrites ; lumières ponctuelles et directionnelles ne sont pas mises en cache. |
| Effets et lueurs | Lots de particules/effets inchangés ; les paramètres temporels d'armure sont inclus dans la comparaison. |
| Plaques et noms | `boards_view` et projection des panneaux inchangés, calculés à chaque image. |
| Aperçus hors écran | Utilisent le même cache que les tampons partagés ; le retour vers la scène réécrit les paramètres différents. |

L'absence de régression visible n'est pas certifiée sans essai visuel ; aucun client graphique
n'a été lancé, conformément à la consigne. Pas de benchmark FPS ou de capture GPU.

## Tests, compilation et coordination

Tests autonomes du cache (aucun verrou Cargo) :

```powershell
rustup run 1.98.1 rustc --edition=2021 --test crates/archrust-engine/src/render_upload_cache.rs -o target/render_upload_cache_tests.exe
& ./target/render_upload_cache_tests.exe
```

Deux tests : première écriture puis stabilité, changement de chacun des 176 octets ;
alternance scène / aperçu / scène avec invalidation correcte.

`cargo +1.98.1 check -p archrust-engine` après le bloc rendu : **réussi** (14,86 s ; avertissement
de hard links du cache incrémental). Après le bloc app.rs : lancé, puis **échec dans un fichier
non modifié**, `animation.rs:45`, octet 233 non UTF-8 dans le commentaire « négatifs ».
Une vérification supplémentaire lancée ensuite est restée en attente du verrou ; elle a été
interrompue au signalement du parent. Aucun Cargo ne reste actif de mon côté et aucune relance
n'a été faite après sa demande. Le check final de l'ensemble partagé reste à refaire par le parent
après correction de l'encodage et stabilisation des écritures concurrentes.

Le benchmark compile et s'exécute sans charger le moteur. Il ne remplace pas cette vérification
finale. Jusqu'à cette étape, animation.rs était resté strictement sous la responsabilité de
l'autre agent ; voir ci-dessous pour la dernière modification explicitement autorisée.

Suite point 30 : **aucun Cargo supplémentaire lancé**, conformément à la demande du parent.
Les quatre tests CPU (deux cache, deux rétention) ont réussi via :

```powershell
rustup run 1.98.1 rustc --edition=2021 --test crates/archrust-engine/examples/performance_cpu.rs -o target/performance_retention_tests.exe
& ./target/performance_retention_tests.exe
```

La compilation directe du profil réel et son exécution ont également réussi. La vérification
Cargo du branchement dans app.rs est laissée au check déjà en file chez le parent ; aucun autre
target-dir ni cache n'a été créé. Bilan de cette suite : rétention indexée appliquée et mesurée,
profil du skinning réel conservé, optimisation HashMap des os rejetée sur mesure défavorable.

## Dernière optimisation appliquée : préparation CPU par pose/clump

Après fin des travaux de l'agent animation, nouvelle autorisation du parent et relecture du
fichier stabilisé : ajout d'un module et de trois lignes ciblées dans `advance`. La fonction
publique `deform` n'a pas été retouchée. Les changements de vitesse des sorts, les tests de clips
et les règles de fin/interruption de cast sont conservés. Aucun changement supplémentaire
d'app.rs ou de rw.rs dans cette étape.

`SkinPoseBatch` est local à l'appel d'`advance`. Pour chaque clump non-wearer distinct :
`world_matrices_with_pose` et `frame_by_index` sont calculés au premier besoin, puis repris par
les géométries suivantes. L'identité de l'Arc distingue deux clumps même avec les mêmes ID d'os.
La recherche linéaire d'ID, plus rapide sur ce modèle que le HashMap testé précédemment, est
conservée mais exécutée une fois au lieu de huit. L'ordre des multiplications flottantes est
identique à `Clump::skin_matrices`. Le monde rooted des attaches reste calculé séparément comme
avant ; aucune transformation de tête/arme/effet n'est déduite approximativement du monde non-rooted.
La liste de préparations est détruite en fin d'appel : aucune invalidation persistante ni adresse
de clump conservée après sa durée de vie.

### Égalité et mesures réellement exécutées

`skinning_cpu` compare désormais `deform` à `SkinPoseBatch::deform` sur **60 poses du cycle de
course, 8 géométries et 1 237 sommets** du corps 96. Tous les composants de position, normale,
UV et couleur sont comparés par `f32::to_bits`, sans epsilon : **égalité exacte réussie**.
Pas d'extension de cette conclusion à tous les corps du jeu.

Sortie finale : `docs/performance-skinning-96-prepare.txt`, même protocole CPU que ci-dessus,
600 poses complètes par essai et quatre essais après chauffe. L'ordre avant/après alterne.
La préparation, ses allocations, toutes les palettes et les déformations des huit pièces sont
incluses dans le temps après ; les poses sont précalculées dans les deux variantes.

| Essai | Deform original, huit pièces (µs) | Préparation partagée + huit déformations (µs) |
|---|---:|---:|
| 1 | 48 566 | 20 709 |
| 2 | 53 170 | 27 895 |
| 3 | 49 719 | 19 910 |
| 4 | 67 742 | 21 986 |
| Médiane | **51 444,5** | **21 347,5** |

Soit environ **58,5 % de temps CPU en moins sur ce traitement mesuré**, ou 85,7 → 35,6 µs
par pose complète de ce corps dans ce benchmark. Ce résultat justifie le branchement réalisé.
Il ne mesure ni uploads GPU, ni FPS, ni scène multijoueur. Les limites de bibliothèque de données
précompilée et de machine partagée restent celles indiquées plus haut ; le nouveau code préparé
est, lui, extrait directement du module ajouté et compilé dans le même exécutable de comparaison.

Deux tests du nouveau module ont aussi été compilés et exécutés directement via rustc :
**2 réussis, 0 échec**. Ils vérifient égalité exacte sur des poses synthétiques successives,
une seule préparation pour un même clump réutilisé, deux préparations pour deux clumps distincts,
inverse d'atomic distincte, poids non normalisés, fallback wearer inchangé et absence de hiérarchie.
Ils sont également inclus dans les tests normaux du moteur pour le prochain passage Cargo.

Reproduction du profil et des assertions sur le corps réel :

```powershell
./tools/profile_skinning_cpu.ps1 -Game 'C:/Users/Marty/Desktop/HomeRPG ArchLord/client/ArchLord/Game'
```

Le script inclut maintenant le module préparé dans l'extraction. Pour reproduire les tests
autonomes après cette extraction, reprendre sa commande rustc, ajouter `--test`, et remplacer
la sortie par `target/skinning_prepared_tests.exe`, puis lancer cet exécutable.

**Éditions terminées. Aucun Cargo lancé dans cette dernière étape : le parent peut faire son
workspace/check final.** Aucun client graphique, fmt, release ou publication.


## Validation d'intégration finale du coordinateur

Le moteur compile et `cargo +1.98.1 test --workspace` réussit après intégration :
547 tests réussis, 0 échec, 3 tests d'assets ignorés par défaut. Les deux tests
réels d'icônes de buffs ont été exécutés explicitement et réussissent aussi.
Voir [le bilan commun](retour-bug-2-personnages.md) et
[le journal final](retour-bug-2-tests-workspace.txt) pour les conditions et limites.
