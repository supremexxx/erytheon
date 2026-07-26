#!/usr/bin/env bash
set -Eeuo pipefail

deploy_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
project_dir="$(cd -- "${deploy_dir}/../.." && pwd)"
env_file="${1:-${deploy_dir}/.env}"
backup_dir="${BACKUP_DIR:-${project_dir}/backups}"

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

postgres_db="$(env_file_value POSTGRES_DB)"
postgres_user="$(env_file_value POSTGRES_USER)"
if [[ -z "${postgres_db}" || -z "${postgres_user}" ]]; then
  printf 'POSTGRES_DB and POSTGRES_USER must be configured.\n' >&2
  exit 1
fi

compose=(docker compose --env-file "${env_file}" -f "${deploy_dir}/compose.yml")
mkdir -p "${backup_dir}"
umask 077

timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
backup_path="${backup_dir}/pyrorisk-${timestamp}.dump"
partial_path="${backup_path}.partial"
checksum_path="${backup_path}.sha256"
trap 'rm -f -- "${partial_path}"' EXIT

"${compose[@]}" exec -T postgres \
  pg_dump --username "${postgres_user}" --dbname "${postgres_db}" \
  --format custom --compress=9 >"${partial_path}"

"${compose[@]}" exec -T postgres pg_restore --list <"${partial_path}" >/dev/null
mv -- "${partial_path}" "${backup_path}"
(
  cd -- "${backup_dir}"
  sha256sum "$(basename -- "${backup_path}")" >"$(basename -- "${checksum_path}")"
  sha256sum --check "$(basename -- "${checksum_path}")" >/dev/null
)

find "${backup_dir}" -type f -name 'pyrorisk-*.dump' -mtime +7 -delete
find "${backup_dir}" -type f -name 'pyrorisk-*.dump.sha256' -mtime +7 -delete
printf 'Backup created: %s\n' "${backup_path}"
printf 'Checksum created: %s\n' "${checksum_path}"
