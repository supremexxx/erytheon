# Phase 3B.9 — Model Promotion Plan

## Serving compatibility (mission §10)

Current path: `Store::active_human_model` loads a `human_model_versions`
row, deserializes it into `risk::LearnedHumanModel`, and
`HeuristicV1::score` calls `LearnedHumanModel::predict`. This path is
untouched by phase 3B.9 and must stay untouched through at least P0-P2.

To support multiple model families later (`logistic_v1`,
`gbm_isotonic_v2`, ...), the simplest safe option is a serializable enum
over the two artifact shapes, not a new `dyn Trait` object — a trait
(as the mission itself suggests as one option) would need to be object-
safe and would require `risk::CellFeatures` to grow a generic or
enum-based raw-feature representation shared by both families, which is
more invasive than this phase should attempt. An enum is simpler and
statically exhaustive:

```rust
enum ScoredModel {
    LogisticV1(risk::LearnedHumanModel),
    GbmIsotonicV2(candidate_artifact::CandidateArtifact),
}

impl ScoredModel {
    fn predict(&self, raw: &BTreeMap<String, serde_json::Value>) -> anyhow::Result<f64> {
        match self {
            Self::LogisticV1(model) => Ok(f64::from(model.predict(&cell_features_from(raw)))),
            Self::GbmIsotonicV2(artifact) => candidate_artifact::score_with_artifact(artifact, raw),
        }
    }
}
```

**Not implemented in this phase** — this is a design to review, not
code to merge yet. `HeuristicV1::score`'s current behavior (v1's own
feature vector, its own fusion with FWI, its own `combustible` gate)
must remain byte-for-byte identical until an authorized future phase
actually introduces this enum and wires it into serving.

## Feature vector v2 (mission §11)

Already implemented as `candidate_artifact::feature_order()` +
`CandidateArtifact::feature_names`/`feature_types` — a `Vec<String>`
confirmed against the real trained artifact (not the mission brief's
own indicative order, which differs), with `validate()` rejecting any
artifact whose `feature_names` doesn't match exactly. No `HashMap`
determines order anywhere in the inference path; `BTreeMap`s are used
only for name-keyed *parameter lookup*, never for the positional
feature vector itself (`score_with_artifact` always iterates
`artifact.feature_names` in its declared `Vec` order).

## Feature availability in production (mission §12)

All 12 features already have an established production data path —
**no schema change is needed to expose any of them**:

| Feature | Production source | Same as v1 uses? |
|---|---|---|
| `wui`, `road`, `agri`, `population`, `poi`, `power_line` | `cell_static.features` (whole JSONB blob, already selected by `Store::risk_inputs`) | Yes — v1 reads the same column |
| `hist` | same `cell_static.features`, refreshed by `static_layers::refresh_history_features` (a manual/periodic CLI command, not a continuous live computation) | Not a direct v1 feature, but the same column v1's serving query already returns |
| `combustible` | same `cell_static.features` | Yes — v1 gates on this exact field |
| `weekend`, `season_sine`, `season_cosine` | pure functions of the date | Yes |
| `public_holiday` | `calendar_days` (production table, refreshed by the same manual static-layers command) | Yes — v1 already reads this via `COALESCE(day.public_holiday, FALSE)` |

