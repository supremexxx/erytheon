# Audit de l’architecture de données d’ERYTHEON

Date de l’audit : 26 juillet 2026  
Périmètre : dépôt local et base PostgreSQL/PostGIS de production sur le VPS  
Nature des opérations réalisées : lecture du code et requêtes PostgreSQL en transaction `READ ONLY`  
État de la migration : aucune migration créée ou appliquée

## 1. Synthèse exécutive

ERYTHEON fonctionne aujourd’hui comme une application monolithique Rust correctement découpée en crates, reliée à une base PostgreSQL/PostGIS unique. SQLx est l’unique couche d’accès à la base. Les migrations sont intégrées au binaire et exécutées automatiquement au démarrage.

La V1 est opérationnelle, mais sa base est organisée autour de neuf tables applicatives dans `public`. Elle ne sépare pas les données reçues, les données normalisées, les features, les résultats, le serving et l’exploitation technique.

Les quatre constats les plus importants sont :

1. La base de production représente environ **7,4 Go**. `risk_scores` occupe environ **3,8 Go**, `forecast_fwi` **1,2 Go**, `corine_france_stage` **1,1 Go**, `fwi_state` **782 Mo** et `cell_static` **472 Mo**.
2. Les prévisions et scores ne sont pas historisés : `retain_forecast_batch` supprime les anciens lots après publication. Il est donc impossible de reconstruire ce que le système savait avant un événement.
3. Les features statiques sont stockées dans un document JSONB unique, sans définition, unité, version, provenance ni relation vers une cellule H3 de référence.
4. La sauvegarde locale quotidienne est actuellement en échec et aucun dump local n’est présent. Aucune migration ne doit être appliquée avant correction et test de restauration.

La migration recommandée est additive. Les tables actuelles restent en place et continuent à alimenter l’algorithme et l’API. Les nouveaux schémas sont introduits progressivement, avec NASA FIRMS comme source pilote. La bascule vers `features`, puis vers `risk` et `serving`, ne doit intervenir qu’après comparaison mesurée avec le parcours actuel.

## 2. Architecture actuelle

### 2.1 Composants

| Composant | Rôle |
| --- | --- |
| `crates/ingest` | Connecteurs et normalisation des sources externes |
| `crates/grid` | Couverture et conversion des cellules H3 |
| `crates/fwi` | Calcul pur des indices FFMC, DMC, DC, ISI, BUI et FWI |
| `crates/risk` | Fusion du risque physique et de la propension humaine |
| `crates/store` | Connexion SQLx, migrations et toutes les requêtes PostgreSQL |
| `crates/engine` | Commandes, pipelines, scheduler, prévisions, backtest et entraînement |
| `crates/api` | API Axum, WebSocket et interface statique |
| PostgreSQL 16 / PostGIS 3.4 | Stockage unique |
| Docker Compose / Caddy | Exécution et exposition HTTPS sur le VPS |

### 2.2 Schémas PostgreSQL actuels

Les seuls schémas non système sont :

| Schéma | Origine | Usage ERYTHEON |
| --- | --- | --- |
| `public` | PostgreSQL / application | Toutes les tables applicatives |
| `tiger` | Extension PostGIS | Aucun usage applicatif identifié |
| `tiger_data` | Extension PostGIS | Aucun usage applicatif identifié |
| `topology` | Extension PostGIS | Aucun usage applicatif identifié |

Il n’existe actuellement aucun schéma métier séparé, aucune vue applicative, aucune vue matérialisée, aucune fonction applicative et aucun trigger applicatif.

### 2.3 Migrations

Huit migrations SQLx sont installées avec succès :

| Version | Description | Installation VPS |
| ---: | --- | --- |
| 1 | `initial` | 18 juillet 2026 |
| 2 | `observation deduplication` | 18 juillet 2026 |
| 3 | `fwi precision` | 18 juillet 2026 |
| 4 | `static layers` | 18 juillet 2026 |
| 5 | `risk api` | 18 juillet 2026 |
| 6 | `forecast horizons` | 18 juillet 2026 |
| 7 | `forecast batches` | 18 juillet 2026 |
| 8 | `human model versions` | 24 juillet 2026 |

Le mécanisme de versionnement existe donc déjà via `_sqlx_migrations`. Il doit être conservé. En revanche, l’exécution automatique des migrations au démarrage de chaque processus mérite une règle de déploiement plus stricte lorsque les migrations deviendront volumineuses.

## 3. Inventaire complet de la base applicative

Les volumes ci-dessous proviennent des statistiques PostgreSQL. Ils sont approximatifs lorsque précisé. Les comptes par horizon sont exacts.

### 3.1 Tables

