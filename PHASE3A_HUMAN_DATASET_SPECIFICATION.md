# ERYTHEON — Phase 3A

## Spécification du dataset humain, audit qualité BDIFF et protocole de validation

## 1. Décision et périmètre

Cette phase est documentaire et analytique. Les lectures de production ont été exécutées en transaction `READ ONLY`. Aucun modèle, endpoint, calcul FWI, schéma ou enregistrement de production n’a été modifié. Aucune migration n’a été créée ou appliquée.

Conclusion :

```text
PHASE 3A SPECIFICATION READY
```

La spécification est assez précise pour préparer une phase 3B additive et testée hors production. Les questions ouvertes recensées à la fin limitent certains audits géographiques, mais ne bloquent pas l’architecture ni les règles scientifiques fondamentales.

## 2. État réel du dépôt

### 2.1 Git et migrations

- Branche : `main`
- Commit audité : `361d46800815d2be8ad49c75932ff42ced64d7a6`
- Migrations SQLx présentes : `0001` à `0010`
- Rollbacks présents : `0009` et `0010`
- Fichier préexistant non suivi : `PHASE2_PRODUCTION_DEPLOYMENT_REPORT.md`
- Aucun commit créé pendant la phase 3A.

### 2.2 Architecture observée

Les schémas `raw`, `staging`, `reference`, `environment`, `human`, `fire`, `features`, `risk`, `validation`, `ml`, `serving` et `ops` existent depuis `0009`. La majorité ne contient pas encore les tables métier prévues pour le dataset humain.

Le flux BDIFF actuel est :

```text
CSV national normalisé
→ crates/ingest/src/fire_history.rs
→ Observation HistoricalIgnition
→ public.ignition_history
→ jointure public.cell_static
→ filtrage des causes humaines
→ échantillonnage de cellules combustibles
→ modèle logistique
→ public.human_model_versions
```

Il n’existe pas encore de copie BDIFF append-only dans `raw`, de staging normalisé versionné, de table métier dans `fire`, d’audit de labels dans `validation` ou de dataset matérialisé dans `ml`.

### 2.3 Tables concernées

- `public.ignition_history` : événements BDIFF et Prométhée normalisés.
- `public.cell_static` : features territoriales JSON par H3.
- `public.calendar_days` : vacances et jours fériés.
- `public.fwi_state` : FWI historique opérationnel récent.
- `public.forecast_fwi` : prévisions FWI.
- `public.risk_scores` : scores opérationnels.
- `public.human_model_versions` : modèles humains versionnés.
- `reference.data_sources`, `ops.import_batches`, `ops.pipeline_runs` : fondation de traçabilité, actuellement utilisée par FIRMS.

### 2.4 Modules concernés

- `crates/ingest/src/fire_history.rs` : lecture BDIFF/Prométhée.
- `crates/engine/src/static_layers.rs` : chargement de l’historique et calcul de `hist`.
- `crates/store/src/lib.rs` : persistance, sélection des labels et contrôles.
- `crates/engine/src/human_model.rs` : entraînement, négatifs et métriques.
- `crates/engine/src/backtest.rs` : replay temporel météo/FWI et classement.
- `crates/risk/src/lib.rs` : features humaines, heuristique et modèle appris.

### 2.5 Règles actuelles

- Résolution de production : H3 8.
- Projection : latitude/longitude WGS84 directement vers une cellule H3.
- Combustibilité : `cell_static.features.combustible`.
- Causes positives : `Malveillance`, `Involontaire (particulier)`, `Involontaire (travaux)` et `Accidentelle`.
- Causes naturelles et inconnues : exclues des positifs supervisés.
- Négatifs : cellules combustibles sélectionnées par hash déterministe, puis associées à une date pseudo-aléatoire déterministe.
- Ratio actif : 4 négatifs par positif.
- Exclusion négative actuelle : uniquement le même couple H3/date qu’un positif du split concerné.
- Features apprises : WUI, route, agriculture, population, POI, ligne électrique, week-end, vacances scolaires, jour férié, sinus et cosinus saisonniers.
- `hist` n’entre pas dans le vecteur du modèle logistique actif. C’est une protection utile contre une fuite directe de la cible.
- Validation du modèle actif : entraînement 2020–2024 et validation 2025.
- Seuil d’activation : ROC-AUC minimale.
- Backtest : reconstruction de l’historique avec les événements strictement antérieurs au jour évalué, ce qui évite une fuite future dans `hist`.

