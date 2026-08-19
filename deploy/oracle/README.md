# Free Oracle deployment

This stack deploys FireSift on one Oracle Cloud Always Free ARM instance. It keeps PostgreSQL private, exposes only Caddy on ports 80/443, runs the live FIRMS and weather scheduler, and optionally uploads one rolling PostgreSQL backup to Cloudflare R2.

> The project's public name is FireSift, but this guide's variable names,
> paths, and service/user names (`pyrorisk`, `PYRORISK_*`, `/opt/pyrorisk`,
> `ERYTHEON_*` runtime variables) intentionally still use the project's
> original internal identifiers. They match what is actually deployed and
> are retained for backward compatibility — see the root
> [`OPEN_SOURCE_READINESS_REPORT.md`](../../OPEN_SOURCE_READINESS_REPORT.md)
> and `CHANGELOG.md` for why.

## 1. Create the VM

Create an Always Free Ubuntu ARM instance with the available free allocation:

- shape: `VM.Standard.A1.Flex`;
- CPU and memory: 2 OCPUs and 12 GB;
- boot volume: 150 GB;
- public IPv4 address: enabled.

In both the Oracle network security list and the instance firewall, allow inbound TCP 22, 80, and 443 plus UDP 443. Keep PostgreSQL port 5432 closed.

## 2. Install Docker

Connect over SSH, then install Docker and the Compose plugin:

```sh
sudo apt-get update
sudo apt-get install -y ca-certificates curl git
curl -fsSL https://get.docker.com | sudo sh
sudo usermod -aG docker "$USER"
```

Log out and reconnect so the Docker group takes effect. Verify the installation:

```sh
docker version
docker compose version
```

## 3. Configure PyroRisk

Clone the repository into the path used by the backup timer:

```sh
sudo mkdir -p /opt/pyrorisk
sudo chown "$USER":"$USER" /opt/pyrorisk
git clone YOUR_REPOSITORY_URL /opt/pyrorisk
cd /opt/pyrorisk/deploy/oracle
cp .env.example .env
chmod 600 .env
```

Edit `.env` and set at least:

- `PYRORISK_IMAGE` to the GHCR image published by `.github/workflows/container.yml`;
- `POSTGRES_PASSWORD` to the output of `openssl rand -hex 32`;
- `FIRMS_MAP_KEY` to the NASA key;
- `PYRORISK_DOMAIN` to `:80` for initial access by IP, or to a DNS name for automatic HTTPS.

The image package must be public for an anonymous pull. For a private package, run `docker login ghcr.io` once with a GitHub token that has `read:packages`.

The initial deployment intentionally keeps the validated Aude AOI and fixture static layers. Live FIRMS and AROME/ARPEGE data are still fetched. Phase 9B supports France, but activation must wait until the real national static files are present; never publish the Aude fixtures as a France surface.

Prepare the official metropolitan department boundary:

```sh
cd /opt/pyrorisk
./deploy/oracle/fetch-france-boundaries.sh
cd deploy/oracle
docker compose --env-file .env -f compose.yml run --rm \
  -e TERRITORY_GEOJSON_PATH=/data/boundaries/departements-1000m.geojson \
  -e TERRITORY_CODES= \
  -e H3_RESOLUTION=8 app territory-plan
```

Download and pre-aggregate the 22 regional Geofabrik extracts without changing the running Aude service:

```sh
cd /opt/pyrorisk
./deploy/oracle/fetch-france-osm-regions.sh
cd deploy/oracle
set -a; . ./.env; set +a
docker run --rm --user "$(id -u):$(id -g)" \
  --volume /opt/pyrorisk/data:/data:rw \
  --env OSM_PATH=/data/osm/regions \
  --env AOI_BBOX=-5.15,41.31,9.57,51.09 \
  --env H3_RESOLUTION=8 \
  "${PYRORISK_IMAGE}" osm-aggregate \
  --output /data/osm/france-h3-r8.jsonl
```

Set `OSM_REGIONS=corse` before the download command to validate only the smallest extract. After installing every other national static file, copy the France values from `.env.production.example`: set `DATA_PROFILE=production`, `OSM_PATH=/data/osm/france-h3-r8.jsonl`, `TERRITORY_GEOJSON_PATH=/data/boundaries/departements-1000m.geojson`, `TERRITORY_LABEL=France métropolitaine`, the national bbox, and H3 resolution 8. Use `TERRITORY_CODES=11,34` with a matching smaller `AOI_BBOX` for a controlled partial import before removing the filter.

