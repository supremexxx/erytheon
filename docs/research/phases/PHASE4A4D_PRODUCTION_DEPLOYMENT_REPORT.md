# ERYTHEON — Phase 4A.4d production deployment report

Date de clôture : 29 juillet 2026  
Révision applicative déployée : `6d91959cc478063b4f6df2e6757e9b799d79d25d`  
Image : `erytheon:phase4a4c-science-6d91959c`

## Conclusion

La chaîne visuelle 4A.4 a été intégrée dans l'ordre imposé, puis déployée sur
le VPS sans recréer PostgreSQL ni Caddy. La carte opérationnelle historique et
la console scientifique utilisent les données réelles. Le modèle v1 reste le
seul modèle actif, le candidat reste `inactive`, et aucun shadow scoring n'a
été activé.

Le déploiement est considéré comme réussi :

- application `healthy`, avec zéro redémarrage après remplacement ;
- PostgreSQL et Caddy strictement conservés ;
- 17 migrations appliquées et zéro migration en échec ;
- routes opérationnelles publiques disponibles ;
- console scientifique protégée par Caddy ;
- huit pages scientifiques et quatre formats d'écran validés dans Chromium ;
- aucun échec réseau applicatif, aucune erreur JavaScript et aucun débordement
  horizontal pendant la matrice de validation ;
- sauvegarde PostgreSQL vérifiée et retour arrière applicatif préparé avant le
  remplacement.

## Intégration GitHub contrôlée

Les pull requests empilées ont été revues, recalées sur `main` quand
nécessaire, validées par la CI puis fusionnées exclusivement par merge commit :

| Ordre | PR | Révision de tête revue | Merge commit |
| --- | --- | --- | --- |
| 1 | #4 — premium scientific dashboard | `98a459765ab9d37939dd345a95d11f2c5df3e33b` | `41385753ddc9514f001b8eafd2130e41224c85f7` |
| 2 | #5 — visual fidelity and shared map | `b7c2f86c8a69fc7456e271c5355fc452a6630da8` | `e2177695e8e62da5b8cb8397a306b965ddfa9380` |
| 3 | #6 — final UI/UX fidelity pass | `79974d9624f0604a8d911165710a360b44cd200e` | `6d91959cc478063b4f6df2e6757e9b799d79d25d` |

Il n'y a eu ni squash, ni rebase, ni force push, ni réécriture de
l'historique. Chaque tête de PR reste identifiable et ancêtre de la révision
applicative finale.

Le diff cumulé de cette chaîne reste dans le périmètre présentation,
documentation et tests de contrat. Aucun fichier de migration, de stockage,
de moteur de risque, de scoring, de pipeline scientifique ou de déploiement
VPS n'a été modifié. La carte conserve les routes `/config`, `/risk`,
`/risk/cell/*`, `/alerts`, `/health`, `/sources` et `/stream`.

## Contrôles de qualité et de sécurité

