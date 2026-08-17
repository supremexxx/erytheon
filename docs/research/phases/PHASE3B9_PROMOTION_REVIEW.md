# Phase 3B.9 — Promotion Review

Final P0 packaging review for the phase 3B.7/3B.8 candidate. Detail in
`MODEL_CANDIDATE_ARTIFACT.md` (artifact format, checksums, parity),
`MODEL_PROMOTION_PLAN.md` (serving compatibility, feature availability,
P0-P7 staging, rollback), and `SHADOW_SCORING_DESIGN.md` (design only,
not implemented).

## 1. Git and environment audit

Continued from phase 3B.8's local commits (`2eec181`, `26fbf47`) — no
phase repeated. This phase added:
`crates/engine/src/candidate_artifact.rs` (new),
`crates/engine/Cargo.toml` (+`sha2` direct dependency),
`Cargo.lock` (regenerated for the new dependency edge),
`migrations/0016_model_candidate_registry.sql` + its rollback (proposed,
**not applied** to any database), and a small `crates/engine/src/main.rs`
addition (`PackageCandidateArtifact` CLI command). Four local commits:
`ec5460d` (code), `e5026c7` (checksum fix), `e9d08cf` (resilience tests
+ benchmark), and this documentation commit. Not pushed. No file under
`crates/api`, `crates/risk`, `human_model.rs`, FIRMS, FWI, or the
scheduler was modified. `git diff origin/main..HEAD --stat` shows the
expected cumulative divergence (52 files, consistent with every prior
phase never having pushed).

Environment: a fresh ephemeral build container `erytheon-3b9-build`
(2 CPU / 4 GiB, `rust:1.94-bookworm`), connected to the preserved
isolated deploy database `erytheon-3b3-deploy-20260727T203310Z` (same
container since phase 3B.3, never recreated). Removed after this
phase's work; the deploy database was disconnected from the temporary
network and left running, unchanged, on only its own network. No new
dump, no new PostgreSQL instance.

## 2. Candidate audit (mission §4)

| Item | Value |
|---|---|
| Dataset logical ID | `erytheon_human_ignition_cell_day_v1_candidate_inclusive_n3_adaptive_geographic_quality` |
| Dataset row fingerprint | `bee1bfaa5401144c5cbffe1f42bf45f7` |
| Feature order (real, confirmed) | wui, road, agri, population, poi, power_line, hist, combustible, weekend, public_holiday, season_sine, season_cosine |
| GBM hyperparameters | n_trees=50, max_depth=3, learning_rate=0.1 |
| Trees | 50 |
| Isotonic calibrator | 1,774 breakpoints, fit on calibration 2024 only |
| Seed | 2026071 |
| Code commit (this run) | `e9d08cf` |
| Training / calibration / test periods | 2020-01-01 to 2023-12-31 / 2024-01-01 to 2024-12-31 / 2025-01-01 to 2025-12-31 |
| Test metrics | ROC-AUC 0.9764, AP 0.9308, Brier 0.0460, ECE 0.0096, lift@10% 3.91 |

Nothing here was retrieved implicitly from "the latest version" — the
logical ID, hyperparameters, and seed are hard-coded constants matching
the phase 3B.7/3B.8 frozen candidate exactly, and the dataset row
fingerprint was checked identical before and after this run's dataset
read (no in-place modification).

## 3. Artifact and checksums

