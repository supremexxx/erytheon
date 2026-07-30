# Phase 4A.5b — Rapport de déploiement contrôlé (partiel)

Date : 30 juillet 2026
Statut global : **PARTIELLEMENT EXÉCUTÉ** — voir §0.

## 0. Limite d'exécution déclarée en préalable

Cette session ne dispose d'**aucun accès SSH ni identifiant** vers le VPS Oracle de production
(`deploy/oracle/README.md` : instance ARM `VM.Standard.A1.Flex`, volume de démarrage 150 Go). Le
workflow GitHub `container.yml` construit et publie l'image sur GHCR mais **ne déploie jamais
automatiquement** — le déploiement réel reste une opération manuelle par SSH décrite dans le
README.

En conséquence, cette intervention a exécuté tout ce qui est vérifiable depuis GitHub/l'environnement
local (fusion, CI, préparation), et **n'a pas** exécuté les étapes nécessitant un accès direct au
VPS (sauvegarde PostgreSQL réelle, application des migrations en production, recréation du
conteneur applicatif, vérification du domaine public réel, mesure disque réelle du VPS). Aucune de
ces étapes n'a été simulée ou fabriquée. Le §B fournit un runbook exact pour leur exécution par
quiconque dispose de l'accès SSH.

**Aucun tag `v0.4.4` ni release GitHub n'a été créé** dans cette intervention : les créer
maintenant équivaudrait à déclarer un déploiement production qui n'a pas eu lieu, ce que
l'ensemble de cette phase interdit explicitement (« ne jamais fabriquer »). Le §37/38 de la
commande de phase seront exécutés séparément, après la validation réelle décrite au §B.

## A. Ce qui a été exécuté et vérifié dans cette session

### A.1 Audit GitHub initial

- `git fetch --all --tags --prune` exécuté ; aucun tag `v0.4.4*` préexistant.
- État de la production connu et non re-vérifié en direct (voir §0) :
  révision applicative déclarée `6d91959` (`v0.4.3-app`), clôture documentaire `b1d18f6`
  (`v0.4.3`).
- Branche Phase 4A.5 : `agent/phase4a5-automated-observability-snapshots`, un seul commit
  `b74feab`.

### A.2 Revue de PR

PR #8 — `Phase 4A.5 — Automate operational observability and scientific snapshots` :

```text
state: OPEN → MERGED
base: main @ b1d18f635287f952ea5d8a792de5b76c1fa3649e (= v0.4.3)
head: agent/phase4a5-automated-observability-snapshots @ b74feab2a59fd1dcb6d4b81dddc2d97b39e7274a
mergeStateStatus: CLEAN
mergeable: MERGEABLE
statusCheckRollup: 2/2 SUCCESS (CI)
reviews: 0 (aucune review requise dans ce dépôt)
changedFiles: 30 (+3704 / -23)
```

Modifications classées :

| Catégorie | Fichiers |
|---|---|
| Migrations | `migrations/0018-0021_*.sql` + 4 rollbacks + correction de `migrations/rollback/0013_feature_snapshot_foundation.down.sql` |
| Store | `crates/store/src/observability.rs` (nouveau), `crates/store/src/lib.rs`, `crates/store/Cargo.toml` |
| Scheduler/CLI | `crates/engine/src/scheduler.rs`, `crates/engine/src/snapshot_pipeline.rs` (nouveau), `crates/engine/src/main.rs` |
| API | `crates/api/src/science.rs` |
| Console | `crates/api/static/science/index.html`, `crates/api/static/science/science.js` |
| Tests | `crates/store/tests/observability.rs` (nouveau), `crates/store/tests/rollback_guard_safety.rs`, `crates/api/tests/science.rs` |
| Documentation | 8 fichiers `PHASE4A5_*`/`AUTOMATED_CANDIDATE_*`/`SNAPSHOT_OPERATIONS_*` |

Confirmé : **aucune modification** dans `crates/risk`, `crates/fwi`, le moteur de scoring, les
modèles, le serving candidat, `deploy/oracle/Caddyfile`, ou les pipelines FIRMS/Open-Meteo
existants (`crates/ingest/src/firms.rs`, `open_meteo.rs` non touchés — seul
`crates/engine/src/scheduler.rs` a reçu l'ajout des trois nouvelles boucles, sans modification des
boucles `poll_firms`/`poll_forecast` existantes).

