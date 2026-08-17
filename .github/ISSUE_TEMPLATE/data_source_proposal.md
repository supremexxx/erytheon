---
name: Data source proposal
about: Propose adding a new weather, satellite, territorial, or historical fire data source
title: "[data source] "
labels: data-source
---

## Source

- Name and provider:
- Official URL:
- What it would add (feature, coverage area, temporal resolution, etc.):

## License

- License name / terms URL:
- Attribution requirements:
- Redistribution allowed? (yes / no / unclear — if unclear, say so; don't guess)
- Any restriction on commercial use, derived datasets, or scale of reuse?

See [`docs/data-sources.md`](../../docs/data-sources.md) for the format
this should eventually be documented in, and mark the license
`REQUIRES LEGAL / LICENSE REVIEW` if you're not certain.

## Access

- Requires an API key / registration? Free or paid?
- Rate limits?
- Data format (CSV, GeoTIFF, GRIB2, GeoJSON, etc.)?

## Integration impact

- Which crate(s) would this touch (`ingest`, `dataset`, `quality`, ...)?
- Does it require a new fixture under `testdata/`? (Real data from this
  source should not be committed — see `docs/data-sources.md`.)
- Any expected impact on existing features, labels, or model inputs?
