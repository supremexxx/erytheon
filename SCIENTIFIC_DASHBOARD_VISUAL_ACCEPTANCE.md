# ERYTHEON — Scientific dashboard visual acceptance

## Purpose

This document freezes the visual gap analysis performed before the Phase 4A.4b
CSS pass. It compares:

- the Phase 4A.4 overview rendered at 1440 × 900;
- the supplied reference, *Console scientifique du risque d’incendie de
  forêt.png*;
- the production operational map observed on 29 July 2026.

The reference is a visual direction, not a source of scientific data. ERYTHEON
must keep showing only values exposed by the existing APIs.

## Initial visual gap matrix

| Area | Phase 4A.4 observation | Reference characteristic | Phase 4A.4b acceptance |
| --- | --- | --- | --- |
| Global background | Pale and clean, but nearly uniform | Layered cold white canvas with perceptible atmospheric depth | A quiet cold canvas with at least two restrained depth planes; no glow or saturated gradient |
| Top bar | 62 px, dense, labels and values too small | Taller, more comfortable utility bar with clearly separated context blocks | 68–72 px desktop height; console title at least 14 px; utility labels at least 9 px and values at least 11 px |
| Sidebar | 176 px and visually compressed | Calm, legible navigation with a confident brand block | 188–196 px at 1440 px; nav text at least 12 px; 42–44 px active rows; stronger but restrained active surface |
| Main spacing | 8–11 px gaps create an administrative density | Modules are dense but have a visible rhythm | 12–16 px desktop gaps and 14–18 px panel padding without wasting space |
| Typography | Numerous 7.5–9 px labels; hard to scan | Small institutional typography that remains readable | No essential label below 9 px; normal data copy 11–13 px; panel headings 10–11 px; KPI values 24–30 px |
| KPI strip | Six flat 85 px tiles with weak separation | Confident metric band with stronger hierarchy | 96–108 px tiles, stronger values, legible metadata, subtle tonal depth and no fintech styling |
| Panels | Border and blur exist but read as flat white cards | Subtle liquid glass with fine highlights and controlled depth | Translucency remains visible over the canvas; 1 px border, inset highlight and restrained multi-plane shadow |
| Borders | Very light and almost invisible on the overview | Fine but definite structural lines | Borders visible at normal brightness without becoming dark boxes |
| Transparency and blur | Technically present, visually imperceptible | Glass is noticeable but not decorative | 18–28 px blur with controlled saturation; glass effect still legible over the background |
| Primary grid | 9/3 columns, but first row lacks a real operational focal point | Large spatial module anchors the mission-control layout | The live map spans the dominant left module and remains at least 360 px high at 1440 px |
| Map | Placeholder states that no map is available | Operational spatial view is the main visual anchor | Exact production Leaflet/CARTO/risk component is embedded; no simulated layer or reconstructed science data |
| Map controls | Not present in the science console | Compact scientific controls, legend and status | Four horizons, threshold, live status, cell count, legend, zoom and attribution stay usable and restrained |
| Map details | Not present | Context appears without replacing the analytical view | Cell selection exposes the existing production detail drawer and can be closed with button or Escape |
| Interpretation factors | Accurate but table feels administrative | A scan-friendly risk-driver module | Preserve the documented limits while improving row height, type size and visual hierarchy |
| Recent events | Dense table with truncated values at 1440 px | Structured analytical table | Keep real event rows; readable 11 px minimum table text; horizontal overflow rather than data loss |
| System journal | Very small, under-emphasised text | Clear operational chronology | 11–12 px event labels, 9–10 px timestamps, visible timeline and calm status markers |
| Right column | Correct information but narrow and visually flat | Region, distribution, quality and health read as a coherent instrument stack | Maintain the factual territory/cause/quality/health panels with more padding, stronger headings and consistent depth |
| Donut | Accurate and restrained, but small labels | Central visual with an explicit legend | Increase visual and label size without inventing an aggregate quality score |
| Data quality | Thin bars and tiny values | Compact but quickly readable audit summary | Larger row rhythm and labels; real counts and ratios only |
| System health | Useful but cramped | Clear service list with understated state marks | 11 px service labels and status values; no large coloured pills |
| Model comparison | Correct but below the fold and very compressed | Wide analytical comparison with charts | Preserve real metrics and inactive-candidate wording; improve table and analysis readability without inventing curves |
| Responsive 1280 | Layout survives but topbar and context become crowded | Desktop hierarchy should remain intact | Map remains dominant; topbar may hide secondary segments but must retain system state and clock |
| Responsive 1024 | Sidebar collapses early and content becomes stacked | Compact mission control, not a broken desktop | Compact sidebar is acceptable; map, controls and details remain usable; no overlap or clipped data |
| Mobile | Horizontal KPI scroller and stacked panels work | Clear priority order | Navigation, KPI scroller, map controls and detail drawer remain keyboard/touch usable; no page-width overflow |

## Objective acceptance checks

The Phase 4A.4b overview is accepted only when all the following are true:

1. At 1440 × 900 the live operational map is visible without scrolling and is
   the largest single module in the first analytical row.
2. The page has no essential text smaller than 9 px and no normal table data
   smaller than 10.5 px.
3. System state, data freshness, active v1 and inactive candidate remain
   unambiguous.
4. The map exposes the existing four horizons and threshold; changing either
   causes a request using the selected values.
5. Loading, empty, API error and Leaflet-unavailable states are legible and do
   not collapse the panel.
6. A selected production risk cell opens the existing cell details; the drawer
   closes with its button and with Escape.
7. At 1440, 1280, 1024, tablet and mobile widths, the page has no accidental
   horizontal viewport overflow. Tables may scroll inside their own wrapper.
8. Keyboard focus remains visible on navigation, map controls, range input,
   buttons and links.
9. No synthetic FWI, score, population, loss, model or system value is added to
   imitate the reference.
10. Browser console has no uncaught error on Overview, Sources, Data Quality,
    Features, Datasets, Models, System or Progress.

## Visual scoring rubric

Each category is scored from 0 to 10 after browser validation:

- composition and hierarchy;
- typography and readability;
- material, depth and glass restraint;
- fidelity to the supplied reference;
- scientific credibility;
- operational map integration;
- responsive behaviour;
- accessibility and interaction clarity.

The release threshold is an average of **8/10**, with no category below 7/10.

## Final Phase 4A.4b assessment

The browser-validated implementation achieved **8.6/10** overall:

| Category | Final score |
| --- | ---: |
| Composition and hierarchy | 8.5 |
| Typography and readability | 8.5 |
| Material, depth and glass restraint | 8.2 |
| Fidelity to the supplied reference | 8.2 |
| Scientific credibility | 9.1 |
| Operational map integration | 9.2 |
| Responsive behaviour | 8.7 |
| Accessibility and interaction clarity | 8.4 |

All ten objective checks above passed. The final normal browser sweep covered
all eight science routes and the four target viewports without application
console errors, accidental viewport overflow or duplicate Leaflet controls.
