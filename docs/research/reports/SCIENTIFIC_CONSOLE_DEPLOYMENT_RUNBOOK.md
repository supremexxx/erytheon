# ERYTHEON — Runbook de la console scientifique privée

Ce document décrit l'exploitation et le rollback de la Phase 4A.2. Il
ne contient aucun secret.

## Accès

URL :

```text
https://pyrorisk.187.77.161.204.sslip.io/science
```

Les credentials sont gérés hors Git sur le VPS :

```text
/opt/pyrorisk/secrets/science-basic-auth-20260728T191700Z.txt
```

Ce fichier doit rester en mode `0600`. Ne jamais copier son contenu dans
un ticket, un rapport, une commande enregistrée ou un log.

## État déployé

```text
répertoire: /opt/pyrorisk/deploy/oracle
image: erytheon:phase4a2-science-84903938
commit: 849039385a14f95df0a95cca69e5987d3b311478
image id: sha256:08f813aff1080169421c7d6ec46c3764b2409468588e309ed094cd5e0d95f6a1
flag: SCIENCE_CONSOLE_ENABLED=true
```

Configuration précédente :

```text
/opt/pyrorisk/phase4a2-rollback/20260728T191700Z
```

Image précédente :

```text
erytheon:phase3b2-2fca1d9
sha256:656c85c3124d710bf6e5768913e03eb3e46b933f3f80f572faa8bbf80505b531
```

Backup pré-déploiement :

```text
/opt/pyrorisk/backups/pyrorisk-20260728T190714Z.dump
SHA-256 578848146d05e277008fffe900ef4835b0caed64b596b7d60d18636d0a2c3725
```

## Contrôles courants

Sur le VPS :

```bash
cd /opt/pyrorisk/deploy/oracle
docker compose --env-file .env -f compose.yml ps
docker inspect pyrorisk-app-1 \
  --format 'image={{.Config.Image}} id={{.Image}} health={{.State.Health.Status}}'
docker logs --since 15m pyrorisk-app-1
```

Depuis un poste externe, sans credentials :

```bash
curl -o /dev/null -sS -w '%{http_code}\n' \
  https://pyrorisk.187.77.161.204.sslip.io/science
curl -o /dev/null -sS -w '%{http_code}\n' \
  https://pyrorisk.187.77.161.204.sslip.io/api/science/overview
```

Les deux réponses attendues sont `401`.

Avec un mécanisme local qui fournit les credentials sans les écrire sur
la ligne de commande, vérifier :

```text
/science                       200
/science/models                200
/api/science/overview          200 JSON
/api/science/models            200 JSON
/health                        200
```

## Rotation des credentials

1. Générer un mot de passe fort sans l'afficher dans l'historique.
2. Produire le hash avec la même version de Caddy :

   ```bash
   docker run --rm -i caddy:2.10-alpine caddy hash-password
   ```

3. Mettre à jour le fichier secret hors Git et les variables
   `SCIENCE_USER`/`SCIENCE_PASSWORD_HASH` dans
   `/opt/pyrorisk/deploy/oracle/.env`.
4. Conserver le fichier en mode `0600`.
5. Valider puis recréer uniquement Caddy :

   ```bash
   cd /opt/pyrorisk/deploy/oracle
   docker compose --env-file .env -f compose.yml config --quiet
   docker compose --env-file .env -f compose.yml \
     up -d --no-deps --force-recreate caddy
   ```

6. Refaire les contrôles anonyme/authentifié. Ne jamais afficher le
   hash ou le mot de passe dans les logs de validation.

## Validation de Caddy

La protection doit toujours couvrir ensemble :

```text
/science
/science/*
/science.css
/science.js
/api/science
/api/science/*
```

Après toute modification :

```bash
cd /opt/pyrorisk/deploy/oracle
docker compose --env-file .env -f compose.yml config --quiet
docker compose --env-file .env -f compose.yml \
  run --rm --no-deps caddy caddy validate --config /etc/caddy/Caddyfile
```

