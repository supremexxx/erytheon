# Phase 4A.5 — Décision d'architecture des snapshots (gate obligatoire)

Ce document doit être validé avant toute implémentation du stockage scientifique complet
(§41 de la commande de phase). Il ne modifie ni le code ni le schéma.

## 1. Hypothèses de volumétrie

- `cell_static` ≈ **920 016 cellules** (H3 résolution 9, confirmé par
  `PHASE4A5_SNAPSHOT_SOURCE_AUDIT.md` §1).
- Les features **statiques ou lentement variables** (`hist`, `wui`, `road`, `agri`, `combustible`,
  `population`, `poi`, `power_line`, calendrier) sont déjà versionnées par bundle via
  `features.feature_snapshots` (audit §8) — elles ne doivent **pas** être recopiées à chaque
  snapshot dynamique, seulement référencées par `static_snapshot_id` (§17 de la commande).
- Le contenu réellement **dynamique** à capturer par cellule/instant est donc réduit à : météo
  (température, humidité, vent, précipitations ≈ 5 valeurs) + FWI (FFMC/DMC/DC/ISI/BUI/FWI = 6
  valeurs), soit ~11 valeurs numériques + identité (h3, valid_at, horizon, référence bundle
  statique).
- Estimation de taille par ligne (tuple PostgreSQL, en-tête + colonnes `real`/`smallint`/`bigint`
  + `timestamptz`, sans TOAST) : **≈ 130–160 octets bruts**. Avec un index composite
  `(h3, valid_at, horizon)` : **≈ +40 %**, soit environ **200 octets par ligne effective**.
  Ce sont des ordres de grandeur, pas une mesure — à confirmer par un `EXPLAIN (ANALYZE, BUFFERS)`
  réel lors du pilote.

## 2. Scénarios chiffrés

| Scénario | Lignes/capture | Fréquence | Taille brute/capture | +index (≈+40%) | Taille/an |
|---|---:|---|---:|---:|---:|
| Quotidien complet, 1 horizon (nowcast) | 920 016 | 1×/jour | ~138 MB | ~193 MB | **~70 GB/an** |
| Quotidien complet, 4 horizons | 3 680 064 | 1×/jour | ~552 MB | ~773 MB | **~280 GB/an** |
| Hebdomadaire complet, 4 horizons | 3 680 064 | 1×/semaine | ~552 MB | ~773 MB | **~40 GB/an** |
| Hebdomadaire complet, 1 horizon (nowcast) | 920 016 | 1×/semaine | ~138 MB | ~193 MB | **~10 GB/an** |
| Par forecast complet (naïf, ~24×/jour si captation à chaque batch) | 3 680 064 | ~24×/jour | ~552 MB | ~773 MB | **~280 TB/an** — disproportionné, à rejeter d'office |
| Metadata-only (manifeste, pas de lignes par cellule) | 1 | horaire ou quotidien | ~0,5–1 KB | négligeable | **< 1 MB/an** |

Le scénario « par forecast complet » n'est acceptable que s'il capture **un seul batch officiel
par jour** (le dernier complet à une heure de référence fixe), pas chaque exécution du scheduler
horaire — sinon son coût rejoint le scénario « quotidien × 4 horizons » multiplié par ~24, ce qui
est manifestement disproportionné pour un VPS partagé avec PostgreSQL opérationnel, les données
FIRMS/BDIFF brutes et les sauvegardes (`deploy/oracle/backup-r2.sh`).

Le scénario « différentiel » n'apporte pas de gain démontré pour la couche météo/FWI : les valeurs
varient spatialement de façon quasi continue (interpolation IDW), donc la plupart des cellules
changent chaque jour — un diff toucherait presque autant de lignes qu'une capture complète, pour
une complexité de code supplémentaire (reconstruction par rejeu). Il n'est donc pas retenu pour
cette phase.

## 3. Contrainte VPS

Le dépôt cible un VPS Oracle (`deploy/oracle/`), architecture ARM (mention explicite de
"correction du cache Cargo arm64" comme hors périmètre dans la commande, cohérente avec un
déploiement Ampere A1), hébergeant déjà PostgreSQL, l'application, Caddy et les sauvegardes R2 sur
le même hôte. Aucune information de capacité disque exacte n'a pu être vérifiée depuis cette
session (pas d'accès VPS direct — limite déjà signalée dans l'audit). Par prudence, un budget
disque additionnel de l'ordre de la **dizaine de gigaoctets par an**, pas de la centaine, est
retenu comme raisonnable sans validation explicite de capacité.

**Conséquence** : les scénarios « quotidien complet » (70–280 Go/an) et « par forecast naïf »
sont **rejetés pour un déploiement automatique en routine**. Seul un scénario hebdomadaire à
horizon unique (~10 Go/an) ou un scénario metadata-only (négligeable) reste dans le budget prudent
sans confirmation VPS complémentaire.

> **STOP BEFORE FULL SCIENTIFIC STORAGE** (§41 de la commande) : la conception « snapshot
> scientifique complet quotidien, tous horizons » n'est **pas retenue** en l'état. L'observabilité
> opérationnelle peut avancer intégralement ; le stockage scientifique de valeurs est ramené à un
> pilote borné (§5).

## 4. PostgreSQL vs fichiers (Option A/B/C, §9 de la commande)