**`hist` specifically** (mission's flagged critical feature): computed
from `ignition_history` via `static_layers::history_kernel`/
`refresh_history_features`, invoked manually, not on a recurring
scheduler. It uses only past/current ignition records (no future
leakage by construction — the kernel only ever reads
`historical_ignitions_until`). Cold start: a cell with zero
`ignition_history` coverage gets `hist = None` at the aggregation level
(`Res8AggregatedFeatures`, mission's own "zero children never defaults
combustible" principle applies analogously here), which flows through
the candidate's existing train-only imputation rule (`ImputationRule`)
rather than a fabricated zero. This is not a new gap this phase
introduces — it is the same behavior v1's own serving path already
has, since `hist` sits in the same `cell_static.features` column.

## Offline/online parity (mission §13) — the one real, measured risk

Measured on the full 2025 comparable population (4,708 rows), reusing
the same `cell_static`/`calendar_days` tables a live scoring path would
query today:

| Field | Exact match rate |
|---|---|
| `wui` | 96.1% |
| `road` | 95.3% |
| `agri` | 96.7% |
| `population` | 97.1% |
| `poi` | 97.0% |
| `power_line` | 98.8% |
| `hist` | 96.3% |
| `combustible` | 99.9% |
| `public_holiday` | 100.0% |

**This is not a feature-availability gap** — it is `cell_static` having
been refreshed at least once since the phase 3B.5 dataset was built,
which is expected, healthy operational behavior for a periodically-
refreshed static layer, not a semantic incompatibility. `current_
snapshot_applied_historically` (already documented since phase 3B.5)
is exactly this: the dataset froze one snapshot in time; production
keeps moving. A ~4% numeric drift and a ~0.1% combustible-flag drift
per feature is consistent with ordinary periodic refresh, not a defect.
`public_holiday` (100%) confirms the calendar path is fully stable.

**This is still a real, quantified risk to carry into any activation
decision**: the frozen candidate's calibration was fit against the
2020-2024 snapshot; if `cell_static` is refreshed again before
activation, the candidate's calibration is not re-validated against
that newer snapshot. The safe mitigation, not yet executed: refresh
`cell_static` once, immediately before any P1 registration, and treat
shadow scoring (P3/P4) as the actual live re-validation of this
parity, since shadow scoring always queries current data, not the
frozen offline snapshot.

## Performance (mission §16)

Measured in the isolated phase 3B.9 build container (not production —
a gross-regression check only), full 2025 test split (4,708 rows):

| Metric | Value |
|---|---|
| Artifact load + validate | 238 µs |
| Unit score p50 | 5.04 µs |
| Unit score p95 | 6.81 µs |
| Unit score p99 | 13.6 µs |
| Batch (4,708 rows) | 27.4 ms total |

Budget proposed: shadow-scoring overhead p95 < 10 ms per request.
Measured p95 (6.81 µs) is roughly three orders of magnitude under that
budget — performance is not a blocker for any P0-P4 step. Production
numbers (network, real request concurrency, actual hardware) will
differ and must be re-measured once shadow scoring is actually
deployed, but nothing here suggests a structural performance risk.

## Resilience (mission §17)

Verified (`crates/engine/src/candidate_artifact.rs` tests): missing/
corrupted artifact, invalid checksum, incompatible version, missing
feature (with and without an imputation fallback), non-finite internal
statistic, extreme/overflowing input, empty byte input — all return
`Err`, none panic (confirmed via `catch_unwind`). No test exercises a
real "candidate scoring times out mid-request and v1 still responds"
scenario, because no serving-path integration exists yet in this phase
to test — that becomes testable once §10's enum/dispatch design is
actually wired in.

## Combustibility sensitivity (mission §21 — unchanged this phase)

Carried forward from phase 3B.8, restated as a formal risk, not
resolved: 339 of 4,708 comparable 2025 rows (7.2%) would be excluded
under a majority/≥50% combustibility rule instead of the current
`any(child)` rule, disproportionately positives (268 of 1,177
positives, 22.8%, vs. 71 of 3,531 negatives, 2.0%). **The candidate
continues to use the same `any(child)` rule it was trained on** — no
rule change occurs in this phase, and none should occur without a
dedicated scientific review of this specific finding.

## P0-P7 staged plan (mission §19)

```
P0 — artifact validated hors production           <- this phase, done
P1 — registration in production as an inactive model   <- needs separate authorization
P2 — load-only verification, no scoring                <- needs separate authorization
P3 — manual, limited shadow scoring                     <- needs separate authorization
P4 — extended shadow scoring                            <- needs separate authorization
P5 — review of shadow results                           <- needs separate authorization
P6 — limited candidate activation                        <- needs separate authorization
P7 — full activation                                      <- needs separate authorization
```

Only P0 is authorized and completed by this phase.

## P0 → P1 blocking criteria (mission §20)

| Criterion | Status |
|---|---|
| Artifact complete | Met — all required fields present, `validate()` passes |
| Checksums valid | Met — 5 checksums computed, round-trip verified |
| Training/inference parity | Met — 0 mismatches, max diff 0.0, on 4,708 rows |
| Offline/online parity | Measured, not 100% — 95.3-99.9% per field, explained above as expected snapshot drift, not a semantic gap |
| Resilience tests | Met — all pass |
| Performance acceptable | Met — p95 6.81 µs, far under a 10 ms budget |
| v1 unchanged | Met — no file under `crates/risk`, `crates/api`, or `human_model.rs` was modified |
| Migration reviewed | Met — `migrations/0016_model_candidate_registry.sql` proposed, not applied |
| Rollback prepared | Met — see below |
| No secrets | Met — `git status` clean, no `.env`/credentials in the diff |
| Documentation complete | Met — this document + 3 companions |
| Tests and clippy green | Met — `cargo fmt`/`clippy -D warnings`/`test --workspace` all pass |

`hist` — the mission's specifically flagged critical feature — **is
available in production with the same semantics** (same `cell_static`
column v1's own serving already reads); the only caveat is temporal
snapshot drift, already covered above and not a "feature indispensable
non disponible" condition. No criterion is unmet.

```
P0 MODEL ARTIFACT VALIDATED
READY FOR INACTIVE PRODUCTION REGISTRATION REVIEW
```

## Rollback (mission §18)

- **Candidate registered but inactive (future P1)**: disable/remove the
  candidate's config entry; v1 remains active throughout; no SQL
  deletion needed since the row was never marked active.
- **Shadow scoring active (future P3/P4)**: flip the feature flag off;
  stop shadow writes; keep already-recorded rows (read-only
  observational data, not deleted).
- **Candidate activated (future P6/P7, out of scope here)**: reactivate
  v1 by the same atomic `UPDATE human_model_versions SET active = FALSE
  WHERE active` + insert-active pattern `activate_human_model` already
  uses; never delete a model version; verify `/health` and a handful of
  real scores before considering the rollback complete.
- No destructive SQL rollback is ever performed once real data exists,
  matching this repo's existing migration `down.sql` convention
  (`migrations/rollback/0016_model_candidate_registry.down.sql` refuses
  to drop the table if any row exists).
