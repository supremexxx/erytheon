# ERYTHEON

ERYTHEON est une plateforme Rust de suivi et d'analyse du risque d'ignition des feux de végétation. Elle combine météo incendie, observations satellitaires, données territoriales et historique des départs de feu sur une grille H3. Le dépôt couvre le service opérationnel, la fondation scientifique des datasets, un modèle candidat non actif et une console scientifique privée.

État de référence : **v0.4.2** — intégration de la fondation scientifique et de la console privée.

## État actuel

| Composant | État |
|---|---|
| Service opérationnel | Déployé et suivi sur VPS privé |
| Modèle v1 | **Actif**, seul modèle servi |
| Candidat `gbm_isotonic_v2` | Enregistré **inactive**, jamais servi |
| Candidate scoring | Non activé |
| Shadow scoring | Non commencé |
| Console scientifique | Déployée en lecture seule, accès privé protégé par Caddy |
| Schéma PostgreSQL | 17 migrations appliquées en production |
| Révision applicative déployée | `849039385a14f95df0a95cca69e5987d3b311478` |
| Révision intégrée sur `main` | `d4730fda09571db491d0611e3308f96d1ee03ebb` |

La production reste volontairement sur la révision applicative `8490393`. L'intégration documentaire et les correctifs de CI présents sur `main` ne constituent pas une instruction de redéploiement.

## Architecture

ERYTHEON est un workspace Cargo composé de neuf crates :

| Crate | Responsabilité |
|---|---|
| `engine` | Configuration, commandes, orchestration, scheduler et binaire |
| `api` | API HTTP Axum, dashboard opérationnel et console scientifique |
| `store` | Accès PostgreSQL/PostGIS, migrations et repositories |
| `ingest` | Connecteurs et normalisation des sources |
| `dataset` | Construction et versioning des datasets scientifiques |
| `quality` | Audits de qualité et règles de validation |
| `risk` | Scoring opérationnel v1 et fusion explicable |
| `fwi` | Canadian Fire Weather Index |
| `grid` | Maillage H3, emprises et conversions géographiques |

Le stockage PostgreSQL/PostGIS sépare les données brutes, le staging, les événements incendie, la validation, les datasets ML, les opérations et les tables applicatives. Les migrations SQLx sont additives et appliquées par l'engine au démarrage.

Les principales sources prises en charge sont :

- NASA FIRMS pour les détections satellitaires ;
- Météo-France et Open-Meteo AROME/ARPEGE pour les observations et prévisions ;
- BDIFF et Prométhée pour l'historique incendie ;
- OpenStreetMap, CORINE Land Cover, INSEE et calendriers territoriaux pour les features statiques.

Le service expose des surfaces H3 pour les horizons `nowcast`, `+6 h`, `+24 h` et `+48 h`. PostgreSQL reste sur un réseau Docker privé ; Caddy est le seul point d'entrée public du déploiement.

## Modèles

### v1 opérationnel

v1 demeure l'unique modèle actif et l'unique source des scores servis par l'API opérationnelle. Cette garantie n'a pas été modifiée par les phases scientifiques récentes.

### Candidat v2

Le candidat `gbm_isotonic_v2` a été entraîné, calibré, comparé à v1, empaqueté et enregistré dans la registry. Son statut est `inactive`.

Il n'existe actuellement :

- aucune activation du candidat ;
- aucun score candidat servi ;
- aucun shadow scoring ;
- aucune décision automatique de promotion.

Toute évolution de ce statut exige une phase séparée, des contrôles explicites et un rollback documenté.

## Console scientifique privée

La console `/science` présente en lecture seule :

- progression des phases ;
- état des sources, imports et pipelines ;
- qualité BDIFF et catégories géographiques ;
- features et calendrier ;
- versions de datasets ;
- modèles v1 et candidat ;
- intégrité du système et des migrations.

Ses endpoints sont regroupés sous `/api/science/*`. Ils ne proposent que des lectures ; ils ne déclenchent ni import, ni entraînement, ni scoring, ni migration, ni activation de modèle.

`SCIENCE_CONSOLE_ENABLED` vaut `false` par défaut. Quand il est désactivé, les routes scientifiques ne sont pas montées. Ce flag est un verrou de déploiement, pas un mécanisme d'authentification. Sur le VPS, Caddy protège séparément ces routes.

Pour une prévisualisation locale sans scheduler et sans chargement de modèle :

```sh
docker compose up -d
cargo run -p engine -- preview-science-console --bind 127.0.0.1:8081
```

Puis ouvrir <http://127.0.0.1:8081/science>.

## Démarrage local

Prérequis :

