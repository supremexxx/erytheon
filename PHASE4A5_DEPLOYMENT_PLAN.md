# Phase 4A.5 — Plan de déploiement (non exécuté)

Conformément au §42 de la commande de phase, cette intervention livre code, tests, migrations,
rapports et PR — **pas de déploiement**. Ce document décrit le plan pour une intervention séparée,
autorisée explicitement.

## 1. Pré-requis avant tout déploiement

- [ ] PR revue et CI verte sur la tête exacte à déployer (`cargo fmt --all -- --check`,
      `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`,
      `cargo test --workspace --locked --no-fail-fast`).
- [ ] Sauvegarde PostgreSQL vérifiée (voir `deploy/oracle/backup-r2.sh` / `backup-local.sh`).
- [ ] Vérification de l'état production réel (santé application, PostgreSQL, Caddy, 17 → 21
      migrations, v1 actif, candidat `inactive`) — ne pas supposer l'état déclaré par une commande
      ou un rapport antérieur sans le revérifier au moment du déploiement.
- [ ] Confirmation que les nouvelles migrations (0018–0021) s'appliquent proprement sur une copie
      de la base de production (ou une base jetable restaurée depuis la dernière sauvegarde).

## 2. Ordre de déploiement

1. Construire et publier l'image applicative (sans changer Caddy ni PostgreSQL).
2. Appliquer les migrations 0018–0021 (additives, testées vides et peuplées, rollbacks vérifiés
   dans les deux états — voir `crates/store/tests/rollback_guard_safety.rs`).
3. Démarrer l'application : le scheduler lance automatiquement
   `snapshot_operational_hourly`, `snapshot_operational_daily` (02:15 UTC) et
   `snapshot_scientific_weekly` (lundi 03:00 UTC) — voir `crates/engine/src/scheduler.rs`.
4. Vérifier `GET /api/science/observability/latest` répond 404 initialement (aucun snapshot),
   puis 200 après la première capture horaire.
5. Vérifier manuellement une capture via
   `erytheon snapshot-operational --cadence daily` avant d'attendre le premier déclenchement
   planifié, pour valider le chemin de bout en bout sur l'environnement réel.
6. Surveiller `observability.snapshot_alerts` pendant au moins 48 h avant de considérer la phase
   stable.

## 3. Rollback applicatif

Revenir à l'image précédente (`v0.4.3-app` ou la révision alors en production) sans exécuter les
migrations `.down.sql` de cette phase : les nouvelles tables sont additives et n'interfèrent avec
aucun chemin de lecture/écriture existant si l'application revient à une version qui les ignore.
Un rollback de schéma (migrations 0021 → 0018) n'est nécessaire que si les tables elles-mêmes
doivent disparaître, et suit l'ordre strict inverse déjà testé (§ voir
`migrations/rollback/00{18,19,20,21}_*.down.sql`) — chaque script refuse s'il existe des données
ou si un migration ultérieure dépendante n'a pas déjà été annulée.

## 4. Ce qui n'est pas déployé par cette phase

- Aucune activation du candidat, aucun shadow scoring, aucun changement du modèle v1.
- Aucune modification de Caddy, de l'authentification (inexistante) ou de `SCIENCE_CONSOLE_ENABLED`.
- Aucune suppression automatique (voir `PHASE4A5_RETENTION_POLICY.md`).

## 5. Point d'arrêt

Après la création de la PR (`agent/phase4a5-automated-observability-snapshots`) et la validation
isolée décrite dans ce dépôt, l'intervention s'arrête. Le déploiement décrit ci-dessus reste une
décision et une action séparées, à autoriser explicitement.
