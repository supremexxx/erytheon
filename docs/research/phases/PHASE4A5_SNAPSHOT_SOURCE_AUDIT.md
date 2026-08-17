# Phase 4A.5 — Audit des sources avant conception des snapshots

Date : 30 juillet 2026
Mode : lecture seule, aucune modification de code ou de schéma dans ce document.

## 0. État de départ vérifié

- `origin/main` = tag `v0.4.3` = commit `b1d18f635287f952ea5d8a792de5b76c1fa3649e` (merge PR #7).
- Tag `v0.4.3-app` = `6d91959cc478063b4f6df2e6757e9b799d79d25d` (merge PR #6), révision applicative
  déployée déclarée par `PHASE4A4D_PRODUCTION_DEPLOYMENT_REPORT.md`.
- La branche de travail locale (`codex/phase4a4d-production-report`) est un ancêtre direct de
  `b1d18f6` avec un arbre identique (`git diff --stat` vide) : elle contient exactement le même
  code que `main`/`v0.4.3`.
- **Non vérifié dans cet audit** : l'état réel du VPS de production (santé applicative,
  PostgreSQL, Caddy, redémarrages). Cette session n'a pas d'accès SSH/API au VPS Oracle. L'état
  décrit dans la commande comme référence est donc traité comme **déclaratif, hérité du dernier
  rapport de déploiement**, pas comme vérifié en direct. Toute décision de déploiement de la
  présente phase devra revérifier l'état production au moment de l'exécution (§42 de la
  commande).

Prochain numéro de migration libre : **`0018`**.

## 1. `public.cell_static`

- Schéma (`migrations/0001_initial.sql:17-21`) : `h3 BIGINT PRIMARY KEY, features JSONB NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()`. Aucune migration ultérieure ne l'altère.
- Écriture : `Store::upsert_cell_static` (`crates/store/src/lib.rs:498-523`), `ON CONFLICT (h3) DO
  UPDATE` — **toujours écrasé en place, jamais historisé**. `update_cell_history`
  (`lib.rs:530-551`) mute également en place via `jsonb_set`.
- Résolution H3 confirmée à 9 (`.env.example:19`, commentaire `crates/dataset/src/features_h3.rs:3`
  : *"stored at H3 resolution 9 (920,016 rows)"*), cohérente avec `AOI_BBOX` (~115 km × 100 km).
- `features.feature_snapshots` (voir §8) fournit déjà un **versioning de métadonnées** du bundle
  `cell_static` (checksum, provenance) sans dupliquer les lignes.

## 2. `public.fwi_state`

- PK `(h3, date)` (`0001_initial.sql:23-33`), précision élargie par `0003_fwi_precision.sql`.
- Écriture : `Store::upsert_fwi_states` (`lib.rs:329-353`), `ON CONFLICT (h3, date) DO UPDATE` —
  **écrasé par (h3, date) en cas de retraitement, pas append-only**.

## 3. Surfaces de risque et forecast

- `public.risk_scores` (`0001_initial.sql:35-47`), PK étendue par `0005_risk_api.sql` (+
  `input_date`) puis `0006_forecast_horizons.sql` (+ `valid_at`, PK finale `(h3, computed_at,
  horizon)`).
- `public.forecast_fwi` (`0006_forecast_horizons.sql:20-32`), miroir de `risk_scores` pour le FWI.
- `public.forecast_batches` (`0007_forecast_batches.sql:1-8`) : `computed_at TIMESTAMPTZ PRIMARY
  KEY, completed_at TIMESTAMPTZ`, index partiel sur `completed_at IS NULL` (batch en cours).
- **"Forecast complet"** = une ligne `forecast_batches` avec `completed_at IS NOT NULL`, positionné
  par `Store::retain_forecast_batch` (`lib.rs:1389-1415`) uniquement après succès de tous les
  horizons (`crates/engine/src/forecast.rs:76-131`).
- **Point critique pour cette phase** : `retain_forecast_batch` supprime systématiquement tous les
  autres batches `forecast_fwi`/`risk_scores`/`forecast_batches` dès qu'un nouveau batch réussit —
  **seul le dernier forecast complet existe en base à un instant donné, aucun historique
  opérationnel n'est conservé**. Un snapshot scientifique doit donc capturer un batch *avant*
  qu'il ne soit remplacé, ou s'appuyer sur une capture indépendante.

## 4. `fire.ignition_events`

- `0011_bdiff_foundation.sql:173-256`. Colonnes clés : `occurred_at`, `occurred_on_local DATE`,
  `geom_original`, `h3`/`h3_resolution`, `cause_source/cause_category/cause_subcategory`,
  `geographic_quality` (contrainte `CHECK` limitée à `'precision_undocumented'`, `0011:231-232`),
  `is_active`.
- Classification cause : `cause_category` CHECK (`0011:224-230`) = `human_known / natural_known /
  unknown / indeterminate`. Ce vocabulaire doit être respecté tel quel dans `ml.snapshot_label_links`
  (§21 de la commande) : ne jamais traiter `unknown`/`indeterminate` comme négatif.
- La classification géographique enrichie (8 catégories) vit séparément dans
  `validation.event_geographic_quality.geographic_category` (`0012:134-138`).

## 5. `raw.firms_observations`

- `0009_data_platform_foundation.sql:239-259`. `id UUID, import_batch_id (FK
  ops.import_batches), source_record_id, retrieved_at, observed_at, payload JSONB, checksum,
  source_version, parsing_status, parsing_error, created_at`.
- Dedupe : unique `(import_batch_id, source_record_id)` (`0010:23-25`) — **dédoublonnage
  intra-batch seulement**, une même détection peut réapparaître dans un batch ultérieur (append-only
  assumé, commentaire `0009:267-268`).

## 6. `ops.import_batches` / `ops.pipeline_runs`

- `0009_data_platform_foundation.sql:85-236`.
- `import_batches` : `status (pending/running/succeeded/partially_succeeded/failed/cancelled),
  started_at, finished_at, records_received/inserted/updated/ignored/rejected, checksum,
  source_version, pipeline_version, error_message`.
- `pipeline_runs` : mêmes statuts, `pipeline_name/version, trigger_type, import_batch_id,
  parent_run_id (auto-FK), metrics JSONB, code_version`.
- Écrits par les pipelines Rust (`crates/store/src/firms.rs`, `bdiff.rs`, `dataset.rs`), lus en
  lecture seule par la console (`crates/store/src/science.rs:301-325` et suivants).
- **`crates/engine/src/scheduler.rs` n'écrit pas directement ces tables** — il appelle
  `firms_pipeline::run(...)` (`scheduler.rs:38-44`) qui les écrit en interne. Le job forecast
  (`poll_forecast`) n'écrit ni `import_batches` ni `pipeline_runs`, seulement
  `forecast_batches`/`risk_scores`/`forecast_fwi` et `source_status`.
- → Les futurs jobs `operational_snapshot` / `scientific_snapshot` / `snapshot_validation` (§26 de
  la commande) devront écrire explicitement dans `ops.pipeline_runs` — ce n'est pas automatique.

## 7. `public.source_status` / `reference.data_sources`

- `source_status` (`0005_risk_api.sql:14-20`) : `id TEXT PK, last_run, last_success,
  observation_count, recent_error`.
- `reference.data_sources` (`0009:39-70`) : `id UUID PK, code UNIQUE, name, category, provider,
  base_url, is_active`.
- Jointure confirmée dans `crates/store/src/science.rs:282-293` (`science_sources`) : `FROM
  public.source_status s LEFT JOIN reference.data_sources d ON d.code = s.id` — les sources sans
  métadonnées apparaissent quand même. Ce pattern de jointure explicite doit être repris pour tout
  nouvel agrégat de fraîcheur.

## 8. `features.feature_snapshots` (nommé `ml.feature_snapshots` dans la commande, en réalité schéma `features`)

- **Correction d'hypothèse** : la table existe dans le schéma `features`, pas `ml`. Elle ne doit
  pas être dupliquée sous un autre nom.
- `0013_feature_snapshot_foundation.sql:12-78`. Colonnes riches : `family, source, provider,
  vintage, valid_from/until, available_from/until, retrieved_at, import_batch_id,
  pipeline_run_id, code_version, normalizer_version, source_checksum, logical_checksum NOT NULL,
  cell_count, h3_resolution, geographic_coverage JSONB, status (draft/validated/active/superseded/
  failed), temporal_classification (6 valeurs), limitations JSONB[]`.
- Contraintes : `UNIQUE(family, logical_checksum)` + unicité partielle *un actif par famille*.
- **Déjà utilisée**, pas seulement schéma : `crates/engine/src/dataset_pipeline.rs:27-74`
  (`CELL_STATIC_FAMILY = "cell_static_bundle"`) l'alimente via
  `Store::cell_static_snapshot_summary()` (`crates/store/src/dataset.rs:537-546`).
- → Le "bundle statique" demandé au §17 de la commande **existe déjà** sous cette forme. La phase
  4A.5 doit **réutiliser** `features.feature_snapshots` comme référence de bundle statique
  (`static_snapshot_id`), pas recréer un mécanisme parallèle. Elle a aussi déjà un champ
  `temporal_classification` à 6 valeurs qui correspond exactement au vocabulaire demandé au §15 de
  la commande (`historical_exact`, etc. à vérifier terme à terme contre les valeurs réelles de la
  contrainte avant migration).

## 9. `ml.dataset_versions`

- `0015_dataset_versioning_foundation.sql:7-65`. Trigger d'immutabilité exact
  (`0015:67-82`) :

```sql
CREATE OR REPLACE FUNCTION ml.forbid_finalized_dataset_version_update()
RETURNS TRIGGER AS $$
BEGIN
    IF OLD.status = 'finalized' THEN
        RAISE EXCEPTION 'refusing modification: dataset version % is finalized and immutable', OLD.id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER dataset_versions_finalized_immutable
    BEFORE UPDATE ON ml.dataset_versions
    FOR EACH ROW EXECUTE FUNCTION ml.forbid_finalized_dataset_version_update();
```

- **C'est le patron direct à reproduire** pour l'immutabilité `published` des snapshots
  scientifiques (§14 de la commande) : un trigger `BEFORE UPDATE` qui refuse toute mutation d'une
  ligne dont le statut est terminal. Le risque documenté par `PR1_INTEGRATION_REVIEW_REPORT.md`
  §8 s'applique à l'identique ici : le trigger bloque `UPDATE` mais pas `DELETE` — à couvrir
  explicitement par le rollback et par une règle applicative de non-suppression.

## 10. `human_model_versions` / `ml.model_candidate_registry`

- `human_model_versions` (`0008_human_model_versions.sql`) : schéma `public` (pas namespacé),
  `active BOOLEAN`, unicité partielle *un seul actif*.
- `ml.model_candidate_registry` (`0016_model_candidate_registry.sql:19-37`) : `status CHECK IN
  ('candidate','inactive')` — **aucune valeur `active` possible au niveau SQL**, verrou déjà en
  place indépendamment de l'application.
- `0017_model_candidate_registry_identity.sql:18-26` ajoute `seed BIGINT NOT NULL` et
  `UNIQUE(model_family, model_name, dataset_logical_id, seed)`.
- Confirme qu'aucune modification de ces tables n'est nécessaire ni permise par la phase 4A.5.

## 11. Mécanisme de migrations

- Appliqué à chaque connexion via `sqlx::migrate!("../../migrations").run(&pool).await?`
  (`crates/store/src/lib.rs:244`).
- Comptage "17 migrations, 0 échec" : `Store::science_overview` interroge `_sqlx_migrations`
  (`science.rs:190-193`, doublon en `science.rs:621/625`). Après ajout des migrations `0018+`, ce
  comptage évoluera automatiquement — pas de code à modifier pour le total, seulement vérifier
  l'affichage console si un total est codé en dur quelque part dans `science.js` (à vérifier avant
  merge).

