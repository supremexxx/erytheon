# Runbook — Snapshots d'observabilité et archive scientifique quotidienne

## 1. Cadence automatique

| Job | Fréquence | Fonction |
|---|---|---|
| `snapshot_operational_hourly` | toutes les heures | `crates/engine/src/scheduler.rs::snapshot_operational_hourly` |
| `snapshot_operational_daily` | 02:15 UTC | `crates/engine/src/scheduler.rs::snapshot_operational_daily` |
| archive scientifique `daily_dense` | premier nowcast complet de chaque date UTC | déclenchée à la fin de `poll_forecast` |

Les jobs démarrent automatiquement avec `pyrorisk run` (via `scheduler::spawn`). Une panne
de capture est journalisée (`tracing::error!`) et **ne fait jamais tomber l'application
principale** — la boucle continue au tick suivant.

L'archive scientifique contient les six composantes FWI de toutes les cellules modélisables,
rangées selon le masque H3 immuable. La première prévision complète de la journée gagne ; les
cycles suivants retrouvent le même snapshot sans dupliquer les données. Une source inconnue, une
prévision vieille de plus de six heures ou une cellule absente bloque la publication. Le pilote
hebdomadaire historique reste utilisable manuellement, mais n'est plus planifié automatiquement.

## 2. Commandes manuelles

```sh
# Capture opérationnelle immédiate
erytheon snapshot-operational --cadence daily

# Pré-requis scientifiques v2, dans cet ordre
erytheon snapshot-static-bundle
erytheon snapshot-coverage-mask

# Capture scientifique pilote pour une date donnée (idempotent)
erytheon snapshot-scientific --date 2026-07-30

# Vérifier qu'un snapshot publié est complet et immuable
erytheon snapshot-verify --id <uuid>

# Comparer J-1 et J-7
erytheon snapshot-compare --days 1,7

# Rapport de rétention (dry-run uniquement)
erytheon snapshot-retention

# Prévisualiser les liens BDIFF matures (aucune écriture par défaut)
erytheon snapshot-link-labels --snapshot-id <uuid>

# Écriture explicite seulement après revue du dry-run
erytheon snapshot-link-labels --snapshot-id <uuid> --apply
```

## 3. Diagnostic d'une alerte critique

1. Consulter `GET /api/science/snapshot-alerts?severity=critical` ou la console
   `/science/observability`.
2. Identifier `rule_id` :
   - `forecast_freshness` / `firms_freshness` → vérifier `poll_forecast`/`poll_firms` dans les
     logs applicatifs, l'état de `public.source_status`.
   - `active_model_count` → vérifier `human_model_versions` (doit contenir exactement une ligne
     `active`) ; c'est une anomalie d'exploitation, pas un problème de la présente phase.
   - `migration_failed` → vérifier `_sqlx_migrations`, ne jamais forcer une migration en échec.
   - `candidate_unexpectedly_active` / `shadow_scoring_unexpected` → ne devraient jamais se
     produire (verrouillés par contrainte SQL / absence de code) ; leur apparition indique une
     régression grave à traiter en priorité, hors de tout scénario normal.
3. Aucune de ces alertes ne déclenche d'action automatique — toute remédiation reste manuelle.

## 4. Panne du snapshotter

Si un job de snapshot échoue de façon répétée :

1. Vérifier les logs (`tracing`) pour le message `"... snapshot capture failed; continuing"`.
2. Confirmer que PostgreSQL est accessible (`pyrorisk`'s `/health`).
3. Exécuter la commande manuelle correspondante pour voir l'erreur complète (les logs du
   scheduler ne portent que `%error`, la commande CLI affiche la chaîne `anyhow` complète).
4. Le service opérationnel (`/risk`, `/alerts`, dashboard) n'est jamais affecté par une panne de
   snapshot — les deux systèmes sont indépendants (le scheduler lance ces jobs comme des tâches
   `tokio::spawn` distinctes de `poll_firms`/`poll_forecast`).

## 5. Rollback des migrations

Ordre strict, chaque script refuse s'il existe des données ou si l'ordre est violé :

```text
0024_daily_dense_scientific_archive.down.sql
0022_scientific_snapshot_hardening.down.sql
0021_snapshot_label_links.down.sql
0020_snapshot_alerts.down.sql
0019_scientific_snapshot_registry.down.sql
0018_observability_foundation.down.sql
```

Le rollback 0024 refuse dès qu'une archive dense existe. Le rollback 0022 refuse dès qu'une
tentative, un bundle, un masque ou un manifeste v2 existe.
Après ce point, conserver les données et effectuer un retour applicatif plutôt qu'un effacement.

Ne jamais exécuter avec `psql -f` sans transaction explicite — chaque script contient déjà son
propre `BEGIN`/`COMMIT` ; ne pas les envelopper dans une transaction supplémentaire qui masquerait
un refus partiel.

## 6. Provenance requise

Avant une capture scientifique v2, fournir explicitement :

```text
ERYTHEON_ENVIRONMENT
ERYTHEON_GIT_REVISION
ERYTHEON_IMAGE_REFERENCE
ERYTHEON_IMAGE_DIGEST
```

Une valeur absente bloque la capture avant publication. Les captures opérationnelles restent
possibles et enregistrent la provenance disponible ainsi que chaque tentative.

## 7. Ce que ce système ne fait jamais

- N'entraîne, ne score, ni n'active aucun modèle.
- Ne modifie jamais `/risk`, le moteur de risque, FWI, ou les seuils métier.
- Ne supprime rien automatiquement (voir `PHASE4A5_RETENTION_POLICY.md`).
- N'envoie ni email, ni SMS, ni webhook — les alertes sont enregistrées et affichées uniquement.