## 3. Audit BDIFF

### 3.1 Source réellement conservée

Le fichier de production normalisé contient exactement :

```text
external_id
occurred_at
municipality
latitude
longitude
surface_ha
cause
```

Département, région, code INSEE, type détaillé de feu, géométrie source, méthode de localisation et précision déclarée ne sont pas présents.

### 3.2 Volumes et période

- BDIFF : 15 956 événements.
- Période BDIFF : 1er janvier 2020 au 31 décembre 2025.
- Prométhée : 1 événement de démonstration daté de 2000.
- Identifiants BDIFF distincts : 15 956.
- `dedupe_key` distinctes : 15 956.
- Identifiants manquants : 0.
- Événements sans timestamp normalisé : 0.
- Événements sans latitude/longitude : 0.
- Événements sans commune : 0.
- Événements sans cause : 0.
- Événements sans surface : 0.
- Événements sans H3 : 0, car la colonne est obligatoire après normalisation.

### 3.3 Causes observées

| Cause source | Nombre | Classe actuelle |
|---|---:|---|
| Inconnue | 8 071 | unknown |
| Malveillance | 2 405 | human_known |
| Involontaire (particulier) | 2 017 | human_known |
| Involontaire (travaux) | 1 349 | human_known |
| Accidentelle | 1 323 | human_known |
| Naturelle | 791 | natural_known |

Les six libellés sont présents chaque année de 2020 à 2025 sans variation orthographique observée dans la base normalisée. Cela démontre la stabilité du normaliseur actuel, pas nécessairement celle de la nomenclature brute du portail.

Répartition des causes humaines :

- 2020 : 991
- 2021 : 1 034
- 2022 : 1 781
- 2023 : 1 371
- 2024 : 700
- 2025 : 1 217

### 3.4 Précision géographique

- Coordonnées distinctes : 5 997.
- H3 distincts : 5 997.
- Communes distinctes : 5 901.
- Groupes commune/coordonnée répétés : 2 607.
- Événements appartenant à ces groupes répétés : 12 562.
- Communes avec plusieurs événements mais une seule coordonnée : 2 528.
- Événements dans ces communes : 12 282.
- Coordonnées partagées par plusieurs noms de communes : 4 groupes, 25 événements.

L’égalité entre coordonnées distinctes et H3 distincts montre qu’aucune collision H3 n’apparaît dans ce chargement, mais ne prouve pas une précision ponctuelle.

Les 12 282 événements associés à une coordonnée unique répétée dans leur commune constituent un signal fort de centroïde communal probable. Ce nombre ne doit pas être présenté comme un nombre confirmé de centroïdes tant qu’aucun référentiel communal et aucun code INSEE ne permettent une comparaison géométrique.

Toutes les coordonnées actuelles doivent donc être qualifiées au mieux `precision_undocumented` avant enrichissement.

### 3.5 Doublons potentiels

- Groupes même jour/même H3 avec plusieurs événements : 418.
- Événements concernés : 918.
- Taille maximale observée : 8.

Ces groupes ne sont pas des doublons démontrés. Plusieurs feux distincts peuvent survenir le même jour dans une commune dont tous les événements utilisent le même centroïde.

### 3.6 Limites documentaires

Le stockage actuel a déjà normalisé et réduit la source. Il ne permet pas d’auditer :

- les colonnes brutes éventuellement présentes dans le portail ;
- la nomenclature source détaillée avant regroupement ;
- le département ou la région source ;
- le code INSEE ;
- la qualité déclarée de la localisation ;
- une éventuelle géométrie différente du point ;
- le type détaillé d’incendie ;
- la provenance de chaque correction du normaliseur.

