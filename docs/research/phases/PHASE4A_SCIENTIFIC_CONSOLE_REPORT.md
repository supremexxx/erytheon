# Phase 4A — Console scientifique en lecture seule — Rapport

## 1. Git au démarrage

Branche `main`, arbre propre. Derniers commits avant cette phase :

```
2e6b7de docs: report P2 load-only verification (phase 3B.11)
48b6b9a feat: add candidate load-only verification
18d8c1e fix: make historical rollback guards transaction-safe
060e827 docs: report P1 inactive registration (phase 3B.10)
```

Contexte : le candidat `gbm_isotonic_v2` est enregistré `inactive` en production, v1 reste
actif, aucun shadow scoring n'a commencé (P3 non entamé).

## 2. Audit de l'interface, des routes et du schéma existants

Voir le détail dans [SCIENTIFIC_CONSOLE_ARCHITECTURE.md](../reports/SCIENTIFIC_CONSOLE_ARCHITECTURE.md) §2.
Résumé : aucun outillage de build frontend (vanilla HTML/CSS/JS embarqué via `include_str!`),
aucune authentification API, convention UUID-as-`String`/`::text`, et deux corrections
factuelles importantes aux hypothèses de la mission :
- les statuts de source réels viennent de `public.source_status`, pas seulement de
  `reference.data_sources` (jointure nécessaire) ;
- la qualité géographique enrichie (8 catégories) vit dans
  `validation.event_geographic_quality`, pas dans la colonne contrainte
  `fire.ignition_events.geographic_quality`.

## 3. Architecture et point d'étape

Architecture proposée, validée par point d'étape avant implémentation (voir
[SCIENTIFIC_CONSOLE_ARCHITECTURE.md](../reports/SCIENTIFIC_CONSOLE_ARCHITECTURE.md)) : séparation
`store::science` (requêtes) / `api::science` (handlers) / `static/science` (frontend vanilla),
verrou de déploiement `SCIENCE_CONSOLE_ENABLED` (défaut `false`), historique de progression en
JSON versionné, commande de prévisualisation `PreviewScienceConsole` sans scheduler.

Deux décisions ont été confirmées par l'utilisateur avant implémentation :
- exposition via `SCIENCE_CONSOLE_ENABLED=false` par défaut (verrou de déploiement, pas une
  authentification réelle — aucune n'existe dans ce projet) ;
- stockage de l'historique des phases dans un fichier JSON versionné plutôt qu'une nouvelle
  table (évite de rétro-dater artificiellement des phases déjà terminées).

## 4. Routes et endpoints livrés

`GET /science`, `/science/{overview,progress,sources,data-quality,features,datasets,models,
system}`, `/science/datasets/:id`, servis uniquement quand `SCIENCE_CONSOLE_ENABLED=true`
(sinon absents du routeur, testé explicitement).

`GET /api/science/{overview,progress,sources,imports,pipelines,data-quality,
data-quality/events,features,calendar,datasets,datasets/{logical_id},models,system}` — détail
complet des contrats dans
[SCIENTIFIC_CONSOLE_DATA_CONTRACTS.md](../reports/SCIENTIFIC_CONSOLE_DATA_CONTRACTS.md).

## 5. Pages livrées (priorité 1 et 2)

Les 8 pages demandées ont été implémentées et vérifiées visuellement dans un navigateur réel
contre la base isolée (voir §8) : Vue d'ensemble, Progression, Sources et pipelines, Qualité des
données, Features et snapshots, Datasets (liste + détail), Modèles, Système et intégrité.

## 6. Requêtes et performance

Mesures effectuées en local dans le conteneur de build, contre la base isolée
`erytheon-3b3-deploy-20260727T203310Z`, 10 requêtes par endpoint :

| Endpoint | min | max | Cible p95 | Statut |
|---|---|---|---|---|
| `/api/science/overview` | 67 ms | 98 ms | < 500 ms | ✓ |
| `/api/science/models` | 5 ms | 9 ms | < 500 ms | ✓ |
| `/api/science/datasets` | 1.3 ms | 2.7 ms | < 500 ms | ✓ |
| `/api/science/data-quality` | 25 ms | 43 ms | < 1 s | ✓ |
| `/api/science/data-quality/events?limit=50` | 1.5 ms | 3.5 ms | < 1 s | ✓ |
| `/api/science/system` | 55 ms | 74 ms | < 500 ms | ✓ |
| `/api/science/sources` | 1.2 ms | 2.9 ms | — | ✓ |
| `/api/science/imports?limit=50` | 1.8 ms | 3.2 ms | — | ✓ |
| `/api/science/pipelines?limit=50` | 1.7 ms | 3.5 ms | — | ✓ |
| `/api/science/features` | 4.6 ms | 9 ms | — | ✓ |

Toutes les cibles sont largement respectées sans matérialisation ni cache.

## 7. Sécurité et exposition

`SCIENCE_CONSOLE_ENABLED` (défaut `false`) contrôle le montage des routes elles-mêmes, pas une
autorisation a posteriori. Documenté explicitement comme un verrou de déploiement et non une
authentification (aucune n'existe dans ce projet). Aucun secret, mot de passe, URL de connexion
ou jeton n'est jamais exposé par une réponse `/api/science/*` (vérifié par lecture du code : les
handlers ne relaient que des DTOs explicitement whitelistés, jamais une ligne brute de
configuration).

## 8. Tests

### Backend (`crates/api/tests/science.rs`, 9 tests, tous `ok`)

- `science_routes_absent_when_console_disabled` — confirme l'absence totale des routes
  `/science*` et `/api/science/*` quand le drapeau est faux.
- `overview_reports_real_counters_and_is_stable_across_calls` — contrat stable, deux appels
  identiques.
- `models_endpoint_never_fabricates_a_missing_candidate_or_v1` — `active_v1`/`candidate`
  toujours présents (potentiellement `null`), candidat jamais `active`.
- `datasets_list_and_missing_detail_are_handled_honestly` — liste + 404 propre sur logical_id
  inconnu.
- `data_quality_summary_and_paginated_events_respect_filters` — pagination bornée à 200, filtre
  par cause vérifié ligne par ligne.
- `sources_imports_and_pipelines_are_paginated_and_filterable`.
- `features_and_calendar_never_hardcode_missing_school_holiday_data_as_zero` — invariant
  `known + unknown == total_days`.
- `system_summary_reports_exactly_one_active_model`.
- `exercising_every_endpoint_writes_nothing_to_the_candidate_registry` — appelle les 12
  endpoints puis revérifie via une connexion fraîche que `ml.model_candidate_registry` n'a pas
  changé de volume — preuve boîte noire de l'absence d'écriture.

### Frontend

Aucun framework de test n'existe dans ce projet pour la partie frontend (aucun `package.json`,
aucun exécuteur JS). Vérification effectuée manuellement dans un navigateur réel contre les
données réelles de la base isolée : les 8 pages ont été chargées et inspectées visuellement
(voir §9). Une régression réelle a été trouvée et corrigée pendant cette vérification (§10).
Limite assumée : pas de suite automatisée côté JS pour cette phase.

