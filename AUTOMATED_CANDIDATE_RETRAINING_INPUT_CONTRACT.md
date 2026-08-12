# Contrat d'entrée pour un futur réentraînement automatisé de candidat (Phase 5, non implémentée)

Ce document prépare une phase future. **Aucun entraînement, aucun scoring, aucune activation
n'est implémenté ici.** Tout candidat produit selon ce contrat restera `inactive` jusqu'à une
décision humaine séparée, conformément à `MODEL_PROMOTION_PLAN.md` et au verrou SQL de
`ml.model_candidate_registry` (`status CHECK IN ('candidate', 'inactive')`, migration 0016).

## 1. Familles de snapshots consommées

- `observability.scientific_snapshots` (contrat v2, statut `published`,
  `completeness_status = 'complete'` et vérification stricte valide uniquement), avec
  `observability.scientific_dense_archives` pour l'historique quotidien compact ou
  `observability.scientific_snapshot_values` pour les anciens snapshots hebdomadaires. La météo
  brute reste absente ; les archives quotidiennes conservent les six composantes FWI dérivées.
- `features.feature_snapshots` (statut `active`) et `features.feature_snapshot_values` pour le
  bundle statique immuable référencé par `static_snapshot_id`.
- `ml.snapshot_label_links` pour les causes BDIFF différées, avec `matching_rule_version`,
  `maturity_status = 'mature'` et `is_current = true` explicites.
- Les snapshots `contract_version = 1` sont consultables et vérifiables en mode legacy, mais
  interdits comme entrée d'un futur dataset automatisé.
- `ml.dataset_versions`/`ml.dataset_rows` restent la voie de construction de dataset existante
  (phase 3B) — ce contrat ne la remplace pas, il prépare une source additionnelle de features
  historisées et vérifiables pour une future version.

## 2. Labels nécessaires

Uniquement `fire.ignition_events` via `ml.snapshot_label_links`, avec :
- `label_class IN ('human_known', 'natural_known', 'unknown', 'indeterminate', 'no_event')` ;
- **`unknown`/`indeterminate` ne sont jamais des négatifs** — règle absolue héritée de
  `NEGATIVE_SAMPLING_DESIGN.md` et de la contrainte `fire.ignition_events.cause_category` ;
- **`natural_known` n'est jamais une absence de feu** ;
- **`raw.firms_observations` n'est jamais un label humain** — `ml.snapshot_label_links` ne
  référence que `fire.ignition_events`, jamais FIRMS directement.

## 3. Délai de maturation

Les causes BDIFF arrivent après le fait ; un candidat futur devra définir un délai de maturation
minimal (ex. N mois après `event_date`) avant qu'un événement soit considéré comme définitivement
étiqueté, pour éviter d'entraîner sur des causes encore susceptibles de révision. Ce délai n'est
pas fixé par la présente phase — à spécifier et justifier scientifiquement avant implémentation.
En conséquence, le rattachement des labels reste une opération manuelle précédée d'un dry-run ;
aucun scheduler ne l'exécute automatiquement et aucune absence d'événement n'est fabriquée.

## 4. Éviter la fuite temporelle

- Un snapshot scientifique ne doit jamais être associé à un label dont la date de connaissance
  (`matched_at`) précède `valid_at` du snapshot dans le mauvais sens : le label doit être connu
  **après** l'instant de validité du snapshot, jamais avant (sinon fuite du futur vers le passé).
- `temporal_classification` du snapshot doit être respectée : un snapshot
  `current_snapshot_applied_historically` ne doit jamais être présenté comme
  `historical_exact` dans un rapport d'entraînement — la classification doit rester visible dans
  le manifeste produit pour tout dataset dérivé.
- Le bundle statique référencé (`static_snapshot_id`) doit être celui en vigueur à la date
  d'entraînement visée, pas le dernier bundle disponible au moment de la construction du dataset.

## 5. Construction train/calibration/test

Reprendre le schéma déjà en place (`ml.dataset_versions.splits`, phase 3B.3) : découpage
chronologique strict, jamais aléatoire, pour éviter la fuite entre périodes. Un futur pipeline
consommant les snapshots 4A.5 devra produire les mêmes garanties de traçabilité
(`ml.dataset_row_snapshots`, provenance explicite) que le pipeline dataset existant.

## 6. Versionnage des négatifs

Réutiliser le vocabulaire et la traçabilité de `NEGATIVE_SAMPLING_DESIGN.md`
(`dataset_exclusions.reason_category`, `negative_strategy`/`negative_parameters` de
`ml.dataset_versions`). Aucune nouvelle stratégie n'est proposée par ce contrat.

## 7. Enregistrement d'un candidat

Reprendre exactement le chemin existant : `ml.model_candidate_registry`, statut limité par
contrainte SQL à `candidate`/`inactive`, `dataset_logical_id` et `dataset_row_fingerprint`
explicites, checksums d'artefact vérifiés (`crates/engine/src/candidate_artifact.rs`,
`candidate_load_verification.rs`). Rien de nouveau n'est requis côté schéma pour l'enregistrement
lui-même.

## 8. Pourquoi tout candidat restera inactif

`ml.model_candidate_registry.status` n'admet que `candidate`/`inactive` au niveau base de données
(`CHECK`, migration 0016) — il n'existe **aucun chemin SQL vers `active`**. L'activation d'un
modèle reste exclusivement une opération sur `human_model_versions`, jamais automatisée, jamais
déclenchée par un pipeline de snapshot ou d'entraînement. Toute future automatisation
d'entraînement devra respecter cette séparation ; ce contrat ne la lève pas et ne le suggère pas.
