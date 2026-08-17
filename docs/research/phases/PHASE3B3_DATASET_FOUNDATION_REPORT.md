# ERYTHEON — Phase 3B.3 dataset foundation report

## A. Scope and status

Phase 3B.3 builds the **foundation** for ML dataset versioning: feature
snapshots, a historical calendar, and a versioned dataset schema
(`ml.dataset_versions` / `dataset_builds` / `dataset_rows` /
`dataset_row_snapshots` / `dataset_event_links` / `dataset_exclusions`), plus
one pilot dataset build used to exercise that architecture end to end.

This phase does **not** train a model, calibrate probabilities, publish new
risk scores, change the API/UI/FWI/FIRMS pipelines, add a scheduler entry, or
finalize a scientific negative-sampling strategy. The pilot negatives
(`pilot_only_deterministic_hash_v1`) exist only to prove the row/exclusion
plumbing works; they are explicitly not presented as a scientific sampling
design.

Commits, in order:

| Commit | Summary |
|---|---|
| `a9b5649` | feat: add dataset foundation and historical feature versioning — migrations `0013`–`0015`, `dataset` crate, `store::dataset`, CLI commands |
| `c661a03` | fix: validate dataset snapshot and calendar foundations — `snapshots.rs`, temporal-classification fix for calendar days, integration tests |
| `f76fd20` | fix: harden pilot dataset rebuild and exclusions — idempotent rebuild, exclusion-reason separation, geographic regression test |

Working tree is clean (`git status --short` empty) and `f76fd20` contains
exactly the three files the hardening pass touched — confirmed by
`git show --name-only --format= HEAD`:

```
crates/engine/src/dataset_pipeline.rs
crates/store/src/dataset.rs
crates/store/src/lib.rs
```

No file is missing from the commit; nothing was left uncommitted.

## B. Migrations `0013`–`0015`

All three are additive-only, each with a guarded rollback that refuses to run
destructively once real data exists (`RAISE EXCEPTION` guard, mirroring the
pattern used in every prior phase).

- **`0013_feature_snapshot_foundation.sql`** — `features.feature_snapshots`:
  one row per versioned feature bundle (family, source, provider, vintage,
  validity/availability windows, checksums, status
  draft/validated/active/superseded/failed, temporal classification,
  limitations, license). Partial unique index enforces at most one `active`
  snapshot per family.
- **`0014_historical_calendar_foundation.sql`** — `features.calendar_rule_versions`
  (mirrors `validation.rule_versions`) and `features.historical_calendar_days`,
  PK `(rule_version_id, date, school_zone)`. A `CHECK` constraint
  (`historical_calendar_days_school_holiday_classification_check`) forbids a
  `NULL school_holiday` from being paired with any classification stronger than
  `unavailable_historically` — a missing source can never be silently upgraded
  to a confident value.
- **`0015_dataset_versioning_foundation.sql`** — `ml.dataset_versions`
  (trigger `forbid_finalized_dataset_version_update` makes a `finalized` row
  immutable), `ml.dataset_builds` (one row per build attempt, multiple per
  version), `ml.dataset_rows` (unique on `(dataset_version_id,
  deterministic_key)`), `ml.dataset_row_snapshots` (provenance), `ml.dataset_event_links`
  (unique on `(dataset_row_id, ignition_event_id)`), `ml.dataset_exclusions`
  (`reason_category` constrained to 13 named values, including
  `missing_features` and `non_combustible_cell` as separate values from the
  start — the schema was always correct; the bug fixed in `f76fd20` was in the
  Rust selection logic, not the constraint).

## C. Feature snapshot: `cell_static` bundle

One active snapshot, registered from the current production `cell_static`
table (isolated DB, restored from the verified production dump):

- `family`: `cell_static_bundle`
- `status`: `active`
- `cell_count`: 920,016 (matches the production `cell_static` row count
  reported in [Phase 3B.1](PHASE3B1_BDIFF_FOUNDATION_REPORT.md) /
  [Phase 3B.2](PHASE3B2_QUALITY_REPORT.md))
- `h3_resolution`: 9
- `temporal_classification`: `current_snapshot_applied_historically` — the
  bundle is today's static-feature state, applied across 2020–2026 because no
  versioned historical alternative exists; this is an explicit, recorded
  approximation, not a claim of historical accuracy
- `logical_checksum`: `9af091544331a4f0ebb0a87fefd7e65d`

`register_feature_snapshot` is idempotent on `(family, logical_checksum)`
(verified by `feature_snapshot_registration_is_idempotent_and_supports_activation`);
re-registering the same bundle returns the existing id rather than inserting a
duplicate row.

## D. Historical calendar 2020–2026

- `calendar_rule_versions`: one row, `logical_id
  erytheon_calendar_generation_v1`, `rule_type public_holiday`, `status active`,
  `checksum b96159bd4c3f3befa4687e8ceb43a8aa670533f3a5967aad092098f42a610d3f`