| Table actuelle | Rôle | Lignes | Taille totale | Clé primaire | Index complémentaires | Géométrie | Lecteurs / écrivains principaux | Criticité |
| --- | --- | ---: | ---: | --- | --- | --- | --- | --- |
| `public.risk_scores` | Surface de risque publiée pour quatre horizons | 3 171 992 exactes | 3 781 Mo | `(h3, computed_at, horizon)` | date/score, H3/date, horizon/date/score | Aucune | écrit par forecast/risk pipeline ; lu par toutes les routes risque et alertes | Critique |
| `public.forecast_fwi` | FWI détaillé correspondant au dernier lot de prévision | 3 171 992 exactes | 1 229 Mo | `(h3, computed_at, horizon)` | `(horizon, computed_at DESC)` | Aucune | écrit par forecast ; lu par détail cellule | Critique |
| `public.corine_france_stage` | Table de travail CORINE chargée hors migrations | ~366 547 | 1 107 Mo | Aucune contrainte déclarée | Aucun index | `geom geometry`, SRID 2154 | processus d’import ponctuel externe ; aucune lecture runtime trouvée | Élevée |
| `public.fwi_state` | État FWI quotidien persistant par cellule | ~5 686 959 | 782 Mo | `(h3, date)` | Aucun autre | Aucune | écrit par recompute/forecast ; lu par forecast, risque et détail cellule | Critique |
| `public.cell_static` | Features statiques agrégées par H3 dans JSONB | ~923 017 | 472 Mo | `h3` | Aucun autre | Aucune | écrit par `load-static` et rafraîchissement historique ; lu par algorithme, backtest, modèle et API | Critique |
| `public.ignition_history` | Incendies historiques normalisés BDIFF/Prométhée | ~15 957 | 11 Mo | `id` | H3/date ; unique partiel source/déduplication | Aucune | écrit par imports historiques ; lu par features, backtest et entraînement | Critique |
| `public.observations` | Observations normalisées génériques ; actuellement FIRMS uniquement | 3 681 | 2,2 Mo | `id` | H3/date ; source/date ; unique partiel source/déduplication | Aucune | écrit par scheduler/backfill ; pas de lecture algorithmique FIRMS actuelle | Élevée |
| `public.calendar_days` | Calendrier scolaire et jours fériés | 1 095 | 224 Ko | `date` | Aucun autre | Aucune | écrit par `load-static` ; lu par risque, backtest et entraînement | Élevée |
| `public.forecast_batches` | Publication atomique du lot de prévision | 1 lot complet | 72 Ko | `computed_at` | index partiel des lots incomplets | Aucune | écrit par forecast ; lu par API pour masquer les lots incomplets | Critique |
| `public.source_status` | Dernier statut de chaque connecteur | 8 | 64 Ko | `id` | Aucun autre | Aucune | écrit par pipelines ; lu par `/health`, `/sources` et scheduler | Élevée |
| `public.human_model_versions` | Artefact JSONB et métriques du modèle humain | 1 | 48 Ko | `id` | unicité partielle d’un modèle actif | Aucune | écrit par entraînement ; lu au démarrage du modèle opérationnel | Critique |
| `public._sqlx_migrations` | Historique SQLx | 8 | 32 Ko | `version` | Aucun autre | Aucune | écrit automatiquement par SQLx | Critique |

`spatial_ref_sys`, `geometry_columns` et `geography_columns` appartiennent à PostGIS et ne sont pas des tables métier ERYTHEON.

### 3.2 Colonnes et contraintes importantes

#### `observations`

- Colonnes : `id`, `source`, `kind`, `h3`, `observed_at`, `payload`, `dedupe_key`.
- Déduplication : unique sur `(source, dedupe_key)` lorsque `dedupe_key` n’est pas nul.
- Provenance présente partiellement dans `payload`, mais absence de `retrieved_at`, batch, URL, version de pipeline, checksum de réponse et statut de parsing.
- Période actuelle : 18 au 25 juillet 2026.
- Source actuelle : 3 681 lignes `firms / active_fire`.

#### `cell_static`

- Colonnes : `h3`, `features`, `updated_at`.
- Clé centrale H3 non reliée à un référentiel.
- Clés JSON connues : `hist`, `wui`, `road`, `agri`, `combustible`, `population`, `poi`, `power_line`, `school_zone`.
- Environ 920 016 documents contiennent chacune des neuf clés ; les statistiques de table indiquent environ 923 017 lignes, écart à contrôler lors de la phase 0.
- Aucun identifiant de feature set, version, source, date de validité ou méthode de normalisation.

#### `fwi_state`

- Colonnes : `h3`, `date`, `ffmc`, `dmc`, `dc`, `isi`, `bui`, `fwi`.
- Période actuelle : 18 au 24 juillet 2026.
- Les états sont mis à jour par upsert ; le même jour peut être recalculé.
- Pas de provenance météo, run, algorithme FWI ou qualité de l’interpolation.

#### `forecast_fwi`

- Colonnes : `h3`, `computed_at`, `valid_at`, `horizon`, six indices FWI.
- Un seul `computed_at` est conservé : 25 juillet 2026 à 01:04:21 UTC.
- Quatre horizons, chacun avec 792 998 cellules.
- Les anciens lots sont supprimés après publication.

#### `risk_scores`

