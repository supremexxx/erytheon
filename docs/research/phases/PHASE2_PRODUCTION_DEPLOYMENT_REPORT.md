# ERYTHEON — Mise en production contrôlée de la phase 2

## Résultat

La migration SQLx `0010` et le pipeline NASA FIRMS centralisé sont déployés en production avec succès. La phase 3 n’a pas commencé. Aucun rollback n’a été nécessaire.

## Chronologie UTC

- 07:35 : contrôles préalables de la production.
- 08:02–08:08 : création et validation du dump complet.
- 08:10–08:14 : tests ciblés du commit sur le VPS.
- 08:14–08:15 : construction de l’image finale.
- 08:16 : démarrage et import fixture hors production.
- 08:17 : appel NASA réel limité hors production.
- 08:19:06 : début de l’arrêt contrôlé de l’application V1.
- 08:19:27 : application arrêtée ; PostgreSQL reste sain.
- 08:19:28–08:19:33 : application de `0010` par le migrateur SQLx.
- 08:20:30 : démarrage de l’image de phase 2.
- 08:20:36 : application déclarée saine.
- 08:20 et 08:50 : deux imports scheduler réussis, espacés de 30 minutes.

L’architecture ne permet pas d’arrêter FIRMS indépendamment de l’API et des autres boucles. L’application entière a donc été arrêtée pendant environ 69 secondes. PostgreSQL et Caddy n’ont pas été arrêtés.

## Sauvegarde préalable

- Dump : `/opt/pyrorisk/backups/pyrorisk-20260726T080243Z.dump`
- Taille exacte : `1 704 120 762` octets
- SHA-256 : `0d567c5327f38cf570e9afcf6da45bfea0bdc0bc083c3bc36dac7927bbb4db2d`
- `sha256sum --check` : `OK`
- `pg_restore --list` : réussi, archive custom PostgreSQL 16.4, 179 lignes de catalogue.
- Les trois dumps antérieurs ont été conservés.

## Commit et artefact

- Branche : `main`
- Commit complet : `361d46800815d2be8ad49c75932ff42ced64d7a6`
- Commit court : `361d468`
- Message : `chore: establish validated phase 2 baseline`
- Arbre propre lors du build.
- Archive Git SHA-256 : `2e77077014e13797e5096e3a62573523409ff490a4729f1721b4ea7568d727c1`
- Image : `erytheon:phase2-361d468`
- Image ID/digest : `sha256:1139ed492698780ca649d18d25cfb76978259bbab5f0481c2fa5a9460acc6363`
- Label de révision OCI : commit complet ci-dessus.
- Rust/Cargo : `1.94.1`.
- SQLx : `0.8.6`.
- Build release verrouillé : réussi.

Le dépôt initial contient 109 fichiers suivis, environ 0,77 Mio. `.env`, secrets, dumps, données, sorties, certificats, logs, captures Playwright et `target/` sont exclus.

## Validation hors production

- Compilation exacte du commit : réussie.
- Tests connecteur FIRMS : 4 réussis.
- Test de compatibilité fixture : 1 réussi.
- Tests pipeline moteur : 2 réussis.
- Test SQLx FIRMS : 1 réussi.
- Démarrage de l’image finale : réussi.
- Migrateur SQLx : réussi.
- PostgreSQL : connecté.
- API `/health`, `/sources` et `/risk` : saines.
- Scheduler fixture : batch et run créés, 5 lignes brutes et 5 publiques, statut `succeeded`.
- Aucun panic ni erreur de liaison.

## Appel NASA réel limité hors production

- Zone : `4.80,43.30,5.00,43.60`
- Fenêtre : 1 jour
- Produit : `VIIRS_SNPP_NRT`
- Batch : `a6e2f603-a49b-49f1-8131-884082ec162c`
- Run : `eae69ed8-aae4-4ad5-8702-8a8ccb358b9f`
- Statut : `succeeded`
- Reçues : 4
- Brutes : 4
- Publiques : 4
- Ignorées : 0
- Rejetées : 0
- Durée : environ 0,65 seconde
- Aucun secret présent dans les logs ou paramètres.

