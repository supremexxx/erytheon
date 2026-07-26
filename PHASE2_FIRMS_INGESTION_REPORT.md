# ERYTHEON — Rapport de phase 2

## 1. Périmètre et état

La phase 2 introduit le pilote NASA FIRMS suivant :

```text
NASA FIRMS
→ ops.import_batches + ops.pipeline_runs
→ raw.firms_observations
→ public.observations
```

Le code et la migration additive `0010` ont été testés hors production sur la copie PostgreSQL isolée issue de la phase 1. Aucun binaire, scheduler, backfill ou changement SQL de cette phase n’a été déployé en production.

## 2. Audit du flux FIRMS V1

- **Connecteur** : `crates/ingest/src/firms.rs`.
- **Service distant** : NASA FIRMS Area CSV API, produit `VIIRS_SNPP_NRT`.
- **Fenêtres** : cinq jours maximum par requête ; les périodes plus longues sont découpées en fenêtres successives.
- **Cadence** : 1 800 secondes, soit 30 minutes.
- **Réponse** : document CSV avec une ligne par détection. La fixture représentative contient `latitude`, `longitude`, `bright_ti4`, `scan`, `track`, `acq_date`, `acq_time`, `satellite`, `instrument`, `confidence`, `version`, `bright_ti5`, `frp` et `daynight`.
- **Dates** : `acq_time` est complété sur quatre chiffres, combiné à `acq_date`, puis interprété en UTC.
- **H3** : latitude et longitude sont projetées avec le `H3Grid` et la résolution configurée.
- **Source et type publics** : `source = "firms"` et `kind = "active_fire"`.
- **Clé historique** : `satellite:date:HHMM:latitude(5 décimales):longitude(5 décimales):version`.
- **Payload public** : coordonnées, températures de brillance, scan, track, satellite, instrument, confiance, version, FRP et cycle jour/nuit. `acq_date` et `acq_time` n’y sont pas ajoutés.
- **HTTP V1** : erreurs remontées par `reqwest`, sans retry propre au connecteur.
- **CSV V1** : la désérialisation collective faisait échouer tout le document dès qu’une ligne était invalide.
- **Réponse vide V1** : aucun traitement explicite robuste.
- **Scheduler V1** : boucle séquentielle, exécution immédiate, cadence de 30 minutes et `MissedTickBehavior::Skip`, donc absence de chevauchement dans une même instance.
- **Backfill V1** : logique d’import FIRMS dupliquée dans la commande, puis export GeoJSON et recalcul de risque.
- **Persistance V1** : transaction courte sur `public.observations`, insertion ligne par ligne avec conflit sur `(source, dedupe_key)`.
- **État opérationnel V1** : uniquement `public.source_status` et logs généraux.
- **Risque d’écriture partielle V1** : récupération, insertion publique et mise à jour de `source_status` étaient des opérations distinctes. Une insertion pouvait réussir avant l’échec de l’état synthétique.

## 3. Architecture retenue

`crates/engine/src/firms_pipeline.rs` est l’orchestrateur unique appelé par le scheduler et le backfill :

1. construction de paramètres sans secret ;
2. création du batch et du pipeline run ;
3. passage immédiat de `pending` à `running` ;
4. récupération NASA ou fixture hors transaction PostgreSQL ;
5. parsing ligne par ligne ;
6. transaction courte commune à `raw` et `public` ;
7. finalisation atomique du batch, du run et de `public.source_status` ;
8. retour d’un résultat structuré avec identifiants, compteurs, statut, durée et observations normalisées.

Les autres sources gardent leurs chemins existants.

## 4. Stratégie transactionnelle

La stratégie A, transaction unique `raw + public`, est retenue.

Le volume FIRMS courant est faible et la priorité de cette phase pilote est d’éviter un état silencieux où une ligne brute serait annoncée comme reçue sans que la compatibilité V1 soit correctement traitée. L’appel distant reste hors transaction longue. Si l’insertion publique échoue, toutes les insertions brutes de ce traitement sont annulées et le batch/run est finalisé `failed`.

Ce choix donne une cohérence forte. Une future phase pourra introduire un rejeu `raw → staging` et réévaluer une stratégie donnant priorité au brut lorsque les mécanismes de reprise seront opérationnels.

## 5. Migration 0010