- Colonnes : `h3`, `computed_at`, `horizon`, `score`, `physical`, `human`, `factors`, `input_date`, `valid_at`.
- Contraintes de domaine `[0,1]` sur les trois scores.
- Un seul lot est conservé, avec 792 998 cellules pour chacun des quatre horizons.
- Absence de `run_id`, version d’algorithme, version de modèle, feature set, paramètres et confiance.
- Les facteurs explicatifs sont dans JSONB.

#### `ignition_history`

- Colonnes : `id`, `occurred_at`, `h3`, `source`, `payload`, `dedupe_key`.
- 15 956 lignes BDIFF et une ligne Prométhée.
- Période : 6 février 2000 au 31 décembre 2025.
- Cause, commune et informations source restent dans `payload`.
- Aucune distinction relationnelle entre incident, cause, périmètre, source et détection.

#### `human_model_versions`

- Périodes d’entraînement et validation explicitement stockées.
- Artefact et métriques en JSONB.
- Unicité d’un seul modèle actif.
- Absence de dataset versionné, checksum, commit du code, feature set, statut de déploiement détaillé et URI d’artefact.

### 3.3 Clés étrangères et dépendances PostgreSQL

- Aucune clé étrangère métier n’existe entre les tables applicatives.
- Les relations H3, lots, scores, FWI et modèles sont donc garanties uniquement par le code.
- Aucune vue ou fonction applicative ne dépend des tables.
- Aucun trigger applicatif n’existe.
- Le seul trigger non système observé appartient à `topology.layer` et provient de PostGIS.

### 3.4 État d’exploitation

- Taille de la base : environ 7,4 Go.
- Espace disque VPS : 96 Go au total, 26 Go utilisés, 71 Go disponibles.
- `risk_scores` contient environ 284 000 tuples morts.
- `forecast_fwi` contient environ 273 000 tuples morts.
- `fwi_state` contient environ 854 000 tuples morts.
- `open_meteo_arome` conserve une dernière réussite au 25 juillet 2026 à 01:12 UTC et une erreur récente `forecast partition 01 failed`.
- Le lot complet antérieur reste correctement servi grâce à `forecast_batches`.

## 4. Inventaire des dépendances dans le code

### 4.1 Connexion et migrations

| Fichier | Responsabilité |
| --- | --- |
| `crates/engine/src/config.rs` | Lit `DATABASE_URL` et toute la configuration |
| `crates/store/src/lib.rs` | Crée le pool `PgPool`, applique les migrations et porte toutes les requêtes |
| `migrations/*.sql` | Schéma versionné SQLx |
| `deploy/oracle/compose.yml` | Fournit PostgreSQL et `DATABASE_URL` au conteneur |
| `docker-compose.yml` | PostgreSQL local |

Il n’y a ni ORM ni query builder métier. Les requêtes SQL sont explicites via `sqlx::query`, ce qui facilite l’audit.

### 4.2 Dépendances par table

| Table | Écritures | Lectures |
| --- | --- | --- |
| `observations` | `Store::insert_observations`, scheduler FIRMS, commande `backfill`, recompute météo historique | aucune lecture FIRMS runtime ; données météo passées directement en mémoire au pipeline |
| `cell_static` | `upsert_cell_static`, `update_cell_history`, `load-static`, `load-fire-history` | forecast, risk pipeline, backtest, entraînement humain, détail API |
| `fwi_state` | `upsert_fwi_states`, recompute météo, forecast | forecast précédent, risk pipeline, détail API |
| `forecast_fwi` | `upsert_forecast_fwi`, suppression des lots échoués/anciens | détail API |
| `risk_scores` | `upsert_risk_scores`, suppression des lots échoués/anciens | `/risk`, `/risk/cell`, `/alerts`, WebSocket indirect |
| `ignition_history` | `upsert_ignition_history`, imports BDIFF/Prométhée | construction de `hist`, backtest, entraînement humain |
| `calendar_days` | `upsert_calendar_days`, `load-static` | risk inputs, backtest, entraînement humain |
| `forecast_batches` | début, abandon et publication du forecast | toutes les lectures de score récent |
| `source_status` | succès/erreur de chaque connecteur | `/health`, `/sources`, logique de redémarrage forecast |
| `human_model_versions` | `activate_human_model` | `active_human_model` au démarrage |
| `_sqlx_migrations` | SQLx | SQLx |
| `corine_france_stage` | processus d’import hors dépôt ou commande opérateur | aucune dépendance runtime trouvée |

### 4.3 Routes API dépendantes

| Route | Tables |
| --- | --- |
| `GET /health` | test SQL + `source_status` |
| `GET /sources` | `source_status` |
| `GET /risk` | `risk_scores`, `forecast_batches` |
| `GET /risk/cell/{h3}` | `risk_scores`, `forecast_batches`, `forecast_fwi`, repli `fwi_state`, `cell_static` |
| `GET /alerts` | `risk_scores`, `forecast_batches` |
| `GET /config` | configuration mémoire, aucune table |
| `WS /stream` | canal mémoire alimenté après calcul ; recharge REST côté interface |

