# Phase 4A.5 — Rapport d'observabilité opérationnelle

## 1. Ce qui a été implémenté

- Schéma `observability` (`migrations/0018_observability_foundation.sql`), table
  `observability.system_snapshots` : agrégats et statuts uniquement, jamais de ligne par cellule.
- Capture idempotente : `Store::capture_system_snapshot(environment, cadence, captured_at, ctx)`
  (`crates/store/src/observability.rs`), UPSERT sur `UNIQUE(environment, capture_date, cadence)` —
  aucune duplication silencieuse possible, garantie par la contrainte SQL, pas seulement par le code
  applicatif.
- Checksum déterministe : `build_system_checksum` sérialise un `BTreeMap` trié par clé (ordre de
  champ fixe) en JSON canonique puis SHA-256 — vérifié stable par
  `system_snapshot_capture_is_idempotent_same_day`
  (`crates/store/tests/observability.rs`).
- Comparaison J-1/J-7 : `Store::compare_system_snapshots(environment, days_ago)` — retourne `None`
  quand l'un des deux jours manque (jamais de comparaison fabriquée), et `relative_delta = None`
  quand la valeur précédente est nulle (`compare_j1_reports_deltas_and_avoids_division_by_zero`).
- Alertes : `Store::evaluate_and_record_alerts` — six règles versionnées (`v1`) : fraîcheur
  forecast/FIRMS (bandes normal/transient/degraded/stale/unavailable), nombre de modèles actifs
  ≠ 1, candidat `active` (défense en profondeur, déjà impossible en base), shadow scoring
  inattendu, migration échouée. Déduplication par `(rule_id, system_snapshot_id)` — un replay ne
  crée pas de doublon (`alerts_flag_missing_active_model_and_are_not_duplicated_on_replay`).
- CLI : `pyrorisk snapshot-operational --cadence {daily,hourly,event} [--at <RFC3339>]`,
  `snapshot-compare --days 1,7`, `snapshot-retention` (dry-run uniquement).
- Scheduler (`crates/engine/src/scheduler.rs`) : `snapshot_operational_hourly` (léger, toutes les
  heures) et `snapshot_operational_daily` (calé à 02:15 UTC, hors des fenêtres `poll_forecast`
  horaire et des timers de sauvegarde `deploy/oracle/systemd/pyrorisk-*.timer`).
- API `GET /api/science/observability/{latest,history,compare}` — 404 explicite si aucun snapshot,
  400 si `days` invalide, jamais de payload non borné (`days` clampé à 366, `limit` à 200).
- Console : section « Observabilité » (`crates/api/static/science/science.js`,
  fonction `PAGES.observability`).

## 2. Ce qui a été volontairement omis

- `caddy_state` reste `non_exposed` par défaut : aucune tentative n'a été faite d'inventer un état
  Caddy depuis PostgreSQL. Une intégration future devra passer cette valeur explicitement via
  `SystemSnapshotContext.caddy_state` (variable d'environnement `ERYTHEON_CADDY_STATE`, lue depuis
  un composant séparé sur le VPS).
- `application_restart_count` reste `None` tant qu'aucune source fiable n'est câblée.
- Un seul environnement (`"default"`) est utilisé pour cette phase pilote — voir
  `PHASE4A5_DEPLOYMENT_PLAN.md` pour la trajectoire multi-environnement.

## 3. Tests

`crates/store/tests/observability.rs` (5 tests), `crates/api/tests/science.rs` (5 tests
supplémentaires), `crates/store/tests/rollback_guard_safety.rs` (2 tests supplémentaires) — tous
exécutés contre une base PostGIS 16/3.4 jetable, `cargo clippy -p store -p engine -p api --all-
targets -- -D warnings` et `cargo fmt --all -- --check` propres.

## 4. Limite connue

Le calcul de `forecast_horizon_count` et `forecast_age_seconds` dépend de
`forecast_batches.completed_at` — or `Store::retain_forecast_batch` (code existant, non modifié)
supprime tout batch antérieur dès qu'un nouveau réussit. Un snapshot capturé juste avant un
remplacement de batch reflète donc l'état juste avant, pas un historique complet des batches
intermédiaires — comportement documenté, pas corrigé (modifier ce mécanisme est hors périmètre de
cette phase).