## Migration SQLx 0010

Commande logique : exécution de l’image finale avec `data-status`, qui appelle le mécanisme `sqlx::migrate!` officiel avant la commande.

- Début : 08:19:27 UTC
- Fin : 08:19:33 UTC
- Durée du conteneur de migration : 6,232 secondes
- Version 10 : une seule ligne, `success = true`
- Checksum SQLx SHA-384 : `fb9c62acf791f3a9b89d5cbda22b24897ea918853d30e8384482474f61f1e7ad0b4e697e2819dd808e22ae16e7df0641`
- Source `nasa_firms` : exactement 1
- Index `raw.firms_observations_batch_source_record_unique` : présent exactement 1 fois
- Verrou ou avertissement de migration : aucun.
- Schéma `public` avant/après migration : strictement identique par `cmp`.

## Premier import production

- Batch : `47876c21-54e8-4453-9c13-50afa9005a9e`
- Run : `dda1f409-40db-4bea-b832-0f8217aedf32`
- Période : 26 juillet 2026 UTC
- Statut : `succeeded`
- Reçues : 658
- Brutes : 658
- Publiques insérées : 0
- Ignorées par déduplication V1 : 658
- Rejetées : 0
- Durée : 4,307 secondes
- `source_record_id` : 658 renseignés et distincts.
- Payload brut : 14 champs FIRMS originaux conservés.
- Batch/run liés et terminés.
- Aucun batch `pending` ou `running`.
- `public.source_status` FIRMS mis à jour normalement.

## Seconde réception et cadences

Deux déclenchements espacés de 30 minutes ont été observés :

| Heure UTC | Batch | Run | Reçues | Brutes | Publiques | Ignorées | Rejetées | Statut |
|---|---|---|---:|---:|---:|---:|---:|---|
| 08:20 | `47876c21-54e8-4453-9c13-50afa9005a9e` | `dda1f409-40db-4bea-b832-0f8217aedf32` | 658 | 658 | 0 | 658 | 0 | succeeded |
| 08:50 | `376f1649-ebfd-40a0-a3e9-9646289a1404` | `1494419e-dc7a-4017-8e34-5f09f2676ded` | 658 | 658 | 0 | 658 | 0 | succeeded |

La seconde réception conserve les mêmes événements dans un nouveau batch brut, sans duplication publique. Total brut après deux cycles : 1 316. Total public avant et après : 4 339.

## Contrôles finaux

- PostgreSQL : `healthy`
- Application : `healthy`
- `/health` : HTTP 200
- `/sources` : HTTP 200
- `/risk` : HTTP 200
- `/alerts` : HTTP 200
- Interface : HTTP 200
- Migrations 1 à 10 : réussies
- Extensions PostgreSQL : inchangées
- Schéma `public` final : identique au schéma préalable
- Observations publiques : 4 339 avant et après
- Batch/run bloqué : 0
- Panic : aucun
- Clé FIRMS dans les logs : absente
- Clé FIRMS et structures sensibles dans paramètres, métriques, payloads et erreurs : absentes

## Autres pipelines et avertissements

Le modèle humain, le calcul de risque, l’API et l’interface fonctionnent. Le pipeline forecast a rencontré l’avertissement Open-Meteo préexistant de limitation de débit, a effectué ses retries puis a continué sans arrêter le scheduler. Cet avertissement n’est pas une régression de phase 2.

## Rollback

Aucun rollback n’a été exécuté. Après les premiers imports réels, le rollback SQL est désormais volontairement bloqué.

En cas de régression future :

1. arrêter l’application/scheduler ;
2. restaurer l’image V1 ;
3. conserver `0010`, la source, l’index, les batches, runs et lignes brutes ;
4. ne supprimer aucune donnée ;
5. analyser hors production.

La sauvegarde complète préalable reste réservée à un incident majeur et ne doit pas être restaurée automatiquement.

## Conclusion

La phase 2 est en production et conforme au périmètre validé. La recommandation est de surveiller les volumes `raw`, la durée des imports et les statuts `partially_succeeded/failed`, puis de définir séparément la phase 3 uniquement après autorisation explicite.