### A.3 Régression de rollback détectée et corrigée

Confirmé dans le code final : la migration `0019` référence `features.feature_snapshots` via une
FK sur `static_snapshot_id`. Le rollback de `0013` (qui `DROP TABLE features.feature_snapshots`)
a été mis à jour pour refuser explicitement tant que `observability.scientific_snapshots` existe
(`migrations/rollback/0013_feature_snapshot_foundation.down.sql`) — vérifié par le test
`rollback_0013_refuses_destructively_once_a_snapshot_exists`
(`crates/store/tests/rollback_guard_safety.rs`), qui roule désormais 0021→0020→0019→0015 avant
0013.

### A.4 Rejeu des migrations en environnement isolé

Exécuté contre PostgreSQL/PostGIS 16/3.4 jetable (conteneur Docker local, pas le VPS) :

- **Cas A (base vide)** : 0001→0021 appliquées, 21/21 réussies. `crates/store/tests/
  rollback_guard_safety.rs` : rollback 0021→0018 en ordre correct réussi ; ordre incorrect refusé
  avec message explicite (`rollback_0021_to_0018_succeeds_only_in_reverse_order_when_empty`).
- **Cas C (rollback peuplé)** : `rollback_0018_refuses_and_preserves_populated_system_snapshots`
  — un `system_snapshots` peuplé fait refuser 0018 (code retour non nul, aucune suppression
  partielle, donnée conservée).
- **Cas D (hors ordre)** : `rollback_0021_to_0018_succeeds_only_in_reverse_order_when_empty`
  inclut la vérification qu'un rollback 0018 avant 0019+ est refusé.
- **Cas B (base représentative)** n'a **pas** été exécuté avec les jeux réels de production
  (cellules statiques réelles, BDIFF réel, FIRMS réel, v1 actif réel) — cette session n'a accès
  qu'aux fixtures versionnées, pas à une copie de la base de production. À faire par l'opérateur
  VPS avant migration réelle (§B.3).

### A.5 CI finale sur `main`

- Fusion effectuée par **merge commit** (pas de squash, pas de rebase, pas de force-push) :
  `gh pr merge 8 --merge`.
- `FINAL_MAIN_SHA = 11b001525fff40ac35f677df950849331b65a039`.
- Workflow `CI` sur ce SHA : **SUCCESS** (`cargo fmt --all -- --check`, `cargo clippy --workspace
  --all-targets --all-features --locked -- -D warnings`, `cargo test --workspace --locked
  --no-fail-fast`, tous verts en CI GitHub).
- Workflow `Container` sur ce SHA : voir §A.6.

### A.6 Construction d'image

Le workflow `Container` (`.github/workflows/container.yml`) construit et publie l'image via
`docker/build-push-action` sur GHCR.

```text
run: https://github.com/supremexxx/erytheon/actions/runs/30502961971
conclusion: success
started: 2026-07-30T00:32:04Z
completed: 2026-07-30T02:44:16Z (~2h12 — cross-compile ARM64 sous QEMU)
image tags: ghcr.io/supremexxx/erytheon:latest
            ghcr.io/supremexxx/erytheon:sha-11b0015
image digest: sha256:ac6a852560f43bb4fc9e188605923c6918a0692e411858ae2b67b55955522fa7
org.opencontainers.image.revision: 11b001525fff40ac35f677df950849331b65a039
```

Note : le run précédent sur `b1d18f6` (avant cette PR, avant toute modification de cette phase)
avait `conclusion: failure` — un opérateur VPS devra vérifier au démarrage du conteneur que
l'image `sha-11b0015` fonctionne réellement, cette réussite de build ne prouve pas encore un
comportement correct à l'exécution en production.

Cette session ne peut pas pousser une image vers un registre différent ni la déployer sur le
VPS — seul GitHub Actions construit et publie l'image ; le pull et le démarrage sur le VPS
restent manuels (§B.6-B.7).

## B. Runbook — étapes restantes (accès SSH requis)