La phase 3B devra commencer par une conservation brute append-only du prochain export, sans réécrire `public.ignition_history`.

## 4. Taxonomie des causes v1

La taxonomie est immuable sous l’identifiant proposé :

```text
erytheon_cause_taxonomy_v1
```

Chaque règle conserve `source_label`, `normalized_label`, version, date d’effet et justification.

| Libellé source | Catégorie | Sous-catégorie | Inclusion positive |
|---|---|---|---|
| Malveillance | human_known | malicious | oui, sous contrôles qualité |
| Involontaire (particulier) | human_known | private_activity_negligence | oui |
| Involontaire (travaux) | human_known | work_activity | oui |
| Accidentelle | human_known | accident_unspecified | oui, analyse de sensibilité |
| Naturelle | natural_known | natural_unspecified | non |
| Inconnue | unknown | unknown_unspecified | non |

Catégories réservées :

- `indeterminate` : libellé présent mais incompatible avec une décision déterministe humain/naturel.
- `invalid_or_unusable` : valeur vide, ligne techniquement invalide ou événement hors périmètre impossible à interpréter.

Règles :

1. comparaison sur une forme Unicode normalisée et espaces périphériques retirés ;
2. mapping par table de règles exacte, jamais par recherche approximative silencieuse ;
3. tout nouveau libellé non mappé devient `indeterminate`, pas `unknown` par défaut ;
4. le libellé original n’est jamais remplacé ;
5. une nouvelle règle exige une nouvelle version ou un amendement versionné ;
6. un mapping peut être rejoué et inversé grâce à la conservation de la source.

`Accidentelle` est humain dans le normaliseur existant, mais son manque de détail justifie une analyse de sensibilité avec et sans cette sous-catégorie.

## 5. Qualification géographique

Taxonomie proposée :

- `precise_reported` : précision explicitement documentée par la source.
- `estimated_reported` : point estimé déclaré par la source.
- `municipality_centroid_confirmed` : proximité avec un centroïde officiel confirmée par code INSEE et référentiel versionné.
- `municipality_centroid_probable` : coordonnée répétée pour une commune, sans confirmation officielle.
- `rounded_coordinate_probable` : précision décimale ou grille d’arrondi détectée.
- `corrected_geometry_separate` : géométrie dérivée disponible, sans écraser l’original.
- `unknown_location` : localisation absente ou inexploitable.
- `precision_undocumented` : point valide mais méthode inconnue.

Champs requis :

- latitude/longitude et géométrie originales ;
- H3 calculé et résolution ;
- méthode de localisation source ;
- catégorie de qualité ;
- précision estimée en mètres ;
- règle et version ;
- motifs de faible confiance ;
- commune et code INSEE ;
- distance au centroïde communal ;
- distance à la cellule combustible la plus proche ;
- géométrie dérivée éventuelle et justification.

La transformation vers une cellule voisine ne doit jamais modifier la cellule originale. Elle doit créer une proposition séparée avec méthode, distance, résolution et niveau de confiance.

## 6. Détection des doublons

Les doublons sont des candidats audités, jamais des suppressions.

### 6.1 Signaux disponibles

- égalité de l’identifiant source ;
- écart temporel ;
- distance entre coordonnées ;
- H3 et résolution ;
- commune ;
- surface brûlée ;
- cause source et cause normalisée.

Le type détaillé du feu, le département et les autres champs bruts ne sont pas disponibles aujourd’hui.

### 6.2 Score proposé v1

- Identifiant source identique : décision `certain_duplicate`, score 1,00.
- Même commune et coordonnées, écart ≤ 5 minutes, même cause, surface identique ou écart relatif ≤ 5 % : `probable_duplicate`, score minimal 0,90.
- Même jour/H3/commune avec au moins deux concordances parmi heure proche, cause identique et surface proche : `possible_duplicate`, score 0,60 à 0,89.
- Même jour/H3 mais signaux contradictoires ou centroïde probable : `indeterminate`, score 0,35 à 0,59.
- Écart temporel important, localisations différentes ou attributs nettement incompatibles : `probably_distinct`, score < 0,35.

