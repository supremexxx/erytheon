#!/usr/bin/env bash
set -Eeuo pipefail

if [[ "${1:-}" != "--yes" ]]; then
  printf 'This replaces the current database. Re-run with --yes to continue.\n' >&2
  exit 1
fi

deploy_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
env_file="${2:-${deploy_dir}/.env}"

if [[ ! -f "${env_file}" ]]; then
  printf 'Missing environment file: %s\n' "${env_file}" >&2
  exit 1
fi

set -a
source "${env_file}"
set +a

required=(POSTGRES_DB POSTGRES_USER POSTGRES_PASSWORD R2_ENDPOINT R2_BUCKET AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY)
for variable in "${required[@]}"; do
  if [[ -z "${!variable:-}" ]]; then
    printf 'Missing required variable: %s\n' "${variable}" >&2
    exit 1
  fi
done

umask 077
temporary_dir="$(mktemp -d)"
trap 'rm -rf "${temporary_dir}"' EXIT
archive="${temporary_dir}/pyrorisk.dump"
compose=(docker compose --env-file "${env_file}" -f "${deploy_dir}/compose.yml")

docker run --rm \
  --env-file "${env_file}" \
  --volume "${temporary_dir}:/backup" \
  amazon/aws-cli:latest \
  --endpoint-url "${R2_ENDPOINT}" \
  s3 cp "s3://${R2_BUCKET}/backups/latest.dump" /backup/pyrorisk.dump \
  --only-show-errors

"${compose[@]}" stop app
restart_app=true
trap 'rm -rf "${temporary_dir}"; if [[ "${restart_app}" == true ]]; then "${compose[@]}" start app; fi' EXIT

"${compose[@]}" exec -T \
  -e PGPASSWORD="${POSTGRES_PASSWORD}" \
  postgres pg_restore \
  --username "${POSTGRES_USER}" \
  --dbname "${POSTGRES_DB}" \
  --clean \
  --if-exists \
  --no-owner <"${archive}"

"${compose[@]}" start app
restart_app=false
printf 'Database restored from s3://%s/backups/latest.dump\n' "${R2_BUCKET}"
