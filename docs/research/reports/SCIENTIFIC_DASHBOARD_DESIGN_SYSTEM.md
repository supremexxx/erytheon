# ERYTHEON — Scientific dashboard design system

## Purpose

This document defines the presentation system delivered for Phase 4A.4. The
validated dashboard mock-up is the composition reference; ERYTHEON remains the
source of every scientific and operational value.

The visual direction is:

```text
advanced scientific command post
clear institutional interface
dense, precise and immediately readable
subtle matte liquid glass
low-saturation natural palette
```

It is not a SaaS template, marketing dashboard, fintech interface, gaming theme
or demonstration populated with synthetic observations.

## Foundation tokens

### Canvas and glass

| Token | Value | Role |
|---|---|---|
| `--canvas` | `#f4f6f4` | global cold-white background |
| `--canvas-alt` | `#eef1ef` | lower canvas depth |
| `--glass-bg` | `rgba(255, 255, 255, 0.66)` | standard panel |
| `--glass-bg-strong` | `rgba(255, 255, 255, 0.82)` | stronger separation |
| `--glass-border` | `rgba(35, 49, 42, 0.10)` | one-pixel structure |
| `--glass-highlight` | `rgba(255, 255, 255, 0.72)` | inset surface edge |

Glass uses `blur(18px) saturate(115%)`. The solid fallback remains readable
when backdrop filtering is unavailable.

### Text

| Token | Value |
|---|---|
| `--text-primary` | `#18211d` |
| `--text-secondary` | `#5c6761` |
| `--text-muted` | `#84908a` |
| `--text-faint` | `#a4ada8` |

### Functional colour

| Meaning | Value |
|---|---|
| scientific accent | `#496d5b` |
| active/healthy | `#4e795f` |
| caution/inactive | `#a87840` |
| unavailable/failure | `#98584f` |
| technical information | `#607886` |
| neutral/not exposed | `#87918c` |

Colour is secondary to text. No state is communicated by colour alone.

### Geometry

```text
small radius      6 px
panel radius      9 px
large radius     12 px
panel border      1 px
main gap          8 px
desktop sidebar 176 px
top bar          62 px
```

The only panel shadow is an inset highlight and a nearly invisible one-to-three
pixel separation. There is no glow or large elevation.

## Typography

The console uses local system fonts only:

```css
-apple-system,
BlinkMacSystemFont,
"SF Pro Display",
"SF Pro Text",
"Segoe UI",
Inter,
Helvetica,
Arial,
sans-serif
```

No remote font is loaded. Technical identifiers use the local system monospace
stack.

| Element | Typical size |
|---|---|
| brand | 15 px, 650 |
| page title | 18 px, 560 |
| metric value | 17–23 px, 530 |
| panel title | 9 px, 650, uppercase |
| table body | 9.5 px |
| table header | 7 px, uppercase |
| metadata | 7.5–9 px |

All dates, counts, metrics and identifiers use tabular numerals.

## Application shell

### Desktop

The shell is a two-column grid:

```text
176 px sidebar | remaining analysis surface
```

The sidebar occupies the full viewport height. The topbar is sticky over the
analysis surface and is split into narrow factual segments:

- scientific console title;
- system state;
- weather freshness;
- active model;
- candidate state;
- UTC clock and date.

Values are loaded from existing read-only endpoints. Until they arrive, the
shell says “En attente”; unavailable observations say “Non exposé”.

### Navigation

The navigation order is:

1. Vue d'ensemble
2. Sources
3. Qualité des données
4. Features
5. Datasets
6. Modèles
7. Système
8. Progression

Items are 40 px high with 17 px monochrome line icons. The active route uses a
low-opacity green-grey surface and `aria-current="page"`.

Only implemented shortcuts are shown. The footer reports the API state without
inventing a service count.

## Dashboard grid

The analysis surface uses twelve columns with 8 px gaps.

The Overview composition is:

```text
six compact metrics across 12 columns
primary analytical area: 9 columns
context column:          3 columns

inside primary:
spatial preview          7 columns
interpretation factors   5 columns
recent events            7 columns
system journal           5 columns
model comparison        12 columns
```

