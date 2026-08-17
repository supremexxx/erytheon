# ERYTHEON — Rapport de déploiement privé Phase 4A.2

Date d'exécution : 2026-07-28 UTC  
Statut : **déploiement terminé**

## Résultat

La console scientifique read-only est disponible à l'adresse :

```text
https://pyrorisk.187.77.161.204.sslip.io/science
```

Les pages, assets et API scientifiques sont protégés par HTTP Basic dans
Caddy. Une requête anonyme reçoit `401`; une requête authentifiée reçoit
`200`. Les identifiants sont configurés hors Git et ne figurent ni dans ce
rapport, ni dans les logs de validation.

Le modèle v1 reste le seul modèle actif. Le candidat `id=1` reste
`inactive`. Aucun scoring candidat, shadow scoring, entraînement, dataset
rebuild ou migration n'a été exécuté.

## 1. Audit Git

État initial :

- HEAD : `1bc3e60331f5adb120d910ecbe4a696fcf408d05`
- arbre propre ;
- branche `main` en avance de 30 commits et en retard de 0 sur
  `origin/main` ;
- aucun secret, dump, fichier temporaire ou artefact non suivi ;
- aucune modification de `crates/risk`, du scoring, de v1, du candidat,
  de FIRMS, de FWI, du scheduler ou des migrations dans les commits de
  console.

Ordre réel des commits demandés :

1. `b2e7f2432a614088c477a389f4ad6fd4fbe914eb` — API scientifique read-only
2. `1516c7d472af2bb5234b3a2a0ce3feb944ec4047` — console scientifique MVP
3. `59afaa7cfbceb6fd6d0d7e4e5b3ace13ae20c7fc` — documentation Phase 4A
4. `f2e7ad05485e8758685dbdc3253b840bc06591f1` — refonte visuelle
5. `1bc3e60331f5adb120d910ecbe4a696fcf408d05` — documentation visuelle

Commits locaux ajoutés pour le déploiement :

- `c5f76d8f7ece0837796a59c586561982b102f642` — préparation Docker,
  Compose et protection Caddy ;
- `ccc4c9f246ecd67880287ea0efbdc36cfefb8bdb` — réponse neutre au probe
  navigateur `/favicon.ico` ;
- `c7b82ba4b80e1ef9e709985a3bbdfcd5d891e109` — libellé
  d'environnement fidèle au VPS ;
- `849039385a14f95df0a95cca69e5987d3b311478` — wrapping des métadonnées
  longues.

Avant le commit de ce rapport, la branche est en avance de 34 commits et
en retard de 0. Aucun push n'a été effectué.

## 2. Audit du VPS avant déploiement

Hôte :

- serveur : `srv1840103`
- compte : `pyrorisk`
- fichiers de déploiement :
  `/opt/pyrorisk/deploy/oracle/compose.yml`,
  `/opt/pyrorisk/deploy/oracle/Caddyfile` et
  `/opt/pyrorisk/deploy/oracle/.env`
- domaine : `pyrorisk.187.77.161.204.sslip.io`
- Caddy : `2.10.2`

Architecture observée :

- `pyrorisk-app-1` sur les réseaux `frontend` et `backend`, port `8080`
  exposé uniquement aux réseaux Docker ;
- `pyrorisk-postgres-1` sur le réseau interne `backend`, volume
  `postgres-data`, port `5432` non publié ;
- `pyrorisk-caddy-1` sur `frontend`, seuls les ports publics 80/443 ;
- données applicatives montées en lecture seule dans `/data` ;
- volumes applicatifs séparés `app-output`, `caddy-data` et
  `caddy-config`.

Version applicative initiale :

- image : `erytheon:phase3b2-2fca1d9`
- image ID :
  `sha256:656c85c3124d710bf6e5768913e03eb3e46b933f3f80f572faa8bbf80505b531`
- SHA-256 du binaire :
  `f0d87d7d4ebfe205a64c81fe32419e20da874627de9c2d6479575f9482716858`
- application healthy, uptime d'environ 36 heures.

Référence HTTP initiale et comportement final identique :