Chaque groupe conserve :

- identifiant stable du groupe ;
- membres ;
- paires comparées ;
- signaux et valeurs ;
- score ;
- version des règles ;
- décision automatique proposée ;
- décision humaine éventuelle ;
- justification ;
- timestamps d’audit.

Les groupes même jour/H3 ne doivent pas recevoir automatiquement une décision de doublon.

## 7. Cas humains non combustibles

Sur 7 094 événements humains connus :

- 6 422 sont associés à une cellule combustible ;
- 650 sont associés à une cellule non combustible ;
- 22 n’ont aucune ligne `cell_static`.

Répartition des 672 cas difficiles :

| Année | Nombre |
|---|---:|
| 2020 | 92 |
| 2021 | 119 |
| 2022 | 122 |
| 2023 | 137 |
| 2024 | 73 |
| 2025 | 129 |

| Cause | Nombre |
|---|---:|
| Involontaire (particulier) | 246 |
| Malveillance | 218 |
| Accidentelle | 125 |
| Involontaire (travaux) | 83 |

Parmi ces cas :

- 620 utilisent une coordonnée répétée ;
- 650 ont une valeur route positive ;
- 611 ont une population positive ;
- 621 ont des POI positifs ;
- 293 ont des lignes électriques positives ;
- aucun n’a WUI ou agriculture positive dans la feature actuelle.

Cette signature est cohérente avec des centroïdes communaux situés en zone urbaine/non combustible. Elle ne prouve pas que l’ignition réelle s’est produite sur une surface non combustible.

Département, distance à la cellule combustible, distance à une route et distance à une zone urbaine ne peuvent pas être mesurés de façon fiable avec le schéma conservé. La phase 3B devra dériver le département par intersection avec un référentiel versionné et calculer les distances hors de la table source.

Traitements à comparer, sans application en 3A :

1. dataset strict : exclusion du modèle principal, mais conservation dans les audits ;
2. dataset inclusif : conservation avec indicateur `low_geographic_confidence` ;
3. représentation multi-cellules autour du point source ;
4. cellule combustible voisine proposée, jamais substituée silencieusement ;
5. analyse H3 8 versus H3 9 ;
6. analyse de sensibilité avec/sans ces événements.

## 8. Couverture des features

Sur 920 016 cellules :

- combustible : 761 560 ;
- route positive : 791 678 ;
- WUI positive : 562 314 ;
- agriculture positive : 560 500 ;
- population positive : 483 363 ;
- POI positifs : 351 620 ;
- ligne électrique positive : 210 653 ;
- historique positif : 108 163.

Les features statiques ont été chargées les 18 et 19 juillet 2026. Elles représentent donc un snapshot actuel appliqué rétroactivement aux événements 2020–2025.

Autres volumes :

- calendrier : 2025–2027 uniquement ;
- FWI opérationnel : 18–25 juillet 2026 ;
- forecast/risk : 792 998 cellules sur quatre horizons.

## 9. Risques scientifiques démontrés

### 9.1 Fuite temporelle

- Les features territoriales 2026 sont utilisées pour des événements 2020–2025.
- Les normalisations spatiales sont calculées sur le snapshot national courant.
- Le calendrier ne couvre que 2025–2027. Pour 2020–2024, l’entraînement actuel remplace silencieusement vacances et jours fériés par `false`.
- Le test 2025 a déjà servi de validation et au seuil d’activation du modèle courant. Il n’est donc plus totalement intact pour les décisions déjà prises.
- Une future feature historique doit être calculée uniquement avec les événements strictement antérieurs à la date de la ligne.

### 9.2 Fuite spatiale

- Chevauchements de H3 positifs : 277 train/calibration, 381 train/test et 149 calibration/test.
- L’échantillonnage actuel partage 121 cellules négatives entre les ensembles d’entraînement et de validation actifs.
- Une validation temporelle seule peut mémoriser des caractéristiques locales.

