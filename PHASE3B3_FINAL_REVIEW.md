# ERYTHEON — Phase 3B.3 final review

Independent review of the dataset foundation phase before it is considered
closed. Performed against the actual local Git state and the isolated VPS
pilot database, not against the summary report alone.

## 1. Git verification

```
git status --short         → empty, clean tree
git log --oneline -10      → edf8b5c (HEAD -> main) docs: add phase 3B.3 dataset foundation report
                              f76fd20 fix: harden pilot dataset rebuild and exclusions
                              c661a03 fix: validate dataset snapshot and calendar foundations
                              a9b5649 feat: add dataset foundation and historical feature versioning
                              2fca1d9 (origin/main, origin/HEAD) ...
```

Commit order is correct: `a9b5649` (foundation) → `c661a03` (validation fixes)
→ `f76fd20` (hardening) → `edf8b5c` (report), all strictly after
`origin/main` (`2fca1d9`), none pushed.

`git show --stat f76fd20` confirms exactly the three files the hardening
pass was supposed to touch: `crates/engine/src/dataset_pipeline.rs`,
`crates/store/src/dataset.rs`, `crates/store/src/lib.rs`. `git show --stat
edf8b5c` confirms exactly the one report file. Nothing else is bundled into
either commit.

`git diff origin/main..HEAD --stat` (4 commits) touches only: `Cargo.toml` /
`Cargo.lock` (new workspace member), the new `dataset` crate, `crates/engine
/src/dataset_pipeline.rs` + `main.rs` (new CLI subcommands only), `crates/
store/src/dataset.rs` + `lib.rs` (new module + new error variants), the new
`crates/store/tests/dataset_foundation.rs`, migrations `0013`–`0015` and
their rollbacks, and the two Markdown reports. No file under `crates/api`,
`crates/risk`, `crates/fwi`, anything FIRMS-related, or
`crates/engine/src/scheduler.rs` appears in the diff — confirmed by grep,
not assumption.

Grepping the full diff for `password|secret|BEGIN (RSA|PRIVATE|OPENSSH)|
api[_-]?key|token` and for `.dump|.sql.gz|target/|.env|.pem` in changed
filenames both return nothing. No secret, no dump, no build artifact, no
real personal data (the diff is schema/code/logic, not data).

**Verdict: clean.**

## 2. Migration review (`0013`–`0015`)

All three are additive-only; none alters or drops anything in `public`,
`fire`, or `validation`. Each has a rollback that refuses to run
(`RAISE EXCEPTION`) once its own tables hold any row, following the
established guarded-rollback pattern from earlier phases.

- **`0013`** (`features.feature_snapshots`): partial unique index enforces
  one `active` row per `family`; a `CHECK` ties `validated_at IS NOT NULL`
  to any status past `draft`/`failed`. Verified against the running code:
  `register_feature_snapshot` always inserts `status = 'draft'`, and
  `activate_feature_snapshot` sets `validated_at = COALESCE(validated_at,
  NOW())` before flipping to `active` — the constraint cannot be violated by
  the current code path.
- **`0014`** (`features.calendar_rule_versions` /
  `historical_calendar_days`): the `historical_calendar_days_school_holiday_
  classification_check` constraint (`school_holiday IS NOT NULL OR
  temporal_classification = 'unavailable_historically'`) is the single most
  important guarantee in this phase against silently fabricating school
  holidays — a `NULL` can never be paired with a stronger classification at
  the database level, not just by convention in application code.
- **`0015`** (`ml.dataset_versions`/`dataset_builds`/`dataset_rows`/
  `dataset_row_snapshots`/`dataset_event_links`/`dataset_exclusions`): the
  `forbid_finalized_dataset_version_update` trigger makes `finalized` truly
  immutable at the database level (not just by convention); `dataset_
  exclusions`'s `reason_category` `CHECK` already listed `missing_features`
  and `non_combustible_cell` as separate values from this migration's first
  commit (`a9b5649`) — the bug fixed in `f76fd20` was exclusively in the
  Rust selection logic in `dataset_pipeline.rs`, never in this constraint.

**Volume/performance risk:** none at current or foreseeable pilot scale.
`historical_calendar_days` holds 2,557 rows (one calendar, 2020–2026, single
school zone); `ml.dataset_rows` holds at most a few thousand rows per
variant. No table here is a candidate for partitioning, and no index here
is expensive to maintain at these volumes. The only large table this phase
reads from is `public.cell_static` (920,016 rows), and it is read via one
aggregate summary query, never pulled row-by-row into the application —
confirmed by reading `cell_static_snapshot_summary` in `crates/store/src/
dataset.rs`, which uses an `MD5` aggregate rather than fetching all rows.

**Constraint permissiveness:** the `dataset_exclusions_subject_check`
(`ignition_event_id IS NOT NULL OR (h3 IS NOT NULL AND local_date IS NOT
NULL)`) correctly requires *some* identifying subject without over-
constraining which kind, since the same table now serves both event-level
exclusions (`certain_duplicate`, `out_of_period`) and cell-day-level ones
(`missing_features`, `non_combustible_cell`, `insufficient_geographic_
quality`). No constraint identified as either too permissive or too
restrictive for its actual usage.