## 12. `crates/engine/src/scheduler.rs`

- Deux jobs seulement, **aucune abstraction de job générique/récurrent** :
  - `poll_firms` (`scheduler.rs:32-60`), cadence définie par `crates/ingest` (`Source::cadence()`).
  - `poll_forecast` (`scheduler.rs:62-135`), intervalle fixe `FORECAST_POLL_INTERVAL = 1h`
    (`scheduler.rs:16`), avec logique de skip au démarrage si déjà frais (lignes 72-84).
- Les deux utilisent `tokio::spawn` + `tokio::time::interval(..).set_missed_tick_behavior(Skip)` —
  pattern à reproduire à l'identique pour les nouveaux jobs `operational_snapshot` /
  `scientific_snapshot` / `snapshot_validation`, en évitant les horaires de `poll_firms` et
  `poll_forecast` (conflit de charge signalé §12 de la commande).
- **Aucun registre de jobs à modifier** : chaque nouveau job sera une boucle indépendante ajoutée
  au point d'orchestration du scheduler, pas une entrée dans une liste existante.

## 13. Autres tables de type snapshot/historique déjà présentes

- `ml.dataset_builds` (`0015:84-117`) : déjà un journal d'essais de construction
  (running/succeeded/failed) — patron à connaître pour ne pas le dupliquer conceptuellement.
