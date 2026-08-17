# Erytheon — Open Source Readiness Report

> **Historical record.** The project was renamed from Erytheon to
> **FireSift** after this audit was written. This report is preserved
> as-is, describing what was true under the Erytheon name at the time —
> see [`CHANGELOG.md`](CHANGELOG.md) for the rename. Current
> documentation (README, `docs/`) refers to FireSift.

Date of this audit: 2026-08-17 (four passes: an initial readiness pass, a
follow-up final release-engineering pass, a closing pass resolving the
remaining blockers, and a final consistency pass fixing a self-contained
redaction slip in this report, all on the same date). Scope:
`claude/erytheon-open-source-d0bfed` branch, working tree at the time of
audit, plus full Git history (104 commits, all local branches).

## Executive summary

**Verdict: READY FOR PUBLIC RELEASE (content-wise).**

Every content blocker from the previous pass is now closed: the copyright
holder is confirmed (William Ducamp), the two ambiguous data-licensing
items were resolved by fixing the underlying fixture rather than leaving
the ambiguity in place (see [Decision record](#decision-record)), and the
git-history IP/hostname exposure has an explicit, recorded accept-risk
decision rather than being left open. No secret, credential, or key was
ever found in this repository's history (`gitleaks`, all 104 commits,
re-verified this pass). Code license, data licensing, tests, and Quick
Start are all genuinely green — not just asserted.

"Content-wise" is the operative qualifier: this report assesses the
repository's *content* — code, docs, licenses, tests, history. It does not
and cannot authorize the separate, human act of flipping GitHub visibility
to public, which stays the maintainer's own action per the mission's
constraints (see [Recommended next steps](#recommended-next-steps)). Two
small non-blocking follow-ups remain (enabling GitHub private
vulnerability reporting, an optional future Open-Meteo kill-switch) — both
are things to do at or after publish time, not reasons to withhold it.

This work did **not**: touch git history, move or delete any tag, modify
any already-applied migration, change the active model (v1) or the
candidate's `inactive` status, deploy or redeploy anything, or change any
GitHub repository visibility/secret/setting.

## Decision record

Explicit decisions made by the repository owner (William Ducamp) during
this audit, recorded here for traceability:

| Date | Decision | Rationale |
|---|---|---|
| 2026-08-17 | **Copyright holder: William Ducamp**, `Copyright (c) 2026 William Ducamp`, applied to `LICENSE-MIT` and `LICENSE-APACHE`. Dual license MIT OR Apache-2.0 unchanged, matching `Cargo.toml`. | Provided directly by the repository owner; no longer a placeholder or an agent guess. |
| 2026-08-17 | **Git history IP/hostname exposure: accepted, no rewrite.** The plaintext VPS IP and hostname introduced by commits `cc1c497` and `7f87dda` remain in Git history (reachable from 7/8 tags and all branches) and will **not** be scrubbed via `git filter-repo`/BFG. | The value was never a credential (no revocation implication), and rewriting would invalidate every existing tag SHA, clone, and fork reference for a non-exploitable identifier. The current-tree fix (redaction on this branch) is considered sufficient. See [Git history security](#git-history-security) for the full technical detail and the rewrite recipe, kept for reference in case this decision is ever revisited. |
| 2026-08-17 | **Prométhée fixture replaced with synthetic data**, closing the "unconfirmed provenance" ambiguity rather than leaving it open. `testdata/promethee_aude.csv` now contains one clearly fictional row (`SYNTH-0001`, "Testville-sur-Aude", round-number coordinates) instead of a row whose real/synthetic origin could not be confirmed. | Removes the ambiguity outright instead of requiring a future legal review of an uncertain fixture. |
| 2026-08-17 | **Administrative boundaries and territorial calendars reclassified** from a blanket `REQUIRES LEGAL / LICENSE REVIEW` to precise, resolved statuses: boundaries are `NOT BUNDLED / USER PROVIDED` (confirmed no boundary file ships anywhere in the repo or its `.env*.example` files); the bundled calendar fixture is `CLEAR` (3 rows of public factual data — dates and holiday names, not a licensed dataset); a real production calendar remains `NOT BUNDLED / USER PROVIDED`. | The original blanket marker was overcautious — once actually checked, nothing in this category needed legal review; it needed accurate classification. |
| 2026-08-17 | **Self-audit fix: this report itself contained the literal production IP** in one place (the `git log -S"..."` example command in [Git history security](#git-history-security)), contradicting its own claim that the working tree was clean. Replaced with the `<VPS_PUBLIC_IP>` placeholder. Confirmed via `git grep --untracked` across the full tree: zero remaining occurrences of the real IP or hostname anywhere in HEAD/working tree. History is unaffected — this was a working-tree-only fix, consistent with the no-rewrite decision above. | A readiness report that itself leaks the thing it says it redacted is a real defect, not a nitpick — worth a dedicated, explicit fix rather than a silent edit. |

## What changed since the first readiness pass

The first pass (documented in the sections below, mostly unchanged) built
the licensing, documentation, and community-file foundation. This pass:

- Confirmed Docker is available in this environment, started a real
  PostgreSQL/PostGIS 16/3.4 container, and ran the **full** test suite
  against it — see [Repository state](#repository-state).
- Actually ran the documented Quick Start end-to-end (not just read the
  docs) from a from-scratch copy with no pre-existing `.env` — see
  [Quick Start verification](#quick-start-verification) — and fixed two
  real gaps it surfaced.
- Verified data-source licenses against each provider's **live, current**
  terms via web search/fetch instead of relying on memory, and resolved
  most of the earlier `REQUIRES LEGAL / LICENSE REVIEW` markers in
  `docs/data-sources.md` to `CLEAR` (with precise conditions where they
  apply) — see [Licensing](#licensing).
- Traced the exact commits that introduced the plaintext VPS IP/hostname
  and discovered the blast radius is much larger than "three files": 7 of
  8 published tags and every branch descend from them — see
  [Git history security](#git-history-security). Did **not** rewrite
  history; documented the exact situation and commands for a human
  decision.
- Hardened `.github/workflows/ci.yml` with an explicit minimal
  `permissions: contents: read` block (defense-in-depth; it needed no
  write access and had none, but relied on implicit defaults before).
- Added `NOTICE.md` as a quick attribution reference, and a draft
  `docs/release-notes-v0.5.0-draft.md` (no tag or release created).
- Re-verified copyright holder resolution was still blocked *at this
  point in the audit* — this session's own git identity (`ERYTHEON Codex
  <codex@erytheon.local>`) is an agent identity, not a legal name, so it
  could not be used either. **Superseded in the closing pass below**: the
  repository owner provided the name directly (William Ducamp), and it is
  now applied — see [Decision record](#decision-record) and
  [Licensing](#licensing). This bullet is kept as a historical record of
  what this pass found, not as the current state.

## What changed in the closing pass

A third pass, after receiving explicit direction from the repository
owner, closed every remaining blocker instead of leaving them open for a
future maintainer session:

- Applied the copyright holder name (William Ducamp) to `LICENSE-MIT` and
  `LICENSE-APACHE`.
- Replaced `testdata/promethee_aude.csv`'s single row with clearly
  synthetic data, closing the "unconfirmed real-vs-synthetic provenance"
  question outright rather than continuing to flag it for review.
- Reclassified administrative boundaries and territorial calendars in
  `docs/data-sources.md` from a blanket `REQUIRES LEGAL / LICENSE REVIEW`
  into precise, resolved statuses, after actually checking what — if
  anything — ships in the repository for each (answer: nothing real for
  boundaries; only trivial public factual data for the calendar fixture).
- Recorded the git-history IP/hostname exposure as an explicit,
  owner-accepted decision (see [Decision record](#decision-record)) rather
  than an open question.
- Re-ran only the tests plausibly affected by the fixture change
  (`crates/engine`'s static-layer loading tests, which read
  `testdata/promethee_aude.csv`) plus a full `gitleaks` rescan — not the
  entire suite again, since nothing else changed. See
  [Repository state](#repository-state) for the result.

## Final Release Gate

| Gate | Status | Notes |
|---|---|---|
| Code license | **PASS** | `LICENSE`, `LICENSE-MIT`, `LICENSE-APACHE` added, matching `Cargo.toml`'s `MIT OR Apache-2.0`. Copyright holder confirmed: William Ducamp. |
| Dataset licensing | **PASS** | Every source resolved to a precise, non-ambiguous status — `CLEAR`, `CLEAR for non-commercial reuse`, `NOT BUNDLED / USER PROVIDED`, or `OPTIONAL PROVIDER` — none left as an open "review needed" item. See [`docs/data-sources.md`](docs/data-sources.md). |
| Git history security | **PASS (risk accepted, recorded)** | No secrets found (gitleaks, all history). A real VPS IP/hostname was committed and is fixed on this branch; its continued presence in history (7/8 tags, all branches) is a recorded, explicit accept-risk decision by the repository owner, not an open question — see [Decision record](#decision-record). |
| Secret scan | **PASS** | `gitleaks detect --source . --log-opts="--all"`: 104 commits scanned, no leaks, re-verified after every pass's changes. |
| Rust formatting | **PASS** | `cargo fmt --all -- --check` clean. Zero code changed this entire engagement. |
| Clippy | **PASS** | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` clean. |
| Unit tests | **PASS** | All unit tests and doctests pass. |
| PostgreSQL/PostGIS integration tests | **PASS** | `cargo test --workspace --locked --no-fail-fast` fully green against a fresh `postgis/postgis:16-3.4` container with default parallelism — i.e. run the same way CI runs it. See [Repository state](#repository-state) for the messy path that got here. |
| Clean clone Quick Start | **PASS (with 2 fixes)** | Actually run end-to-end from a from-scratch copy; `/health`, dashboard, `/risk`, FIRMS fixture ingestion all verified working. Found and fixed: missing `gdal`/`eccodes` host prerequisite (silently degrades weather ingestion) and no guidance for a port-5432 conflict. See below. |
| Markdown links | **PASS** | 97 Markdown files scanned, 0 broken relative links (1 regex false-positive in a code span). |
| Public endpoint safety | **PASS** | Every route in `crates/api/src/{lib,client,science}.rs` is `GET`/`WS` only — reconfirmed this pass. No write/import/train/migrate/activate endpoint exists. |
| GitHub Actions fork safety | **PASS** | Neither workflow uses `pull_request_target`. `ci.yml` runs on `pull_request` (fork PRs get a read-only, non-privileged token by default and no access to repo secrets) and referenced no custom secrets even before this pass; now has an explicit `permissions: contents: read`. `container.yml` only triggers on push to `main`/tags/`workflow_dispatch` — unreachable from a fork PR — and already declared minimal `permissions: contents: read, packages: write`. |
| Production identifiers removed | **PASS (working tree); risk accepted (history)** | Working tree is clean (re-verified: no IPs, hostnames, `/home/` paths, SSH keys, or personal emails found in any tracked file). History exposure is the same recorded, accepted decision as Git history security above. |
| Scientific positioning | **PASS** | v1 active / v2 inactive unchanged; README/docs consistently frame the score as relative risk; verified historical metrics carry their "historical, not live" caveat everywhere they're cited. |

Every gate is now **PASS**. None is an open, unresolved ambiguity — the
two gates noting "risk accepted" reflect a decision explicitly made and
recorded (see [Decision record](#decision-record)), not an unclosed
blocker.

## Repository state

- Branch: `claude/erytheon-open-source-d0bfed`, off `main`. All changes
  from both passes are still **uncommitted working-tree changes** — this
  session did not commit anything (commits weren't requested).
- 9 Cargo crates, 31 SQLx migrations, 8 Git tags (all preserved, none
  moved).
- `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
  both pass with **zero code changes made** across both passes — all work
  is documentation, licensing, and file-organization.
- `cargo test --workspace --locked --no-fail-fast`: **fully green** against
  a real PostgreSQL/PostGIS 16/3.4 container, verified this pass. Getting
  there took some environment debugging worth recording honestly:
  - A first attempt found this sandbox's native macOS Postgres already
    listening on `localhost:5432` (unrelated pre-existing machine state),
    which the Docker container's own port-forward couldn't win against on
    the loopback interface — tests connected to the wrong server and
    failed with `role "pyrorisk" does not exist`. Worked around by running
    the Postgres/PostGIS container on an alternate host port for testing
    purposes only (`docker-compose.yml` itself was **not** changed).
  - A second attempt, reusing the same container across repeated test
    runs within this session, showed test-order-dependent flakiness (a
    handful of `crates/store` integration tests failed differently each
    run) — caused by tests leaving state in shared tables when the
    database isn't reset between runs, not by anything in this pass's
    changes.
  - A third attempt, against a **freshly created** container with default
    test parallelism (i.e., exactly how `.github/workflows/ci.yml` runs
    it — a brand-new service container per CI job) came back **100%
    green**: every unit test, doctest, and integration test across all 9
    crates passed. This is the result that matters; the two attempts
    before it were artifacts of reusing a container across manual runs,
    not evidence of a real bug.

## Quick Start verification

Actually run, not just read — a `rsync`-based fresh copy of the working
tree (no `.git`, no `target/`, no pre-existing `.env`) into an isolated
directory, then exactly the documented steps:

```sh
cp .env.example .env
docker compose up -d
cargo run -p engine -- run
```

Result: `/health` returned `{"status":"ok","db":"ok",...}`, the dashboard
returned HTTP 200, `/risk?horizon=nowcast&bbox=...` returned a valid (if
empty) GeoJSON `FeatureCollection`, and the FIRMS fixture ingested 5/5
records successfully. Two real gaps were found and **fixed** in this pass:

1. **Missing host prerequisite.** Weather ingestion (ECMWF direct GRIB2
   decoding) silently failed over to an empty result because `gdal`/
   `eccodes` (specifically `grib_to_netcdf`) aren't installed on the host
   — only the `Dockerfile` installs them, for the containerized runtime.
   Running `engine` directly on the host (exactly what the Quick Start
   documents) needs them too. The service didn't crash and gave no error
   pointing at the missing tool; a first-time contributor would have had
   to guess. **Fixed**: added the prerequisite and install commands
   (Debian/Ubuntu and macOS) to both `README.md`'s Quick Start and
   `CONTRIBUTING.md`'s local setup.
2. **No guidance for a port-5432 conflict.** This sandbox's pre-existing
   native Postgres made `docker compose up -d` bind port 5432 ambiguously
   (Docker's proxy came up "healthy" but loopback traffic still routed to
   the other Postgres instance). **Fixed**: added a one-line note to the
   README's Quick Start pointing at the fix (stop the conflicting service,
   or remap the port in `.env`/`docker-compose.yml`).

Both fixes are documentation-only; `docker-compose.yml` itself was not
changed.

## Git history security

Findings, most severe first.

| Severity | Finding | Status |
|---|---|---|
| HIGH (current tree) | A real production VPS public IP address and system hostname were committed in plaintext across three docs (now under `docs/research/`) | **Fixed on this branch** — replaced with `<VPS_PUBLIC_IP>` / `<VPS_HOSTNAME>` placeholders |
| MEDIUM (history) | Traced via `git log --all -S"<VPS_PUBLIC_IP>"` (run with the actual value, not the placeholder): the IP/hostname were introduced by exactly two commits, `cc1c497` and `7f87dda`. Both are ancestors of `main` and of **7 of the repository's 8 tags** (`v0.4.2` through `v0.4.5`/`-app` variants — only `v0.4.2-app` predates them) and of **every local and remote branch**. | **Accepted, no rewrite** — see [Decision record](#decision-record). |
| INFO | `gitleaks detect --source . --log-opts="--all"`: 104 commits scanned, **no leaks found** — no API keys, tokens, passwords, or private-key material anywhere in history, re-verified after this pass. | No action needed |
| INFO | `git log --all --diff-filter=A --name-only` for `.pem`/`.p12`/`.pfx`/`.dump`/`.sql`: only schema migration `.sql` files (expected) and backup *scripts* (not backup data) were ever committed. | No action needed |

### Why history was not rewritten

The IP/hostname is **not a credential** — it identifies a VPS, not a way
to access one — so there is no revocation step it forces, unlike a leaked
secret. But scrubbing it from history would mean rewriting essentially the
entire project history (7 of 8 published tags, every branch), which:

- changes every downstream SHA, invalidating any existing clone, fork, or
  reference to those tags;
- requires a force-push, which this session is not authorized to perform
  without explicit instruction, and which the general safety rules treat
  as a "confirm first" action even when authorized in principle;
- is explicitly flagged by the mission instructions as something to
  "prepare, not launch automatically," precisely because of this scale of
  blast radius.

**If the maintainer decides to do it anyway**, here is the exact
situation to hand to `git filter-repo` (not installed in this
environment; install via `pip install git-filter-repo` or your package
manager):

```sh
# 1. Back up first — this rewrites nearly all of history.
git bundle create erytheon-full-backup.bundle --all

# 2. The two commits that introduced the values (pre-move paths):
#    cc1c497  docs: record phase 4A.5b production deployment (executed and validated)
#    7f87dda  docs: document private scientific console deployment

# 3. Replace the literal values across all of history (adjust the actual
#    IP/hostname you're removing — do not paste them into a public issue):
git filter-repo --replace-text <(printf '%s==><VPS_PUBLIC_IP>\n%s==><VPS_HOSTNAME>\n' "$OLD_IP" "$OLD_HOSTNAME")

# 4. Re-verify before pushing anything:
gitleaks detect --source . --log-opts="--all"
git fsck
git tag --list   # confirm which tags moved and by how much

# 5. Force-push is required after any history rewrite. Do this only with
#    explicit authorization, and only after confirming no fork/clone of
#    this repository already exists that would be orphaned by it.
```

Given the IP was never a secret, the pragmatic recommendation is: **leave
history as-is**, ship the current-tree fix, and treat this as documented,
accepted residual exposure — but that is the maintainer's call, not this
session's.

## Licensing

### Code

- `Cargo.toml` already declared `license = "MIT OR Apache-2.0"`; `LICENSE`,
  `LICENSE-MIT`, `LICENSE-APACHE` added to match.
- **Copyright holder resolved: William Ducamp.** No legal name existed
  anywhere in the codebase, README, or Cargo metadata, and this session's
  own git identity (`ERYTHEON Codex <codex@erytheon.local>`) was correctly
  identified as an agent identity and not used as a guess. The name was
  provided directly by the repository owner and applied as
  `Copyright (c) 2026 William Ducamp` in both `LICENSE-MIT` and
  `LICENSE-APACHE`. Dual license (MIT OR Apache-2.0) unchanged.

### Data — re-verified against live provider terms this pass

`docs/data-sources.md` now states a per-source status, verified
2026-08-17 against each provider's current published terms (not from
memory):

| Source | Status | What was verified |
|---|---|---|
| NASA FIRMS | `CLEAR` | Free use, attribution requested |
| Météo-France | `CLEAR` | Etalab Licence Ouverte 2.0 |
| ECMWF IFS Open Data | `CLEAR` | CC-BY-4.0, fully open at native resolution since 2025-10-01 (confirmed via ECMWF's own announcement) |
| Open-Meteo | `OPTIONAL PROVIDER` | Data is CC-BY-4.0 (commercial-safe), but the *free API* is contractually non-commercial-use-only with rate limits — commercial deployment needs a paid plan. Both README and `docs/data-sources.md` now state this precisely instead of implying an unconditional free backend. |
| BDIFF | `CLEAR for non-commercial reuse` | BDIFF's own mentions légales state commercial/advertising reuse needs prior request; non-commercial research reuse (Erytheon's current positioning) is not restricted by that clause |
| Prométhée | `NOT BUNDLED` | Confirmed: resale explicitly prohibited, use must be declared; merged into BDIFF in 2023 (legacy source going forward). The bundled fixture is now synthetic (see [Decision record](#decision-record)), closing the earlier provenance question. |
| OpenStreetMap | `CLEAR, with conditions` | ODbL 1.0, share-alike risk for derived databases (unchanged from first pass) |
| CORINE Land Cover | `CLEAR` | Copernicus "full, open and free access" policy confirmed |
| INSEE (Filosofi) | `CLEAR` | Etalab Licence Ouverte 2.0 confirmed |
| Administrative boundaries | `NOT BUNDLED / USER PROVIDED` | Confirmed no boundary file ships anywhere in the repo or any `.env*.example` |
| Territorial calendars | `CLEAR` (fixture) / `NOT BUNDLED` (production) | Bundled fixture is 3 rows of public factual data (dates, holiday names); a real production calendar is user-supplied |

Added `NOTICE.md` as a short, quick-reference attribution list (full
detail stays in `docs/data-sources.md`). No source in
`docs/data-sources.md` is left marked `REQUIRES LEGAL / LICENSE REVIEW`
as of this pass.

## Open-Meteo policy

Investigated whether Erytheon's Open-Meteo fallback has a way to be
disabled for a commercial deployer who shouldn't be using the free tier —
it does not; there's no environment toggle, it's wired directly into the
weather-forecast failover chain (`crates/engine/src/forecast.rs`). Adding
one would be a real code change with production-behavior implications
(the failover chain exists for resilience), which this pass's scope (docs
and release-readiness, not new features) argues against making
unilaterally. Documented instead — see the table above and
`docs/data-sources.md#open-meteo` — and flagged as a reasonable, small,
future engineering task rather than done here.

## Documentation architecture

Unchanged from the first pass: ~70 root-level `PHASE*`/report-style
Markdown files moved into `docs/research/phases/` (45 files) and
`docs/research/reports/` (29 files) via `git mv`, cross-links fixed,
`docs/research/README.md` added as an index. Root now holds `README.md`,
`LICENSE`, `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE.md`, `CHANGELOG.md`,
`ROADMAP.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`,
`GOVERNANCE.md`, this report, and the release-notes draft.

## Developer experience / community files

Unchanged from the first pass (`CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`,
`SECURITY.md`, `GOVERNANCE.md`, issue/PR templates, `dependabot.yml`),
plus this pass's additions: `NOTICE.md`,
`docs/release-notes-v0.5.0-draft.md`, and the `gdal`/`eccodes`
prerequisite added to `CONTRIBUTING.md`'s setup steps.

## CI

- `.github/workflows/ci.yml`: added an explicit `permissions: contents:
  read` block (it needed no write access before either, but relied on
  implicit repository/org defaults rather than declaring it — small,
  real, defense-in-depth hardening).
- Verified fork-PR safety for both workflows: neither uses
  `pull_request_target`; `ci.yml` runs on plain `pull_request` (fork PRs
  get a restricted token and no access to repository secrets by GitHub's
  own default); `container.yml` only triggers on push to `main`/tags/
  manual dispatch, which a fork PR cannot cause, and already declared
  minimal `permissions: contents: read, packages: write`.
- No custom `secrets.*` referenced anywhere except `container.yml`'s use
  of the automatic `GITHUB_TOKEN` to push to GHCR — safe given it can't be
  triggered by a fork PR.

## Production safety

No deployment, DNS, secret, migration, or model-activation action was
taken in either pass. No file under `migrations/` was edited. The
`pyrorisk` internal-name identifier (binary name, default DB user/name,
Docker volume name, env defaults) remains unchanged — a rename would touch
running deployment configuration for no open-source-readiness benefit; see
the first pass's [Production safety] note (folded into this report).

## Remaining blockers

**None that block calling this repository's content ready.** Both hard
blockers from the previous pass are closed (copyright holder confirmed;
git-history exposure explicitly accepted and recorded — see
[Decision record](#decision-record)). What remains are follow-ups, not
blockers:

1. **Enable GitHub private vulnerability reporting** (or otherwise stand
   up a private security contact) referenced in `SECURITY.md` — a GitHub
   repository *setting*, not repository content, so it's most natural to
   do at or immediately after the visibility change itself.
2. **Add the Open-Meteo kill-switch** as a follow-up engineering task (not
   a blocker for code publication, since no real data is bundled and
   Erytheon's own current non-commercial positioning is within Open-Meteo's
   free-tier terms, but worth doing before recommending Erytheon for a
   commercial fork) — see [Open-Meteo policy](#open-meteo-policy).

## Recommended next steps

1. This repository's content is `READY FOR PUBLIC RELEASE` — the engineering
   gates (tests, Quick Start, CI safety, endpoint safety, code and data
   licensing) are genuinely green, and the two decisions that needed a
   human (copyright holder, git-history exposure) are made and recorded.
2. Making the GitHub repository actually public remains the maintainer's
   own action — this session did not and will not flip that setting.
3. Cut the `v0.5.0` tag using the draft in
   [`docs/release-notes-v0.5.0.md`](docs/release-notes-v0.5.0.md)
   as a starting point — review and edit before publishing, don't publish
   verbatim.
4. At or after going public: enable GitHub private vulnerability
   reporting, secret scanning, and Dependabot alerts (`dependabot.yml`
   here only covers version-update PRs, not alerting).
5. Longer term: work the "Open-source track" phases in `ROADMAP.md`
   (Phase B public release → Phase C public platform → Phase D
   prospective validation → Phase E shadow candidate → Phase F scientific
   decision).

## Published

Executed 2026-08-17, on explicit instruction from William Ducamp (see
[Decision record](#decision-record)):

- All work from this audit committed to `main` in 15 logically-scoped
  commits, merged from `claude/erytheon-open-source-d0bfed` with
  `--no-ff`, and pushed to `origin/main`.
- GitHub repository `supremexxx/erytheon` visibility changed from private
  to **public**.
- Enabled: secret scanning, secret scanning push protection, private
  vulnerability reporting, Dependabot alerts, Dependabot security
  updates.
- Fixed the live repository description (was still the old, commercial
  "professional platform" framing from before this project's
  repositioning) and set accurate topics.
- README and Quick Start verified rendering correctly on the live public
  GitHub page, including the license badges GitHub auto-detected from
  `LICENSE-MIT`/`LICENSE-APACHE`, and internal doc links resolving.
- `v0.5.0` tagged and published as a GitHub Release from
  [`docs/release-notes-v0.5.0.md`](docs/release-notes-v0.5.0.md).

Phase B of the "Open-source track" in `ROADMAP.md` is complete as of this
entry.
