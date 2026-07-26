# ERYTHEON — Phase 3B.2 quality report

## A. Initial audit

- Base commit: `1d30285a05abdb0a478767a66c999a70c2f5900a`, branch `main`, clean before work.
- Existing migrations: `0001` through `0011`; `0012` was available.
- H3 production resolution: 8.
- `fire.ignition_events`: 15,956 immutable events from 2020 through 2025.
- Causes: 7,094 human, 791 natural, 8,071 unknown, 0 indeterminate.
- Distinct coordinates/H3: 5,997; municipalities: 5,901.
- Repeated coordinate groups: 2,607; events: 12,566.
- Municipalities with multiple events and one coordinate: 2,528, covering 12,282 events.
- Same-day/H3 groups: 420, covering 918 events, maximum size 8.
- `cell_static`: 920,016 H3 rows with combustible, road, population, POI, WUI,
  agriculture, power-line, history and school-zone features.
- Available administrative data is limited to department boundary files used
  by territory planning. No versioned commune polygons, official centroids or
  INSEE code mapping is present in the database.

The previous audit values of 12,562 repeated events and 418 day/H3 groups
differ because the current production query returns 12,566 and 420. No source
event was changed to force agreement.

## B. Architecture

Migration `0012_bdiff_quality_foundation.sql` creates schema `validation` and:

- `rule_versions`
- `coordinate_groups`
- `event_label_quality`
- `event_geographic_quality`
- `event_combustibility_assessments`
- `combustible_cell_candidates`
- `duplicate_candidate_pairs`
- `duplicate_candidate_groups`
- `duplicate_candidate_members`

All objects are additive. Foreign keys use `ON DELETE RESTRICT`. Results are
unique by event/rule or group/rule. Scores are bounded and categories checked.
The full result bundle is persisted in one transaction using grouped JSON
recordsets rather than row-by-row round trips.

## C. Label quality

- Human high confidence: 5,771.
- Human medium confidence (`Accidentelle`): 1,323.
- Natural high confidence: 791.
- Unknown confidence: 8,071.
- Accident sensitivity flags: 1,323.
- Unknown values remain unknown and natural values remain observed fires.

## D. Geography

- Probable-centroid groups: 703, covering 7,539 events.
- Possible-centroid groups: 845, covering 2,892 events.
- Not-centroid-like groups: 3,006.
- Undetermined groups: 1,443, covering 2,519 events.
- Rounded-coordinate assessments: 997 events.
- Confirmed centroids: 0 because no official versioned reference exists.

## E. Combustibility

Human cohort:

- combustible original cell: 6,422;
- non-combustible original cell: 650;
- missing `cell_static`: 22;
- difficult total: 672.

Nearest combustible proposal for difficult human cases:

- ring 1: 495;
- ring 2: 86;
- none through ring 2: 91.

The difficult cohort is strongly associated with probable municipal
centroids: 486 of 672 events. Among rows with features, difficult cases have
higher average road, population and POI signals and zero combustible/WUI
signal. This supports several plausible explanations—urban centroids,
location imprecision or feature coverage—but does not prove one cause.

## F. Potential duplicates

- Candidate pairs scored: 633.
- Certain: 78.
- Probable: 37.
- Possible: 424.
- Indeterminate: 93.
- Probably distinct: 1.
- Review groups retained: 632, each with exactly two direct-evidence members.
- Difficult human events appearing in a candidate group: 40.

No transitive clustering is performed, preventing a weak `A-B-C` chain from
becoming an automatic merged group.

## G. Tests

- Quality unit tests: 11 passed.
- SQLx quality integration test: passed.
- Empty rollback: passed.
- Rollback with validation data: refused as required.
- Reapplication after empty rollback: passed.
- Real-copy dry-run: 15,956 events, 0 errors, 5.3 seconds.
- First grouped persistence: 19.0 seconds.
- Idempotent replay: 10.7 seconds with identical logical checksum.
- Logical checksum:
  `fef4adac90bacfee387ddd1da1faabc748e226216c2e79d8fea4f9462c7338de`.