Le schéma `0009` autorisait les doublons accidentels d’une même ligne au sein d’un batch et ne contenait pas la source de référence stable. `0010` ajoute uniquement :

- la source idempotente `nasa_firms` dans `reference.data_sources` ;
- un index unique partiel sur `(import_batch_id, source_record_id)`.

La migration ne modifie aucun objet `public`.

```sql
INSERT INTO reference.data_sources (
    id, code, name, category, provider, description, base_url, is_active
)
VALUES (
    '00000000-0000-4000-8000-000000000010'::UUID,
    'nasa_firms',
    'NASA FIRMS',
    'satellite',
    'NASA',
    'Near-real-time satellite active-fire detections from the FIRMS service.',
    'https://firms.modaps.eosdis.nasa.gov/api/area/csv',
    TRUE
)
ON CONFLICT (code) DO NOTHING;

CREATE UNIQUE INDEX IF NOT EXISTS firms_observations_batch_source_record_unique
    ON raw.firms_observations (import_batch_id, source_record_id)
    WHERE source_record_id IS NOT NULL;

COMMENT ON INDEX raw.firms_observations_batch_source_record_unique IS
    'Prevents accidental duplicate source rows inside one import batch while preserving the same detection across later batches.';
```

## 6. Rollback 0010

Le rollback refuse de s’exécuter si une ligne brute, un batch NASA FIRMS ou un run lié existe. Il ne touche jamais `public.observations`.

```sql
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM raw.firms_observations) THEN
        RAISE EXCEPTION
            'rollback refused: FIRMS raw observations exist';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM ops.pipeline_runs
        WHERE import_batch_id IN (
            SELECT id
            FROM ops.import_batches
            WHERE source_id = '00000000-0000-4000-8000-000000000010'::UUID
        )
    ) THEN
        RAISE EXCEPTION
            'rollback refused: NASA FIRMS pipeline runs exist';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM ops.import_batches
        WHERE source_id = '00000000-0000-4000-8000-000000000010'::UUID
    ) THEN
        RAISE EXCEPTION
            'rollback refused: NASA FIRMS import batches exist';
    END IF;
END
$$;

DROP INDEX IF EXISTS raw.firms_observations_batch_source_record_unique;

DELETE FROM reference.data_sources
WHERE id = '00000000-0000-4000-8000-000000000010'::UUID
  AND code = 'nasa_firms';
```

## 7. Types et contrats internes

- `FirmsRow` : clé source, payload brut, date observée éventuelle, version source, observation V1 éventuelle et erreur de parsing.
- `FirmsFetch` : lignes et nombre de documents, avec compteurs reçu/accepté/rejeté.
- `FirmsImportStart` : type, déclencheur, période, paramètres et versions.
- `FirmsImportIds` : identifiants du batch et du run.
- `FirmsPersistenceResult` : reçu, brut inséré, public inséré, doublons et rejets.
- `FirmsTerminalState` : `succeeded`, `partially_succeeded`, `failed` ou `cancelled`.
- `FirmsImportResult` : identifiants, compteurs, statut, durée et observations normalisées.

## 8. Idempotence, erreurs et statuts

### Idempotence

- **Dans un batch** : unicité `(import_batch_id, source_record_id)`.
- **Entre batches** : la même ligne est conservée à nouveau dans `raw`.
- **Dans V1** : la contrainte historique `(source, dedupe_key)` continue d’éviter les doublons dans `public.observations`.
- **Source** : insertion `ON CONFLICT (code) DO NOTHING`.

### Réponses et lignes

- Une erreur HTTP finalise le batch et le run en `failed`.
- Un document totalement illisible finalise en `failed`.
- Une réponse vide valide donne un batch `succeeded` avec zéro ligne.
- Une ligne désérialisable mais invalide est conservée avec ses champs CSV originaux, `parsing_status = rejected` et `parsing_error`.
- Une ligne valide est conservée en brut puis normalisée avec la logique V1 exacte.
- Un ou plusieurs rejets donnent `partially_succeeded`.
- Un doublon intra-batch n’est inséré ni en brut ni en public.
- Une observation déjà publique reste dans le nouveau batch brut, mais n’est pas réinsérée dans `public`.

Les erreurs persistées et les logs utilisent des messages génériques. La clé FIRMS, les URL signées, les mots de passe et les variables d’environnement ne sont jamais inclus.

### Compteurs

