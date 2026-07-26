# BDIFF quality audit

Phase 3B.2 adds versioned, non-destructive assessments over
`fire.ignition_events`. It does not modify source events, the active model,
the API, the interface, FWI, FIRMS, or `public`.

## Command

```text
pyrorisk audit-bdiff-quality --dry-run --rules-version v1 --recalculate
pyrorisk audit-bdiff-quality --rules-version v1 --recalculate
pyrorisk audit-bdiff-quality --dry-run --rules-version v1 --year 2025
pyrorisk audit-bdiff-quality --dry-run --rules-version v1 --source-record-id EVENT_ID
```

The command is manual and is not registered in the scheduler.

## Rule versions

- `erytheon_taxonomy_v1`
- `erytheon_label_quality_v1`
- `erytheon_geographic_quality_v1`
- `erytheon_combustibility_assessment_v1`
- `erytheon_duplicate_rules_v1`

Each rule stores parameters, code version, status and a SHA-256 checksum.
Reusing a logical identifier with different content is rejected.

## Label quality

- Known human causes remain proposed human positives.
- `Accidentelle` remains human with medium confidence and a required
  sensitivity-analysis flag.
- Natural events remain fires in a separate natural cohort.
- Unknown causes remain in the unknown cohort and are never negatives.
- Unmapped labels remain indeterminate.

## Geographic quality

Without an official, versioned municipal-centroid reference, no event can be
classified as `municipality_centroid_confirmed`. Repetition across at least
five events and two years in one municipality produces only
`municipality_centroid_probable`.

Original coordinates, geometry, H3 and H3 resolution are copied into the
assessment for traceability. They are never updated.

## Combustibility

The original `cell_static` feature document is recorded when available.
For non-combustible or missing cells only, H3 rings 1 and 2 are inspected.
At most five combustible candidates are retained, ranked by ring and
geodesic distance between H3 centres. These candidates are proposals only.

## Potential duplicates

Candidate generation is bounded by day, H3 proximity and municipality/time
signals. Each direct pair receives explicit raw signals and weighted
contributions. Day and H3 alone cannot prove a duplicate.

Groups contain only direct pairwise evidence. Weak transitive chains such as
`A-B` and `B-C` never automatically merge `A`, `B` and `C`.

Thresholds for v1:

- certain: `0.92`
- probable: `0.75`
- possible: `0.55`

Because `fire.ignition_events` already deduplicates on `(source_id,
source_record_id)` during phase 3B.1 ingestion, no two rows reaching this
audit can ever share a source identity. `certain_duplicate` therefore cannot
rest on a weighted score threshold alone: the score is clamped to `[0, 1]`,
so several different subsets of signals (for example municipality, distance
and time, without a matching H3 cell or a similar surface) can all saturate
to the same value as a pair with full evidence. `certain_duplicate` requires
both `score >= 0.92` and simultaneous convergence of every strong signal:
same municipality, same H3, same cause, distance `<= 25m`, time
`<= 30 minutes`, and surface relative difference `<= 5%`, with no centroid
ambiguity. Any pair missing one of these signals is at most
`probable_duplicate`. This still expresses circumstantial certainty about a
duplicate *report*, not a proven identity match — no identity match is
structurally possible at this stage.

No event is merged, deleted or deactivated.

## Rollback

The rollback is allowed only while every validation table is empty. Once a
rule, assessment, pair, group or decision exists, the rollback raises an
error. After real results exist, retain the schema and data, stop the manual
command and correct behavior with a new migration and rule version.

## Production procedure

Production deployment is not part of phase 3B.2 development. A future
authorized deployment must create a fresh backup, verify it, revalidate the
exact commit, apply migration `0012`, run a dry-run, compare checksums and
counts, then explicitly authorize the first persisted audit.
