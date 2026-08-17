# ERYTHEON — Phase 4B observed backlog

This backlog is derived only from Phase 4A.3 production observations. It is not an
implementation plan and none of these capabilities is included in Phase 4A.3.

Priority meanings: P1 should be considered first, P2 is useful after the operational
baseline is secure, and P3 is optional/usage-dependent.

| ID | Need | Production evidence | User | Scientific value | Complexity | Dependencies | Priority |
|---|---|---|---|---|---|---|---|
| 4B-SRC-01 | Show forecast freshness, last complete batch and a degraded-state warning | Open-Meteo had no complete forecast for ≈16 h while bounded retries continued | operator, scientist | prevents stale forecast data being mistaken for current data | medium | define freshness SLO; expose existing source/batch timestamps | P1 |
| 4B-SRC-02 | Provide privacy-conscious route/status/latency telemetry for the private console | historical Caddy request counts and latency were unavailable | operator | makes reliability conclusions evidence-based | medium | access-log privacy policy, retention and aggregation | P1 |
| 4B-QUAL-01 | Add temporal and cause filters to ignition-event exploration | the event API already pages by cause, but production contains 15,956 events across strongly imbalanced categories | scientist | supports reproducible inspection of subsets and unknown causes | medium | filter contract, indexed query review | P2 |
| 4B-DATA-01 | Explain dataset variants and compare strict/inclusive N2/N3 when the registry is populated | registry was empty, so the four requested variants could not be compared | model validator | makes sampling and exclusion choices reviewable | high | production dataset registry/builds must exist; stable definitions | P2 |
| 4B-DATA-02 | Explore dataset exclusions by reason and split | detail API already aggregates exclusions, but there is no production dataset to exercise the flow | model validator | exposes selection bias and data loss | medium | 4B-DATA-01 and populated registry | P2 |
| 4B-MOD-01 | Add ROC, precision-recall and calibration visual comparisons | model page exposes scalar metrics only; candidate has 50 trees and 1,774 calibration points | model validator | improves validation of ranking and calibration beyond headline scores | high | approved plotting approach, metric provenance, accessibility | P2 |
| 4B-FEAT-01 | Add a feature snapshot catalogue/history view with missingness and vintage | production snapshot registry currently contains zero rows | scientist, operator | makes temporal leakage and feature provenance auditable | high | snapshot production pipeline and registry populated | P2 |
| 4B-GEO-01 | Add H3/geographic exploration of BDIFF quality | 920,016 static cells and geographic-quality aggregates exist, but the console has no spatial exploration | scientist | reveals territorial concentration and quality artefacts | high | map privacy/performance review; H3 aggregation contract | P3 |
| 4B-ACC-01 | Improve compact-width navigation discoverability | mobile navigation is horizontally scrollable and usable, but not all destinations are simultaneously visible | all console users | accessibility/usability rather than new science | low | usage feedback from mobile sessions | P3 |
| 4B-PERF-01 | Return model metadata separately from the full candidate artifact | `/api/science/models` was 88,950 B versus sub-15 KiB for other endpoints, although p95 was only 28.9 ms | operator, model validator | none directly; improves transfer and limits accidental artifact exposure | medium | API compatibility and artifact-detail access design | P3 |
| 4B-EXP-01 | Offer explicitly scoped exports of filtered tables | current validation requires manual API/SQL comparison | scientist, reviewer | improves reproducibility of reviewed subsets | medium | authorization, row limits, audit trail, CSV escaping | P3 |

## Ordering recommendation

First resolve the operational prerequisites behind `4B-SRC-01` and `4B-SRC-02`.
Then prioritize quality/dataset/model validation items according to which registries
are populated in production. H3 mapping, exports and payload reshaping should follow
demonstrated usage rather than being added by default.

P3 shadow scoring is not part of this backlog and must not begin merely because a
Phase 4B item is selected.