| Route | Avant | Après |
|---|---:|---:|
| `/` | 200 | 200 |
| `/health` | 200 | 200 |
| `/sources` | 200 | 200 |
| `/alerts` | 200 | 200 |
| `/risk` sans bbox | 400 | 400 |

Avant déploiement, PostgreSQL et l'application étaient healthy, Caddy
tournait, les migrations allaient de 1 à 17, aucun verrou long n'était
présent et aucune migration n'était en attente.

## 3. Tests pré-production

Les tests ont été exécutés sur le VPS avec Rust `1.94.1`, dans un
conteneur de build isolé relié uniquement à la base de validation
`erytheon-3b3-deploy-20260727T203310Z` :

```text
cargo fmt --all -- --check                                      PASS
cargo clippy --workspace --all-targets --all-features --locked
  -- -D warnings                                                PASS
cargo test --workspace --locked                                 PASS
```

La suite inclut neuf tests scientifiques et les gardes de rollback. Les
routes ont été vérifiées absentes avec le flag désactivé et présentes
avec le flag activé. La revue de code confirme :

- le flag ne fait que monter les routeurs frontend/API ;
- aucun endpoint scientifique d'écriture ;
- aucun SQL d'écriture dans le store scientifique ;
- aucune dépendance à `score_with_artifact` ;
- échappement HTML/XSS et états loading/error/empty couverts ;
- aucune migration source au-delà de la migration 17 déjà appliquée.

## 4. Build release exact

Image finale :

```text
erytheon:phase4a2-science-84903938
```

Traçabilité :

| Élément | Valeur |
|---|---|
| commit source | `849039385a14f95df0a95cca69e5987d3b311478` |
| archive Git SHA-256 | `3403a8ae647074d0af04eb9b84447bce6f3c8cf7ee944aecd321df8c157a40fa` |
| binaire SHA-256 | `0ef8bbe7eb65ff459e24ce70c7e90a752bc99dcbb067aa2cc6e57914381a8fb5` |
| image ID/digest local | `sha256:08f813aff1080169421c7d6ec46c3764b2409468588e309ed094cd5e0d95f6a1` |
| taille | 136 073 515 octets |
| date OCI UTC | `2026-07-28T19:32:58Z` |
| Rust | 1.94.1 |
| SQLx | 0.8.6 |

Labels OCI :

```text
org.opencontainers.image.revision=849039385a14f95df0a95cca69e5987d3b311478
org.opencontainers.image.created=2026-07-28T19:32:58Z
org.opencontainers.image.title=erytheon
erytheon.phase=phase4A.2
erytheon.science_console=true
```

Commande de build, exécutée dans le répertoire extrait de l'archive Git :

```bash
docker build \
  --build-arg ERYTHEON_GIT_COMMIT=849039385a14f95df0a95cca69e5987d3b311478 \
  --build-arg OCI_REVISION=849039385a14f95df0a95cca69e5987d3b311478 \
  --build-arg OCI_CREATED=2026-07-28T19:32:58Z \
  --build-arg OCI_TITLE=erytheon \
  --build-arg ERYTHEON_PHASE=phase4A.2 \
  --build-arg ERYTHEON_SCIENCE_CONSOLE=true \
  -t erytheon:phase4a2-science-84903938 .
```

Deux images intermédiaires de la même phase ont été construites pendant
les corrections issues de la revue visuelle. Elles ont été remplacées
par l'image exacte ci-dessus et conservées pour analyse.

## 5. Backup PostgreSQL

Backup effectué avant le premier remplacement applicatif :

| Élément | Valeur |
|---|---|
| date UTC | `2026-07-28T19:07:14Z` |
| chemin | `/opt/pyrorisk/backups/pyrorisk-20260728T190714Z.dump` |
| format | PostgreSQL custom |
| taille | 1 805 587 685 octets |
| SHA-256 | `578848146d05e277008fffe900ef4835b0caed64b596b7d60d18636d0a2c3725` |
| `sha256sum -c` | PASS |
| `pg_restore --list` | PASS, 512 lignes lors du contrôle initial |

Les cinq backups antérieurs ont été conservés ; aucun backup n'a été
supprimé.