- Workspace compilation: passed.
- The global legacy API test remains outdated because it expects the old
  `PyroRisk` dashboard title instead of `ERYTHEON`; this is unrelated and was
  not modified.

## H. Modifications

- Migration and protected rollback `0012`.
- New `quality` crate for pure deterministic rules.
- New store quality persistence module.
- New engine quality pipeline and manual CLI command.
- Deterministic 18-scenario fixture.
- SQLx integration test.
- Operational documentation and this report.

## I. Risks

- No official municipal centroid reference is available.
- Pair thresholds remain scientific v1 assumptions requiring expert review.
- Exact duplicates may be overestimated when timestamps and municipal
  coordinates are imprecise.
- Candidates beyond H3 ring 2 are intentionally not searched.
- Reading 107,080 relevant static cells takes about one second.
- Full first persistence is acceptable at the current volume but should remain manual.

## J. Future production proposal

No production deployment was performed. A future controlled deployment must
backup production, verify the exact commit, apply `0012`, run dry-run first,
compare the checksum and all cohort counts, then separately authorize persisted
validation results. No quality scheduler should be created.

## K. Decision (original)

PHASE 3B.2 READY FOR REVIEW

## L. Review correction (post-review, same commit lineage, not yet deployed)

A dedicated scientific review of the `78` pairs originally classified
`certain_duplicate` found that `assess_duplicate` never checks source
identity — which is expected, since `fire.ignition_events` already
deduplicates on `(source_id, source_record_id)` during phase 3B.1 ingestion,
so no two distinct rows here can ever share one. The concern is that the
weighted score is clamped to `[0, 1]`: several different subsets of signals
(for example municipality + distance + time + cause, without a matching H3
cell or a close surface) all saturate to the same score as a pair with full
convergent evidence, so `certain_duplicate` could be reached from partial,
circumstantial evidence alone.

**Correction applied** (`crates/quality/src/lib.rs`, `assess_duplicate`):
`certain_duplicate` now requires `score >= 0.92` **and** simultaneous
convergence of every strong signal — same municipality, same H3, same
cause, distance `<= 25m`, time `<= 30 minutes`, surface relative difference
`<= 5%`, and no centroid ambiguity. A pair missing any one of these signals
is demoted to at most `probable_duplicate`, per the instruction to retreat
to a weaker category rather than overclaim certainty. No event was changed,
merged or deleted; only the classification boundary moved.

- `raw_signals` now also records `full_evidence_convergence` for
  auditability.
- Rule `erytheon_duplicate_rules_v1` parameters were updated in code to
  document `certain_requires_full_evidence_convergence` and the exact
  signal list; since no production instance has ever persisted this rule
  version (phase 3B.2 has not been deployed), this is a pre-deployment
  correction, not a silent change of an active, deployed rule version —
  `ensure_quality_rule` would reject any attempt to change an already
  persisted rule's checksum under the same `logical_id`.
- Added `crates/quality/src/lib.rs::saturated_score_without_full_convergence_is_not_certain`,
  a unit test proving a pair that saturates the clamped score to `1.0`
  while missing only the surface-similarity signal is now
  `probable_duplicate`, not `certain_duplicate`.

**Known limitation of this review**: this session has no reachable
PostgreSQL/PostGIS instance (no local service on the configured
`DATABASE_URL` port, no running Docker daemon) and no access to the
isolated copy of production BDIFF data used to produce the original
`78`/`37`/`424`/`93`/`1` counts in section F. The corrected classification
boundary was verified with deterministic unit tests only. **The `78`
certain / `37` probable counts in section F are therefore stale** — they
reflect the pre-correction rule. Before any deployment, `audit-bdiff-quality
--dry-run --rules-version v1` must be re-run against the same isolated
copy to obtain the corrected counts, which are expected to show fewer
`certain_duplicate` and more `probable_duplicate` pairs, with the total
candidate pair count (`633`) unchanged since candidate generation itself
was not modified.

## M. Decision (this review)

See final report delivered in chat for the authoritative status. As of this
correction: unit-level scientific rules pass and the identified issue is
fixed at the code level; a real-data re-run to confirm updated counts is a
prerequisite for deployment sign-off and is not yet done.
