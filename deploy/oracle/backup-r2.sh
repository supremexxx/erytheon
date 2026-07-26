#!/usr/bin/env bash
set -Eeuo pipefail

deploy_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
env_file="${1:-${deploy_dir}/.env}"

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

"${compose[@]}" exec -T \
  -e PGPASSWORD="${POSTGRES_PASSWORD}" \
  postgres pg_dump \
  --username "${POSTGRES_USER}" \
  --dbname "${POSTGRES_DB}" \
  --format custom \
  --compress 9 \
  --no-owner \
  --file - >"${archive}"

archive_bytes="$(wc -c <"${archive}" | tr -d ' ')"
if (( archive_bytes > 9663676416 )); then
  printf 'Backup is larger than 9 GiB; refusing to exceed the R2 free storage budget.\n' >&2
  exit 1
fi

docker run --rm \
  --env-file "${env_file}" \
  --volume "${temporary_dir}:/backup:ro" \
  amazon/aws-cli:latest \
  --endpoint-url "${R2_ENDPOINT}" \
  s3 cp /backup/pyrorisk.dump "s3://${R2_BUCKET}/backups/latest.dump" \
  --only-show-errors

printf 'R2 backup uploaded: s3://%s/backups/latest.dump (%s bytes)\n' "${R2_BUCKET}" "${archive_bytes}"