## 4. Start and inspect

```sh
cd /opt/pyrorisk/deploy/oracle
docker compose --env-file .env -f compose.yml pull
docker compose --env-file .env -f compose.yml up -d
docker compose --env-file .env -f compose.yml ps
docker compose --env-file .env -f compose.yml logs -f app
```

The application applies its migrations automatically. Its first forecast normally starts immediately. Verify it through Caddy:

```sh
curl http://YOUR_SERVER_IP/health
```

Operational forecasts use the public ECMWF IFS 0.25-degree open-data service directly, without
an account or API key. The application downloads only the required GRIB byte ranges, decodes them
inside the container, and keeps the derived grids under `WEATHER_CACHE_DIR` (default:
`/app/out/weather`). Open-Meteo AROME and ECMWF remain bounded fallbacks if the direct ECMWF
acquisition fails. A complete forecast batch is published atomically; the preceding complete batch
remains served if every provider fails.

When `PYRORISK_DOMAIN` contains a DNS name that resolves to the VM, Caddy obtains and renews the TLS certificate automatically; use `https://YOUR_DOMAIN/health` instead.

## 5. Load real static files

Copy production source files under `/opt/pyrorisk/data` using the paths declared in `compose.yml`. Then set `DATA_PROFILE=production` and run:

```sh
cd /opt/pyrorisk/deploy/oracle
docker compose --env-file .env -f compose.yml run --rm app data-status
docker compose --env-file .env -f compose.yml run --rm app load-static
docker compose --env-file .env -f compose.yml run --rm app data-status
docker compose --env-file .env -f compose.yml restart app
```

Production mode rejects missing files and fixture paths rather than silently publishing incomplete static data.

## 6. Enable daily local backups

Install the daily PostgreSQL backup timer. Dumps are retained for seven days in `/opt/pyrorisk/backups`.

The deployment account must own the environment file because both the deployment script and the
backup service run as `pyrorisk`. Keep it private to that account:

```sh
sudo chown pyrorisk:pyrorisk /opt/pyrorisk/deploy/oracle/.env
sudo chmod 600 /opt/pyrorisk/deploy/oracle/.env
```

`backup-local.sh` deliberately reads only `POSTGRES_DB` and `POSTGRES_USER` from `.env`; it does not source the file as shell code. This allows ordinary configuration values to contain spaces or accented characters without executing them.

For each run, the script:

1. writes a custom-format dump to a temporary `.partial` file;
2. validates its catalogue with `pg_restore --list`;
3. atomically renames it to `pyrorisk-<UTC timestamp>.dump`;
4. creates `pyrorisk-<UTC timestamp>.dump.sha256`;
5. validates that checksum before reporting success.

```sh
sudo cp systemd/pyrorisk-local-backup.service systemd/pyrorisk-local-backup.timer /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now pyrorisk-local-backup.timer
sudo systemctl start pyrorisk-local-backup.service
systemctl list-timers pyrorisk-local-backup.timer
```

Verify the latest archive independently:

```sh
cd /opt/pyrorisk/backups
sha256sum --check pyrorisk-YYYYMMDDTHHMMSSZ.dump.sha256
docker compose --env-file ../deploy/oracle/.env \
  -f ../deploy/oracle/compose.yml exec -T postgres \
  pg_restore --list <pyrorisk-YYYYMMDDTHHMMSSZ.dump >/dev/null
```

Local dumps protect against database mistakes but not against total VPS loss. Keep provider snapshots enabled or add the off-site backup below.

### Isolated restoration drill

Never test a restoration over the production database. Use a separate PostgreSQL container, a separate Docker volume, no published port, and a target database created from `template0`. The PostGIS image automatically adds extensions to its bootstrap database; restoring into that bootstrap database would conflict with the PostGIS schemas present in the dump.

The validated sequence is:

```text
create isolated Docker volume
→ start postgis/postgis:16-3.4 without a published port
→ wait until "PostgreSQL init process complete"
→ create the target database from template0
→ pg_restore --no-owner --exit-on-error
→ compare extensions, migrations, schema, constraints, indexes,
  exact row counts, time ranges and deterministic samples
```

Keep the restored container and volume until the restoration report has been reviewed. Remove them only with explicit operator approval. The production application does not need to be stopped for this drill.

### Production replacement restore