## 6. Protection et configuration

Caddy protège dans un même matcher :

```text
/science
/science/*
/science.css
/science.js
/api/science
/api/science/*
```

Le hash Caddy et l'utilisateur sont injectés depuis le `.env` du VPS.
Les identifiants récupérables sont stockés hors Git dans :

```text
/opt/pyrorisk/secrets/science-basic-auth-20260728T191700Z.txt
```

Le fichier est en mode `0600`. Le secret n'est pas reproduit ici.

`SCIENCE_CONSOLE_ENABLED=true` est défini en production. Un hash
canonique comparant l'ancien et le nouveau `.env` après exclusion de
`PYRORISK_IMAGE` et des quatre variables science est identique :

```text
1d122b3b597f259d4970e93615d8fa52fb8d0654f266a8456db718f8c8bdb7aa
```

`DATABASE_URL`, FIRMS, FWI, scheduler, territoires et paramètres de
risque sont donc inchangés.

Headers observés sur une réponse scientifique authentifiée :

```text
Referrer-Policy: no-referrer
X-Content-Type-Options: nosniff
X-Frame-Options: DENY
```

Le header `Server` est supprimé sur les réponses proxifiées. Aucun CORS
permissif, directory listing, secret, hash Basic Auth, `DATABASE_URL`,
stack trace ou détail SQL n'est exposé.

## 7. Déploiement

La configuration précédente est sauvegardée dans :

```text
/opt/pyrorisk/phase4a2-rollback/20260728T191700Z
```

L'application finale a démarré à `2026-07-28T19:35:05Z` :

```text
container: pyrorisk-app-1
container id: 784af51534a157d2c73353e628df10c1995895ad5beccd055e397b0d87037877
image: erytheon:phase4a2-science-84903938
image id: sha256:08f813aff1080169421c7d6ec46c3764b2409468588e309ed094cd5e0d95f6a1
health: healthy
```

Le conteneur PostgreSQL est resté le même
(`fbecc890d704…`) pendant toutes les bascules. Il n'a été ni redémarré,
ni recréé et son volume n'a pas été touché.

Le seul message SQL de démarrage est la notice PostgreSQL indiquant que
`_sqlx_migrations` existe déjà ; aucune migration nouvelle n'a été
appliquée.

## 8. Vérifications HTTP et read-only

Sans credentials, `/science`, une route profonde, les deux assets et les
API overview/models répondent tous `401`.

Avec credentials, les neuf routes frontend répondent `200 text/html`,
les assets `200` avec le bon type, et les endpoints suivants répondent
`200` avec du JSON valide :

```text
/api/science/overview
/api/science/progress
/api/science/sources
/api/science/imports?limit=5&offset=0
/api/science/pipelines?limit=5&offset=0
/api/science/data-quality
/api/science/data-quality/events?limit=5&offset=0
/api/science/features
/api/science/calendar
/api/science/datasets
/api/science/models
/api/science/system
```

Le registre de datasets de production est actuellement vide. La liste
retourne `[]`, l'UI affiche son état vide réel et une ressource de détail
inconnue retourne une erreur JSON structurée `dataset_not_found`.

`POST`, `PUT`, `PATCH` et `DELETE` sur
`/api/science/overview` retournent tous `405` avec une réponse JSON.

## 9. Validation SQL/UI et modèles

Comparaison directe PostgreSQL/API/UI :

| Donnée | Valeur |
|---|---:|
| événements BDIFF | 15 956 |
| cause humaine connue | 7 094 |
| cause naturelle connue | 791 |
| cause inconnue | 8 071 |
| versions de dataset | 0 |
| migrations | 17, maximum 17 |
| modèle v1 | id 1, actif |
| `trained_at` v1 | `2026-07-24 20:55:48.823149+00` |
| candidat | id 1, `inactive`, `gbm_isotonic_v2` |
| checksum candidat | `868333c5afc0898ff4dc0cb3a4c922eae851fd28ecca1834e666bc40833fcd74` |
| tables shadow applicatives | 0 |