### 9.3 Faux négatifs

- Les négatifs actuels excluent seulement les positifs humains connus du même split et du même couple cellule/date.
- Les causes inconnues, feux naturels, événements proches dans le temps ou l’espace et événements non encore publiés peuvent contaminer les négatifs.
- L’échantillonnage uniforme de cellules peut produire un problème artificiellement facile et mal représenter les contextes météo à risque.

### 9.4 Biais

- 51 % des causes sont inconnues.
- Les centroïdes communaux probables concentrent artificiellement les événements dans des cellules urbaines.
- Les événements non combustibles sont actuellement exclus du modèle sans audit explicite.
- La nomenclature source est déjà regroupée et masque les mécanismes détaillés.
- Les données statiques sont plus complètes dans certains territoires que dans d’autres.
- Une case-control logistic regression n’est pas naturellement une probabilité absolue.

## 10. Spécification du dataset humain v1

### 10.1 Identité

Nom logique :

```text
erytheon_human_ignition_cell_day_v1
```

Unité primaire : cellule H3 8 × date civile Europe/Paris. Une ligne positive peut référencer plusieurs événements source ; les événements restent distincts dans la table métier. Cette unité évite de surpondérer automatiquement plusieurs enregistrements du même jour et de la même cellule.

Une future version par fenêtre de 6 ou 24 heures nécessitera une validation préalable de la précision des heures BDIFF.

### 10.2 Cible

- `human_ignition = 1` : au moins un événement `human_known` admissible dans la cellule-date.
- `human_ignition = 0` : cellule-date combustible sélectionnée sans événement humain connu, naturel ou inconnu dans les fenêtres d’exclusion.
- `natural_known` : cohorte séparée, jamais équivalente à une absence de feu.
- `unknown` : cohorte descriptive exclue du dataset binaire.
- `indeterminate` et `invalid_or_unusable` : exclus avec motif.

### 10.3 Positifs

Inclusion stricte :

- source BDIFF ;
- taxonomie `human_known` v1 ;
- timestamp valide ;
- H3 original conservé ;
- règles de doublons exécutées ;
- qualité géographique enregistrée ;
- features disponibles et versionnées ;
- aucune feature future.

Exclusions du dataset strict, mais conservation dans les audits :

- doublon certain déjà représenté par un autre membre ;
- qualité géographique inconnue incompatible avec l’analyse ;
- absence de features ;
- cellule non combustible, jusqu’à décision de sensibilité ;
- événement hors période ou territoire.

Un dataset inclusif parallèle doit conserver les cas géographiques difficiles avec des indicateurs, afin de mesurer le biais introduit par le dataset strict.

### 10.4 Naturels et inconnus

Les naturels forment une classe d’évaluation secondaire permettant de tester si le modèle humain les classe différemment. Ils ne sont pas les négatifs principaux.

Les inconnus ne participent ni aux positifs ni aux négatifs. Leur présence interdit l’utilisation de leur cellule-date et d’une fenêtre spatiale/temporelle voisine comme négatif sûr.

### 10.5 Négatifs

Population éligible :

- cellule combustible dans le snapshot autorisé ;
- date du split ;
- aucune cause humaine connue dans la cellule/date ;
- aucun événement naturel, inconnu ou indéterminé dans la cellule/date ;
- exclusion autour des événements : au minimum H3 k-ring 1 et ±1 jour, à tester en sensibilité ;
- features et météo disponibles à la date ;
- territoire/saison/FWI comparables aux positifs.

Échantillonnage :

- stratification région ou département dérivé ;
- mois/saison ;
- quantile FWI ou contexte météo ;
- densité territoriale ;
- qualité de couverture des features ;
- mélange de négatifs proches et éloignés des positifs ;
- plafonnement de la réutilisation d’une cellule ;
- graine explicite ;
- tri et identifiants déterministes.

