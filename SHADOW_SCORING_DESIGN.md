# Phase 3B.9 — Shadow Scoring Design (not implemented, not activated)

A design only. No code in this phase implements or activates shadow
scoring; nothing here changes serving behavior, the API, or v1's
scored responses.

## Goal

Let the candidate score every request v1 already scores, without ever
being the response the caller sees, so its live behavior can be
observed before any activation decision.

## Shape

```
v1 (active)         -> scores the request -> served to the caller (unchanged)
candidate (inactive) -> scores the same request -> recorded, never served
```

Guardrails (all required before this is ever built):

- **Disabled by default.** A feature flag or explicit config value
  (e.g. `SHADOW_SCORING_ENABLED=false` by default), not a code branch
  that "just runs" once the candidate is registered.
- **Never blocks or slows the real response.** The candidate's score
  is computed after v1's response is already determined, ideally
  off the request's critical path (e.g. spawned as a background task
  that the request handler does not await), and any error scoring the
  candidate is caught and logged, never propagated to the caller.
- **Never changes what's returned.** The API response shape, its
  score, its `top_factors`, and its status code are v1's alone,
  unconditionally, whether shadow scoring is on or off, and whether
  the candidate succeeds or fails.
- **No scheduler.** Shadow scoring only runs synchronously inside an
  already-happening scoring request; it does not add a new recurring
  job. A one-off manual command (mission §14: "commande manuelle") is
  the only way to backfill or batch-run it, if ever needed.

## Where it would attach

`crates/risk/src/lib.rs`'s `IgnitionModel::score` / `HeuristicV1::score`
is the natural site: after v1 computes its `RiskScore`, an optional
second call — behind the flag — would score the candidate on the same
`CellFeatures` via `CandidateArtifact`/`score_with_artifact`
(`crates/engine/src/candidate_artifact.rs`) and write the result
somewhere the caller never sees (§ below). This is a design decision to
record now, not code to add now — no such call exists in
`HeuristicV1::score` today, and this phase does not add one.

## Storage (mission §15)

Proposed additive table `ml.model_shadow_scores`, only if/when P3+ is
authorized:

```sql
CREATE TABLE ml.model_shadow_scores (
    id BIGSERIAL PRIMARY KEY,
    scored_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    h3 BIGINT NOT NULL,
    date DATE NOT NULL,
    active_model_id BIGINT NOT NULL,      -- human_model_versions.id
    candidate_model_family TEXT NOT NULL,  -- e.g. "gbm_isotonic_v2"
    active_score DOUBLE PRECISION NOT NULL,
    candidate_score DOUBLE PRECISION,      -- NULL on candidate failure
    score_diff DOUBLE PRECISION,           -- NULL on candidate failure
    candidate_error TEXT,                  -- populated only on failure
    feature_checksum TEXT NOT NULL,
    latency_micros BIGINT,
    pipeline_run TEXT,
    git_commit TEXT NOT NULL
);
CREATE INDEX model_shadow_scores_scored_at_idx ON ml.model_shadow_scores (scored_at);
```

Before creating this migration for real: measure expected volume (rows
per day at current request rate), decide a retention window (e.g. 90
days, dropped via a scheduled cleanup — itself a future, separately
authorized job, not this phase's scheduler), confirm no personal data
is stored (H3 cell + date + scores only, no user/request identifiers),
and confirm the migration is additive and reversible while the table
is empty (matching this repo's existing `down.sql` convention: refuse
to roll back once real rows exist).

**Not created in this phase.** Only a design; the eventual migration
belongs to whichever phase actually authorizes P3.

## Failure handling

If the candidate scoring fails (missing artifact, corrupted checksum,
feature unavailable, panic-free `Err` from `score_with_artifact`), the
shadow write records `candidate_error` and a `NULL` `candidate_score`
— never retried inline, never surfaced to the caller, never causing
the real (v1) response to change or fail.

## Rollback

Turning shadow scoring off is a single config/flag flip — no data
migration, no model change, no v1 impact. Already-recorded shadow rows
are kept (they're read-only observational data) unless a separate,
explicit cleanup is authorized.