- Rust `1.97.1` — épinglé par `rust-toolchain.toml` ;
- Docker avec Compose ;
- `curl`.

```sh
cp .env.example .env
docker compose up -d
cargo run -p engine -- run
```

Le service répond ensuite sur :

- <http://localhost:8080/health> — santé du service et des sources ;
- <http://localhost:8080/> — dashboard opérationnel ;
- `GET /risk` — surfaces GeoJSON H3 ;
- `GET /alerts` — cellules dépassant un seuil ;
- `GET /risk/cell/{h3}` — explication d'une cellule ;
- `WS /stream` — mises à jour de risque.

Le profil par défaut utilise les fixtures versionnées. `DATA_PROFILE=production` refuse les fichiers de test, les sources statiques manquantes et les échecs silencieux. Les données réelles et les secrets doivent rester sous les chemins ignorés par Git.

## Déploiement et versions

Deux tags distinguent le binaire réellement construit de l'état intégré final :

- `v0.4.2-app` pointe sur `849039385a14f95df0a95cca69e5987d3b311478`, révision applicative actuellement déployée ;
- `v0.4.2` pointe sur `d4730fda09571db491d0611e3308f96d1ee03ebb`, tête intégrée incluant documentation et correctifs de validation.

Les tags existants sont immuables. Un futur redéploiement doit construire une nouvelle révision explicitement validée ; il ne doit pas déplacer `v0.4.2-app` ou `v0.4.2`.

La procédure de déploiement historique est décrite dans [`deploy/oracle/README.md`](deploy/oracle/README.md). Les détails spécifiques à la console sont dans [`SCIENTIFIC_CONSOLE_DEPLOYMENT_RUNBOOK.md`](SCIENTIFIC_CONSOLE_DEPLOYMENT_RUNBOOK.md).

## Feuille de route

Les fondations de données, les datasets candidats, l'entraînement expérimental, la comparaison v1/candidat, le packaging, l'enregistrement inactif et la console scientifique privée sont terminés ou validés.

La prochaine étape est **Phase 4A.3 — stabilisation** : usage réel de la console, cohérence des chiffres, ergonomie, monitoring, erreurs API et rate limits. Les visualisations avancées de Phase 4B puis le shadow scoring limité P3 viendront seulement après cette stabilisation.

Voir [`ROADMAP.md`](ROADMAP.md) pour l'état détaillé et les critères de passage.

## Limites scientifiques

- Le score représente un risque relatif, pas une probabilité absolue de départ de feu.
- FIRMS observe des événements récents ; il ne prédit pas à lui seul les ignitions futures.
- La qualité dépend de la couverture et de la fraîcheur des sources territoriales.
- Certaines features statiques présentent une dérive temporelle entre entraînement et production.
- Le registre de datasets et certains snapshots peuvent être vides en production ; la console doit afficher cet état sans extrapolation.
- Les performances historiques du candidat ne prouvent pas sa stabilité sur les données courantes.
- Aucun résultat candidat ne doit être interprété comme opérationnel avant une phase de shadow scoring contrôlée.

## Documentation

- [`ROADMAP.md`](ROADMAP.md) — phases terminées et suite recommandée ;
- [`PR1_INTEGRATION_COMPLETION_REPORT.md`](PR1_INTEGRATION_COMPLETION_REPORT.md) — clôture de l'intégration v0.4.2 ;
- [`SCIENTIFIC_CONSOLE_ARCHITECTURE.md`](SCIENTIFIC_CONSOLE_ARCHITECTURE.md) — architecture et garanties read-only ;
- [`SCIENTIFIC_CONSOLE_USER_GUIDE.md`](SCIENTIFIC_CONSOLE_USER_GUIDE.md) — utilisation de la console ;
- [`SCIENTIFIC_CONSOLE_DATA_CONTRACTS.md`](SCIENTIFIC_CONSOLE_DATA_CONTRACTS.md) — contrats API ;
- [`MODEL_CANDIDATE_ARTIFACT.md`](MODEL_CANDIDATE_ARTIFACT.md) — format et intégrité du candidat ;
- [`V1_CANDIDATE_COMPARISON.md`](V1_CANDIDATE_COMPARISON.md) — comparaison scientifique ;
- [`MODEL_PROMOTION_PLAN.md`](MODEL_PROMOTION_PLAN.md) — critères de promotion ;
- [`SHADOW_SCORING_DESIGN.md`](SHADOW_SCORING_DESIGN.md) — conception P3, non implémentée.

## Développement

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked --no-fail-fast
```

Les tests d'intégration nécessitent PostgreSQL/PostGIS. Les fixtures versionnées appartiennent à `testdata/`; les données réelles, sorties et secrets ne doivent jamais être commités.