L’interface ne dépend pas directement de PostgreSQL. Elle dépend des contrats JSON de l’API, ce qui permet de migrer la base derrière une couche de compatibilité.

### 4.4 Tâches et automatisations

| Tâche | Déclenchement | Fréquence |
| --- | --- | --- |
| Scheduler FIRMS | processus `engine run` | immédiat puis toutes les 30 minutes |
| Scheduler forecast | processus `engine run` | immédiat puis toutes les heures |
| Synchronisation BDIFF | timer systemd | le 15 de chaque mois |
| Sauvegarde PostgreSQL locale | timer systemd | quotidienne vers 02:30 UTC |
| Sauvegarde R2 | script présent | aucun timer installé observé |
| Imports OSM/CORINE/INSEE/calendrier | commandes opérateur | ponctuel |

## 5. Cartographie des flux actuels

### 5.1 NASA FIRMS

```text
NASA FIRMS Area CSV
→ FirmsSource
→ parsing et normalisation Rust
→ projection H3
→ Store::insert_observations
→ public.observations
→ export GeoJSON optionnel
```

- Scheduler : 30 minutes.
- Backfill manuel : fenêtre configurable, découpée par limites FIRMS.
- Idempotence : `source + dedupe_key`.
- Rejouabilité : partielle tant que l’API source permet de récupérer la période.
- Traçabilité : statut global dans `source_status`, pas de batch ni réponse brute.
- Risque : une réponse reçue ne peut pas être reproduite exactement si la source change.
- Usage actuel : détection d’incendies actifs, pas entrée directe du score futur.

### 5.2 Open-Meteo / AROME-ARPEGE

```text
Open-Meteo
→ grille de points météo par partition
→ parsing en mémoire
→ interpolation IDW vers les cellules H3
→ calcul FWI
→ public.fwi_state + public.forecast_fwi
→ fusion risque
→ public.risk_scores
→ publication atomique via public.forecast_batches
→ API
→ interface
```

- Fréquence : une heure.
- Déclenchement : scheduler ou commande `forecast`.
- Rejouabilité : faible ; ni réponse brute ni forecast externe versionné ne sont conservés.
- Erreurs : le lot en cours est masqué ; un lot échoué est supprimé ; le dernier lot complet reste visible.
- Double temporalité déjà présente : `computed_at` et `valid_at`.
- Risque majeur : impossible de répondre à « que savait le système à une date donnée ? ».

### 5.3 Données statiques OSM, CORINE, INSEE et calendrier

```text
Fichiers locaux
→ connecteurs ingest
→ observations normalisées en mémoire
→ agrégation et normalisation nationale par H3
→ public.cell_static / public.calendar_days
→ algorithme, modèle et API
```

- Déclenchement manuel.
- OSM peut être préagrégé en cache JSONL.
- `cell_static` est remplacée par upsert, sans version précédente.
- La normalisation `[0,1]` dépend du territoire traité.
- Les sources et versions exactes ne sont pas enregistrées avec chaque feature.
- La table `corine_france_stage` est un artefact lourd non géré par les migrations.

### 5.4 BDIFF / Prométhée

```text
Export BDIFF ou fichier Prométhée
→ normalisation
→ projection H3
→ public.ignition_history
→ recalcul de la feature historique dans public.cell_static
→ entraînement du modèle humain
→ public.human_model_versions
```

- BDIFF : mensuel.
- Idempotence : `source + dedupe_key`.
- Le fichier normalisé courant remplace le précédent sur disque.
- Les versions brutes reçues ne sont pas conservées dans PostgreSQL.
- Le modèle est réentraîné après synchronisation lorsque les périodes le permettent.

### 5.5 Serving

```text
public.risk_scores + public.forecast_fwi + public.cell_static
→ requêtes Store
→ API Axum
→ dashboard Leaflet
```

Les requêtes API portent directement sur les tables de calcul. Il n’existe pas de projection `serving`. Les index actuels rendent la V1 utilisable, mais l’historisation future ne pourra pas être ajoutée sans séparer lecture opérationnelle et archives.

## 6. Analyse détaillée de l’algorithme actuel

### 6.1 Entrées

Pour chaque cellule H3 :

- `fwi` issu de `fwi_state` ou du calcul forecast en mémoire ;
- `hist`, `wui`, `road`, `agri`, `population`, `poi`, `power_line`, `combustible` issus de `cell_static.features` ;
- `school_holiday` et `public_holiday` issus de `calendar_days` ;
- date et horizon ;
- modèle humain actif issu de `human_model_versions`, s’il existe.

### 6.2 Composante physique

```text
physical = clamp(FWI / FWI_MAX, 0, 1)
```

Le FWI est calculé à partir de température, humidité, vent et précipitations. Les codes d’humidité persistants utilisent l’état du jour précédent ou les valeurs standard par défaut si l’état manque.

Hypothèses implicites :

- quatre voisins météo avec interpolation inverse de la distance ;
- état FWI standard lorsqu’aucun historique n’existe ;
- `FWI_MAX` vient de la configuration runtime et n’est pas enregistré avec le score ;
- les unités sont garanties par les connecteurs, pas par la base.