Restoring over production is a separate destructive operation and was not performed during the phase 0 drill. If it becomes necessary:

1. confirm the selected dump and SHA-256;
2. record current database and API health;
3. stop only the application container to prevent writes;
4. restore with `--clean --if-exists --no-owner`;
5. rerun the logical verification;
6. restart the application and verify `/health`, `/risk`, `/alerts` and the dashboard.

The existing `restore-r2.sh --yes` follows the stop/restore/start pattern for an R2 archive. A local equivalent must retain the same explicit confirmation requirement.

## 6A. Synchronize BDIFF without using workstation storage

No manual download is required. The synchronizer submits the public BDIFF search for metropolitan France, downloads the official ZIP with the same temporary session, extracts `Incendies.csv`, joins municipality centres from the official Geo API, atomically replaces one normalized CSV, imports it into PostgreSQL, refreshes only the `hist` feature, and deletes the raw archive. The default interval starts in 2020 because BDIFF warns that older non-Mediterranean records can be less exhaustive; the final year is automatically the last closed campaign.

The import does not publish partial department risk batches. Refreshed history features become visible only when the normal scheduler completes and atomically publishes the next full France forecast.

The portal currently serves an incorrect TLS intermediate chain to some Linux clients. The deployment includes the matching public HARICA/GEANT intermediate and appends it to Ubuntu's trusted root bundle for this download only; certificate verification is never disabled.

Rows attached to dissolved historical communes are excluded when the current official commune reference cannot provide an unambiguous centre. The synchronization aborts if exclusions exceed `BDIFF_MAX_REJECT_RATIO` (1% by default), preventing a damaged reference join from silently replacing valid data.

Test one synchronization:

```sh
cd /opt/pyrorisk
./deploy/oracle/sync-fire-history.sh
```

Install the monthly timer after the test succeeds:

```sh
sudo cp deploy/oracle/systemd/pyrorisk-fire-history.service deploy/oracle/systemd/pyrorisk-fire-history.timer /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now pyrorisk-fire-history.timer
systemctl list-timers pyrorisk-fire-history.timer
```

Only `/opt/pyrorisk/data/bdiff/france.csv`, the reusable commune-centre reference, and the normalized PostgreSQL rows are retained. `BDIFF_MIN_ROWS`, `BDIFF_MAX_BYTES`, and `BDIFF_MIN_FREE_KIB` prevent a truncated response or low-disk condition from replacing the current dataset.

## 7. Configure the free R2 backup

Create one private R2 bucket and an API token limited to object read/write for that bucket. Fill `R2_ENDPOINT`, `R2_BUCKET`, `AWS_ACCESS_KEY_ID`, and `AWS_SECRET_ACCESS_KEY` in `.env`.

Test one backup:

```sh
cd /opt/pyrorisk/deploy/oracle
./backup-r2.sh
```

The script writes only `backups/latest.dump`, replacing the previous object, and refuses an archive larger than 9 GiB. Install the daily systemd timer:

```sh
sudo cp systemd/pyrorisk-backup.service systemd/pyrorisk-backup.timer /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now pyrorisk-backup.timer
systemctl list-timers pyrorisk-backup.timer
```

Restore only when required:

```sh
./restore-r2.sh --yes
```

The restore command stops the application during replacement and starts it again afterward.

## 8. Update

After a new container is published:

```sh
cd /opt/pyrorisk
git pull --ff-only
cd deploy/oracle
docker compose --env-file .env -f compose.yml pull app
docker compose --env-file .env -f compose.yml up -d app caddy
docker image prune -f
```

For a workstation-to-VPS deployment that never transfers local data, backups, build artifacts, or `.env`, run from the repository root:

```sh
PYRORISK_SSH_TARGET=pyrorisk@YOUR_SERVER_IP \
PYRORISK_SSH_KEY="$HOME/.ssh/YOUR_KEY" \
./deploy/oracle/deploy-code.sh
```

The image is built on the VPS and unused Docker images are pruned only after the application restarts successfully.

## Operational limits

- Keep provider snapshots or backups enabled in addition to the local database dumps.
- Keep only the latest operational risk batch in PostgreSQL and one rolling R2 backup.
- Do not expose ports 5432 or 8080 publicly.
- Monitor disk usage with `df -h` and Docker usage with `docker system df`.
- The France rollout should use H3 resolution 8 and department jobs; H3 resolution 9 remains a selective high-risk zoom layer.