- `records_received` : lignes présentées à la persistance.
- `records_inserted` : nouvelles lignes normalisées dans `public`.
- `records_ignored` : doublons intra-batch et conflits historiques publics.
- `records_rejected` : lignes brutes sans observation normalisée.
- Les métriques du run ajoutent `raw_inserted`.

## 9. Payload brut et checksum

Le payload brut est un objet JSON construit avec les noms d’en-têtes et valeurs textuelles du CSV reçu. Il permet de reconstruire la normalisation sans correction destructive.

Aucun checksum par ligne n’est calculé dans cette phase. La clé source déterministe et l’unicité intra-batch répondent au besoin opérationnel actuel ; calculer SHA-256 sur chaque petite ligne n’apporterait pas de contrôle supplémentaire utile. Le champ existant reste disponible pour un futur checksum de document ou de réponse archivée.

## 10. Rejeu futur

Un batch à rejouer se sélectionnera par `ops.import_batches.id`, statut ou période. Ses lignes sont récupérables par `raw.firms_observations.import_batch_id`. `parsing_status` et `parsing_error` distinguent les lignes normalisables des rejets. La clé source et la `dedupe_key` historique empêcheront les doublons publics.

La phase 3 devra définir :

- le lien explicite entre une ligne brute et sa sortie de staging ;
- la version du normaliseur utilisée au rejeu ;
- un statut de rejeu et ses métriques ;
- le traitement des lignes rejetées après correction ;
- la politique de checksum si des documents complets sont archivés.

## 11. Scheduler, backfill et source_status

- La cadence FIRMS reste 30 minutes.
- La boucle reste séquentielle avec ticks manqués ignorés ; un import long ne se superpose pas au suivant dans une instance.
- Une erreur est journalisée et la boucle continue.
- Le scheduler et le backfill appellent le même pipeline.
- Le backfill conserve l’export GeoJSON et le recalcul existant après un import réussi.
- Aucun backfill national n’a été lancé.
- `public.source_status` reste mis à jour pour `/health`, `/sources` et la compatibilité V1.
- `ops` fournit désormais l’historique détaillé.

## 12. Tests exécutés

### Tests unitaires et fixtures

- Connecteur FIRMS : 4 tests réussis.
- Fixture FIRMS d’intégration : 1 test réussi.
- Pipeline moteur : 2 tests réussis.
- Vérifications couvertes : normalisation, UTC, H3, clé source, payload brut, ligne invalide, réponse vide, fenêtres de requête, paramètres sans secret et statut vide/partiel.
- `cargo fmt --check` : réussi.

### Test SQLx/PostgreSQL isolé

Test réussi sur `pyrorisk_phase1_test`, accessible uniquement par tunnel local :

- source créée une seule fois malgré deux initialisations ;
- batch et run créés et liés ;
- première exécution : 6 lignes reçues, 5 brutes insérées, 1 doublon intra-batch ;
- seconde exécution : 5 nouvelles lignes brutes historiques, 0 nouvelle ligne publique ;
- identifiants publics inchangés à la seconde exécution ;
- ligne rejetée conservée et statut `partially_succeeded` ;
- statut `failed` et timestamps finaux vérifiés ;
- erreur PostgreSQL simulée : transaction annulée et nombre de lignes brutes inchangé ;
- lignes publiques de test, lignes brutes, runs et batches supprimés après le test ;
- état `public.source_status` restauré.

### Non-régression V1

L’ancien contrat `Source::fetch` et le nouveau `fetch_batch` ont été exécutés sur la même fixture. Égalité stricte confirmée pour les 5 observations :

- nombre ;
- source ;
- kind ;
- H3 ;
- timestamp UTC ;
- payload JSON ;
- dedupe key.

La répétition en base confirme que le résultat final de `public.observations` reste idempotent.

### Migration et rollback isolés

- `0010` appliquée sur la copie isolée.
- Test de protection avec un batch FIRMS artificiel : rollback refusé, code de sortie non nul.
- Batch artificiel supprimé, puis rollback réussi.
- Après rollback : source `nasa_firms` = 0, index `0010` = 0, migration `10` = 0.
- Réapplication réussie.
- Après réapplication : migration `10` = 1, source = 1, index = 1, raw résiduel = 0, batch de test résiduel = 0.
- Empreinte SQLx SHA-384 de `0010` : `fb9c62acf791f3a9b89d5cbda22b24897ea918853d30e8384482474f61f1e7ad0b4e697e2819dd808e22ae16e7df0641`.
- Artifacts isolés : `/opt/pyrorisk/phase2-tests/20260725t223712z`.