- **Option B (Parquet/Arrow/JSONL + manifeste PostgreSQL)** : écartée pour ce pilote. Elle ajoute
  une dépendance (écriture de fichiers immuables, gestion d'un espace de stockage hors base,
  cohérence manifeste/fichier, backup séparé) alors que le volume retenu (~10 Go/an, scénario
  hebdomadaire à horizon unique) reste largement dans les capacités d'une table PostgreSQL
  indexée. Cette option redevient pertinente si la cadence ou la couverture augmentent
  significativement dans une phase ultérieure — à documenter comme option de repli, pas à
  implémenter maintenant.
- **Option C (différentiel)** : écartée pour la couche dynamique (§2), conservée implicitement
  pour la couche statique — déjà couverte par `features.feature_snapshots` (référencement par
  `static_snapshot_id`, pas de nouvelle mécanique).
- **Option A retenue (tables PostgreSQL partitionnées)** pour les valeurs scientifiques, à cadence
  hebdomadaire et horizon unique (`nowcast`) pour ce pilote. Partitionnement mensuel par
  `valid_at` (§34 de la commande) : bénéfice réel seulement après plusieurs mois d'accumulation ;
  pour la taille pilote (~10 Go/an), une table non partitionnée avec index `(h3, valid_at)` est
  suffisante au départ. Le partitionnement est documenté comme évolution différée, pas implémenté
  dans cette phase, pour éviter la complexité SQLx/migrations qu'il introduirait sans bénéfice
  mesurable à ce volume.

## 5. Architecture retenue (pilote)

```
Niveau 1 — observability.system_snapshots         PostgreSQL, quotidien + horaire léger, permanent
Niveau 2 — features.feature_snapshots (existant)   déjà en place, référencé, pas dupliqué
           observability.scientific_snapshots      manifeste PostgreSQL, permanent (métadonnées seules)
Niveau 3 — observability.scientific_snapshot_values PostgreSQL non partitionné, HEBDOMADAIRE,
                                                     horizon nowcast uniquement, pilote
```

- Le niveau 3 ne capture que l'horizon `nowcast`, une fois par semaine, en référençant le dernier
  bundle statique publié (`static_snapshot_id` → `features.feature_snapshots.id`) plutôt que de
  dupliquer les features statiques.
- Le niveau 2 (manifeste) peut tourner plus souvent (quotidien) sans coût significatif, y compris
  pour des jours où aucune capture de valeurs n'est faite — le manifeste peut alors documenter
  `status=metadata_only` ou équivalent pour les jours sans capture complète, évitant de laisser
  croire à une absence de snapshot.
- Extension à une cadence quotidienne ou multi-horizon : possible dans une phase ultérieure, mais
  conditionnée à une confirmation explicite de capacité disque VPS (mesure réelle, pas estimation),
  compte tenu de l'écart ×7 à ×28 avec le pilote retenu.

## 6. Atomicité, rétention et coûts opérationnels

- Publication atomique : `building → validated → published` avec trigger d'immutabilité identique
  au patron `dataset_versions_finalized_immutable` (audit §9) — `BEFORE UPDATE` refusant toute
  mutation d'une ligne `published`, complété par une règle applicative (pas seulement SQL) contre
  la suppression, le trigger ne couvrant pas nativement `DELETE`.
- Rétention proposée pour le pilote (à raffiner en §25/`PHASE4A5_RETENTION_POLICY.md`) :
  manifestes et alertes conservés indéfiniment (coût négligeable) ; valeurs hebdomadaires
  conservées en routine sans purge automatique tant que le volume annuel reste sous ~10–15 Go —
  toute suppression reste `--dry-run` uniquement dans cette phase, jamais activée.
- Coût de sauvegarde : ~10 Go/an de valeurs scientifiques s'ajoutent au flux `backup-r2.sh`
  existant — impact marginal comparé aux scénarios rejetés.
- Temps d'écriture estimé pour une capture hebdomadaire (920 016 lignes) : à mesurer lors du
  pilote via des lots bornés (`batch INSERT` de quelques milliers de lignes, transactions
  courtes) plutôt qu'un `INSERT` unique massif, conformément au §36 de la commande.

## 7. Risques ouverts

1. Estimation de taille de ligne non mesurée en conditions réelles (TOAST, alignement, `fillfactor`)
   — à valider par un pilote isolé avant décision définitive de cadence.
2. Capacité disque VPS réelle non vérifiée depuis cette session — le budget "dizaine de Go/an"
   est une hypothèse prudente, pas une donnée confirmée.
3. Le dernier forecast complet est écrasé par `retain_forecast_batch` avant capture possible si le
   job de snapshot hebdomadaire et le job forecast se chevauchent — nécessite une capture
   déclenchée immédiatement après un `forecast_batches.completed_at` connu, pas une lecture
   asynchrone tardive (à concevoir précisément en §18 de la commande).

## 8. Conclusion de la gate

```
PHASE 4A.5 ARCHITECTURE DECISION
OPERATIONAL OBSERVABILITY: FULL IMPLEMENTATION AUTHORIZED (level 1+2, negligible cost)
SCIENTIFIC METADATA MANIFESTS: FULL IMPLEMENTATION AUTHORIZED (negligible cost)
SCIENTIFIC VALUE STORAGE: NAIVE DAILY / MULTI-HORIZON DESIGN REJECTED (disproportionate: 70-280 GB/year)
SCIENTIFIC VALUE STORAGE: SCOPED PILOT RECOMMENDED (weekly, nowcast-only, ~10 GB/year, PostgreSQL non partitioned)
NO FULL SCIENTIFIC STORAGE UNTIL PILOT VOLUME IS MEASURED IN PRODUCTION
```