Ratios à comparer : 1:1, 1:4 et 1:10. Le ratio 1:4 reste le benchmark, pas une vérité scientifique.

### 10.6 Splits

- entraînement : 2020–2023 ;
- calibration : 2024 ;
- test final verrouillé : 2025 ;
- suivi prospectif : 2026.

Comptages humains avant autres exclusions :

- entraînement : 5 177, dont 4 707 combustibles ;
- calibration : 700, dont 627 combustibles ;
- test : 1 217, dont 1 088 combustibles.

Les règles sont figées avant toute nouvelle lecture des performances 2025. Comme 2025 a déjà été observé pour le modèle actif, 2026 devra servir de confirmation prospective réellement indépendante.

### 10.7 Traçabilité d’une ligne

Chaque ligne référence :

- dataset/version et état ;
- commit Git et migration ;
- taxonomie ;
- règles géographiques et doublons ;
- source et batch ;
- événement(s) métier éventuel(s) ;
- cellule, résolution, date et split ;
- label et justification ;
- méthode de sélection ;
- seed ;
- features et versions de sources ;
- timestamps de disponibilité ;
- contrôles qualité ;
- motifs d’inclusion/exclusion ;
- checksum logique.

## 11. Validité temporelle des variables

| Famille | Règle |
|---|---|
| Météo | utiliser uniquement observation/prévision émise avant le temps de prédiction ; conserver `issued_at` et `valid_at` |
| FWI | état calculé seulement avec la météo disponible jusqu’au jour précédent ou à l’heure de prévision définie |
| Routes | snapshot daté ; accepter comme approximation stable avec analyse par millésime |
| Population | millésime INSEE explicite ; pas de valeur publiée après l’événement sans drapeau d’approximation |
| Agriculture/CORINE | millésime du produit et date de validité |
| WUI | dérivation versionnée à partir des snapshots OSM/CORINE |
| POI | snapshot OSM versionné |
| Lignes électriques | snapshot OSM versionné |
| Calendrier | table complète reconstruite pour chaque année 2020–2026 |
| Historique feux | événements strictement antérieurs à la date/heure de la ligne |
| Agrégats spatiaux | calcul dans le split et avec données antérieures ; paramètres de voisinage versionnés |
| Normalisation | paramètres ajustés uniquement sur l’entraînement, puis appliqués sans recalcul à calibration/test |
| Imputation | ajustée sur l’entraînement uniquement |

Les features statiques dont aucun snapshot historique n’existe doivent porter `historical_availability = approximate_current_snapshot` et faire l’objet d’une analyse de sensibilité, plutôt que d’être présentées comme historiquement exactes.

## 12. Protocole de validation

### 12.1 Validation temporelle

1. Développer variables, règles et hyperparamètres sur 2020–2023.
2. Ajuster exclusivement la calibration sur 2024.
3. Verrouiller modèle, seuils et calibration.
4. Évaluer une seule fois sur 2025.
5. Publier ensuite les performances prospectives 2026 sans réentraînement rétroactif silencieux.

### 12.2 Validation géographique

Méthode A : leave-one-department/group-out.

- dériver le département avec un référentiel administratif versionné ;
- regrouper les départements peu représentés ;
- entraîner sans le groupe ;
- évaluer uniquement sur le groupe tenu à l’écart.

Méthode B : blocs spatiaux.

- construire des blocs H3 parents ou une grille indépendante ;
- tenir des blocs entièrement hors entraînement ;
- ajouter un buffer spatial pour réduire la dépendance entre voisins ;
- répéter sur plusieurs folds géographiques déterministes.

Une troisième lecture par régions administratives entièrement non vues est recommandée pour la communication opérationnelle.

### 12.3 Calibration

- calibrateur ajusté uniquement sur 2024 ;
- comparer calibration logistique et isotonic regression ;
- choisir sur log loss, Brier et courbe de calibration 2024 ;
- appliquer le calibrateur figé à 2025 ;
- ne parler de probabilité absolue que si l’échantillonnage case-control est corrigé par la prévalence et validé prospectivement.

