# Phase 4A.5 — Rapport du pilote de snapshots scientifiques

## 1. Périmètre livré

Conformément à `PHASE4A5_SNAPSHOT_ARCHITECTURE_DECISION.md`, le stockage de valeurs scientifiques
est un **pilote borné** : cadence hebdomadaire, horizon `nowcast` uniquement.

- Manifeste : `observability.scientific_snapshots` (`migrations/0019_scientific_snapshot_registry.sql`)
  — identité, période, versions, checksum, couverture, statut, référence au bundle statique.
- Valeurs : `observability.scientific_snapshot_values` — une ligne par cellule H3, colonnes
  météo/FWI, `data_status` (`observed`/`imputed`/`missing`).
- Cycle de vie : `building → validated → published`, trigger
  `scientific_snapshots_published_immutable` (calqué sur
  `ml.forbid_finalized_dataset_version_update`, migration 0015) — toute `UPDATE` sur une ligne
  `published` échoue. Le trigger ne bloque pas `DELETE` ; aucune voie de suppression n'est exposée
  côté application (`store::observability` n'a pas de fonction de suppression).
- Idempotence : `Store::capture_weekly_scientific_snapshot` — un identifiant logique
  `scientific-weekly-nowcast-{date}` ; un rejeu sur un manifeste déjà `published` est un no-op
  (retourne le manifeste existant, ne recalcule rien) ; un rejeu sur `building`/`failed` reprend le
  calcul depuis zéro (les valeurs partielles sont supprimées puis réinsérées dans la même
  transaction logique).
- Checksum déterministe : agrégat SQL `string_agg(h3 || ':' || fwi || ':' || data_status, ','
  ORDER BY h3)` puis `digest(..., 'sha256')` (extension `pgcrypto`, ajoutée par la migration 0018)
  — l'ordre est fixé explicitement, pas dépendant d'un `HashMap`.
- Référencement du bundle statique : `static_snapshot_id` pointe vers
  `features.feature_snapshots` (table déjà existante depuis la phase 3B.3, réutilisée telle
  quelle, pas dupliquée) — les features statiques/lentement variables ne sont jamais recopiées.

## 2. Limite scientifique majeure : la météo brute n'est pas persistée

L'audit (`PHASE4A5_SNAPSHOT_SOURCE_AUDIT.md` §3) a révélé que la météo interpolée par cellule
(température, humidité, vent, précipitations) n'existe **nulle part en base** — elle est calculée
en mémoire dans `crates/engine/src/forecast.rs`/`weather.rs` puis jetée après dérivation du FWI.
Seul le FWI dérivé (`forecast_fwi` : FFMC/DMC/DC/ISI/BUI/FWI) est persisté.

Conséquence : `observability.scientific_snapshot_values.{temperature,humidity,wind_speed,
wind_direction,precipitation}` restent `NULL` avec `data_status='missing'` dans ce pilote — ce
n'est **pas un bug**, c'est une absence de source honnêtement déclarée plutôt qu'une valeur
fabriquée. Capturer la météo brute nécessiterait d'instrumenter le pipeline forecast lui-même,
explicitement interdit par le périmètre de cette phase (« ne pas modifier le moteur de risque »).
Une phase séparée devra décider si ce besoin justifie une modification du pipeline forecast.

## 3. Classification temporelle

`temporal_classification` réutilise exactement le vocabulaire déjà défini par
`features.feature_snapshots` (migration 0013) plutôt que d'inventer un second enum légèrement
différent : `historical_exact`, `historical_snapshot`, `stable_approximation`,
`current_snapshot_applied_historically`, `unavailable_historically`, `derived_past_only`. Le
pilote marque ses captures `current_snapshot_applied_historically` : le FWI capturé est l'état
courant au moment T, pas une reconstruction historique exacte.

## 4. Tests

`weekly_scientific_snapshot_is_idempotent_and_published_immutable`
(`crates/store/tests/observability.rs`) : capture initiale publiée, checksum stable, rejeu
idempotent (même `id`, même checksum, pas de doublon dans `scientific_snapshot_values`),
`cell_count_expected == cell_count_present + missing_count`. Vérifié manuellement en base
(§ »Migrations« ) que le trigger d'immutabilité refuse toute `UPDATE` post-publication.

## 5. Labels différés (BDIFF)

`ml.snapshot_label_links` (migration 0021) prépare l'association différée cause BDIFF ↔ snapshot,
sans reconstruire de dataset. `label_class` réutilise exactement
`fire.ignition_events.cause_category` (`human_known`/`natural_known`/`unknown`/`indeterminate`)
plus `no_event` ; contrainte `CHECK` garantissant qu'un lien `no_event` n'a jamais d'
`ignition_event_id` et vice-versa. `Store::link_snapshot_label` est un simple `INSERT ... ON
CONFLICT DO NOTHING`, aucun dataset n'est construit automatiquement.

## 6. Volumétrie observée

Base de développement actuelle : `cell_static` vide (0 lignes) → snapshot pilote trivialement
complet (`cell_count_expected = 0`). Le calcul réel de coût (§2 de
`PHASE4A5_SNAPSHOT_ARCHITECTURE_DECISION.md`, ~10 Go/an pour 920 016 cellules) reste une
**estimation**, pas une mesure — à confirmer lors du premier déploiement pilote sur une base
peuplée, avant toute extension de cadence ou d'horizon.