Le Caddyfile est bind-mounté. Si le fichier a été remplacé
atomiquement, recréer Caddy afin qu'il voie le nouvel inode.

## Rollback applicatif

Le rollback ne restaure pas la base : aucune migration scientifique
n'a été appliquée. Ne restaurer le backup que si une écriture
indésirable est démontrée et après une autorisation séparée.

### 1. Prévalidation

```bash
rollback_dir=/opt/pyrorisk/phase4a2-rollback/20260728T191700Z
deploy_dir=/opt/pyrorisk/deploy/oracle

docker image inspect erytheon:phase3b2-2fca1d9
docker compose \
  --env-file "$rollback_dir/.env" \
  -f "$rollback_dir/compose.yml" \
  config --quiet
```

### 2. Restaurer les fichiers de configuration

```bash
rollback_dir=/opt/pyrorisk/phase4a2-rollback/20260728T191700Z
deploy_dir=/opt/pyrorisk/deploy/oracle

cp "$rollback_dir/.env" "$deploy_dir/.env"
cp "$rollback_dir/compose.yml" "$deploy_dir/compose.yml"
cp "$rollback_dir/Caddyfile" "$deploy_dir/Caddyfile"
chmod 600 "$deploy_dir/.env"
```

La configuration précédente retire le flag science et la protection
associée ; elle référence l'image `erytheon:phase3b2-2fca1d9`.

### 3. Revenir à l'ancienne application

```bash
cd /opt/pyrorisk/deploy/oracle
docker compose --env-file .env -f compose.yml config --quiet
docker compose --env-file .env -f compose.yml up -d --no-deps app

for attempt in $(seq 1 45); do
  status=$(docker inspect pyrorisk-app-1 \
    --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}')
  test "$status" = healthy && break
  sleep 2
done
test "$status" = healthy
```

Ne pas inclure le service PostgreSQL dans cette commande.

### 4. Restaurer le routage Caddy

```bash
cd /opt/pyrorisk/deploy/oracle
docker compose --env-file .env -f compose.yml \
  up -d --no-deps --force-recreate caddy
```

### 5. Contrôler le retour arrière

```bash
docker compose --env-file .env -f compose.yml ps
curl -fsS https://pyrorisk.187.77.161.204.sslip.io/health
curl -o /dev/null -sS -w '%{http_code}\n' \
  https://pyrorisk.187.77.161.204.sslip.io/
curl -o /dev/null -sS -w '%{http_code}\n' \
  https://pyrorisk.187.77.161.204.sslip.io/science
```

Vérifier également dans PostgreSQL :

```sql
SELECT id, active, trained_at
FROM human_model_versions
ORDER BY id;

SELECT id, status, model_family, artifact_checksum
FROM ml.model_candidate_registry
ORDER BY id;
```

Résultat attendu : v1 `id=1 active=true`, candidat `id=1
status=inactive`. Conserver la nouvelle image pour diagnostic.

## Réactivation de la release 4A.2

Après résolution de la cause du rollback :

1. remettre les trois fichiers de déploiement 4A.2 ;
2. confirmer `PYRORISK_IMAGE=erytheon:phase4a2-science-84903938` et
   `SCIENCE_CONSOLE_ENABLED=true` sans afficher les secrets ;
3. valider Compose et Caddy ;
4. remplacer uniquement `app`, attendre `healthy`, puis recréer Caddy ;
5. répéter les contrôles HTTP, modèles et logs.

## Escalade

Effectuer immédiatement le rollback applicatif si :

- l'application ne devient pas healthy ;
- `/health` ou la carte régressent ;
- une route scientifique devient publique ;
- une migration inattendue apparaît ;
- le candidat change de statut ;
- un chargement/scoring candidat ou shadow apparaît dans les logs.

Ne pas tenter de corriger FIRMS, FWI, le scheduler, les modèles ou la
base dans le cadre de ce runbook.
