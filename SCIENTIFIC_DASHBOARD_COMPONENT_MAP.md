# ERYTHEON — Scientific dashboard component map

## Scope

This matrix is the implementation contract for Phase 4A.4. The reference
mock-up defines composition and density; ERYTHEON endpoints remain the only
source for scientific and operational values. A component is deliberately
unavailable when no current endpoint exposes the required observation.

| Reference component | ERYTHEON component | Data source | Endpoint | Current implementation | Honest fallback | Responsive and accessibility |
|---|---|---|---|---|---|---|
| Global status bar | `TopBar` | application/database status, source timestamps, active and candidate model state | `GET /api/science/overview`, `GET /api/science/sources` | Yes | “Non exposé” or “En attente” | Segments collapse progressively; values remain textual |
| Fixed left navigation | `SideNavigation` | versioned route catalogue | `/science/*` routes | Yes | Not applicable | Icon rail on tablet, horizontal scroll on mobile, `aria-current` |
| Compact KPI row | `MetricStrip` | BDIFF, FIRMS, cells, models, weather-source timestamp | overview + sources | Yes | “Indisponible” without synthetic value | Horizontal rail below 768 px |
| Main FWI map | `SpatialPreview` | no map, FWI surface or territorial geometry is exposed | none | No scientific map in 4A.4 | Explicit Phase 4B unavailable state plus real cell/event counts | Text summary; no fictitious geography |
| FWI/ISI/BUI tabs | none | indices not exposed | none | No | Omitted | Not applicable |
| Risk drivers table | `InterpretationFactors` | versioned scientific limitations and live registry state | overview + versioned Phase 3B reports | Partial | Physical drivers explicitly reported as not exposed | Text and status, never colour alone |
| High-risk locations | `RecentIgnitionTable` | recent active ignition events | `GET /api/science/data-quality/events?limit=6` | Replaced with real recent events | Empty table state | Semantic table in focusable local scroll region |
| System journal | `SystemJournal` | recent import batches and pipeline runs | `GET /api/science/imports`, `GET /api/science/pipelines` | Yes | “Aucune exécution récente” | Ordered list with time, type and textual status |
| Territory summary | `TerritorySummary` | static cell and BDIFF counts | overview/system | Partial | Resolution/region omitted when not exposed | Definition list |
| Risk distribution donut | `DonutChart` | real BDIFF cause categories | `GET /api/science/data-quality` | Yes, relabelled “Répartition des causes” | Empty chart state | SVG title/description plus textual legend |
| Data-quality gauge | `QualitySummary` | cause, geography and duplicate counts | `GET /api/science/data-quality` | Bars only; no invented global score | Explicit unavailable items | Text values accompany bars |
| System health | `SystemHealth` | DB, migrations, source success, model registry | overview/system/sources | Partial | Caddy marked “non exposé” | Status dot plus text |
| Model comparison | `ModelComparisonTable` | paired Phase 3B.8 comparison exposed by API | `GET /api/science/models` | Yes | Unavailable metrics omitted | Semantic table and keyboard tooltips |
| Calibration diagram | `CalibrationUnavailable` | calibration points are not exposed | none | No | Explicit API limitation | Textual empty state |
| Additional time series | `ComparablePopulation` | paired comparison population | `GET /api/science/models` | Compact real-data summary | Empty state | Textual percentage and bar |
| Source registry | `SourceTable` | source status rows | `GET /api/science/sources` | Yes | Empty source registry | Focusable table |
| Import and pipeline history | `RunTables` | latest read-only execution rows | imports/pipelines endpoints | Yes | Empty histories | Focusable tables |
| Dataset registry | `DatasetTable` | registered versions/build summaries | datasets endpoints | Yes | “Aucune version enregistrée” | Focusable table |
| Feature catalogue | `FeatureTable` | snapshots and calendar summary | features endpoint | Yes | Separate empty snapshot/calendar states | Focusable table and definition list |
| Project timeline | `ProgressTable` | version-controlled phase catalogue | progress endpoint | Yes | Empty phase catalogue | Semantic table |

## Deferred to Phase 4B

- Geographic map and H3 exploration.
- FWI and physical-driver values or trends.
- Calibration curve points, ROC and precision-recall curves.
- Live high-risk locations, exposed population, loss and alert counts.
- Any metric requiring a new API contract, SQL query or model execution.

These omissions are intentional. The dashboard never reconstructs unavailable
scientific observations in the browser and never substitutes demonstration
data.
