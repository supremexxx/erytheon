# Phase 3B.9 — Candidate Model Artifact

The `CandidateArtifact` format packaging the frozen phase 3B.7
GBM+isotonic candidate for a future promotion review. Built and
verified via:

```
pyrorisk package-candidate-artifact --seed <i64>
```

Official run: `seed=2026071`, code commit `e9d08cf`, artifact size
85,615 bytes (JSON), dataset row fingerprint `bee1bfaa5401144c5cbffe1f42bf45f7`.

## Format (`crates/engine/src/candidate_artifact.rs`)

```
artifact_version              u32, currently 1
model_family                  "gbm_isotonic_v2"
model_name                    "human_ignition_propensity_v2"
git_commit
dataset_logical_id            erytheon_human_ignition_cell_day_v1_candidate_inclusive_n3_adaptive_geographic_quality
dataset_row_fingerprint
feature_names                 Vec<String>, 12 entries, real training order (see below)
feature_types                  BTreeMap<name, "f64"|"bool">
normalization_parameters       BTreeMap<name, {method, statistics}>
imputation_parameters          BTreeMap<name, ImputationRule>
gbm_hyperparameters            {n_trees: 50, max_depth: 3, learning_rate: 0.1}
gbm                            GbmModel (50 trees)
isotonic_breakpoints / values  parallel Vec<f64>, 1774 points
training_period / calibration_period / test_period
seed
metrics                        the frozen test-2025 SplitMetrics
created_at
scientific_interpretation
known_limitations              Vec<String>
```

**Real feature order** (confirmed from the trained artifact, not the
mission brief's own indicative order, which differs):

```
wui, road, agri, population, poi, power_line, hist, combustible,
weekend, public_holiday, season_sine, season_cosine
```

JSON was chosen over a binary format: 85 KB is well within "reasonable
size" for a JSON document, it's human-inspectable for audit, and the
project has no existing binary-model-serialization convention to
extend. No raw training data is stored in the artifact — only fitted
parameters (tree structure, calibrator breakpoints, per-feature
statistics) and descriptive metadata.

`CandidateArtifact::validate()` rejects the artifact if: the version
is unsupported, the feature order doesn't match the checksummed
training order exactly, any feature lacks normalization/imputation/type
entries, the isotonic breakpoints/values arrays have mismatched
lengths, or any calibrator/statistics value is non-finite. Loading
never proceeds partway on a broken artifact.

## Checksums (mission §6)

| Checksum | Value (this run) |
|---|---|
| `artifact_checksum` | `868333c5afc0898ff4dc0cb3a4c922eae851fd28ecca1834e666bc40833fcd74` |
| `gbm_checksum` | `be8f10dc9b6e1426ae5b19d2f1688219c06ee46f18660dc8e26e8a89cffee97a` |
| `calibrator_checksum` | `9c0229b10b21c4a2caa74c24f3c3bb68f322f967c8590ad71293fed3454d570a` |
| `transforms_checksum` | `e0c5895b783b64677215ae944245dae248de9e44a61a636875ee605d203acc26` |
| `feature_list_checksum` | `ef89adc1d59d5959c87c46ec86b7479ee66cad401db2376f41b42263ed0399b9` |

`artifact_checksum` excludes `created_at`, `metrics`,
`scientific_interpretation`, and `known_limitations` (non-scientific
timestamp and descriptive-only fields, mission §6). Every name-keyed
field is a `BTreeMap`, never a `HashMap`, so the checksum can never
depend on hash-map iteration order.

**A real bug found and fixed this phase**: the first packaging attempt
failed its own round-trip verification — a trained `poi` feature's
`std_dev` printed as `0.047431670861761484` before a JSON round trip
and `0.04743167086176149` after, a difference at the 16th significant
digit (floating-point summation-order sensitivity, not data
corruption). Checksumming raw f64 bit patterns made the checksum
spuriously fail to survive its own serialize/deserialize cycle. Fixed
by quantizing every float to 13 significant decimal digits before
hashing — far more precision than any statistic here needs, and immune
to this class of noise. A regression test pins the exact values that
exposed the bug.

## Training/inference parity (mission §7)

`score_with_artifact` is a training-independent inference path: it
only calls `GbmModel::predict`, `apply_normalization`, and
`apply_isotonic` — never `fit_gbm`, `fit_train_only_transform`, or
`fit_isotonic` (training-only functions). Verified on the full 2025
test split (4,708 rows): **0 mismatches, maximum absolute difference
0.0** between the training-path score and the reload-and-score path
(tolerance 1e-9).

Tested failure modes (all fail the load/score rather than producing a
silently wrong score): missing feature (imputed if a rule exists, else
`Err`), unknown extra feature (ignored), shuffled feature order
(rejected at `validate()`), incompatible artifact version, corrupted/
truncated JSON, empty byte input, a missing required JSON field, a
non-finite internal statistic, and an extreme/overflowing input value.
`score_with_artifact` never panics — confirmed via a `catch_unwind`
test — it always returns `Result`.

## Score semantics (mission §8)

Named `human_ignition_propensity_v2`, deliberately not
`probability_of_fire`. Documented interpretation, carried in the
artifact itself: *"relative human ignition propensity, calibrated
against the sampled 2024 calibration distribution, not a demonstrated
absolute real-world probability."* A future API surface would need to
distinguish this score from v1's own (differently-scoped) learned
human component, from FWI/physical risk, and from any operational
fusion of the two — none of which are introduced in this phase.

## Known limitations (carried in the artifact)

- All non-calendar features use the current `cell_static` snapshot
  applied uniformly across 2020-2025 training dates
  (`current_snapshot_applied_historically`, phase 3B.5/3B.6).
- Combustible eligibility uses `any(child)`; 339 of 4,708 comparable
  2025 rows (7.2%) would be excluded under a majority/≥50% rule,
  disproportionately positives (268/1,177 positives vs. 71/3,531
  negatives) — phase 3B.8 finding, not independently resolved.
- No faithful comparison exists against v1's full FWI-fused
  `RiskScore`, only against v1's learned human component alone.
- Negative sampling design differs structurally from v1's uniform
  combustible-cell sampling.

See `PHASE3B9_PROMOTION_REVIEW.md` for the offline/online feature
parity finding (a new, real discovery this phase, not carried in the
artifact's own `known_limitations` list since it concerns deployment
timing, not the model itself).
