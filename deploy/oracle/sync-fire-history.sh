#!/usr/bin/env bash
set -Eeuo pipefail

deploy_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
project_dir="$(cd -- "${deploy_dir}/../.." && pwd)"
env_file="${1:-${deploy_dir}/.env}"

if [[ ! -f "${env_file}" ]]; then
  printf 'Missing environment file: %s\n' "${env_file}" >&2
  exit 1
fi

env_file_value() {
  local name="$1"
  local value
  value="$(awk -F= -v key="${name}" '$1 == key {sub(/^[^=]*=/, ""); print; exit}' "${env_file}")"
  if [[ "${value}" == \"*\" || "${value}" == \'*\' ]]; then
    value="${value:1:${#value}-2}"
  fi
  printf '%s' "${value}"
}

setting() {
  local name="$1"
  local fallback="$2"
  local value="${!name:-}"
  if [[ -z "${value}" ]]; then
    value="$(env_file_value "${name}")"
  fi
  printf '%s' "${value:-${fallback}}"
}

search_url="$(setting BDIFF_SEARCH_URL 'https://bdiff.agriculture.gouv.fr/incendies')"
export_url="$(setting BDIFF_EXPORT_URL 'https://bdiff.agriculture.gouv.fr/incendies/zip')"
from_year="$(setting BDIFF_HISTORY_FROM_YEAR '2020')"
to_year="$(setting BDIFF_HISTORY_TO_YEAR "$((10#$(date -u +%Y) - 1))")"
if [[ ! "${search_url}" =~ ^https:// ]] || [[ ! "${export_url}" =~ ^https:// ]]; then
  printf 'BDIFF URLs must use HTTPS.\n' >&2
  exit 1
fi
if (( from_year < 1973 || to_year < from_year )); then
  printf 'Invalid BDIFF history interval: %s-%s.\n' "${from_year}" "${to_year}" >&2
  exit 1
fi

data_dir="$(setting BDIFF_HOST_DIR "${project_dir}/data/bdiff")"
reference_dir="$(setting BDIFF_REFERENCE_DIR "${project_dir}/data/reference")"
target="${data_dir}/france.csv"
communes="${reference_dir}/commune-centres.json"
communes_url="$(setting COMMUNES_CENTRES_URL 'https://geo.api.gouv.fr/communes?fields=nom,code,codeDepartement,centre&format=json&geometry=centre')"
minimum_rows="$(setting BDIFF_MIN_ROWS '500')"
maximum_bytes="$(setting BDIFF_MAX_BYTES '104857600')"
minimum_free_kib="$(setting BDIFF_MIN_FREE_KIB '1048576')"
maximum_reject_ratio="$(setting BDIFF_MAX_REJECT_RATIO '0.01')"
negatives_per_positive="$(setting HUMAN_MODEL_NEGATIVES_PER_POSITIVE '4')"
compose=(docker compose --env-file "${env_file}" -f "${deploy_dir}/compose.yml")

mkdir -p "${data_dir}" "${reference_dir}"
exec 9>"${data_dir}/.sync.lock"
if ! flock -n 9; then
  printf 'Another BDIFF synchronization is already running.\n' >&2
  exit 1
fi

available_kib="$(df -Pk "${data_dir}" | awk 'NR == 2 {print $4}')"
if (( available_kib < minimum_free_kib )); then
  printf 'At least %s KiB free is required before BDIFF synchronization.\n' "${minimum_free_kib}" >&2
  exit 1
fi

temporary_dir="$(mktemp -d "${data_dir}/.sync.XXXXXX")"
trap 'rm -rf "${temporary_dir}"' EXIT
cookie_jar="${temporary_dir}/cookies.txt"
selection_page="${temporary_dir}/selection.html"
raw_archive="${temporary_dir}/bdiff.zip"
raw_export="${temporary_dir}/Incendies.csv"
normalized_export="${temporary_dir}/france.csv"
system_ca_bundle="${SSL_CERT_FILE:-/etc/ssl/certs/ca-certificates.crt}"
bdiff_intermediate="${deploy_dir}/certs/HARICA-GEANT-TLS-R1.pem"
bdiff_ca_bundle="${temporary_dir}/bdiff-ca-bundle.pem"

if [[ ! -r "${system_ca_bundle}" ]] || [[ ! -r "${bdiff_intermediate}" ]]; then
  printf 'Missing CA bundle required for verified BDIFF HTTPS access.\n' >&2
  exit 1
fi
cat "${system_ca_bundle}" "${bdiff_intermediate}" >"${bdiff_ca_bundle}"

if [[ ! -s "${communes}" ]] || [[ -n "$(find "${communes}" -mtime +30 -print -quit 2>/dev/null)" ]]; then
  communes_next="${temporary_dir}/commune-centres.json"
  curl --fail --location --retry 3 --compressed --silent --show-error \
    "${communes_url}" --output "${communes_next}"
  python3 -c 'import json,sys; data=json.load(open(sys.argv[1])); assert len(data) > 30000' "${communes_next}"
  mv "${communes_next}" "${communes}"
  chmod 0644 "${communes}"
fi

for command in curl flock python3; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "${command}" >&2
    exit 1
  fi
done

curl --fail --location --retry 3 --compressed --silent --show-error \
  --cacert "${bdiff_ca_bundle}" \
  --cookie-jar "${cookie_jar}" --cookie "${cookie_jar}" --get \
  --data-urlencode "if[dateAlerteDeb][date]=01/01/${from_year}" \
  --data-urlencode 'if[dateAlerteDeb][time][hour]=0' \
  --data-urlencode 'if[dateAlerteDeb][time][minute]=0' \
  --data-urlencode "if[dateAlerteFin][date]=31/12/${to_year}" \
  --data-urlencode 'if[dateAlerteFin][time][hour]=23' \
  --data-urlencode 'if[dateAlerteFin][time][minute]=59' \
  --data-urlencode 'if[zone]=13' \
  --data-urlencode 'if[submit]=' \
  "${search_url}" --output "${selection_page}"

curl --fail --location --retry 3 --compressed --silent --show-error \
  --cacert "${bdiff_ca_bundle}" \
  --cookie "${cookie_jar}" --referer "${search_url}" \
  "${export_url}" --output "${raw_archive}"
archive_bytes="$(wc -c <"${raw_archive}" | tr -d ' ')"
if (( archive_bytes == 0 || archive_bytes > maximum_bytes )); then
  printf 'Unexpected BDIFF archive size: %s bytes.\n' "${archive_bytes}" >&2
  exit 1
fi
python3 - "${raw_archive}" "${raw_export}" <<'PY'
import sys
import zipfile

with zipfile.ZipFile(sys.argv[1]) as archive:
    with archive.open("Incendies.csv") as source, open(sys.argv[2], "wb") as destination:
        destination.write(source.read())
PY
if [[ ! -s "${raw_export}" ]]; then
  printf 'The BDIFF archive does not contain a usable Incendies.csv file.\n' >&2
  exit 1
fi

python3 "${deploy_dir}/normalize-bdiff.py" \
  --input "${raw_export}" \
  --communes "${communes}" \
  --output "${normalized_export}" \
  --max-reject-ratio "${maximum_reject_ratio}"
rows="$(( $(wc -l <"${normalized_export}") - 1 ))"
if (( rows < minimum_rows )); then
  printf 'Only %s normalized rows; refusing to replace the current national export.\n' "${rows}" >&2
  exit 1
fi

mv "${normalized_export}" "${target}"
chmod 0644 "${target}"
"${compose[@]}" run --rm app load-fire-history --source bdiff
validation_year="${to_year}"
training_to_year="$((validation_year - 1))"
if (( training_to_year >= from_year )); then
  "${compose[@]}" run --rm app train-human-model \
    --train-from "${from_year}-01-01" \
    --train-to "${training_to_year}-12-31" \
    --validation-from "${validation_year}-01-01" \
    --validation-to "${validation_year}-12-31" \
    --negatives-per-positive "${negatives_per_positive}"
fi

printf 'BDIFF synchronization complete: %s-%s, %s rows, %s bytes retained at %s\n' \
  "${from_year}" "${to_year}" "${rows}" "$(wc -c <"${target}" | tr -d ' ')" "${target}"
