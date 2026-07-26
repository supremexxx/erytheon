#!/usr/bin/env bash
set -euo pipefail

readonly SOURCE_URL="https://etalab-datasets.geo.data.gouv.fr/contours-administratifs/latest/geojson/departements-1000m.geojson"
readonly DESTINATION="${1:-/opt/pyrorisk/data/boundaries/departements-1000m.geojson}"
readonly TEMPORARY="${DESTINATION}.tmp"

mkdir -p "$(dirname "${DESTINATION}")"
curl --fail --location --retry 3 --silent --show-error \
  "${SOURCE_URL}" \
  --output "${TEMPORARY}"

if ! grep -q '"FeatureCollection"' "${TEMPORARY}"; then
  echo "Downloaded document is not a GeoJSON FeatureCollection" >&2
  rm -f "${TEMPORARY}"
  exit 1
fi

mv "${TEMPORARY}" "${DESTINATION}"
chmod 0644 "${DESTINATION}"
echo "Downloaded ${DESTINATION}"
if command -v sha256sum >/dev/null 2>&1; then
  sha256sum "${DESTINATION}"
else
  shasum -a 256 "${DESTINATION}"
fi