Dans l’état actuel, le score doit être nommé `propension relative` ou `rang de risque`.

### 12.4 Métriques

- ROC-AUC ;
- average precision et PR-AUC ;
- log loss ;
- Brier score ;
- calibration intercept/slope ;
- courbes de calibration ;
- précision/rappel à budget opérationnel fixé ;
- recall dans les top 1 %, 5 % et 10 % ;
- précision parmi les cellules les mieux classées ;
- lift par décile/percentile ;
- résultats par département/région ;
- résultats par saison ;
- résultats par quantile FWI ;
- résultats par qualité géographique ;
- résultats strict/inclusif ;
- intervalles d’incertitude par bootstrap spatial ou temporel.

### 12.5 Benchmarks

1. modèle actuellement en production ;
2. heuristique actuelle ;
3. baseline simple : régression logistique avec saison, route, WUI et population ;
4. nouveau candidat sur dataset versionné.

Le remplacement exige une amélioration robuste sur 2025 et les holdouts géographiques, sans dégradation critique de calibration ou de rappel opérationnel, puis une confirmation prospective 2026.

## 13. Architecture technique proposée

Flux :

```text
raw.bdiff_records
→ staging.bdiff_events_normalized
→ fire.ignition_events
→ validation.event_label_quality
→ validation.duplicate_candidate_groups
→ features.feature_snapshots
→ ml.dataset_versions
→ ml.dataset_rows
→ ml.dataset_event_links
→ serving (phase ultérieure)
```

### 13.1 Tables proposées

`raw.bdiff_records`

- payload source append-only ;
- batch, identifiant source, récupération, checksum et parsing ;
- aucune correction destructive.

`staging.bdiff_events_normalized`

- types normalisés ;
- libellé source conservé ;
- statut de parsing ;
- version du normaliseur ;
- lien raw.

`fire.ignition_events`

- événement métier stable ;
- source et source record ;
- temps, géométrie originale, H3/résolution ;
- commune/code INSEE dérivé ;
- cause originale et taxonomie ;
- aucune suppression automatique.

`validation.event_label_quality`

- catégorie de cause ;
- qualité géographique ;
- combustibilité ;
- exclusions proposées ;
- règles/version et justification.

`validation.duplicate_candidate_groups` et membres

- score, décision, signaux, règles et membres.

`features.feature_snapshots`

- source, millésime, disponibilité, checksum, paramètres de normalisation.

`ml.dataset_versions`

- version immuable, commit, migrations, périodes, règles, sources, seed, statistiques et checksum ;
- états `draft`, `validated`, `finalized`, `failed` ;
- une version `finalized` devient immuable.

`ml.dataset_rows`

- identifiant logique déterministe ;
- cellule/date/split ;
- label, sélection, features et provenance ;
- checksum.

`ml.dataset_event_links`

- relation plusieurs événements vers une cellule-date positive.

`ml.dataset_exclusions`

- événement/cellule-date exclu, motif et règle.

### 13.2 Migration future

Nom envisagé :

```text
0011_human_dataset_foundation.sql
```

Elle devra être additive, sans lecture applicative basculée et sans migration automatique des données existantes. Le rollback devra refuser de supprimer les tables si une version de dataset, une décision qualité, un groupe de doublons ou une ligne métier réelle existe.

Plusieurs migrations peuvent être préférables :

1. fondation BDIFF raw/staging/fire ;
2. qualité/duplication ;
3. versionnement dataset.

Cette séparation réduit le risque et est recommandée par rapport à une migration monolithique.

### 13.3 Modules Rust envisagés

- `ingest::bdiff_raw` : conservation source.
- `engine::bdiff_pipeline` : batch/run et orchestration.
- `store::bdiff` : persistance transactionnelle.
- `quality::cause_taxonomy` : mapping pur et versionné.
- `quality::geography` : qualification sans correction silencieuse.
- `quality::duplicates` : signaux et scoring.
- `dataset::builder` : lignes, exclusions, splits et checksum.
- `validation::protocol` : métriques temporelles/géographiques.