### Suite complète du workspace

```
cargo fmt --all -- --check   -> ok (aucune modification nécessaire après corrections)
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings  -> exit 0
cargo test --workspace --locked  -> exit 0 (0 échec, y compris les 3 tests
                                     rollback_guard_safety.rs de la phase 3B.11)
```

## 9. Vérification visuelle (livrable obligatoire)

Chaque page a été chargée dans un navigateur réel pointé (via un tunnel SSH vers un conteneur
docker éphémère isolé) sur `preview-science-console` contre la base isolée
`erytheon-3b3-deploy-20260727T203310Z` :

- **Vue d'ensemble** : cartes d'état système (application/PostgreSQL/migrations/modèle
  actif/candidat/shadow scoring), compteurs de données réels
  (`bdiff_events_total: 15956`, `bdiff_human_known: 7094`, `bdiff_natural_known: 791`,
  `bdiff_unknown: 8071`, `firms_observations_total: 26527`, `cell_static_total: 920016`),
  avertissements scientifiques ouverts affichés en clair.
- **Progression** : chronologie verticale, phase 2 marquée « en production », commits réels
  associés à chaque phase.
- **Qualité des données** : répartition par cause, classification des doublons, **qualité
  géographique à 3 catégories réelles** (`municipality_centroid_probable: 7539`,
  `precision_undocumented: 7420`, `rounded_coordinate_probable: 997`), combustibilité, table
  d'exploration paginée.
- **Features et snapshots** : `temporal_classification` `current_snapshot_applied_historically`
  mis en gras, calendrier historique avec **« Vacances scolaires indisponibles : 2557 — donnée
  historiquement indisponible »** (jamais un zéro silencieux) à côté de « Vacances scolaires
  connues : 0 ».
- **Datasets** (liste et détail) : 6 versions listées, `row_count`/`positive_count`/etc.
  affichés « — » quand réellement `null` en base (confirmé par requête directe `psql`), page de
  détail calculant malgré tout les répartitions réelles par split/label et par catégorie
  d'exclusion (ex. `train · label 1 : 5009`, `insufficient_geographic_quality : 3624`).
