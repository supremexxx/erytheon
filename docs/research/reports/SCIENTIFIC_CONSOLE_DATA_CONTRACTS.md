# Console scientifique ERYTHEON — Contrats de données (Phase 4A)

Toutes les routes ci-dessous sont montées sous `/api/science/*`, uniquement quand
`SCIENCE_CONSOLE_ENABLED=true`. Toutes sont `GET`. Aucune n'écrit en base. Toutes les dates
sont en UTC (ISO 8601). Un champ absent de la table source est rendu `null` en JSON — jamais
converti en `0` ou en chaîne vide.

## GET /api/science/overview

Compteurs globaux réels : `app_status`, `db_status`, `migrations_applied`, `active_model_id`,
`candidate_status`, `candidate_model_family`, `bdiff_events_total`, `bdiff_human_known`,
`bdiff_natural_known`, `bdiff_unknown`, `firms_observations_total`, `cell_static_total`,
`feature_snapshots_total`, `dataset_versions_total`, `dataset_builds_total`,
`human_model_versions_total`. Cible p95 : < 500 ms.

## GET /api/science/progress

Retourne le contenu de `crates/api/static/science/phases.json` (fichier versionné, pas une
requête base). Tableau d'objets `{id, label, title, status, commits[], summary, environment,
production_affected, risks?[]}`.

## GET /api/science/sources

`LEFT JOIN public.source_status s ON reference.data_sources d ON d.id = s.code`. Champs :
`id, category, last_success, observation_count, recent_error`.

## GET /api/science/imports?source=&status=&limit=&offset=

Pagination : `limit` par défaut 50, plafonné à 200 ; `offset` par défaut 0. Filtre optionnel par
`source` (code source) et `status`. Lignes de `import_batches` : `id, source_code, status,
started_at, records_received, records_inserted, records_rejected`.

## GET /api/science/pipelines?pipeline=&status=&limit=&offset=

Même pagination. Lignes de pipeline runs : `id, pipeline_name, status, started_at,
error_message`.

## GET /api/science/data-quality

Résumé agrégé : `bdiff_events_total, coordinate_groups_total,
duplicate_candidate_pairs_total, cause_counts[], duplicate_classification_counts[],
geographic_quality_counts[], combustibility_counts[]`. Chaque `*_counts` est un tableau
`{category, count}`. `geographic_quality_counts` provient de
`validation.event_geographic_quality.geographic_category` (8 catégories possibles), **pas** de
`fire.ignition_events.geographic_quality` (contraint à une seule valeur en base). Cible p95 :
< 1 s.

## GET /api/science/data-quality/events?cause=&limit=&offset=

Table paginée d'événements : `occurred_on_local, h3, cause_category, cause_subcategory,
geographic_quality`. Cible p95 : < 1 s.

## GET /api/science/features

`{snapshots: FeatureSnapshotRow[], calendar: CalendarSummary}`. Un snapshot porte
`temporal_classification` ; la valeur `current_snapshot_applied_historically` signale
explicitement qu'un snapshot courant a été appliqué de façon uniforme à tout l'historique
d'entraînement (limite scientifique connue, jamais masquée). `CalendarSummary` porte
`school_holiday_known_days` et `school_holiday_unknown_days` séparément — l'indisponibilité
historique des vacances scolaires n'est **jamais** repliée dans un compteur à zéro ; l'API
garantit `known + unknown == total_days`.

## GET /api/science/calendar

`CalendarSummary` seul (même contrat que le sous-objet `calendar` de `/features`).

## GET /api/science/datasets

Liste `ml.dataset_versions` : `id, logical_id, name, variant, status, seed, checksum,
row_count, positive_count, negative_count, exclusion_count, created_at, finalized_at`. Ces
compteurs sont des colonnes directes de la table — pour les datasets en statut `draft` (ou
certains `validated`), ils peuvent être `null` en base réelle si jamais calculés à ce niveau ;
la console ne les invente pas. Cible p95 : < 500 ms.

## GET /api/science/datasets/{logical_id}

`404 dataset_not_found` si absent. Sinon `{summary: DatasetVersionSummaryRow, build_count: i64,
splits: [{split, label, count}], exclusions: [{reason_category, count}]}`. `splits` et
`exclusions` sont calculés en direct par agrégation sur `ml.dataset_rows` /
`ml.dataset_exclusions` filtrés par `dataset_version_id` — donc renseignés même quand le
`row_count` du résumé est `null`.

## GET /api/science/models

`{active_v1: {id, trained_at, metrics} | null, candidate: ModelCandidateRow | null,
comparison: {...}, scientific_interpretation}`.

- `active_v1` vient de `Store::active_human_model()` (méthode déjà existante, réutilisée telle
  quelle).
- `candidate` vient de `ml.model_candidate_registry`, trié par `created_at DESC`, jamais
  `status = 'active'` (contrainte `CHECK` en base — ce statut est structurellement impossible).
- `comparison` est la constante figée `phase_3b8_comparison()` : gain d'AP +0.3473 (IC 95 %
  [0.3157, 0.3852]), ROC-AUC/AP/lift appariés v1 vs candidat sur la population de test 2025
  commune. Champ `source` indique explicitement `"PHASE3B8_PROMOTION_GAP_REPORT.md (phase
  3B.8, not a live database query)"` — ce n'est pas une valeur recalculée à chaque appel.
- Cible p95 : < 500 ms.

## GET /api/science/system

`{migrations_applied, migrations_failed, active_model_count, candidate_registry_count,
cell_static_total, ignition_events_total, dataset_versions_total, last_firms_success,
last_bdiff_success}`. `active_model_count` doit valoir 1 en fonctionnement normal ; toute autre
valeur est un signal d'anomalie affiché tel quel, jamais masqué.

## Enveloppe d'erreur

Identique au reste de l'API : `{"error": {"code": "...", "message": "..."}}`. Codes utilisés par
la console : `dataset_not_found` (404), `database_unavailable` (503, erreur SQL générique — le
message ne fuite jamais le texte brut d'une erreur `sqlx`, seulement un message générique côté
client ; le détail est loggé côté serveur via `tracing::error!`).

## Ce que la console ne fait jamais

Aucune route n'accepte de `POST`/`PUT`/`PATCH`/`DELETE`. Aucune requête SQL de ce module ne
contient `INSERT`, `UPDATE`, `DELETE`, ni `CALL`/`EXECUTE` d'une procédure. Aucune route ne
déclenche `scheduler::spawn`, aucune n'importe FIRMS/BDIFF, aucune ne calcule un score modèle.