- Les tables `validation.*` de `0012_bdiff_quality_foundation.sql` (coordinate_groups,
  event_label_quality, event_geographic_quality, event_combustibility_assessments,
  duplicate_candidate_pairs/groups/members) forment déjà une couche versionnée et non destructive
  (`rule_version_id`), conceptuellement proche de ce que demande la commande pour les règles de
  dégradation (§23) — même style à reprendre (`rule_id` + `rule_version`).
- `ml.model_candidate_registry` est déjà, par construction, un historique append-only de candidats.
- Aucune table nommée `snapshot_history` ou `audit_log` n'existe par ailleurs.

## 14. Rétention/suppression existante

- Très limitée et concentrée sur les tables opérationnelles de forecast :
  `abort_forecast_batch`/`retain_forecast_batch` (`lib.rs:1366-1415`) suppriment systématiquement
  tout batch qui n'est pas le dernier réussi — un précédent de "rétention = garder exactement 1",
  pas une politique par fenêtre temporelle.
- Aucune tâche de purge planifiée, aucun `RETENTION`/`purge` générique ailleurs dans `crates/`.
- → La politique de rétention de la phase 4A.5 (§25 de la commande) est un mécanisme entièrement
  nouveau ; il n'y a pas de code existant à réutiliser au-delà du principe transactionnel déjà
  appliqué dans `retain_forecast_batch`.