- **Modèles** : modèle actif v1 (`id=1`, métriques JSON complètes,
  `validation_average_precision: 0.5030`, `validation_roc_auc: 0.8056`), candidat de test
  (`registry id=28`, `inactive`, tous les checksums), comparaison v1/candidat phase 3B.8
  (`ROC-AUC 0.7836 → 0.9764`, `AP 0.584 → 0.9308`, gain `+0.3473` IC 95 % `[0.3157, 0.3852]`),
  sémantique du score rappelée en bas de page.
- **Sources et pipelines** : 8 sources listées avec dernière réussite/erreur réelle
  (`open_meteo_arome` : « forecast partition 01 failed »), imports NASA FIRMS récents.
- **Système et intégrité** : `migrations_applied: 17`, `migrations_failed: 0`,
  `active_model_count: 1` (« unique, comme attendu »), `candidate_registry_count: 1`,
  `cell_static_total: 920016`.

## 10. Bug trouvé et corrigé pendant la vérification

Un bug de rendu a été découvert en observant la page Vue d'ensemble dans le navigateur : la
fonction JS `card(label, value, sub)` appliquait `escapeHtml()` sur `label`, alors que certains
appelants passent la sortie de `def(key, label)` (qui produit déjà du HTML sûr,
`<span class="sci-defterm" ...>...</span>`) comme `label`. Le HTML était donc échappé deux fois
et s'affichait comme texte brut au lieu d'un badge de définition. Corrigé dans
`crates/api/static/science/science.js` (le paramètre `label` de `card()` n'est plus ré-échappé —
tous les appelants passent soit une chaîne statique sûre, soit du HTML déjà échappé par `def()`).
Reconstruit, redéployé, revérifié visuellement : les trois cartes concernées (« Modèle actif »,
« Candidat », « Shadow scoring ») affichent désormais correctement le terme avec son
soulignement pointillé de tooltip.

## 11. Validation en environnement isolé

Toute la validation (backend, performance, vérification visuelle) a été effectuée contre la
base `erytheon-3b3-deploy-20260727T203310Z` (conteneur isolé de la phase 3B.3, toujours actif),
via un conteneur de build éphémère `erytheon-4a-build` sur un réseau docker dédié
`erytheon-4a-net`, sans jamais toucher au conteneur de base isolée lui-même. Un candidat de test
(`row_id=28`, mêmes checksums que le candidat réel de production) a été temporairement inséré
dans `ml.model_candidate_registry` de la base isolée pour peupler la page Modèles pendant la
capture ; il a été supprimé après vérification (`DELETE ... WHERE id = 28`, confirmé
`COUNT(*) = 0` après suppression).

## 12. Nettoyage effectué

- Processus `preview-science-console` arrêté (confirmé : plus de service à l'écoute sur 8081).
- Tunnel SSH local fermé (`pkill -f "ssh -f -N -L 8081"`).
- Candidat de test `id=28` supprimé de la base isolée.
- Conteneur éphémère `erytheon-4a-build` supprimé.
- Réseau docker `erytheon-4a-net` déconnecté du conteneur de base isolée puis supprimé.
- Conteneur de base isolée `erytheon-3b3-deploy-20260727T203310Z` laissé intact et actif, comme
  pour toutes les phases précédentes.

## 13. Backlog phase 4B (hors périmètre assumé de cette phase)

- Cartographie avancée de la console scientifique.
- Exploration détaillée événement par événement (au-delà de la table paginée actuelle).
- Shadow scoring (P3, non commencé).
- Comparaison temporelle en direct au-delà de la constante phase 3B.8.
- Export de données (CSV/Parquet).
- Authentification réelle si une exposition au-delà d'un environnement de validation isolé est
  un jour envisagée.

## 14. Plan de déploiement futur (non exécuté cette phase)

Aucun déploiement en production n'a eu lieu. Si une future phase décide d'exposer la console
au-delà d'un environnement isolé : (1) ajouter une authentification réelle (aucune n'existe
aujourd'hui dans l'API) avant toute activation de `SCIENCE_CONSOLE_ENABLED=true` sur une
instance accessible publiquement ; (2) documenter cette décision dans une revue dédiée, comme
pour les phases de promotion de modèle ; (3) ne jamais activer le drapeau par défaut dans la
configuration de production sans cette étape.

## 15. Commits locaux (aucun push)

Voir le journal Git après application de cette phase — trois commits locaux :
`feat: add read-only scientific console API`, `feat: add scientific console MVP`,
`docs: document scientific console`.

---

```
PHASE 4A SCIENTIFIC CONSOLE MVP COMPLETED
READ-ONLY SCIENTIFIC DATA VISIBLE
MODEL V1 REMAINS ACTIVE
CANDIDATE REMAINS INACTIVE
NO SHADOW SCORING
NO PRODUCTION DEPLOYMENT
NO PUSH
```