**Migration/code coherence:** verified consistent. The only historical
mismatch (missing `::uuid` cast on `calendar_rule_version_id` in
`create_dataset_version`) was already caught and fixed in `c661a03`, before
this review.

**Verdict: no blocking migration issue.**

## 3. Scientific accuracy review

Checked each claim in `PHASE3B3_DATASET_FOUNDATION_REPORT.md` against the
database and code directly, not taken at face value:

| Claim | Verified |
|---|---|
| `cell_static` snapshot classified `current_snapshot_applied_historically` | Yes — confirmed by direct query against `features.feature_snapshots`. |
| 2026 static-feature values not presented as historically exact for 2020–2025 | Yes — the classification itself is the mechanism; no report language claims historical accuracy. |
| School holidays `NULL` + `unavailable_historically` | Yes — 2,557/2,557 rows, both confirmed by query. |
| Pilot negatives still marked `pilot_only` | Yes — `PILOT_STRATEGY_ID = "pilot_only_deterministic_hash_v1"`, and the report explicitly calls this "not the final scientific negative strategy" in two places. |
| Pilot results not presented as the final dataset | Yes — report section A states the foundation-only scope explicitly and section N recommends the negative-sampling design as the next, not-yet-started step. |
| Active v1 model still only an operational benchmark | Out of scope for this phase's code (no model-serving file was touched — confirmed by the diff in §1), and the report does not claim otherwise. |
| No absolute probability claimed | Confirmed — the report describes counts, checksums, and architecture, not calibrated probabilities. |

**Verdict: no overstated or misleading claim found in the final report.**

## 4. Code review

Read directly: `crates/dataset/{calendar,checksums,exclusions,negatives,rows,
snapshots,splits,temporal}.rs`, `crates/store/src/dataset.rs`, `crates/
engine/src/dataset_pipeline.rs`, `crates/store/tests/dataset_foundation.rs`.

- **Determinism**: every identifier (`deterministic_row_key`, `row_checksum`,
  the pilot negative hash, the dataset build checksum) is a pure function of
  its stated inputs; none reads wall-clock time, random state, or row order.
- **Sort-before-checksum**: `row_checksum` explicitly sorts `quality_flags`
  and `snapshot_ids` before hashing (`rows.rs`), and this is unit-tested
  (`row_checksum_ignores_input_order_of_flags_and_snapshots`).
- **Identifier stability**: `deterministic_row_key` is keyed on
  `(dataset_logical_id, h3, date, category)` only — stable across rebuilds,
  confirmed by `deterministic_key_is_stable_and_distinguishes_category`.
- **Timezone**: the calendar and split logic operate on `chrono::NaiveDate`
  (civil dates), not on timestamps requiring timezone conversion; the
  `Europe/Paris` label in `DatasetVersionSpec.timezone` is descriptive
  metadata. This is consistent with `fire.ignition_events.occurred_on_local`
  already being a local civil date at ingestion (phase 3B.1), not a new
  assumption introduced here. Non-blocking note: if a future feature ever
  needs true tz-aware arithmetic (e.g., a `TIMESTAMPTZ`-based cutoff near
  midnight), this module does not provide it — flagged as an open risk (§I
  below), not a defect in what exists today.
- **Leap years**: `dataset::calendar` is explicitly tested for both a leap
  year (2020, 2024 → 366 days) and a non-leap year (2025 → 365 days).
- **Strict/inclusive separation**: `build_human_dataset` computes
  `strict_ok` once per cell-day and always pushes into `inclusive_rows`
  first, then conditionally into `strict_rows` — inclusive is a strict
  superset of strict's positives, by construction, not by convention.
- **Geographic classification**: now covered by `dataset_pipeline::tests::
  strict_mode_rejects_only_the_genuinely_low_confidence_real_categories` and
  `strict_mode_selection_is_not_driven_by_fictional_categories`, both
  re-verified green on the isolated VPS build in this review.
- **`missing_features` / `non_combustible_cell` separation**: verified
  fixed by reading the current `if !features_present { MissingFeatures }
  else if combustible != Some(true) { NonCombustibleCell } else { ... }`
  order, with the inline comment explaining why the order matters.
- **Draft vs. finalized rebuild behavior**: `get_or_create_dataset_version`
  reuses a matching draft/validated/building row, rejects (`
  DatasetVersionParametersChanged`) a same-`logical_id` row whose defining
  parameters differ, and rejects (`DatasetVersionFinalized`) any reuse
  attempt against a `finalized` row — enforced doubly, by this function and
  by the database trigger.
- **New build on rejeu, no row duplication**: `start_dataset_build` always
  inserts a new `ml.dataset_builds` row; `ml.dataset_rows` is upserted with
  `ON CONFLICT (dataset_version_id, deterministic_key) DO UPDATE SET
  deterministic_key = ml.dataset_rows.deterministic_key` (a no-op update used
  only to obtain `RETURNING id`), so a replay never inserts a second row for
  the same key. Directly verified on the isolated DB in this review's own
  session: `count(DISTINCT deterministic_key) = count(*)` on both variants
  after three total build attempts against the same two dataset versions.
