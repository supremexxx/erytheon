#!/usr/bin/env bash
set -euo pipefail

readonly BASE_URL="https://download.geofabrik.de/europe/france"
readonly DESTINATION="${1:-/opt/pyrorisk/data/osm/regions}"
readonly MIN_FREE_KIB_ALL_REGIONS=$((8 * 1024 * 1024))
readonly ALL_REGIONS=(
  alsace
  aquitaine
  auvergne
  basse-normandie
  bourgogne
  bretagne
  centre
  champagne-ardenne
  corse
  franche-comte
  haute-normandie
  ile-de-france
  languedoc-roussillon
  limousin
  lorraine
  midi-pyrenees
  nord-pas-de-calais
  pays-de-la-loire
  picardie
  poitou-charentes
  provence-alpes-cote-d-azur
  rhone-alpes
)

contains_region() {
  local wanted="$1"
  local region
  for region in "${ALL_REGIONS[@]}"; do
    if [[ "${region}" == "${wanted}" ]]; then
      return 0
    fi
  done
  return 1
}

file_md5() {
  if command -v md5sum >/dev/null 2>&1; then
    md5sum "$1" | awk '{print $1}'
  else
    md5 -q "$1"
  fi
}

mkdir -p "${DESTINATION}"

if [[ -n "${OSM_REGIONS:-}" ]]; then
  IFS=',' read -r -a regions <<<"${OSM_REGIONS}"
else
  regions=("${ALL_REGIONS[@]}")
fi

for region in "${regions[@]}"; do
  if ! contains_region "${region}"; then
    echo "Unsupported Geofabrik France region: ${region}" >&2
    exit 1
  fi
done

if [[ ${#regions[@]} -eq ${#ALL_REGIONS[@]} ]]; then
  available_kib="$(df -Pk "${DESTINATION}" | awk 'NR == 2 {print $4}')"
  if ((available_kib < MIN_FREE_KIB_ALL_REGIONS)); then
    echo "At least 8 GiB free is required before downloading all France extracts" >&2
    exit 1
  fi
fi

for region in "${regions[@]}"; do
  filename="${region}-latest.osm.pbf"
  target="${DESTINATION}/${filename}"
  partial="${target}.part"
  checksum_url="${BASE_URL}/${filename}.md5"
  source_url="${BASE_URL}/${filename}"
  expected="$(curl --fail --location --retry 3 --silent --show-error "${checksum_url}" | awk '{print $1}')"

  if [[ -f "${target}" && "$(file_md5 "${target}")" == "${expected}" ]]; then
    echo "Verified existing ${target}"
    continue
  fi

  echo "Downloading ${source_url}"
  curl --fail --location --retry 3 --continue-at - --show-error \
    "${source_url}" \
    --output "${partial}"

  actual="$(file_md5 "${partial}")"
  if [[ "${actual}" != "${expected}" ]]; then
    echo "Checksum mismatch for ${partial}: expected ${expected}, got ${actual}" >&2
    exit 1
  fi

  mv "${partial}" "${target}"
  chmod 0644 "${target}"
  echo "Downloaded and verified ${target}"
done

echo "OSM extracts ready in ${DESTINATION}"