### 6.3 Composante humaine

Si un modèle appris actif existe, une régression logistique utilise onze variables ordonnées :

1. interface habitat-forêt ;
2. densité routière ;
3. activité agricole ;
4. population ;
5. points d’intérêt ;
6. lignes électriques ;
7. week-end ;
8. vacances scolaires ;
9. jour férié ;
10. sinus saisonnier ;
11. cosinus saisonnier.

Le modèle actuel est un modèle cas-témoins régularisé. Il produit une propension relative, pas une probabilité absolue. Les contrôles négatifs sont échantillonnés de manière déterministe parmi les cellules combustibles.

Sans modèle appris, un fallback heuristique combine `hist`, `wui`, `road` et `agri`, avec multiplicateur calendaire.

Hypothèses et défauts :

- les features sont supposées déjà normalisées et compatibles avec celles de l’entraînement ;
- leur version n’est pas stockée avec le modèle ;
- les jours absents de `calendar_days` deviennent faux par défaut ;
- `hist` est exclue du modèle appris pour éviter la fuite de cible, mais reste utilisée par le fallback ;
- la cause humaine dépend de catégories BDIFF connues et de la qualité des coordonnées publiques.

### 6.4 Fusion

```text
score = physical^alpha × human^beta
```

Une cellule non combustible reçoit toujours zéro. Les trois contributions positives dominantes sont stockées.

Les paramètres `FWI_MAX`, `alpha`, `beta` et poids heuristiques viennent des variables d’environnement. Ils ne sont pas historisés dans `risk_scores`.

### 6.5 Sorties

- `forecast_fwi` : six indices FWI par cellule, lot et horizon ;
- `risk_scores` : score global, physique, humain et facteurs ;
- WebSocket : notification des cellules actualisées ;
- API : GeoJSON, alertes et détail cellule.

### 6.6 Backtest

Le backtest :

- lit `cell_static`, `ignition_history` et `calendar_days` ;
- charge des archives SYNOP depuis des fichiers ;
- recalcule le FWI et le score en mémoire ;
- reconstruit la densité historique sans utiliser les incendies futurs ;
- produit un rapport Markdown ;
- ne persiste ni dataset, ni run, ni prédictions, ni métriques.

Il existe donc une logique de validation utile, mais elle n’est pas encore reproductible à partir de la base seule.

## 7. Problèmes et risques

### 7.1 Critiques

| Problème | Impact |
| --- | --- |
| Sauvegarde quotidienne en échec et aucun dump local présent | Une migration ou panne peut entraîner une perte sans rollback vérifié |
| Suppression des anciens lots dans `retain_forecast_batch` | Perte d’historique des prévisions et scores ; validation rétrospective impossible |
| Absence de version de features et paramètres dans les scores | Un score ne peut pas être reproduit exactement |
| `cell_static` est l’entrée centrale non versionnée de l’algorithme | Une réimportation modifie implicitement le sens du modèle actif |
| Aucune relation entre lot, forecast, score, modèle et données d’entrée | Traçabilité scientifique insuffisante |

### 7.2 Élevés

| Problème | Impact |
| --- | --- |
| Réponses brutes FIRMS/Open-Meteo non conservées | Impossibilité de rejouer fidèlement un import |
| Absence de `reference.h3_cells` | Pas de géométrie centrale, multi-résolution ou mapping administratif stable |
| Aucune clé étrangère métier | Intégrité dépendante du code uniquement |
| API directement branchée sur les tables de calcul | Historisation future susceptible de dégrader les performances |
| `corine_france_stage` de 1,1 Go hors migrations | Objet non traçable, non reproductible par le schéma versionné |
| Migrations automatiques au démarrage | Risque de verrouillage ou d’indisponibilité lors de futures migrations lourdes |
| FWI recalculé/upserté sans provenance | Impossible de distinguer mesure, estimation et version météo |
| Modèle sans dataset ni feature set liés | Reproductibilité ML incomplète |

### 7.3 Moyens

| Problème | Impact |
| --- | --- |
| JSONB de features sans dictionnaire | Contrat implicite dispersé entre plusieurs modules |
| `source_status` ne conserve que le dernier état | Perte de l’historique des erreurs et volumes |
| Absence de partitionnement des grandes tables futures | Croissance et maintenance difficiles |
| Nombre important de tuples morts | Gonflement et coût de maintenance, surtout lors des lots échoués |
| Calendrier limité à 2025–2027 | Entraînements historiques antérieurs traitent les jours comme non fériés |
| Documentation encore nommée PyroRisk dans plusieurs fichiers | Dette de cohérence, sans impact sur les données |

### 7.4 Faibles

| Problème | Impact |
| --- | --- |
| Codes de sources en texte libre | Risque d’incohérences à long terme |
| Horizon en texte libre | Une contrainte ou référentiel serait préférable |
| Pas de colonnes `created_at` techniques sur certaines tables | Audit opérationnel plus difficile |