La progression affiche P1 et P2 terminés et P3 non commencé. Il n'y a
qu'un modèle actif, aucune nouvelle ligne de modèle et aucun changement
de `trained_at`. Les logs contiennent zéro occurrence des signatures
de chargement/scoring candidat ou shadow recherchées.

## 10. Validation visuelle

Une session Chromium réelle et authentifiée a validé :

- overview à 1440 px ;
- progression à 1280 px ;
- qualité des données à 1024 px ;
- datasets à 375 × 812 px ;
- modèles à 1280 px, y compris un tooltip.

Les captures sont conservées localement sous
`output/playwright/phase4a2/` et ignorées par Git. Les styles et le
JavaScript chargent en `200`, les chiffres sont ceux de production, la
navigation et les états sont corrects, aucun mixed content ni 404 asset
n'est observé et la console navigateur contient 0 erreur et 0 warning.
Le contrôle final de la page Modèles confirme
`scrollWidth == clientWidth == 1280`.

La validation a conduit à trois corrections avant l'image finale :

1. réponse `204` au probe favicon pour supprimer un 404 sans importance ;
2. remplacement du libellé trompeur « validation isolée » par
   « production VPS » sur un hôte distant ;
3. wrapping des identifiants et checksums longs.

## 11. Performance

Dix requêtes séquentielles authentifiées, sans charge agressive :

| Endpoint | p50 | p95 approx./max | erreurs |
|---|---:|---:|---:|
| overview | 90,16 ms | 100,92 ms | 0/10 |
| data-quality | 35,57 ms | 41,13 ms | 0/10 |
| datasets | 10,51 ms | 16,94 ms | 0/10 |
| models | 13,76 ms | 16,72 ms | 0/10 |
| system | 87,96 ms | 107,09 ms | 0/10 |

Tous les objectifs p95 sont respectés.

## 12. Rollback

Le rollback est purement applicatif :

- ancienne image présente et inspectable ;
- configuration précédente sauvegardée ;
- ancien Compose validé avec `docker compose config --quiet` ;
- ancien Caddyfile validé avec Caddy 2.10 ;
- commandes documentées dans
  `SCIENTIFIC_CONSOLE_DEPLOYMENT_RUNBOOK.md`.

Un dry-run a été préféré à une bascule complète supplémentaire pour ne
pas provoquer un redémarrage inutile du scheduler de production. Aucune
restauration de base n'est requise.

## 13. Incidents et risques ouverts

- Lors de la création du backup, `pg_dump` avait terminé mais le script
  d'encapsulation n'avait pas renommé le `.partial`. Le fichier exact a
  été validé par checksum et `pg_restore --list`, puis renommé
  explicitement. Le backup final est valide.
- Le remplacement atomique du Caddyfile a changé son inode ; le bind
  mount du conteneur nécessitait un `--force-recreate`. La configuration
  active a ensuite été revalidée.
- Chaque redémarrage applicatif lance le scheduler existant, sans lien
  avec le flag science. Le dernier poll FIRMS a reçu 285 lignes, inséré
  0 observation publique et ignoré 285 doublons ; il a créé ses
  métadonnées habituelles d'import. Le forecast de démarrage a rencontré
  le rate limit Open-Meteo après trois retries et a été abandonné par le
  scheduler avec « continuing ». L'application est restée healthy et
  les paramètres scheduler/FIRMS/FWI n'ont pas été modifiés.
- Le registre de datasets et les snapshots de features sont vides en
  production ; l'interface l'affiche honnêtement.
- Les limites scientifiques déjà publiées restent ouvertes :
  temporalité de `cell_static`, règle `any(child)`, vacances scolaires
  absentes et absence volontaire de shadow scoring/P3.

## Conclusion

```text
PHASE 4A.2 PRIVATE VPS DEPLOYMENT COMPLETED
SCIENTIFIC CONSOLE AVAILABLE ON VPS
SCIENCE ROUTES PROTECTED
READ-ONLY BEHAVIOR CONFIRMED
V1 REMAINS ACTIVE
CANDIDATE REMAINS INACTIVE
NO CANDIDATE SCORING
NO SHADOW SCORING
NO PUSH
```