See `MODEL_CANDIDATE_ARTIFACT.md` in full. 85,615-byte JSON artifact;
5 checksums (full artifact, GBM, calibrator, transforms, feature list),
all `BTreeMap`-keyed for `HashMap`-order independence, all quantized to
13 significant digits to survive a JSON round trip (a real bug found
and fixed this phase — see that document's dedicated section).

## 4. Training/inference parity

0 mismatches, maximum absolute difference 0.0, across the full 4,708-row
2025 test split, comparing the training-path score against the
independent `score_with_artifact` path after a real serialize/
deserialize round trip. Parity is demonstrated, not assumed.

## 5. Feature availability and offline/online parity

All 12 features have an established production data path — no schema
gap, including for `hist` (the mission's specifically flagged critical
feature). Measured offline/online parity: 95.3-99.9% exact match per
field on the real 2025 population, explained as expected `cell_static`
snapshot drift since the phase 3B.5 dataset build (a periodically-
refreshed static layer, not a semantic incompatibility) — full detail
and the specific reasoning in `MODEL_PROMOTION_PLAN.md` §"Offline/online
parity". This is the one real, quantified risk this phase surfaces; it
does not block P0 but should inform P1 timing (refresh `cell_static`
shortly before any registration, and treat shadow scoring as the true
live re-validation).

## 6. Performance and resilience

Performance: artifact load 238 µs, unit score p50/p95/p99 = 5.04/6.81/
13.6 µs, batch of 4,708 rows in 27.4 ms — far under a proposed 10 ms
p95 shadow-overhead budget (isolated-container measurement, not
production, but no structural concern). Resilience: every tested
failure mode (missing/corrupted artifact, invalid checksum, wrong
version, missing feature, non-finite value, extreme input, empty
input) returns `Err`, never panics, confirmed via `catch_unwind`.

## 7. Serving compatibility and feature vector v2

Design only, not implemented: an enum (`ScoredModel::LogisticV1` /
`GbmIsotonicV2`) is proposed over a `dyn Trait`, since it avoids
forcing `risk::CellFeatures` into a shared generic shape across two
structurally different feature vectors. v1's own code path
(`Store::active_human_model`, `human_model.rs`, `risk::LearnedHumanModel`,
`HeuristicV1::score`) is entirely unmodified. The candidate's feature
vector v2 is a typed, checksummed `Vec<String>`
(`candidate_artifact::feature_order()`), never a `HashMap`-ordered
structure.

## 8. Database registration

`human_model_versions` (migration 0008) is v1-specific: its
`train_from`/`validation_from` columns and `CHECK (train_to <
validation_from)` constraint encode v1's own chronological holdout,
incompatible with the candidate's train/calibration/test split.
Proposed instead: `migrations/0016_model_candidate_registry.sql`, a new
additive `ml.model_candidate_registry` table restricted to `status IN
('candidate', 'inactive')` — deliberately unable to represent
"active", so it can never accidentally supersede v1. **Reviewed and
written, not applied to any database this phase.**

## 9. Shadow scoring

Design only (`SHADOW_SCORING_DESIGN.md`): disabled by default, never on
the request's critical path, never changes the served response, no
scheduler. Proposed storage `ml.model_shadow_scores`, not created —
volume/retention to be assessed before any future migration.

## 10. Rollback

Prepared for every future stage (candidate registered/inactive, shadow
active, candidate eventually activated) — see
`MODEL_PROMOTION_PLAN.md` §"Rollback". No destructive SQL rollback is
ever performed once real data exists.

## 11. Combustibility sensitivity (unchanged)

Carried forward from phase 3B.8 as a formal, still-open risk: 339 of
4,708 comparable rows (7.2%) would be excluded under a majority/≥50%
rule, disproportionately positives. The rule is not changed this phase;
the candidate continues training/scoring against `any(child)`, the
rule it was actually trained on.

## 12. Quality gates

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
--all-features --locked -- -D warnings`, and `cargo test --workspace
--locked` all pass clean in the isolated build container (69 unit tests
in `candidate_artifact::tests`, all existing tests elsewhere
undisturbed).

## 13. Verdict

Checked against the mission's own P0→P1 blocking criteria (§20; full
table in `MODEL_PROMOTION_PLAN.md`): every criterion is met. `hist`
is available in production with the same semantics as training. The
one real risk found (offline/online snapshot drift) is measured,
explained, and does not meet the mission's "critical feature
unavailable" blocking condition — it is a deployment-timing
consideration for P1, not a packaging defect.

```
PHASE 3B.9 PROMOTION REVIEW COMPLETED
P0 MODEL ARTIFACT VALIDATED
READY FOR INACTIVE PRODUCTION REGISTRATION REVIEW
NO PRODUCTION DEPLOYMENT
NO PUSH
```