## 8. Architecture cible adaptée

La cible proposée respecte une seule base PostgreSQL/PostGIS et les schémas demandés :

```text
raw → staging → reference/environment/human/fire
                    ↓
                 features
                    ↓
                  risk
                    ↓
               validation
                    ↓
                 serving
```

`ops` trace chaque étape. `ml` versionne datasets, entraînements et modèles.

### 8.1 Principes d’adaptation

- Ne pas déplacer immédiatement les tables `public`.
- Ne pas renommer de table existante pendant les premières phases.
- Introduire des identifiants UUID pour les nouveaux batches et runs, tout en conservant H3 en `BIGINT`.
- Centraliser H3 dans `reference.h3_cells`, avec résolution et géométries.
- Séparer prévision météo externe, FWI calculé et score de risque.
- Rendre `risk.cell_risk_snapshots` append-only.
- Construire `serving.current_risk_cells` à partir des snapshots publiés.
- Conserver `forecast_batches` et les requêtes actuelles jusqu’à validation du nouveau serving.
- N’ajouter le partitionnement qu’aux nouvelles grandes tables et après mesure.

### 8.2 Fondation minimale autorisée

La première migration, après validation explicite, devrait uniquement :

- créer `raw`, `staging`, `reference`, `environment`, `human`, `fire`, `features`, `risk`, `validation`, `ml`, `serving`, `ops` ;
- créer `reference.data_sources` ;
- créer `ops.import_batches` ;
- créer `ops.pipeline_runs` ;
- créer une structure générique minimale `raw.source_responses` et/ou la table pilote `raw.firms_observations` ;
- ajouter les contraintes de statut, dates et idempotence ;
- ne créer aucun trigger de double écriture ;
- ne modifier aucune table `public`.

## 9. Mapping entre existant et cible

| Table actuelle | Rôle actuel | Cible principale | Stratégie | Compatibilité | Risque | Priorité | Rollback |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `observations` | FIRMS normalisé générique | `raw.firms_observations` puis `fire.satellite_detections` | double persistance centralisée temporaire | maintenir `public.observations` | Faible | 1 | arrêter nouvelle écriture, tables additives conservées |
| `source_status` | dernier état source | `reference.data_sources`, `ops.import_batches`, `ops.pipeline_runs` | conserver et continuer l’upsert actuel | vue/adaptateur possible plus tard | Faible | 1 | aucun changement public |
| `ignition_history` | incendies historiques aplatis | `fire.incidents`, `fire.incident_causes`, `fire.incident_sources`, `fire.incident_cell_links` | backfill append + rapport d’écart | maintenir table actuelle | Moyen | 3 | supprimer uniquement données nouvelles après contrôle |
| `cell_static` | toutes les features statiques | `human.*`, `reference.land_cover`, `features.static_cell_features` | reproduire à l’identique puis versionner | adaptateur Store ou vue | Élevé | 4 | retour au lecteur actuel |
| `calendar_days` | calendrier | `reference.calendar_days` ou feature temporelle versionnée | copie contrôlée | vue ou double lecture temporaire | Faible | 3 | retour public |
| `fwi_state` | état FWI quotidien | `environment.environmental_cell_metrics` ou `features.cell_features_daily` | copier avec run et provenance | lecteur actuel maintenu | Élevé | 4 | retour public |
| `forecast_fwi` | dernier FWI forecast | `environment.weather_forecasts` + `features.cell_features_hourly` | nouvelle historisation, pas de suppression | table publique reste serving V1 | Élevé | 4–6 | désactiver nouveau chemin |
| `forecast_batches` | atomicité publication | `risk.risk_runs` et statut de publication | mapper chaque nouveau run | conserver le mécanisme actuel | Moyen | 6 | retour public |
| `risk_scores` | dernier score opérationnel | `risk.cell_risk_snapshots` | écriture parallèle append-only | `serving.current_risk_cells` plus tard | Critique | 6 | arrêter écriture parallèle |
| `human_model_versions` | modèle humain actif | `ml.models`, `ml.model_versions`, `ml.training_runs`, `ml.datasets` | backfill métadonnées sans modifier le chargement actuel | garder lecteur actuel | Moyen | 8 | aucun impact runtime |
| `corine_france_stage` | staging ponctuel | `staging.corine_cleaning` ou fichier externe reproductible | inventorier origine puis reconstruire | aucune dépendance runtime | Moyen | 3 | conserver table actuelle |
| `_sqlx_migrations` | versionnement | inchangé | conserver SQLx | n/a | Faible | 1 | standard SQLx |

## 10. Plan de migration

### Phase 0 — Sécurisation

1. Corriger la sauvegarde locale qui échoue sur le chargement de `.env`.
2. Produire un dump complet daté.
3. Vérifier `pg_restore --list`.
4. Restaurer dans une base ou instance temporaire et comparer tables/comptes.
5. Capturer les réponses de référence de l’API.
6. Capturer un échantillon déterministe de scores, FWI et features.
7. Mesurer les durées et tailles.
8. Documenter RPO et RTO.