Les étapes ci-dessous doivent être exécutées par un opérateur disposant d'un accès SSH au VPS
Oracle, dans l'ordre, sans en sauter aucune. Chaque commande de vérification doit produire un
résultat réel avant de passer à la suivante.

### B.1 Contrôle de volumétrie VPS (§11 de la commande)

```sh
ssh <host>
df -h
df -i
docker system df
du -sh /var/lib/docker/volumes/*/  # ou le volume PostgreSQL réel
```

Relever espace total/utilisé/libre, inodes, taille PostgreSQL, taille backups, taille images
Docker. Le volume de démarrage documenté est de **150 Go** (`deploy/oracle/README.md`). Ne pas
déployer si l'espace libre après l'estimation du pilote scientifique (~10 Go/an, voir
`PHASE4A5_SNAPSHOT_ARCHITECTURE_DECISION.md`) tombe sous 20-25 % de marge.

### B.2 Sauvegarde PostgreSQL obligatoire

```sh
DUMP=erytheon-pre-phase4a5-$(date -u +%Y%m%d-%H%M%S).dump
docker exec <postgres_container> pg_dump -U pyrorisk -Fc pyrorisk > "$DUMP"
sha256sum "$DUMP" > "$DUMP.sha256"
sha256sum -c "$DUMP.sha256"
pg_restore --list "$DUMP" | head -50
```

Ne poursuivre qu'après vérification réussie du checksum et du catalogue `pg_restore --list`.
Conserver tous les backups précédents (ne rien supprimer).

### B.3 Test sur copie représentative avant migration réelle (Cas B, §8)

```sh
# Sur une base restaurée depuis le dump ci-dessus, PAS la production :
pg_restore -d erytheon_migration_rehearsal "$DUMP"
DATABASE_URL=postgres://.../erytheon_migration_rehearsal cargo run -p engine -- run &
# vérifier 21/21 migrations, v1 actif inchangé, candidat inchangé, tables de snapshots vides
```

### B.4 État pré-déploiement (§14)

Capturer et archiver (sans secret) : révision applicative actuelle, container ID, uptime, restart
count, health, migrations `17/0`, taille DB, espace disque, modèle actif, candidat, dernière
activité FIRMS/météo. Tester `/health`, `/risk`, `/alerts`, `/sources`, `/science`,
`/science/overview`, `/science/system`, `/api/science/overview` sur le domaine réel.

### B.5 Plan de rollback préparé (§15)

Conserver l'image actuellement déployée (tag/digest), la configuration de conteneur actuelle, le
backup validé (§B.2) et son checksum, avant toute bascule.

### B.6 Application des migrations 0018–0021 en production

L'image est déjà construite et publiée par CI (§A.6) — pas besoin de la reconstruire sur le VPS.

```sh
cd /opt/pyrorisk
# Utiliser l'image déjà publiée plutôt que de reconstruire sur le VPS :
#   ghcr.io/supremexxx/erytheon:sha-11b0015
#   digest sha256:ac6a852560f43bb4fc9e188605923c6918a0692e411858ae2b67b55955522fa7
sed -i 's#^PYRORISK_IMAGE=.*#PYRORISK_IMAGE=ghcr.io/supremexxx/erytheon:sha-11b0015#' deploy/oracle/.env
docker compose -f deploy/oracle/compose.yml pull app
docker compose -f deploy/oracle/compose.yml run --rm app cargo run -p engine -- run
# Les migrations sont appliquées au démarrage de l'application (sqlx::migrate! au boot,
# voir crates/store/src/lib.rs::Store::connect) -- surveiller les logs de ce run/premier
# démarrage plutôt qu'exécuter une commande de migration séparée.
```

Vérifier `21 migrations réussies, 0 échec`, aucune table historique modifiée, v1 toujours actif,
candidat toujours `inactive`, aucune ligne de snapshot avant le premier déclenchement. **Si une
migration échoue : STOP, ne pas démarrer la nouvelle application, ne pas improviser de SQL
destructif.**

### B.7 Déploiement applicatif (§18)

Recréer uniquement le conteneur applicatif (jamais PostgreSQL, jamais Caddy), avec l'image
`ghcr.io/supremexxx/erytheon:sha-11b0015` déjà publiée (§A.6) :