- `historical_calendar_days`: 2,557 rows, `2020-01-01`–`2026-12-31`
- Public holidays: computed exactly via the Meeus/Jones/Butcher Easter
  algorithm plus the fixed French holiday set — deterministic, unit-tested
  (9 tests in `dataset::calendar`, including a check that no future year's
  rule can leak information into a past year's classification).
- School holidays: **no verified source exists**. `school_holiday` is `NULL`
  for all 2,557 rows, and `temporal_classification` is
  `unavailable_historically` for all of them — enforced, not just reported, by
  the `0014` `CHECK` constraint. School holidays were not fabricated.

## E. Pilot dataset: strict and inclusive

Built by `engine::dataset_pipeline::build_human_dataset` over `human_known`
BDIFF events, 2020–2026, `seed 2026071`.

**Events vs. cell-days vs. rows — these are three different counts, not one:**

| Quantity | Count |
|---|---|
| Admissible `human_known` events in period | 7,094 |
| Distinct positive `(h3, local_date)` cell-days | 6,849 |
| Inclusive dataset rows (6,849 positive + 100 pilot negative) | 6,949 |
| Strict dataset rows (2,566 positive + 100 pilot negative) | 2,666 |

7,094 events collapse to 6,849 cell-days because several events can share the
same H3 cell and the same local date; the dataset's observation unit is the
cell-day, not the event, so this gap is expected and is preserved rather than
hidden.

**Exclusions — corrected, honestly re-measured, identical per variant since
the exclusion list is computed once before the variant split:**

| `reason_category` | Count (per variant) |
|---|---|
| `certain_duplicate` | 3 |
| `insufficient_geographic_quality` | 3,624 |
| `missing_features` | 22 |
| `non_combustible_cell` | 637 |
| **Total** | **4,286** |

Before the `f76fd20` fix, `missing_features` and `non_combustible_cell` were
merged into a single `non_combustible_cell` count of 659 per variant (22 + 637).
The fix is a pure reclassification — no event gained or lost admissibility,
and the totals were not forced to match any prior expectation; they were
re-measured directly against the isolated DB after rebuilding.

**Geographic categories actually assigned** (verified by direct query against
`validation.event_geographic_quality`, and now pinned by a regression test in
`crates/engine/src/dataset_pipeline.rs`): `municipality_centroid_probable`,
`precision_undocumented`, `rounded_coordinate_probable`. The phase 3A
specification's `precise_reported`/`estimated_reported` categories are never
assigned by `quality::assess_geography` and do not drive selection.
`precision_undocumented` — the documented ceiling of achievable quality today
— is accepted in strict mode; the other two are not.

## F. Pilot negatives (`pilot_only_deterministic_hash_v1`)

100 negatives per variant, sampled from combustible cells via a
`splitmix64`-style deterministic hash of `(h3, date, seed)`, excluding any
cell-date already carrying a known fire event of any cause (human, natural, or
unknown). This is explicitly a pilot mechanism to exercise the row/exclusion
architecture — not a scientific negative-sampling strategy. That design
remains open and is out of scope for this phase.

## G. Splits

`train 2020-2023`, `calibration 2024`, `test 2025`, `prospective 2026` —
recorded per row and per dataset version; assignment is a pure function of the
event's year (`Split::for_year`), unit-tested.

## H. Determinism and idempotence

Verified directly on the isolated pilot DB
(`erytheon-3b3-deploy-20260727T203310Z`), not assumed:

1. Rebuilt strict + inclusive with the corrected code (first invocation after
   `f76fd20`): both variants returned `reused_existing_version: true` against
   the pre-existing draft versions from the earlier (pre-hardening) pilot
   build, since parameters matched — no raw unique-constraint error.
2. Replayed each variant once more, identical `seed 2026071`: **same
   `dataset_version_id`, same final checksum** (`2234640f…` strict,
   `e7b2f5b7…` inclusive) both times; only the `build_id` changed (new row in
   `ml.dataset_builds` each time, `status succeeded`, three build rows per
   variant now on record — the original pre-hardening build plus the two
   post-hardening runs, all three sharing the identical checksum).
3. `count(DISTINCT deterministic_key) = count(*)` on `ml.dataset_rows` for
   both variants (2,666 / 6,949) — no duplicated rows across the three build
   attempts.
4. `ml.dataset_exclusions` is scoped per `dataset_version_id` and replaced
   (`DELETE` then `INSERT` in one transaction) on every build, so replaying
   never accumulates duplicate exclusion rows.

A `finalized` dataset version cannot be rebuilt at all
(`DatasetVersionFinalized`); a same-`logical_id` rebuild with different
defining parameters is rejected (`DatasetVersionParametersChanged`) rather than
silently accepted or silently ignored.

## I. Bugs found and fixed this phase

1. **Geographic category mismatch** (found during the initial pilot build,
   fixed in `c661a03`/carried through): the strict-mode filter checked against
   `precise_reported`/`estimated_reported`, categories `quality::assess_geography`
   never assigns. This excluded 100% of positives from strict mode on first
   dry run. Fixed by inverting to a `LOW_CONFIDENCE_GEOGRAPHIC_CATEGORIES` list
   naming the two genuinely low-confidence categories that are actually
   assigned, and now covered by a dedicated regression test (`f76fd20`).
2. **Calendar day temporal-classification bug** (`c661a03`): every historical
   calendar day was hardcoded to `HistoricalExact` even when `school_holiday`
   was `None`, which would have violated the `0014` `CHECK` constraint on the
   first real invocation. Fixed to `UnavailableHistorically` when no source
   exists.
3. **Missing `::uuid` cast** in `create_dataset_version`'s nullable
   `calendar_rule_version_id` parameter (`c661a03`), caught by a new
   integration test, not by manual inspection.
4. **Non-idempotent rebuild** (`f76fd20`): a second `build-human-dataset`
   invocation under the same `logical_id`/seed failed with a raw
   `dataset_versions_logical_id_key` violation. Fixed with
   `get_or_create_dataset_version`.
5. **Mixed exclusion reasons** (`f76fd20`): `combustible != Some(true)` was
   checked before `!features_present`, so cells with no `cell_static` row at
   all (`combustible: None`) were misclassified as `non_combustible_cell`
   instead of `missing_features`. Fixed by checking missing-features first.

## J. Performance

The pilot build touches ~14,000 candidate events and ~920,016 `cell_static`
rows only via one MD5-aggregate summary query (`cell_static_snapshot_summary`),
never pulling the full table into application memory. Row/snapshot/event-link
persistence is batched via `jsonb_to_recordset` in a single transaction rather
than row-by-row round trips. The full workspace build (cold cache, 2 vCPU / 2
GiB isolated container) completed in 2m30s; the full `dataset`+`store`+`engine`
test suite in under a second of test time; each pilot build/replay invocation
completed in a few seconds against the isolated DB.

## K. Non-destructivity

No migration in this phase alters or drops an existing column, table, or row
from `fire.*`, `validation.*`, or `public.cell_static`. `fire.ignition_events`
was read-only throughout — no dataset-pipeline code path writes to it.
Rollback scripts for `0013`–`0015` follow the existing guarded-refusal pattern
and were not executed against data-bearing tables. The idempotence fix
(`get_or_create_dataset_version`, `persist_dataset_exclusions`) only ever
deletes/replaces rows that are themselves this pipeline's own derived output
(`ml.dataset_exclusions`, scoped by `dataset_version_id`) — never a source
event.

## L. VPS state and cleanup

- Isolated DB container `erytheon-3b3-deploy-20260727T203310Z` (postgis/postgis:16-3.4):
  left running, holding the validated pilot dataset described above, on its
  original network `erytheon-3b3-deploy-net`.
- Ephemeral build container `erytheon-3b3-build2` (rust:1.94-bookworm): used
  for this hardening pass's build/test/rebuild cycle, removed after use.
- Temporary network `erytheon-3b3-net` (created solely to let the build
  container reach the isolated DB): container disconnected, network removed.
- Transferred archive `/tmp/erytheon-pilot-hardening.tar.gz` on the VPS:
  deleted after extraction.
- Production containers (`pyrorisk-app-1`, `pyrorisk-postgres-1`,
  `pyrorisk-caddy-1`) were not touched at any point in this phase.
- Disk usage on the VPS: 39G/96G (40%) — unchanged in any concerning way by
  this work.

## M. Open risks

- The pilot negative strategy is a placeholder; the real scientific
  negative-sampling design (spatial/temporal control, class balance,
  confounding with combustibility) is still undecided.
- `school_holiday` has no data source; any model trained before one is found
  will lack that feature entirely for the full 2020–2026 span.
- The `cell_static` snapshot is applied uniformly across 2020–2026
  (`current_snapshot_applied_historically`); real historical drift in
  land-use/WUI/road/population features before "today" is not represented.
- `insufficient_geographic_quality` currently excludes 3,624 of 7,094 events
  (about half) from strict mode per variant-pair; the strict dataset's
  positive count (2,566) is materially smaller than the inclusive one (6,849)
  as a direct consequence.
- Feature values in the pilot rows are placeholders (`0.0` for
  wui/road/agri/population/poi/power_line) — the pilot exercises row/version
  plumbing, not real feature extraction; wiring real `cell_static` values into
  `RowFeatures` is unfinished work.

## N. Recommended next step (not started)

Design and specify the real negative-sampling strategy (spatial control,
temporal control, class ratio, interaction with `insufficient_geographic_quality`
exclusions) as its own reviewed decision, before wiring real `cell_static`
feature values into `RowFeatures` and before any model-training phase begins.

---

```
PHASE 3B.3 READY FOR REVIEW
NO PRODUCTION DEPLOYMENT
NO MODEL TRAINING
```