- **Transactions**: `persist_dataset_rows` (rows + snapshots + event links)
  and `persist_dataset_exclusions` (delete + insert) each run inside one
  `sqlx` transaction, committed only at the end.
- **Recovery after error / partial build**: on a `persist_dataset_rows`
  failure, `build_human_dataset` calls `finish_dataset_build(..., false, ...,
  Some(error))` and `set_dataset_version_status(..., "failed")` before
  propagating the error — a partial build is marked failed and auditable,
  not left in an ambiguous `building` state. Not exercised end-to-end by an
  integration test in this phase (no test forces a mid-transaction failure);
  flagged as a non-blocking gap below.

**Verdict: no blocking code defect found.**

## 5. Pilot results consistency

Recomputed directly against the isolated DB (`erytheon-3b3-deploy-
20260727T203310Z`), not copied from the prior report without re-checking:

| Quantity | Count |
|---|---|
| `human_known` events admissible in period | 7,094 |
| Distinct positive `(h3, local_date)` cell-days | 6,849 |
| Strict dataset rows | 2,666 (2,566 positive + 100 pilot negative) |
| Inclusive dataset rows | 6,949 (6,849 positive + 100 pilot negative) |

The 7,094 → 6,849 gap is fully accounted for by events sharing a cell-day
with at least one other event (the dataset's observation unit is the
cell-day, not the event) — consistent with `ml.dataset_event_links`
supporting multiple `ignition_event_id`s per `dataset_row_id`.

The 6,849 → 2,566 gap (inclusive positives → strict positives) is fully
accounted for by the strict-only, **cell-day-level** exclusions, re-measured
per variant:

| Reason | Count |
|---|---|
| `insufficient_geographic_quality` | 3,624 |
| `missing_features` | 22 |
| `non_combustible_cell` | 637 |
| **Total cell-day-level exclusions from strict** | **4,283** |

`6,849 − 4,283 = 2,566` exactly. `certain_duplicate` (3 rows) is an
**event-level** exclusion — it removes a duplicate *candidate* event from a
group while the group still contributes one positive cell-day row (via its
anchor) to both variants — so it correctly does not enter this subtraction.
The per-variant total in `ml.dataset_exclusions` (4,286) is the sum of the 3
event-level rows plus these 4,283 cell-day-level rows; conflating the two
granularities before subtracting would produce a false 3-row gap. Verified
directly against the isolated DB rather than assumed.

**Verdict: pilot volumes are exactly consistent once event-level and
cell-day-level exclusion counts are kept separate.**

## 6. Anomalies

### Blocking
None found.

### Non-blocking
1. No integration test exercises a genuine mid-transaction failure of
   `persist_dataset_rows`/`persist_dataset_exclusions` to confirm the
   `failed` status path end-to-end (code review only, not test-verified).
2. `dataset::calendar`/`splits` operate on `NaiveDate` without their own
   timezone-aware cutoff logic; correct today because upstream data is
   already localized, but worth a explicit note if a future feature needs a
   true instant-based (not calendar-date-based) boundary.
3. The 3-row event-level-vs-cell-day-level subtraction discrepancy in §5 is
   arithmetic, not a defect, but is exactly the kind of thing a future
   reader could misinterpret as a bug; documented here to close that gap.

## 7. Risks

**Scientific risks**
- No real negative-sampling strategy exists yet; the pilot's 100-per-variant
  hash-based negatives are not statistically designed.
- `cell_static` features are a single present-day snapshot applied across
  2020–2026; historical land-use/WUI/road/population drift before "today" is
  not represented in any row.
- School-holiday feature is entirely unavailable for the full studied period.
- `insufficient_geographic_quality` removes roughly half of admissible
  events from strict mode; the strict dataset is materially smaller and its
  representativeness relative to the inclusive dataset has not been
  characterized.

**Operational risks**
- The isolated VPS DB and its data are the only place these results have
  been produced; nothing is reproducible from a fresh clone without also
  replaying the earlier ingestion/quality phases against a real production
  dump.

**Migration risks**
- None identified beyond the already-mitigated ones in §2 (guarded rollback,
  additive-only).

**Volume risks**
- None at current scale; would need re-assessment only if the historical
  period or H3 resolution were substantially widened.

## 8. Recommendations

**Before push**: none required beyond normal CI (fmt/clippy/test) on the
four unpushed commits; no defect requires rework first.

**Before production**: do not apply migrations `0013`–`0015` to production
until a real (non-pilot) dataset build is planned, since these tables have
no production consumer yet and no operational benefit until phase 3B.4/3B.5
produce a dataset worth serving from them.

**Before training**: resolve the negative-sampling strategy (this review's
Part B, below) and decide how to represent (or explicitly exclude) the
school-holiday and historical-feature-drift limitations before any model
is fit on this dataset.

---

```
PHASE 3B.3 REVIEW PASSED
```
