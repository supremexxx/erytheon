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

## K. Decision

PHASE 3B.2 READY FOR REVIEW
