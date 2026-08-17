# ERYTHEON — Application de la migration 0009 en production

Date : 26 juillet 2026  
Migration : `0009_data_platform_foundation.sql`  
Résultat : **réussite**  
Rollback : **non exécuté**  
Phase 2 : **non commencée**

## 1. Chronologie

- début de l'intervention : `2026-07-25T23:24:47Z` ;
- début de l'application SQLx réussie : `2026-07-25T23:37:56Z` ;
- fin de l'application SQLx réussie : `2026-07-25T23:38:01Z` ;
- durée mesurée de la commande SQLx : 4 secondes ;
- fin des contrôles : `2026-07-25T23:41:48Z` ;
- durée totale : 17 minutes et 1 seconde.

Aucun service de production n'a été redémarré. Une seule connexion capable d'exécuter SQLx a été utilisée.

## 2. Contrôles préalables

Avant application :

- PostgreSQL : `healthy` et `accepting connections` ;
- application : `healthy` ;
- `GET /health` : HTTP 200, `status=ok`, `db=ok` ;
- interface : HTTP 200 ;
- migrations présentes : versions 1 à 8, toutes avec `success=true` ;
- migration 9 : absente ;
- aucune migration inconnue ;
- 13 tables historiques dans `public` ;
- 99 colonnes historiques dans `public` ;
- 24 contraintes historiques dans `public` ;
- 23 index historiques dans `public`.

L'empreinte SHA-256 du schéma-only initial de `public` était :

```text
3c685afd40f4b6069b2ab710b6122523771d774343e67eb14c8e75d1fc4f8cd0
```

## 3. Nouvelle sauvegarde

- dump : `/opt/pyrorisk/backups/pyrorisk-20260725T232534Z.dump` ;
- taille : `1 653 743 707` octets ;
- checksum SHA-256 :

```text
8d753a4ec18c043e31c386870b2ea2a8620b64bff1834dc5a78ed0783d43f8f3
```

Validations :

- génération au format custom PostgreSQL : réussie ;
- vérification `sha256sum --check` : `OK` ;
- lecture complète du catalogue par `pg_restore --list` : réussie ;
- dump validé précédent `/opt/pyrorisk/backups/pyrorisk-20260725T223712Z.dump` : conservé ;
- aucun dump, conteneur ou volume supprimé.

## 4. État de référence conservé

Les preuves avant/après sont conservées dans :

```text
/opt/pyrorisk/migration-0009-production/20260725T232447Z
```

Elles comprennent :

- schémas ;
- tables, colonnes, contraintes et index de `public` ;
- migrations SQLx ;
- extensions ;
- comptes exacts de toutes les tables `public` ;
- dump schema-only de `public` et son checksum ;
- réponse `/health` ;
- réponses API et ressources de l'interface ;
- journaux SQLx, applicatifs et PostgreSQL.

Aucun secret n'est stocké dans ces preuves.

## 5. Application SQLx

Procédure utilisée :

1. ouverture d'un tunnel SSH temporaire vers le conteneur PostgreSQL ;
2. exécution directe du test compilé `platform_foundation` ;
3. connexion par `Store::connect`, mécanisme officiel du projet qui appelle le migrateur SQLx ;
4. application automatique de la seule migration absente, `0009` ;
5. exécution immédiate des tests SQLx de fondation ;
6. fermeture du tunnel après les vérifications.

Commande logique, avec secret masqué :

```sh
DATABASE_URL="<secret>" \
  target/debug/deps/platform_foundation-<hash> --nocapture
```

Résultat :

```text
running 1 test
test data_platform_foundation_is_additive_and_enforced ... ok

test result: ok. 1 passed; 0 failed
```

SQLx enregistre exactement une ligne :

```text
9 | data platform foundation | true
```

### Incidents non destructifs avant l'application

- Une première commande locale de préparation a refusé de charger le fichier `.env`, car une valeur textuelle existante n'était pas quotée. Elle s'est arrêtée avant toute connexion SQL valide.
- Une tentative via `cargo test` est restée bloquée sur le verrou local des artefacts, puis a été tuée avant le démarrage du test. La version 9 était toujours absente après cette tentative.
- La commande directe du binaire déjà compilé a ensuite réussi en 4 secondes.

Ces incidents étaient strictement locaux. Ils n'ont produit ni migration partielle, ni redémarrage, ni changement de données.

## 6. Objets créés

Schémas :

```text
environment
features
fire
human
ml
ops
raw
reference
risk
serving
staging
validation
```

Tables :

```text
reference.data_sources
ops.import_batches
ops.pipeline_runs
raw.firms_observations
```

Index applicatifs :

```text
reference.data_sources_code_unique
ops.import_batches_source_started_at_idx
ops.import_batches_started_at_idx
ops.import_batches_status_idx
ops.pipeline_runs_import_batch_idx
ops.pipeline_runs_name_started_at_idx
ops.pipeline_runs_parent_idx
ops.pipeline_runs_started_at_idx
ops.pipeline_runs_status_idx
raw.firms_observations_import_batch_idx
raw.firms_observations_observed_at_idx
```