Le modèle opérationnel ne doit pas dépendre de ces modules tant qu’une phase de validation distincte n’a pas autorisé le basculement.

## 14. Plan de tests

### 14.1 Unitaires

- mapping exact des six causes ;
- nouveau libellé vers `indeterminate` ;
- conservation du libellé source ;
- détection de centroïde probable ;
- catégories géographiques ;
- score de doublon et justification ;
- non-déduplication sur jour/H3 seul ;
- règles positifs/naturels/inconnus ;
- exclusion spatiale/temporelle des négatifs ;
- absence de feature future ;
- normalisation ajustée uniquement sur train ;
- reproductibilité avec seed.

### 14.2 SQLx

- création idempotente des sources/règles ;
- traçabilité raw → staging → fire ;
- version de dataset ;
- refus de modifier une version finalisée ;
- inconnus conservés ;
- exclusions et doublons conservés ;
- contraintes d’identifiants ;
- rollback transactionnel ;
- refus du rollback destructif ;
- coexistence sans changement de `public`.

### 14.3 Données

- comptages par année/cause ;
- identifiants BDIFF uniques ;
- champs manquants ;
- coordonnées/H3 ;
- centroïdes probables ;
- groupes de doublons ;
- non combustibles et sans features ;
- répartition géographique dérivée ;
- séparation stricte des années ;
- aucun événement 2024/2025 dans les agrégats train ;
- aucun unknown/natural dans les négatifs ;
- cohérence strict/inclusif.

### 14.4 Reproductibilité

À commit, sources, versions de règles et seed identiques :

- mêmes identifiants logiques ;
- mêmes lignes et splits ;
- mêmes exclusions ;
- mêmes statistiques ;
- même checksum global.

## 15. Rollback futur

Avant toute donnée réelle, le rollback pourra supprimer uniquement les objets additifs.

Après création d’un audit ou dataset :

- rollback SQL destructif refusé ;
- arrêt du nouveau pipeline ;
- conservation de toutes les tables et données ;
- retour du binaire précédent ;
- migration corrective ultérieure si nécessaire.

`public.ignition_history`, `cell_static`, le modèle actif et la production restent intacts.

## 16. Questions ouvertes

1. Le portail BDIFF permet-il d’obtenir durablement les colonnes brutes, codes INSEE, département/région, type détaillé et précision de localisation ?
2. Existe-t-il une documentation officielle du regroupement des causes vers les six libellés normalisés ?
3. Quel référentiel communal et quel millésime doivent servir à confirmer les centroïdes ?
4. Quels millésimes exacts OSM, CORINE et INSEE sont juridiquement et techniquement disponibles pour 2020–2025 ?
5. L’unité opérationnelle cible doit-elle rester cellule-jour ou devenir cellule-fenêtre 6/24 heures ?
6. Quelle politique métier doit encadrer l’utilisation secondaire des feux naturels ?
7. Quel budget opérationnel SDIS doit définir les métriques de rappel/précision en top cellules ?

Ces questions doivent être résolues avant finalisation du premier dataset, mais ne bloquent pas la fondation additive ni les audits.

## 17. Résumé des anomalies à ne pas masquer

- 8 071 causes inconnues.
- 12 282 événements à coordonnée communale unique répétée.
- 418 groupes même jour/H3.
- 672 événements humains non combustibles ou sans features.
- Snapshot territorial 2026 appliqué au passé.
- Calendrier absent pour 2020–2024 dans l’entraînement actuel.
- 121 cellules négatives partagées entre train et validation actifs.
- Chevauchements H3 positifs entre tous les splits.
- Test 2025 déjà consulté pour le modèle actif.
- Département, région et précision source absents du stockage courant.

Ces constats invalident toute interprétation définitive des métriques actuelles, mais ne démontrent pas que le modèle est inutilisable. Ils imposent une reconstruction versionnée et une validation plus rigoureuse avant tout remplacement.
