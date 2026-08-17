# Rapport Phase 0 — sauvegarde et restauration PostgreSQL

Date d’exécution : 25–26 juillet 2026  
Base source : PostgreSQL 16.4 / PostGIS 3.4.3 sur le VPS ERYTHEON  
Périmètre autorisé : sauvegarde, checksum, validation, restauration isolée et comparaison  
Résultat global : succès

## 1. Cause exacte de l’échec

Le service quotidien `pyrorisk-local-backup.service` échouait avec le statut 127 avant de lancer `pg_dump`.

Le script exécutait :

```sh
source "$DEPLOY_DIR/.env"
```

Le fichier `.env` contient une valeur de territoire avec des espaces et des caractères accentués. Comme ce fichier était interprété comme du code shell, le second mot de cette valeur était traité comme une commande.

Erreur observée dans systemd :

```text
/opt/pyrorisk/deploy/oracle/.env: line 34: métropolitaine: command not found
```

Le correctif supprime entièrement le `source` de `.env`. Le script extrait uniquement `POSTGRES_DB` et `POSTGRES_USER` avec un parseur ciblé. Aucun secret n’est affiché.

## 2. Fichiers modifiés

| Fichier | Modification |
| --- | --- |
| `deploy/oracle/backup-local.sh` | parsing sûr, dump `.partial`, validation du catalogue, renommage atomique, SHA-256 et vérification |
| `deploy/oracle/README.md` | procédure de sauvegarde, contrôle indépendant, restauration isolée et restauration de production |
| `PHASE0_BACKUP_RESTORE_REPORT.md` | présent rapport |

Le script corrigé a été transféré seul vers `/opt/pyrorisk/deploy/oracle/backup-local.sh`. Aucun rebuild et aucun redémarrage n’ont été effectués.

## 3. Commandes utilisées

Les commandes ci-dessous sont présentées sans identifiant secret, mot de passe ni contenu de `.env`.

### Contrôle local

```sh
bash -n deploy/oracle/backup-local.sh
shellcheck deploy/oracle/backup-local.sh
git diff --check -- deploy/oracle/backup-local.sh
```

### Transfert ciblé

```sh
rsync -az -e "ssh -i <clé-SSH>" \
  deploy/oracle/backup-local.sh \
  <utilisateur>@<VPS>:/opt/pyrorisk/deploy/oracle/backup-local.sh

ssh -i <clé-SSH> <utilisateur>@<VPS> \
  "chmod 0755 /opt/pyrorisk/deploy/oracle/backup-local.sh"
```

### Sauvegarde

```sh
/opt/pyrorisk/deploy/oracle/backup-local.sh
```

La commande exécutée par le script est fonctionnellement :

```sh
docker compose --env-file .env -f compose.yml exec -T postgres \
  pg_dump --username <utilisateur-db> --dbname <base> \
  --format custom --compress=9 >pyrorisk-<timestamp>.dump.partial
```

Puis :

```sh
pg_restore --list <pyrorisk-<timestamp>.dump.partial
mv pyrorisk-<timestamp>.dump.partial pyrorisk-<timestamp>.dump
sha256sum pyrorisk-<timestamp>.dump >pyrorisk-<timestamp>.dump.sha256
sha256sum --check pyrorisk-<timestamp>.dump.sha256
```

### Restauration isolée

Une instance `postgis/postgis:16-3.4` séparée a été créée :

- sans port publié ;
- avec un volume Docker distinct ;
- avec des identifiants temporaires générés localement sur le VPS et jamais affichés ;
- avec le répertoire de sauvegarde monté en lecture seule.

Après l’initialisation complète de l’image :

```sh
createdb --template template0 pyrorisk_restore_test

pg_restore \
  --dbname pyrorisk_restore_test \
  --no-owner \
  --exit-on-error \
  /backups/pyrorisk-20260725T223712Z.dump
```

### Comparaison

Des requêtes SQL en lecture ont comparé :

- version PostgreSQL ;
- extensions et versions ;
- migrations SQLx et leurs checksums ;
- colonnes ;
- contraintes ;
- index ;
- géométrie ;
- comptes exacts ;
- plages temporelles ;
- empreintes MD5 d’échantillons déterministes de 100 lignes par table ;
- tailles physiques.

Les résultats bruts sont conservés sur le VPS dans :

```text
/opt/pyrorisk/restore-tests/20260725T223712Z-v3/
```

## 4. Dump créé

| Propriété | Valeur |
| --- | --- |
| Chemin | `/opt/pyrorisk/backups/pyrorisk-20260725T223712Z.dump` |
| Taille exacte | `1 653 743 685` octets |
| Taille lisible | environ `1,54 Gio` |
| Format | archive PostgreSQL custom, compression niveau 9 |
| Catalogue | 93 lignes, lisible par `pg_restore --list` |
| Date de fin | 25 juillet 2026 à 22:42:41 UTC |

Fichier de checksum :

```text
/opt/pyrorisk/backups/pyrorisk-20260725T223712Z.dump.sha256
```

## 5. Checksum SHA-256

```text
c9538b7d5de82b8cd428f548896affc294fe17a58f1810535c207dd60e4dd217
```

Résultat de la vérification :

```text
pyrorisk-20260725T223712Z.dump: OK
```

## 6. Résultats de la restauration

Instance validée :

| Propriété | Valeur |
| --- | --- |
| Conteneur | `erytheon-restore-test-20260725t223712z-v3` |
| Volume | `erytheon-restore-test-20260725t223712z-v3-data` |
| Base | `pyrorisk_restore_test` |
| Début | 25 juillet 2026 à 22:44:59 UTC |
| Fin | 25 juillet 2026 à 22:46:56 UTC |
| Durée | 1 minute 57 secondes |
| État | restauration complète, conteneur conservé |