Critère de sortie : restauration testée, pas seulement dump créé.

### Phase 1 — Fondation

- Créer les douze schémas.
- Créer le référentiel de sources.
- Créer `ops.import_batches` et `ops.pipeline_runs`.
- Créer la structure raw minimale.
- Ajouter tests SQLx et documentation.
- Aucun changement de flux runtime.

### Phase 2 — Pilote NASA FIRMS

- NASA FIRMS est préférée à Open-Meteo :
  - volume actuel faible ;
  - pipeline isolé et idempotent ;
  - payload source déjà disponible ;
  - absence d’impact direct sur le calcul du score ;
  - Open-Meteo connaît actuellement des erreurs de partition et produit des volumes nationaux beaucoup plus élevés.
- Enregistrer chaque réponse ou enregistrement reçu dans `raw`.
- Enregistrer le batch et les compteurs.
- Maintenir `public.observations`.
- Centraliser l’écriture parallèle dans un seul service de persistance FIRMS.

### Phase 3 — Normalisation FIRMS

- Construire `staging.firms_cleaning`, rejouable depuis `raw`.
- Alimenter `fire.satellite_detections`.
- Produire un rapport d’écart avec `public.observations`.
- Valider nombre, H3, timestamp, déduplication et payload.

### Phase 4 — Features V1

- Créer définitions, feature sets, versions et generation runs.
- Produire `features.static_cell_features` avec les neuf valeurs actuelles.
- Fixer explicitement unité, normalisation et valeurs manquantes.
- Comparer chaque valeur à `cell_static`.

### Phase 5 — Lecture parallèle

- Ajouter un adaptateur Store qui peut lire soit `public.cell_static`, soit `features`.
- Exécuter les deux parcours sur un jeu de référence.
- Comparer FWI, composante humaine, composante physique, score et top facteurs.
- Tolérance initiale proposée : égalité booléenne/texte et erreur absolue `≤ 1e-6` pour les valeurs flottantes reproduites sans changement de formule.

### Phase 6 — Historisation du risque

- Créer `risk.risk_runs` et `risk.cell_risk_snapshots`.
- Écrire chaque lot en append-only.
- Associer code, paramètres, modèle et feature set.
- Conserver le chemin public actuel pour l’API.
- Définir une politique de rétention avant tout volume national prolongé.

### Phase 7 — Serving

- Créer `serving.current_risk_cells` et `serving.active_alerts`.
- Optimiser selon les requêtes API mesurées.
- Migrer route par route.
- Comparer réponses JSON et temps de réponse.

### Phase 8 — Validation et ML

- Persister runs de backtest, prédictions et métriques.
- Versionner datasets et entraînements.
- Relier incidents, snapshots et modèles.

### Phase 9 — Dépréciation

- Arrêter les écritures anciennes une par une.
- Observer au moins un cycle opérationnel défini.
- Sauvegarder avant chaque suppression.
- Ne supprimer qu’après validation explicite.

## 11. Stratégie de compatibilité temporaire

1. Les tables `public` restent la source de production au début.
2. La double écriture est limitée au service de persistance de la source pilote.
3. Aucun trigger n’est recommandé pour la double écriture : il masquerait la logique et compliquerait les erreurs partielles.
4. Chaque double écriture doit avoir un résultat explicite et des compteurs dans `ops`.
5. Les lecteurs sont migrés derrière `Store`, jamais directement dans l’API ou l’algorithme.
6. Un drapeau de configuration peut sélectionner l’ancien ou le nouveau lecteur pendant la comparaison.
7. Les vues de compatibilité ne seront utilisées que lorsque les types et performances auront été mesurés.

## 12. Stratégie d’introduction de `features`

### 12.1 Contrat V1

Créer un feature set `operational_risk_v1` qui reproduit exactement :

- `hist` ;
- `wui` ;
- `road` ;
- `agri` ;
- `population` ;
- `poi` ;
- `power_line` ;
- `combustible` ;
- calendrier scolaire/public au moment du calcul.

Chaque définition doit stocker :

- nom stable ;
- type ;
- unité ;
- plage attendue ;
- source ;
- formule ;
- territoire de normalisation ;
- fréquence ;
- gestion des valeurs manquantes ;
- version de pipeline ;
- statut.

### 12.2 Forme de stockage

Pour la première version, une table large versionnée par cellule est préférable à un modèle EAV :

```text
features.static_cell_features
(feature_set_version_id, h3_index, valid_from, generated_at,
 hist, wui, road, agri, population, poi, power_line, combustible, provenance)
```

Cette forme est lisible, performante et proche du contrat Rust actuel. Les nouvelles familles de features peuvent avoir leurs propres tables horaires ou quotidiennes.

## 13. Comparaison ancien / nouveau calcul

### 13.1 Jeu de référence

- Zone : un département représentatif ou un ensemble fixe de cellules H3.
- Période : un lot forecast complet archivé.
- Entrées : météo brute, états FWI précédents, features statiques, calendrier, modèle actif et configuration.
- Sorties : six indices FWI, `physical`, `human`, `score`, facteurs.

