# ERYTHEON — Scientific liquid glass UI guide

## Intent

The scientific console is a private reading surface for operational and scientific
evidence. Its visual language must communicate precision, restraint and traceability,
not marketing performance.

The Phase 4A.4 direction is:

```text
quiet mission control
institutional scientific interface
subtle liquid glass
dense but breathable information
real data, visible provenance
```

It is explicitly not a startup dashboard, a leaderboard, a fintech interface or a
decorative glassmorphism exercise.

## Palette

The palette is neutral, cold and low-saturation.

| Token family | Role | Reference values |
|---|---|---|
| Canvas | global atmosphere | `#eef1ef`, `#e8ecea` |
| Glass | translucent surfaces | white/grey at 66–90% alpha |
| Ink | text hierarchy | `#17201d`, `#4f5b56`, `#74807b` |
| Lines | structure | forest-grey at 13–22% alpha |
| Forest | primary scientific accent | `#315b4c`, `#5f7d70` |
| Slate | technical context | `#526a78` |
| Ochre | inactive or caution state | `#8a6748` |
| Brick | failure or high-risk state | `#89544d` |

Accent colors are functional. They never create decorative gradients, neon glows or
large saturated areas.

## Typography

The interface uses a system-first stack:

```css
Inter,
-apple-system,
BlinkMacSystemFont,
"SF Pro Text",
"SF Pro Display",
"Segoe UI",
system-ui,
sans-serif
```

Rules:

- page titles are modest, tightly tracked and never marketing-sized;
- section kickers use small uppercase text for orientation, not decoration;
- values use tabular numerals;
- identifiers, commits and checksums use the system monospace stack;
- body copy remains at 10.5–13 px because this is a dense expert interface;
- muted text must remain readable and never become ornamental grey-on-grey.

## Glass rules

Glass is a material treatment, not a visual effect.

Use:

- translucent neutral surfaces;
- `backdrop-filter` between 18 and 24 px;
- one-pixel low-contrast borders;
- an inset white highlight;
- extremely light ambient depth;
- matte fallbacks when backdrop filtering is unavailable.

Avoid:

- glow;
- large or dark shadows;
- multicolour gradients;
- specular highlights;
- stacked transparent surfaces without a structural reason;
- rounded bubble cards.

The main panel radius is 11 px. The surface should still read as a technical sheet,
not a floating consumer-app tile.

## Global structure

### Mission bar

The 64 px top bar contains:

- the ERYTHEON monogram and console identity;
- private-console state;
- current UTC time;
- a restrained read-only status.

It remains sticky and translucent. It does not display invented system or freshness
data.

### Navigation

Desktop navigation is a 236 px compact scientific sidebar with:

- a small section label;
- thin line icons;
- text-first destinations;
- one two-pixel active indicator;
- a factual read-only footer.

At tablet width it becomes a sticky horizontal navigation rail. Below 420 px, icons
remain visible and labels collapse to preserve space.

### Analysis surface

The content area is capped at 1,580 px and uses a twelve-column grid. Typical layouts:

- 8 + 4 for primary evidence and contextual limitations;
- 7 + 5 for operations tables;
- 6 + 6 for comparable audit panels;
- 12 for status strips and dense tables.

Each page starts with an eyebrow, a restrained title, factual context and a compact
live-reading indicator.

## Components

### Status strip

Status strips are compact matrices, not KPI cards. Each cell contains:

- a small technical label;
- a factual value;
- no decorative icon;
- no oversized status badge.

On mobile they use two columns and preserve document width.

### Metric matrix

Metric cells share borders inside one analytical surface. Values are prominent but
not oversized. Optional sublabels explain provenance or interpretation.

Metrics must remain direct API values; no calculated trend or simulated comparison
may be introduced only for presentation.

### Panels

Panels contain a section header with:

- functional kicker;
- title;
- optional explanatory note;
- small section index.

Tinted panels are reserved for scientific context, inactive registries or limitations.

### Tables

Tables use:

- sticky translucent headers;
- tabular numerals;
- restrained row hover;
- local keyboard-focusable horizontal scrolling;
- thin separators;
- no pagination decoration beyond existing controls.

The document itself must never widen because of a table.

### Charts

The existing bars retain the same data and meaning. Their visual treatment uses:

- six-pixel matte tracks;
- low-saturation forest, slate and mineral tones;
- direct values in monospace;
- no animation, glow, area fill or invented axis.

### States

Operational states are represented by a five-pixel point plus text:

- forest for active, valid or healthy;
- slate for running or production;
- ochre for inactive, draft or review;
- brick for failed, blocked or rejected.

No large pill or saturated background is used.

### Empty and error states

Empty states state the absence of data plainly. Error states show the escaped API
message inside a restrained technical panel. Neither uses illustrations, marketing
copy or fake examples.

### Tooltips

Scientific-definition tooltips:

- remain linked through `aria-describedby`;
- work with pointer and keyboard focus;
- stay inside the viewport;
- use dark neutral translucent glass for contrast;
- populate through `textContent`, never HTML injection.

## Page composition

| Page | Primary composition |
|---|---|
| Overview | technical strip, metric matrix, contextual risks, component table |
| Progress | versioned programme register in one dense technical sheet |
| Sources | operational totals, source freshness, import and pipeline tables |
| Data Quality | audit metrics, four compact distributions, event exploration |
| Features | snapshot catalogue plus contextual calendar coverage |
| Datasets | analytical registry; detail pages use identity/population/split/exclusion grid |
| Models | model status strip, comparative metrics, active/candidate sheets, limitations |
| System | technical metrics, component health and integrity guardrails |

## Responsive rules

Validated breakpoints:

- 1,440 px: full twelve-column mission-control layout;
- 1,280 px: dense desktop layout;
- 1,024 px: horizontal navigation and stacked contextual panels;
- 375 px: icon navigation, two-column status/metrics and local table scrolling.

At every width:

- `document.scrollWidth` must equal `window.innerWidth`;
- tables scroll within `.sci-table-scroll`;
- no glass layer may obscure text;
- section headers and status values must wrap rather than overlap;
- active navigation remains visible and keyboard reachable.

## Accessibility

- A skip link targets the scientific content.
- Active navigation uses `aria-current="page"`.
- Focus indicators use a two-pixel forest outline.
- Tooltips remain keyboard accessible.
- All icons are decorative and hidden from assistive technology.
- Status meaning is always present in text; colour is secondary.
- `prefers-reduced-motion` collapses transitions and animation.
- A solid translucent fallback is provided for browsers without backdrop filtering.

## Maintenance boundaries

A visual change must not:

- add or modify API routes;
- modify JSON payloads or SQL;
- manufacture system status, freshness, metrics or scientific conclusions;
- change access control or read-only behavior;
- introduce an external UI framework or font dependency;
- turn Phase 4B scientific capabilities into decorative placeholders.

New components should reuse the existing tokens and structural primitives before
adding another visual pattern.