Les index de clés primaires sont également présents sur les quatre tables.

## 7. Contraintes et commentaires

Contrôles réussis :

- 4 clés primaires ;
- 4 clés étrangères ;
- 1 contrainte d'unicité explicite ;
- contraintes de statut présentes ;
- contraintes temporelles présentes ;
- contraintes de compteurs non négatifs présentes ;
- payloads et paramètres JSONB contrôlés ;
- liaisons source/batch/run/FIRMS validées ;
- 12 commentaires de schéma sur 12 ;
- 4 commentaires de table sur 4 ;
- 31 commentaires de colonne ;
- commentaire append-only présent sur `raw.firms_observations`.

Les tests ont volontairement provoqué quatre violations de contraintes :

- compteur reçu négatif ;
- date de fin antérieure au début ;
- statut de pipeline inconnu ;
- compteur inséré négatif.

PostgreSQL les a toutes refusées. Ces quatre messages `ERROR` attendus expliquent intégralement les erreurs du journal pendant le test. Aucun message `ERROR`, `FATAL` ou `PANIC` n'apparaît après la fin du test.

Toutes les insertions de test ont été exécutées dans des transactions annulées. Les quatre nouvelles tables contiennent chacune exactement 0 ligne.

## 8. Comparaison de `public`

Résultats avant/après :

| Élément | Différence |
|---|---:|
| Liste des tables | 0 ligne |
| Colonnes | 0 ligne |
| Contraintes | 0 ligne |
| Index | 0 ligne |
| Schéma SQL complet | 0 ligne |
| Extensions | 0 ligne |
| Migrations historiques 1–8 | 0 ligne |

Les comptes exacts de toutes les tables métier sont identiques.

Seul changement de compte :

```text
public._sqlx_migrations : 8 → 9
```

Ce changement est attendu et correspond uniquement à l'enregistrement réussi de `0009`.

Confirmation :

- aucune colonne historique modifiée ;
- aucune contrainte historique modifiée ;
- aucun index historique modifié ;
- aucune extension ajoutée ou modifiée ;
- aucune donnée métier supprimée ou modifiée ;
- aucune donnée métier migrée ;
- aucune table de `public` modifiée.

## 9. Contrôles API

Tous les appels ont répondu avec succès :

- `GET /health` : `status=ok`, `db=ok` ;
- `GET /sources` : 8 sources ;
- `GET /risk` sur la France avec limite 1 : 1 score retourné ;
- cellule retournée : `883964260bfffff` ;
- score de cette cellule : `0.9958645` ;
- `GET /risk/cell/883964260bfffff` : détail complet retourné ;
- `GET /alerts?threshold=0.8&limit=5` : 5 alertes retournées.

## 10. Contrôle de l'interface

Chargements HTTP réussis :

- page principale : HTTP 200, `12 034` octets ;
- JavaScript : HTTP 200, `29 997` octets ;
- CSS : HTTP 200, `19 886` octets.

La page contient :

- la marque ERYTHEON ;
- le conteneur de carte ;
- les indicateurs de score ;
- la liste des zones à surveiller ;
- les infobulles ;
- les appels vers `/risk`, `/risk/cell/{h3}` et `/alerts`.

Le flux qui alimente la carte et les scores a été vérifié par l'appel `/risk`, qui retourne une cellule et un score valides. Les ressources nécessaires au rendu sont accessibles.

## 11. Santé et journaux finaux

- application : `healthy` ;
- PostgreSQL : `healthy` ;
- application démarrée depuis `2026-07-25T21:25:01Z`, donc non redémarrée pour la migration ;
- PostgreSQL démarré depuis `2026-07-18T21:33:33Z`, donc non redémarré ;
- aucune erreur applicative liée à SQLx, aux migrations ou aux nouveaux schémas ;
- aucune erreur PostgreSQL après les quatre rejets intentionnels des tests.

Un avertissement préexistant reste visible avant l'intervention : le fournisseur Open-Meteo a parfois limité le débit et certains recalculs planifiés ont échoué en continuant normalement. Ce comportement est antérieur et sans lien avec `0009`.

## 12. Conclusion

La migration `0009_data_platform_foundation.sql` est appliquée avec succès en production.

- rollback non nécessaire et non exécuté ;
- aucun flux FIRMS modifié ;
- aucune double écriture ajoutée ;
- aucun pipeline modifié ;
- aucun algorithme, contrat API ou composant d'interface modifié ;
- aucune donnée métier migrée ;
- phase 2 non commencée.

Recommandation : conserver le dump et les preuves, observer les journaux pendant un cycle applicatif normal, puis attendre une validation explicite avant de concevoir ou démarrer la phase 2.