### 13.2 Contrôles

| Niveau | Contrôle |
| --- | --- |
| Import | lignes reçues, acceptées, rejetées, doublons et checksums |
| Normalisation | H3, timestamps UTC, unités et payloads |
| Features | comparaison colonne par colonne |
| Calcul | comparaison cellule/horizon |
| API | snapshot JSON canonique |
| Performance | durée, mémoire, lignes lues et plan SQL |

### 13.3 Tolérances

- Entiers, booléens, identifiants et catégories : égalité stricte.
- Features reproduites depuis les mêmes entrées : `abs(diff) ≤ 1e-6`.
- FWI et scores : `abs(diff) ≤ 1e-6` si aucun changement numérique ; toute tolérance plus large doit être justifiée.
- Nombre de cellules : égalité stricte.
- Aucun écart silencieux : rapport des maxima, quantiles et cellules divergentes.

## 14. Sauvegarde et rollback

### 14.1 État constaté

- `pyrorisk-local-backup.timer` est activé.
- La dernière exécution a échoué avec le statut 127.
- Cause observée : le script source `.env` et interprète la valeur non quotée contenant `France métropolitaine`.
- Le répertoire `/opt/pyrorisk/backups` n’existe pas.
- Aucun timer R2 n’est installé.

### 14.2 Procédure requise avant migration

1. Corriger le parsing de configuration du script de sauvegarde.
2. Exécuter un dump custom PostgreSQL.
3. Vérifier sa liste avec `pg_restore --list`.
4. Calculer et conserver un SHA-256.
5. Restaurer vers une base temporaire distincte.
6. Vérifier extensions, migrations, tables, comptes, contraintes et échantillons.
7. Conserver au moins un dump local daté et une copie distante si R2 est validé.

### 14.3 Rollback Phase 1

La phase 1 est additive. Le rollback applicatif consiste à déployer le binaire précédent, qui ignore les nouveaux schémas. Les nouveaux objets ne doivent pas être supprimés immédiatement : ils peuvent être conservés hors chemin runtime jusqu’à analyse. Un script SQL de retrait ciblé peut être documenté, mais ne doit être exécuté qu’après sauvegarde et validation explicite.

## 15. Fichiers prévus pour la première intervention

Liste proposée, non créée à ce stade :

| Fichier | Changement prévu |
| --- | --- |
| `migrations/0009_data_platform_foundation.sql` | Schémas et tables fondatrices additives |
| `migrations/rollback/0009_data_platform_foundation.down.sql` | Procédure de rollback documentée |
| `crates/store/src/lib.rs` | Exposition minimale des nouveaux repositories, ou extraction progressive |
| `crates/store/src/platform.rs` | Types et requêtes `ops`, `reference` et `raw` |
| `crates/store/tests/platform_foundation.rs` | Tests d’idempotence, contraintes et transactions |
| `crates/engine/src/firms_pipeline.rs` | Préparation du pipeline pilote centralisé |
| `crates/engine/src/scheduler.rs` | Appel centralisé, sans changer la cadence |
| `crates/engine/src/main.rs` | Utilisation du même pipeline pour backfill et scheduler |
| `deploy/oracle/backup-local.sh` | Correction préalable du chargement de configuration |
| `deploy/oracle/README.md` | Procédure de sauvegarde/restauration vérifiée |
| `README.md` | Architecture cible et statut de transition |
| `DATABASE_ARCHITECTURE_AUDIT.md` | Mise à jour des décisions validées |

La correction de sauvegarde appartient à la phase 0 et doit précéder `0009`.

## 16. Décisions nécessitant validation

Les décisions réellement structurantes sont :

1. Valider NASA FIRMS comme source pilote.
2. Valider que la phase 0 commence par réparer et tester les sauvegardes avant toute migration.
3. Valider le modèle de double écriture applicative centralisée, sans trigger.
4. Valider une table large pour `features.static_cell_features`, plutôt qu’un modèle EAV.
5. Valider l’usage d’UUID pour les nouveaux batches/runs et de `BIGINT` pour H3.
6. Définir la politique de rétention des réponses brutes, forecasts et snapshots nationaux.
7. Décider si les migrations de production restent automatiques au démarrage ou deviennent une étape de déploiement séparée.
8. Définir la durée minimale d’exécution parallèle avant bascule d’un lecteur.
9. Valider l’emplacement des artefacts ML volumineux : disque VPS, stockage objet ou autre.
10. Valider le niveau de service attendu pour les sauvegardes : RPO, RTO et copie hors VPS.

## 17. Questions bloquantes

Une seule réponse est nécessaire avant la première intervention :

> Autorises-tu la phase 0 limitée à la réparation de la sauvegarde, la création d’un dump vérifié et un test de restauration, sans aucune modification du schéma métier ?

Les choix de rétention, RPO/RTO et stockage distant peuvent être affinés pendant cette phase, avant la création de la migration `0009`.