Les contrôles suivants ont réussi sur la révision exacte
`6d91959cc478063b4f6df2e6757e9b799d79d25d` :

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked --no-fail-fast
node --check crates/api/static/dashboard.js
node --check crates/api/static/science/science.js
jq empty crates/api/static/science/phases.json
git diff --check
```

Les tests d'intégration Rust ont utilisé une instance PostGIS jetable, supprimée
après la suite. La CI GitHub de `main` a également réussi sur la même révision.

La revue a confirmé :

- 17 fichiers de migration, inchangés par rapport à la production précédente ;
- aucun secret détecté dans les 11 commits de la chaîne ;
- aucun chemin utilisateur local suivi ;
- aucun dump, binaire ou artefact de production ajouté ;
- aucune activation du candidat et aucun champ ou journal de shadow scoring.

## Image reproductible

L'image Linux AMD64 a été construite localement depuis la révision exacte avec
Rust `1.94` dans le builder défini par le Dockerfile.

| Élément | Valeur |
| --- | --- |
| Image ID / manifest list | `sha256:9bf7133acfd2dfb052544b3dd5f1c5051b63598cec968694cde71c7164d0830f` |
| Architecture | `linux/amd64` |
| Taille locale | 136 091 602 octets |
| Label OCI revision | `6d91959cc478063b4f6df2e6757e9b799d79d25d` |
| Phase | `4A.4c` |
| Console scientifique | `true` |
| Carte partagée | `true` |
| SHA-256 du binaire | `fb6ecd1124774c0ce3a3975f2876526e2c22f2eb2c6bfb31272ca2261f68fbf1` |

L'ID et le label de révision ont été revérifiés après chargement sur le VPS.

## Sauvegarde et rollback

Avant toute mutation applicative, un dump PostgreSQL custom a été créé sans
supprimer les sauvegardes existantes :

```text
/opt/pyrorisk/backups/pyrorisk-20260729T153849Z.dump
taille : 1 856 107 968 octets
sha256 : e6e353eb86de690a5d7fca934c21114ffb61b0744305843af99842070abf3ca7
catalogue pg_restore : 512 entrées
```

Le checksum a été validé et le catalogue a été relu par la version
`pg_restore` du conteneur PostgreSQL.

Le dossier de rollback
`/opt/pyrorisk/phase4a4d-rollback/20260729T163117Z` contient les configurations
précédentes, les inspections des trois conteneurs, les identifiants
pré-déploiement et l'image précédente
`erytheon:phase4a3-science-36027bf`. Le rollback prévu ne recrée que le service
`app`.

## Déploiement

L'unique variable modifiée dans la configuration de production est la
référence `PYRORISK_IMAGE`. La configuration Compose a été validée avant
application.

La commande de remplacement a ciblé uniquement `app` avec `--no-deps`.

| Service | Résultat |
| --- | --- |
| Application | recréée, `healthy`, zéro redémarrage |
| PostgreSQL | identifiant inchangé `fbecc890d704…` |
| Caddy | identifiant inchangé `a6eeb463f730…` |

L'application a démarré le 29 juillet 2026 à 16:32 UTC. Ses journaux de
démarrage ne contiennent ni erreur, ni panique, ni migration en échec, ni
trace de shadow scoring. Le scheduler opérationnel existant et l'import FIRMS
ont repris normalement.

## Validation de production

### État scientifique et opérationnel

```text
app_status              ok
db_status               ok
migrations_applied      17
migrations_failed       0
active_model_id         1
active_model_count      1
candidate_id            1
candidate_status        inactive
cell_static_total       920016
ignition_events_total   15956
```

La carte opérationnelle retourne une vraie `FeatureCollection` de 2 000
cellules au seuil testé, avec des géométries H3 et des scores réels. La route
des alertes retourne 100 alertes au même seuil.

Les routes publiques `/health`, `/`, `/config`, `/alerts` et `/sources`
répondent `200`. Les accès anonymes à `/science`, `/science/overview` et
`/api/science/overview` répondent `401`.

Le secret Basic Auth n'est pas stocké dans le dépôt ni réintroduit dans les
commandes de cette phase. L'invariance du conteneur et de la configuration
Caddy, le refus anonyme externe, ainsi que les réponses `200` de toutes les
routes UI/API directement derrière Caddy confirment que le contrôle existant
et l'application protégée restent raccordés. La valeur du credential n'a pas
été extraite ni affichée pendant cette intervention.

### Validation navigateur

La validation a été exécutée dans Chromium contre le conteneur réellement
déployé, via un tunnel SSH temporaire fermé après le test.

- `/science/overview` : `200` à 1440×900, 1280×800, 1024×768 et 375×812 ;
- `/science/sources`, `/science/data-quality`, `/science/features`,
  `/science/datasets`, `/science/models`, `/science/system` et
  `/science/progress` : `200` ;
- aucune erreur JavaScript ;
- aucune réponse applicative en erreur ;
- aucun débordement horizontal de page ;
- carte scientifique chargée avec 2 000 zones et contrôles Leaflet ;
- carte opérationnelle originale chargée avec ses contrôles, 2 000 zones et
  100 alertes.

Les captures locales de validation ne contiennent aucun credential et ne sont
pas suivies par Git.

### Performance légère

Dix requêtes séquentielles ont été exécutées par endpoint depuis le conteneur
applicatif :

| Endpoint | p50 | p95 / max | Taille moyenne | Erreurs |
| --- | ---: | ---: | ---: | ---: |
| science overview | 0,103 s | 0,202 s | 574 o | 0 |
| risk nowcast, 2 000 cellules | 0,280 s | 0,383 s | 635 326 o | 0 |
| alerts nowcast, 100 lignes | 0,173 s | 0,258 s | 20 116 o | 0 |

Après validation, l'application utilisait environ 25 MiB, PostgreSQL environ
2,39 GiB et Caddy environ 14 MiB. Le disque conservait environ 45 GiB libres.

## Publication

La publication distingue la révision applicative effectivement construite de
la révision documentaire finale :

```text
v0.4.3-app -> 6d91959cc478063b4f6df2e6757e9b799d79d25d
v0.4.3     -> merge commit documentaire contenant ce rapport
```

Les tags antérieurs ne sont ni déplacés ni recréés. La release `v0.4.3`
documente cette distinction et conserve la révision applicative comme source
de vérité du binaire en production.

## Limites et suite

Une source météo signale encore une erreur récente dans le contexte
opérationnel exposé ; le défaut est antérieur au redesign et n'empêche ni la
carte nowcast, ni la console, ni les imports FIRMS. Il doit rester sous
surveillance opérationnelle.

Cette clôture n'autorise aucune Phase 4B, aucun nouveau dataset, aucune
activation de modèle et aucun shadow scoring. La prochaine évolution doit
faire l'objet d'un périmètre et d'une revue distincts.

```text
PHASE 4A.4D COMPLETED
STACKED PRS MERGED IN ORDER
APPLICATION-ONLY DEPLOYMENT COMPLETED
POSTGRESQL AND CADDY PRESERVED
V1 ACTIVE
CANDIDATE INACTIVE
NO SHADOW SCORING
ROLLBACK READY
```
