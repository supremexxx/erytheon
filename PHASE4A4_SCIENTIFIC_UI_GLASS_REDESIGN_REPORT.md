# ERYTHEON — Phase 4A.4 scientific UI glass redesign report

## Conclusion

Phase 4A.4 rebuilds the complete presentation of the private scientific console as a
quiet, premium mission-control interface. The change is front-end only and uses the
existing production API responses without altering their meaning or lifecycle.

## Scope

Branch:

```text
agent/phase4a4-scientific-ui-glass-redesign
```

Base:

```text
7caa6cca19f874625f86b1e9403cabd011e8fa0a
```

Modified application files:

```text
crates/api/static/science/index.html
crates/api/static/science/science.css
crates/api/static/science/science.js
```

Documentation:

```text
SCIENTIFIC_UI_GLASS_REDESIGN_GUIDE.md
PHASE4A4_SCIENTIFIC_UI_GLASS_REDESIGN_REPORT.md
```

## Explicitly unchanged

No file was changed under:

```text
crates/api/src
crates/store/src
crates/engine/src
crates/risk/src
crates/fwi/src
crates/ingest/src
migrations
```

Consequently this phase introduces:

- no API or route change;
- no SQL or payload change;
- no model or scoring change;
- no candidate activation;
- no shadow scoring;
- no scheduler, FIRMS or Open-Meteo change;
- no access-control change;
- no migration or database write;
- no deployment.

## Initial visual audit

The previous console was functionally sound and intentionally sober, but the
presentation still read as an internal administration tool:

- flat global bar with limited hierarchy;
- text-only navigation with weak spatial orientation;
- sections differentiated mostly by headings and borders;
- status and metrics arranged as utilitarian grids;
- little distinction between primary evidence and scientific context;
- responsive behavior correct but visually compressed rather than deliberately
  recomposed.

The redesign retains the proven 4A.3 table containment, honest empty states and
keyboard tooltips.

## Visual system delivered

### Console frame

- sticky 64 px mission bar with ERYTHEON identity, private state, UTC time and
  restrained read-only indicator;
- compact desktop sidebar with original destinations and thin inline SVG icons;
- sticky horizontal tablet/mobile navigation;
- scientific mode footer that states real-data/read-only behavior without inventing
  system facts.

### Material and palette

- neutral cold canvas with extremely low-contrast ambient depth;
- translucent matte surfaces using controlled blur;
- one-pixel forest-grey borders;
- near-zero elevation rather than large shadows;
- forest, slate, ochre and mineral accents used only for semantics;
- no glow, neon, saturated gradient or external visual dependency.

### Information hierarchy

- a consistent page header for every route;
- twelve-column analytical layouts;
- indexed panel headers and factual notes;
- compact status strips and metric matrices;
- contextual side panels for risks, integrity and scientific limitations;
- denser, sticky-header tables with local scrolling;
- low-saturation bar distributions;
- more legible model and dataset technical sheets.

## Page-by-page result

| Route | Result |
|---|---|
| `/science/overview` | mission-control synthesis with status, primary metrics, risks and component state |
| `/science/progress` | serious versioned programme register |
| `/science/sources` | operational source/import/pipeline hierarchy |
| `/science/data-quality` | audit metrics plus four restrained distributions and event table |
| `/science/features` | provenance catalogue with contextual calendar coverage |
| `/science/datasets` | analytical registry and redesigned detail composition |
| `/science/models` | institutional comparison, active/candidate sheets and limitations |
| `/science/system` | premium technical sheet for migrations, registries and integrity |

No data was added, removed, transformed or simulated for these compositions.

## Accessibility

- skip link added;
- active route now exposes `aria-current="page"`;
- inline icons are excluded from the accessibility tree;
- keyboard focus is visible across links, controls, tables and definitions;
- tooltip position is clamped vertically and horizontally;
- tooltip content still uses `textContent`;
- status remains textual rather than colour-only;
- reduced-motion preferences are respected;
- non-blur fallback surfaces remain readable.

## Browser validation

Real Chromium loaded the local redesigned assets against the production read-only
science APIs through an SSH tunnel. Production code, data and configuration were not
modified.

Matrix:

```text
8 pages × 4 viewports = 32 checks
1440×900
1280×800
1024×768
375×812
```

Results:

- 32/32 routes returned HTTP 200;
- 32/32 rendered the expected page heading;
- zero console errors in the normal matrix;
- zero page errors;
- zero UI error states;
- zero document-level horizontal overflow;
- exactly one active navigation item on every route;
- all tables remained inside valid local scroll regions;
- no loading state remained after network idle.

Additional checks:

- a focused scientific term displayed its tooltip inside the viewport with the
  expected accessible description;
- a simulated 503 displayed an escaped, intelligible error and recovered after the
  route was restored;
- mobile model status uses a two-column matrix at 375 px;
- the built-in data favicon removed the previous `/favicon.ico` 404.

Representative captures are stored locally under:

```text
output/playwright/phase4a4/
```

They are intentionally ignored by Git.

## Quality checks

Executed:

```text
node --check crates/api/static/science/science.js
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked --no-fail-fast
```

All checks passed. The full workspace suite used an isolated disposable
`postgis/postgis:16-3.4` instance on the test-only local port expected by the
repository. Science integration tests passed 10/10 with zero ignored tests.

## Files deliberately preserved

The pre-existing untracked files below are unrelated and remain untouched:

```text
PR1_INTEGRATION_REVIEW_REPORT.md
rust-toolchain 2.toml
```

## Final statement

```text
FULL SCIENTIFIC CONSOLE VISUAL REDESIGN COMPLETED
FRONT-END ONLY
NO DATA LOGIC CHANGED
NO API CHANGED
NO ROUTES CHANGED
NO MODEL CHANGE
NO SCORING CHANGE
NO MIGRATION
PREMIUM APPLE-INSPIRED LIQUID GLASS SCIENTIFIC UI DELIVERED
```