```sh
docker compose -f deploy/oracle/compose.yml up -d --no-deps app
docker compose -f deploy/oracle/compose.yml logs -f app
```

Attendre `application healthy`, `restart count = 0`. Vérifier dans les logs : révision
`11b001525fff40ac35f677df950849331b65a039`, migrations au niveau 21, scheduler initialisé
(les trois nouvelles boucles `snapshot_operational_hourly/daily`, `snapshot_scientific_weekly`
doivent apparaître au démarrage), aucune erreur SQL, aucun panic, v1 chargé normalement.

### B.8 Validation manuelle avant activation des cadences (§19–§21)

```sh
erytheon snapshot-operational --cadence daily
# noter : id, capture_date, checksum
erytheon snapshot-operational --cadence daily   # rejeu : même id, même checksum attendu
erytheon snapshot-compare --days 1,7
erytheon snapshot-scientific --date $(date -u +%F)
erytheon snapshot-verify --id <uuid>
erytheon snapshot-scientific --date $(date -u +%F)   # rejeu idempotent attendu
erytheon snapshot-retention   # dry-run uniquement, aucune suppression attendue
```

Si un doublon incohérent apparaît côté opérationnel : **désactiver les jobs de snapshot, ne pas
poursuivre vers le pilote scientifique**. Si le rejeu scientifique réécrit une copie complète sans
justification : **désactiver le job scientifique, conserver le job opérationnel**.

### B.9 Validation des endpoints et de la console (§29–§32)

```sh
curl -u <basic-auth> https://<domain>/api/science/observability/latest
curl -u <basic-auth> https://<domain>/api/science/observability/history
curl -u <basic-auth> https://<domain>/api/science/observability/compare?days=1,7
curl -u <basic-auth> https://<domain>/api/science/snapshots
curl -u <basic-auth> https://<domain>/api/science/snapshot-alerts
```

Vérifier pagination, `days`/`severity` invalides → 400, id inconnu → 404, aucun secret exposé.
Ouvrir `/science/observability` en navigateur réel (desktop + mobile), confirmer l'honnêteté du
premier jour (« J-1 indisponible », « J-7 indisponible », pas de courbe fabriquée).

### B.10 Non-régression opérationnelle (§32)

Comparer `/config`, `/risk`, `/risk/cell/{h3}`, `/alerts`, `/health`, `/sources`, `/stream` avant/
après. Mêmes scores, mêmes horizons, même modèle v1, carte inchangée.

### B.11 Activation des cadences (§28)

N'activer qu'après B.8 et B.9 validés. Autoriser uniquement : horaire léger, quotidien 02:15 UTC,
hebdomadaire `nowcast`. Ne jamais activer : cadence scientifique quotidienne, multi-horizon,
rétention destructive.

### B.12 Tags et release (§37–§38) — **à faire en dernier, après tout ce qui précède**

```sh
git tag --list 'v0.4.4*'   # doit être vide ; si un tag existe déjà, STOP, ne pas le déplacer
git tag -a v0.4.4-app <révision réellement déployée> -m "ERYTHEON v0.4.4 application revision"
git tag -a v0.4.4 <SHA> -m "ERYTHEON v0.4.4"
git push origin v0.4.4-app v0.4.4
gh release create v0.4.4 --title "ERYTHEON v0.4.4 — Automated observability and scientific snapshots" --notes-file <notes>
```

Ne créer ces tags qu'après validation réelle en production (B.1–B.11), jamais avant.

## C. Critères de réussite non encore confirmés

Faute d'accès VPS dans cette session, les points suivants du §40 de la commande restent **non
confirmés** (ni infirmés) : sauvegarde validée en conditions réelles, 21 migrations en production,
application healthy en production, snapshot opérationnel réel créé et idempotent, snapshot
scientifique réel créé, projection de volumétrie réelle mesurée, cadences activées en production,
cartes/console validées sur le domaine réel, v0.4.4 publiée.

Confirmés : PR fusionnée proprement, CI `main` verte, migrations et rollbacks validés en
environnement isolé, régression de rollback détectée et corrigée, aucune modification hors
périmètre.
