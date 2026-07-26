# Test data

## `fwi_reference.csv`

This is the 48-day standard FWI System test sequence from Van Wagner and Pickett (1985), as distributed and validated by version 1.9.2 of the Natural Resources Canada `cffdrs` R package. The source station is at latitude 40°N and uses the standard northern monthly day-length factors.

The input weather columns and six expected components were copied from `cffdrs/data/test_fwi.csv` and `cffdrs/tests/testthat/data/fwi_01.csv`. The expected values are rounded by the source fixture to a varying number of decimal places. Tests derive the absolute tolerance from the final displayed digit of each expected value, plus a small floating-point allowance.

References:

- Van Wagner, C. E., and T. L. Pickett. 1985. *Equations and FORTRAN program for the Canadian Forest Fire Weather Index System*. Forestry Technical Report 33.
- Van Wagner, C. E. 1987. *Development and Structure of the Canadian Forest Fire Weather Index System*. Forestry Technical Report 35.
- Natural Resources Canada, `cffdrs` R package 1.9.2, <https://cran.r-project.org/package=cffdrs>.

## `firms_viirs_snpp.csv`

This fixture contains five unmodified VIIRS S-NPP active-fire records from the official NASA FIRMS educational sample dated 2023-07-12. The records form one cluster near Fos-sur-Mer in southern France; they are outside the default Aude AOI because the public educational sample contained no Aude detection on that date.

The fixture preserves the exact 14-column FIRMS Area API schema and is used whenever `FIRMS_MAP_KEY` is absent. Source: <https://firms.modaps.eosdis.nasa.gov/content/notebooks/sample_viirs_snpp_071223.csv>.

## `meteo_france_synop.csv`

This fixture contains the complete official SYNOP CSV rows for Perpignan, Saint-Girons, Millau, and Toulouse-Blagnac at 2025-07-16 12:00 UTC. The rows come from Météo-France's 2025 annual SYNOP archive published on data.gouv.fr; fields and SI units are preserved exactly. The connector converts Kelvin to Celsius and metres per second to kilometres per hour during normalization.

Source archive: <https://meteofrance.s3.sbg.io.cloud.ovh.net/data/synchro_ftp/OBS/SYNOP/synop_2025.csv.gz>, listed by the data.gouv.fr “Observations SYNOP” dataset.

## Phase 4 fixtures

`osm_features.csv` contains unchanged OSM identifiers and coordinates returned by Overpass for roads, buildings, and public parking around Carcassonne. It uses the normalized geometry schema also produced by the direct Geofabrik PBF loader. Source data is © OpenStreetMap contributors under ODbL.

`bdiff_aude.csv` contains six public fields for 94 Aude records displayed by BDIFF for 2025: five winter records plus all 89 forest-fire alerts returned for June through August. It preserves source identifier, alert time and UTC offset, municipality, public municipality-centre coordinate, surface, and grouped cause. `promethee_aude.csv` contains one legacy Aude record from 2000, paired with the OSM municipality centre because no more precise public coordinate is exposed.

`bdiff_pipeline_fixture.csv` is a synthetic, deterministic Phase 3B.1 fixture. It covers the six documented cause groups, an unmapped cause, invalid coordinates, timestamp and surface, a missing identity, an intra-batch duplicate, and two distinct events sharing one date and coordinate. It contains no personal data.

`corine_aude.csv` contains five CORINE Land Cover 2018 samples in the default AOI. Their polygon identifiers and three-digit classes were checked against the official CLC 2018 web map; the production path accepts the downloadable GeoTIFF. Source product: Copernicus Land Monitoring Service CLC 2018, V2020_20u1.

`insee_filosofi_200m.csv` contains six complete, unchanged rows for municipality code 11069 from INSEE's official Filosofi 2019 metropolitan 200-metre CSV. Official EPSG:3035 grid identifiers and imputation flags are retained.

`calendar_zone_c.csv` contains 2025-07-14 through 2025-07-16. The school-holiday flag comes from the Ministry of Education calendar for Toulouse/Zone C; the public-holiday flag comes from the DINUM metropolitan public-holiday API.

Sources:

- <https://download.geofabrik.de/europe/france/languedoc-roussillon.html>
- <https://bdiff.agriculture.gouv.fr/incendies>
- <https://land.copernicus.eu/en/products/corine-land-cover/clc2018>
- <https://www.insee.fr/fr/statistiques/7655475?sommaire=7655515>
- <https://data.education.gouv.fr/api/explore/v2.0>
- <https://calendrier.api.gouv.fr/jours-feries/>

## Phase 6 fixtures

`meteo_france_synop_archive.csv` contains the official archive columns used by the historical loader for 2025-06-05 and 2025-06-06. It includes one `mq` missing-value marker to verify that incomplete stations are skipped while the day remains usable. Full monthly archives are downloaded to ignored `data/synop/` rather than committed.

Sources:

- <https://donneespubliques.meteofrance.fr/donnees_libres/Txt/Synop/Archive/>
- <https://bdiff.agriculture.gouv.fr/incendies>
