# Phase 3B.8 — Promotion Gap Report

Closes the single gap left open by phase 3B.7: whether the GBM+isotonic
candidate is genuinely superior to the active v1 model, on a shared,
fairly defined 2025 population. Detail in
[V1_CANDIDATE_COMPARISON.md](V1_CANDIDATE_COMPARISON.md); this document
covers the git/environment audit, the pre-registered promotion
criterion's precise interpretation, and the final decision.

## Git and environment audit

Continued directly from phase 3B.7's local commits (`ff84fbd`,
`24543db`) — no phase repeated. This phase's code:
`crates/engine/src/v1_candidate_comparison.rs` (new), plus small
visibility changes in `crates/engine/src/model_experiments.rs` to
allow reuse of its metric/training primitives (`pub(crate)`, no
behavior change) and `crates/engine/src/main.rs` (new CLI command).
Committed locally as `2eec181` (`feat: compare candidate model with
active v1`), not pushed. No changes to `crates/api`, serving, FIRMS,
FWI, the scheduler, or `human_model_versions` (read-only access via
the existing `Store::active_human_model`).

Environment: a fresh ephemeral build container `erytheon-3b8-build`
(2 CPU / 4 GiB, `rust:1.94-bookworm`, matching phase 3B.7's toolchain
version) on a new network `erytheon-3b8-net`, connected to the
preserved isolated deploy database `erytheon-3b3-deploy-20260727T203310Z`
(same container across phases 3B.3-3B.8, never recreated). Source
transferred via the same explicit include-list `tar`, SHA-256 verified
on both ends. No new PostgreSQL instance, no new dump, no production
volume touched. Both the ephemeral container and its network were
removed after this phase's work, and the deploy database was
disconnected from the temporary network and left exactly as it was
found, still running on only its own network.

## Precise interpretation of the promotion criterion

The pre-registered criterion from phase 3B.7 was
`min_average_precision_gain_over_v1 >= 0`. This phase operationalizes
it as: **AP_candidate − AP_v1, computed on the exact same rows, with a
95% paired block-bootstrap confidence interval that must not include
or fall below zero** for the gain to count as demonstrated (not merely
"the point estimate happens to be positive"). Distinguishing the four
possible outcomes:

| Outcome | This run |
|---|---|
| Gain clearly positive (CI entirely above 0) | **Yes — CI [0.3157, 0.3852]** |
| Gain ponctuel positif but CI straddles 0 | No |
| Gain compatible with zero | No |
| Degradation (CI entirely below 0) | No |

The measured gain (+0.347 AP, +0.204 ROC-AUC) is large and the CI is
nowhere near zero — this is not a marginal or ambiguous result.

## Checking the other four criteria are undisturbed

Phase 3B.7 already established (and this phase does not re-derive or
re-optimize) that the candidate meets:

| Criterion | Status |
|---|---|
| `min_roc_auc >= 0.60` | Met (0.9764) |
| `max_brier_score <= 0.20` | Met (0.0460) |
| `max_ece <= 0.10` | Met (0.0096) |
| `min_lift_at_10pct >= 1.5` | Met (3.91) |
| `min_average_precision_gain_over_v1 >= 0` | **Now met** (§ above) |

No hyperparameter was re-chosen after seeing this phase's results — the
candidate is the exact frozen `(n_trees=50, max_depth=3,
learning_rate=0.1)` GBM with isotonic calibration from phase 3B.7,
confirmed byte-for-byte reproducible in metrics (§4 of
`V1_CANDIDATE_COMPARISON.md`).

## Comparison fidelity check

Population fidelity: 100% of the candidate test population (4,708/4,708
rows) is directly scoreable by the real v1 artifact — no exclusion, no
comparability bias to control for. 10 of 11 v1 features reconstruct
exactly from the candidate dataset; the 11th (`school_holiday`) is
bridged using v1's own existing production convention
(`COALESCE(_, FALSE)`), not a new approximation invented for this
comparison. v1 was scored using its real, active, unmodified artifact
(`human_model_versions.id=1`) — Approach A (mission §6), not a
documentary feature bridge. No retraining of v1 occurred at any point.

## Sensitivity checks that could have changed the conclusion

- **Combustibility rule**: majority/≥50%/≥75% would exclude 7.2% of
  the comparable population (disproportionately positives, a real
  finding worth its own follow-up) but leaves the remaining 92.8%
  untouched — not large enough to threaten the AP-gain conclusion.
- **Strict N2**: run with the frozen hyperparameters only; ROC-AUC
  0.9479, AP 0.7497 — consistent with the same dataset-family pattern
  seen in phase 3B.7 (principal, strict N3), not an outlier that would
  reverse the conclusion.
- **Reproducibility**: the candidate's frozen training/calibration
  replayed to the documented tolerance; no drift detected.

None of these sensitivity checks surface a reason to distrust the
paired comparison.

## Final decision

All five conditions from the mission's decision protocol are satisfied:

1. The v1 comparison is faithful (real artifact, 100% comparable
   population, one well-justified single-feature bridge).
2. The candidate does not degrade AP — it improves it substantially.
3. The other four promotion criteria remain satisfied (established in
   phase 3B.7, undisturbed here).
4. No major comparability bias was found (§ above).
5. The result is stable at bootstrap (CI far from zero) and under the
   combustibility/strict-N2 sensitivity checks.
6. No reproducibility defect was found.

```
PHASE 3B.8 V1 COMPARISON COMPLETED
MODEL CANDIDATE READY FOR PROMOTION REVIEW
NO PRODUCTION DEPLOYMENT
NO PUSH
```

This is a recommendation to open a promotion review, not a promotion
itself — no model was deployed, no serving code was touched, no new
operational score was published, and no push occurred.
