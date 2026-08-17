#!/usr/bin/env bash
set -Eeuo pipefail

readonly SOURCE_URL="https://etalab-datasets.geo.data.gouv.fr/contours-administratifs/latest/geojson/communes-1000m.geojson"
readonly DESTINATION="${1:-/opt/pyrorisk/data/boundaries/communes-1000m.geojson}"
readonly TEMPORARY="${DESTINATION}.tmp"

mkdir -p "$(dirname "${DESTINATION}")"
curl --fail --location --retry 3 --silent --show-error "${SOURCE_URL}" --output "${TEMPORARY}"
grep -q '"FeatureCollection"' "${TEMPORARY}" || {
  echo "Downloaded commune catalog is not a GeoJSON FeatureCollection" >&2
  exit 1
}
mv "${TEMPORARY}" "${DESTINATION}"
chmod 0644 "${DESTINATION}"
sha256sum "${DESTINATION}"