At 1,280 px, the primary/context split becomes 8/4. At compact desktop width,
the context modules move below the primary analysis. No component is scaled
down until it becomes unreadable.

## Components

### `MetricStrip`

- six compact modules on wide desktop;
- label, value and factual detail/status;
- 85 px minimum height;
- horizontally scrollable rail on mobile;
- no synthetic trend, alert or forecast value.

### `GlassPanel`

- one-pixel low-contrast border;
- 9 px radius;
- 10 px internal padding;
- restrained uppercase technical heading;
- optional factual note and version/source marker.

### `SpatialPreview`

The current API exposes cell and event counts but no map geometry, risk surface
or FWI layer. The component therefore shows:

- an explicitly unavailable cartographic state;
- a decorative, non-geographic H3-like background;
- real cell and event counts;
- “FWI courant — Non exposé”;
- Phase 4B as the documented future scope.

It never implies a territory, forecast or risk value.

### `CompactTable`

- semantic `thead` and `th scope="col"`;
- sticky neutral header;
- local focusable scroll container;
- tabular figures;
- restrained row hover;
- no document-level overflow.

Compact tables may wrap explanatory cells. Identifier-heavy tables retain local
horizontal scrolling.

### `SystemJournal`

Imports and pipeline runs are merged by their real timestamps and sorted newest
first. Each entry shows time, textual state, source/pipeline name and factual
detail. The vertical line is purely structural.

### `DonutChart`

- dependency-free SVG;
- real category counts only;
- textual legend and percentages;
- accessible title and description;
- no animation or invented score.

### `HorizontalMetricBar`

Bars compare observed counts within a declared denominator. Values remain
visible in text. A bar never represents an inferred health score.

### `ModelComparisonTable`

The table uses the version-controlled Phase 3B.8 comparison exposed by the
existing models endpoint. v1 is labelled active, the candidate inactive, and no
winner badge is used.

Calibration points are not exposed, so the calibration panel remains an honest
unavailable state.

### `EmptyState` and `ErrorState`

Empty states name the absent registry or observation plainly. Error messages are
escaped before insertion. No illustration, demonstration row or optimistic
status is substituted.

### `ScientificTooltip`

- keyboard and pointer accessible;
- linked through `aria-describedby`;
- positioned inside the viewport;
- content assigned through `textContent`;
- high-contrast neutral glass.

## Responsive behaviour

### 1,440 px and above

- 176–180 px sidebar;
- six metrics in one row;
- complete 9/3 Overview split;
- all contextual panels visible in the right column.

### 1,280–1,439 px

- 168 px sidebar;
- 8/4 Overview split;
- reduced topbar segment padding;
- primary scientific hierarchy preserved.

### 1,024–1,279 px

- 72 px icon rail;
- primary and context areas become full width;
- context modules use two columns;
- metrics remain one compact row.

### Below 840 px

- 56 px mobile brand bar;
- named 40 px navigation toggle and two-column drawer;
- 50 px technical topbar;
- horizontal metric rail;
- stacked analysis panels;
- locally scrollable tables.

At 375 × 812, `document.scrollWidth` equals `window.innerWidth`.

## Accessibility

- skip link to the scientific content;
- one semantic `h1` per page;
- semantic navigation and context aside;
- `aria-current` for the active route;
- named mobile menu with `aria-expanded`;
- visible focus ring;
- table regions focusable by keyboard;
- SVG chart title, description and textual legend;
- tooltip focus support;
- textual state accompanying every status point;
- `prefers-reduced-motion` support;
- no motion, parallax or animated chart.

## Maintenance boundary

A presentation change must not:

- add or change an API route;
- change an API payload or SQL query;
- reconstruct unavailable scientific observations in the browser;
- activate or score the candidate;
- alter access control, Caddy, scheduler or database state;
- introduce a framework, CDN, remote font or large chart dependency.

The component-to-data contract is maintained in
`SCIENTIFIC_DASHBOARD_COMPONENT_MAP.md`.
