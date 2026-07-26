#!/usr/bin/env bash
set -Eeuo pipefail

: "${PYRORISK_SSH_TARGET:?Set PYRORISK_SSH_TARGET, for example pyrorisk@server.example}"

deploy_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
project_dir="$(cd -- "${deploy_dir}/../.." && pwd)"
remote_dir="${PYRORISK_REMOTE_DIR:-/opt/pyrorisk}"
ssh_options=(-o BatchMode=yes -o ConnectTimeout=15)
if [[ -n "${PYRORISK_SSH_KEY:-}" ]]; then
  ssh_options+=(-i "${PYRORISK_SSH_KEY}")
fi

rsync -az \
  -e "ssh ${ssh_options[*]}" \
  --exclude '.env' \
  --exclude '.git' \
  --exclude '.playwright-cli' \
  --exclude 'backups' \
  --exclude 'data' \
  --exclude 'out' \
  --exclude 'target' \
  "${project_dir}/" "${PYRORISK_SSH_TARGET}:${remote_dir}/"

ssh "${ssh_options[@]}" "${PYRORISK_SSH_TARGET}" bash -s -- "${remote_dir}" <<'REMOTE'
set -Eeuo pipefail
remote_dir="$1"
deploy_dir="${remote_dir}/deploy/oracle"
cd "${deploy_dir}"
image="$(awk -F= '$1 == "PYRORISK_IMAGE" {sub(/^[^=]*=/, ""); print; exit}' ./.env)"
if [[ "${image}" == \"*\" || "${image}" == \'*\' ]]; then
  image="${image:1:${#image}-2}"
fi
: "${image:?PYRORISK_IMAGE is missing from deploy/oracle/.env}"
docker build --tag "${image}" "${remote_dir}"
docker compose --env-file .env -f compose.yml up -d app caddy
docker image prune --force
docker compose --env-file .env -f compose.yml ps
REMOTE