### Limites de validation

- Aucun appel NASA réel n’a été lancé : la fixture déterministe évite les limites d’API et tout risque de fuite de clé.
- Le contrôle Clippy global a été interrompu avant analyse par les performances du système de fichiers local ; il n’a produit aucun diagnostic de code. Les compilations ciblées, tests ciblés et `cargo fmt --check` ont réussi.
- L’application de production n’a pas été redémarrée. Le démarrage effectif du nouveau binaire devra être testé dans l’étape de préproduction autorisée avant déploiement.

## 13. Fichiers modifiés

- `crates/ingest/src/firms.rs`
- `crates/ingest/src/lib.rs`
- `crates/ingest/tests/firms.rs`
- `crates/store/Cargo.toml`
- `crates/store/src/firms.rs`
- `crates/store/src/lib.rs`
- `crates/store/tests/firms_ingestion.rs`
- `crates/engine/src/firms_pipeline.rs`
- `crates/engine/src/main.rs`
- `crates/engine/src/scheduler.rs`
- `migrations/0010_firms_ingestion_support.sql`
- `migrations/rollback/0010_firms_ingestion_support.down.sql`
- `README.md`
- `PHASE2_FIRMS_INGESTION_REPORT.md`

## 14. Preuves de périmètre

- Aucun fichier d’API ni d’interface n’est modifié.
- Aucun fichier du calcul de risque ou de FWI n’est modifié.
- Aucun pipeline météorologique n’est modifié.
- Aucune table `risk_scores`, `forecast_fwi`, `fwi_state` ou `cell_static` n’est modifiée.
- `0010` ne contient aucune instruction visant le schéma `public`.
- Les autres sources conservent leur orchestration existante.
- Le contrat public FIRMS est comparé strictement par test.

## 15. Procédure future de déploiement

Cette procédure nécessite une validation distincte :

1. créer et vérifier un nouveau dump PostgreSQL ;
2. construire l’image avec le code validé ;
3. arrêter temporairement uniquement le scheduler FIRMS pour éviter un import pendant la migration ;
4. appliquer `0010` une seule fois ;
5. vérifier la source, l’index et `_sqlx_migrations` ;
6. déployer le nouveau binaire ;
7. lancer une exécution FIRMS courte et contrôlée ;
8. vérifier batch, run, raw, public et `source_status` ;
9. vérifier `/health`, `/sources` et les observations visibles ;
10. réactiver le scheduler normal ;
11. surveiller au moins deux cadences.

## 16. Vérification après déploiement

- Un seul `reference.data_sources.code = 'nasa_firms'`.
- Un seul enregistrement SQLx version 10 réussi.
- Un batch et un run liés par appel.
- Aucun batch connu laissé durablement `running`.
- Nombre brut cohérent avec les lignes reçues moins doublons intra-batch.
- Nombre public cohérent avec la déduplication V1.
- Réception répétée visible dans plusieurs batches bruts.
- Aucun secret dans paramètres, erreurs ou logs.
- Santé API, sources, risques, FWI et météo inchangés.

## 17. Procédure de rollback

Le rollback SQL n’est possible que tant qu’aucune donnée FIRMS de phase 2 n’existe :

1. arrêter le scheduler FIRMS ;
2. restaurer le binaire V1 ;
3. vérifier l’absence de raw, batch et run FIRMS ;
4. exécuter le rollback protégé `0010` ;
5. retirer la version 10 via l’outil SQLx normal ;
6. redémarrer et vérifier l’application.

Après le premier import réel, le rollback SQL est volontairement bloqué afin de ne supprimer ni métadonnée ni historique. Dans ce cas, le rollback opérationnel consiste à restaurer le binaire V1 et à laisser les objets additifs inutilisés. Une restauration complète du dump pré-déploiement reste le dernier recours.

## 18. Décisions demandant validation

1. autoriser ou non une étape de préproduction avec un appel NASA réel limité ;
2. autoriser l’application de `0010` en production ;
3. autoriser le déploiement du nouveau pipeline ;
4. définir ultérieurement la politique de rejeu `raw → staging` de la phase 3.

La phase 2 s’arrête ici dans l’attente d’une validation explicite.