### Extensions

Les deux bases contiennent les mêmes extensions et versions :

- `plpgsql 1.0` ;
- `postgis 3.4.3` ;
- `postgis_topology 3.4.3` ;
- `postgis_tiger_geocoder 3.4.3` ;
- `fuzzystrmatch 1.2`.

### Migrations

Les huit migrations SQLx sont présentes, réussies et leurs checksums sont identiques.

### Comptes exacts

| Table | Production | Restauration | Écart |
| --- | ---: | ---: | ---: |
| `calendar_days` | 1 095 | 1 095 | 0 |
| `cell_static` | 920 016 | 920 016 | 0 |
| `corine_france_stage` | 368 393 | 368 393 | 0 |
| `forecast_batches` | 1 | 1 | 0 |
| `forecast_fwi` | 3 171 992 | 3 171 992 | 0 |
| `fwi_state` | 5 678 004 | 5 678 004 | 0 |
| `human_model_versions` | 1 | 1 | 0 |
| `ignition_history` | 15 957 | 15 957 | 0 |
| `observations` | 3 681 | 3 681 | 0 |
| `risk_scores` | 3 171 992 | 3 171 992 | 0 |
| `source_status` | 8 | 8 | 0 |

### Empreintes

Les empreintes suivantes sont strictement identiques :

- schéma des colonnes ;
- contraintes ;
- index ;
- géométrie CORINE ;
- plages temporelles ;
- échantillons des onze tables applicatives ;
- migrations SQLx.

Le fichier `logical.diff` est vide.

## 7. Écarts constatés

### Écart physique attendu

| Mesure | Production | Restauration |
| --- | ---: | ---: |
| Taille de la base | 7 763 857 891 octets | 4 233 802 211 octets |

La restauration est environ 3,53 Go plus petite. Cet écart est attendu :

- un dump logique ne transporte pas les tuples morts ;
- les tables et index sont reconstruits ;
- la fragmentation et le gonflement de production disparaissent.

Il ne correspond à aucune perte logique : comptes, objets, contraintes et échantillons sont identiques.

Les écarts les plus importants concernent `risk_scores`, `forecast_fwi` et `cell_static`, cohérents avec les tuples morts constatés pendant l’audit.

### Deux essais de restauration conservés

Deux essais isolés ont précédé la restauration valide :

1. Le premier a démarré `pg_restore` pendant le redémarrage interne réalisé par l’image PostGIS à la fin de son initialisation. La connexion temporaire a été fermée.
2. Le deuxième a restauré vers la base bootstrap où PostGIS avait déjà créé `tiger`, provoquant un conflit `schema "tiger" already exists`.

Le troisième essai attend explicitement la fin de l’initialisation et restaure vers une base vierge créée depuis `template0`. Il a réussi.

Ces essais n’ont jamais été connectés à la production. Leurs conteneurs et volumes sont conservés jusqu’à validation, conformément à l’interdiction de suppression.

## 8. Procédure de rollback

### Rollback de cette phase

Aucun schéma métier et aucune donnée de production n’ayant été modifiés, aucun rollback PostgreSQL de production n’est nécessaire.

Le seul changement opérationnel est le script de sauvegarde. Son rollback technique consisterait à remettre sa version précédente, mais cela réintroduirait l’échec documenté et n’est donc pas recommandé.

Après validation explicite, les trois conteneurs et volumes temporaires pourront être arrêtés puis supprimés. Cette opération ne concernera que les copies de test.

### Rollback futur de production

Si une future opération exige une restauration de production :

1. vérifier le SHA-256 du dump ;
2. arrêter le conteneur applicatif pour empêcher toute écriture ;
3. conserver PostgreSQL accessible uniquement au processus de restauration ;
4. restaurer avec `--clean --if-exists --no-owner` ;
5. exécuter la comparaison logique ;
6. redémarrer l’application ;
7. vérifier `/health`, les routes risque/alertes et l’interface.

Cette procédure est destructive et nécessite une autorisation distincte.

## 9. Recommandations pour la copie distante

1. Conserver les dumps locaux datés ; ne pas dépendre uniquement d’un objet `latest.dump`.
2. Envoyer le dump et son fichier `.sha256` vers un bucket privé distinct du VPS.
3. Utiliser des identifiants limités au bucket de sauvegarde.
4. Chiffrer le transport et activer le chiffrement au repos du fournisseur.
5. Conserver au minimum plusieurs générations afin qu’une corruption tardivement détectée ne remplace pas l’unique copie saine.
6. Définir une politique compatible avec le budget, par exemple plusieurs sauvegardes quotidiennes récentes, quelques hebdomadaires et des mensuelles.
7. Vérifier le checksum après téléchargement lors de chaque exercice de restauration.
8. Tester régulièrement une restauration complète, pas uniquement l’existence de l’objet.
9. Superviser le timer et alerter si aucun dump récent valide n’existe.
10. Éviter d’inclure `.env`, clés API ou secrets dans les rapports et noms d’objets.

## 10. Confirmation d’intégrité

- Aucune migration `0009` n’a été créée ou exécutée.
- Aucun schéma métier n’a été créé ou modifié en production.
- Aucune table applicative n’a été modifiée manuellement.
- Aucune donnée métier n’a été supprimée.
- L’algorithme, l’API et l’interface n’ont pas été modifiés.
- PostgreSQL, l’application et Caddy n’ont pas été redémarrés.
- L’API de production est restée saine pendant les opérations.
- Aucun secret n’est présent dans ce rapport.

La phase 0 s’arrête ici dans l’attente d’une validation explicite avant toute phase 1.