## 15. Constats structurants pour la conception

1. **`cell_static` et `fwi_state` n'ont aucune historisation native.** Toute capture scientifique
   par valeur doit lire ces tables à l'instant T et les copier — il n'existe aucun mécanisme
   existant à réutiliser pour cela.
2. **`features.feature_snapshots` (schéma `features`, pas `ml`) existe déjà et sert déjà de
   registre de bundle statique.** La phase 4A.5 doit l'étendre/la référencer, pas la dupliquer —
   contrairement à l'hypothèse par défaut de la commande qui l'appelle `ml.feature_snapshots`.
3. **Le dernier forecast complet est écrasé dès qu'un nouveau est prêt** (`retain_forecast_batch`).
   Un snapshot scientifique quotidien doit donc capturer le forecast du jour *avant* sa
   suppression opérationnelle, ou accepter de ne capturer que le dernier état connu au moment de
   la capture (comportement à documenter explicitement, pas à corriger — modifier ce comportement
   est hors périmètre : "ne pas modifier le moteur de risque").
4. **Le trigger `dataset_versions_finalized_immutable` est le patron exact** à reproduire pour
   `published` → immuable sur les nouveaux manifestes scientifiques.
5. **Aucune infrastructure de job générique** dans le scheduler : les nouveaux jobs de snapshot
   seront du code neuf, pas des plugins dans un système existant.
6. **`ops.pipeline_runs`/`ops.import_batches` ne sont pas alimentés par le scheduler
   actuel** : les nouveaux jobs devront explicitement y écrire pour respecter le §26 de la
   commande.
7. **Aucune vérification directe de l'état VPS production n'a pu être faite dans cette session** —
   à traiter comme une limite de cet audit, pas comme une confirmation de l'état "healthy" déclaré.
