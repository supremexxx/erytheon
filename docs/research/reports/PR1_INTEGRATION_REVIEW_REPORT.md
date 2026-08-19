> **Document historique — archivé le 20 août 2026.**
> Ce rapport documente une revue ponctuelle réalisée le **28 juillet 2026**
> sur une pull request dans son **état de l'époque** (têtes de branche,
> résultats CI et déploiement observés ce jour-là). Le verdict
> **« BLOQUÉ »** ci-dessous ne décrit **pas** l'état actuel de `main` — la
> PR concernée a depuis été fusionnée, et les défauts qu'elle documente
> (échec Clippy, fixtures de test non autonomes, reconstruction Docker
> incomplète) doivent être réévalués à partir de l'historique Git et des
> validations exécutées aujourd'hui, pas à partir de ce texte. Conservé
> comme trace d'audit, pas comme référence de l'état courant du projet.

# ERYTHEON — Rapport de revue d’intégration de la PR #1

Date de la revue : 2026-07-28  
Mode : lecture seule  
Verdict : **BLOQUÉ** (verdict historique — voir l'avertissement ci-dessus)

Ce rapport documente les vérifications réalisées sans fusion, réécriture
d’historique, modification applicative, push, tag, release ou intervention sur
la production.

## 1. Identification de la PR

- Dépôt : `supremexxx/erytheon`
- PR : `#1 — Deploy the private scientific console and candidate validation pipeline`
- Branche source : `agent/phase4a2-private-science-console`
- Branche cible : `main`
- Statut : ouverte, draft
- Contenu GitHub constaté : 35 commits, 87 fichiers, `+20 835 / -122`

## 2. SHA de base

La base distante effective de la PR et `origin/main` sont :

```text
2fca1d920fa9d87674e1574a88633fcc88eb4cb1
```

Le `main` local était déjà avancé jusqu’à la tête de revue. Il ne doit donc pas
être utilisé comme preuve de l’état de GitHub ; les contrôles de relation ont
été faits contre `origin/main` et la base déclarée par la PR.

## 3. SHA de tête

La branche distante et la tête GitHub de la PR correspondent exactement à :

```text
7f87ddaef4dbe177c476e02432b63d8c83994d31
```

La revue et la CI locale ont été réalisées en checkout détaché de ce SHA.

## 4. SHA applicatif déployé

La révision applicative déclarée comme construite et déployée est :

```text
849039385a14f95df0a95cca69e5987d3b311478
```

L’archive Git de ce commit reconstruite localement avait l’empreinte :

```text
3403a8ae647074d0af04eb9b84447bce6f3c8cf7ee944aecd321df8c157a40fa
```

Cette empreinte correspond à celle consignée dans le rapport de déploiement.

## 5. Analyse de l’historique

- La plage `2fca1d9..7f87dda` contient exactement 35 commits.
- L’historique de cette plage est linéaire.
- Aucun commit manquant ou parasite n’a été identifié.
- `2fca1d9` est un ancêtre de `7f87dda`.
- Le merge-base entre `origin/main` et la tête est exactement `2fca1d9`.
- Dans l’état distant observé pendant la revue, un fast-forward est donc
  techniquement possible.
- Aucun rebase, squash ou autre réécriture n’est nécessaire ni recommandé.
- Les 35 SHA existants doivent être conservés.

Le constat de fast-forward vaut pour la tête revue. Toute correction ajoutée
après cette revue devra faire l’objet d’une nouvelle vérification du nouveau
SHA et de l’état alors courant de `origin/main`.

## 6. Analyse des secrets

Une analyse de la plage complète des 35 commits a été exécutée avec
`gitleaks 8.30.1` en mode lecture seule et avec masquage intégral :

```text
35 commits analysés
environ 884 893 octets analysés
0 fuite détectée
```

La recherche textuelle large demandée a produit de nombreux faux positifs :
noms de variables, fixtures de test, exemples de configuration et
documentation contenant notamment `password`, `DATABASE_URL`, `api_key`,
`basic_auth` ou `authorization`.

Constats complémentaires :

- aucun bloc de clé privée ;
- aucun hash bcrypt littéral ;
- aucune URL contenant des identifiants ;
- aucun token ou secret exploitable trouvé ;
- seuls `.env.example`, `.env.production.example` et
  `deploy/oracle/.env.example` sont suivis ;
- les valeurs sensibles des exemples sont des placeholders ;
- aucun chemin utilisateur `/Users/...` ni chemin `.ssh` n’est suivi.

La documentation mentionne volontairement une adresse de VPS et des chemins
d’exploitation, par exemple `/opt/pyrorisk/secrets/`, les sauvegardes et les
rollbacks. Aucun contenu secret n’y figure. Cela constitue une exposition
d’information opérationnelle de faible gravité, pas une fuite d’identifiant.

Conclusion : **aucun secret détecté dans la PR**.

## 7. Analyse des artefacts

L’arbre complet de `7f87dda` et les ajouts/suppressions de la PR ont été
inspectés.

Aucun élément indésirable n’a été trouvé :

- aucun dump PostgreSQL ou base locale ;
- aucune archive ;
- aucun journal ou fichier temporaire ;
- aucun cache ou répertoire `target/` ;
- aucun binaire compilé ;
- aucune donnée privée ;
- aucune capture contenant des secrets ;
- aucun artefact de modèle inattendu.

Le plus gros fichier suivi est `Cargo.lock`, d’environ 92 Ko. Aucun fichier
anormalement gros et aucune entrée binaire dans le diff n’ont été détectés.
Les fichiers SQL présents sont les migrations et rollbacks attendus.

## 8. Revue migrations

Les migrations `0013` à `0017` ont été lues intégralement et appliquées sur une
base PostGIS 16 / 3.4 jetable. SQLx a amené une base neuve au niveau 17 et n’a
pas tenté de réappliquer une migration sur une base déjà au niveau 17.

### 0013 — feature snapshot foundation

- ajout du schéma de snapshots de features ;
- contraintes de cohérence et clés étrangères vers les données
  opérationnelles ;
- unicité famille/checksum et unicité partielle d’un snapshot actif par
  famille ;
- aucune écriture destructive sur les tables historiques publiques.

### 0014 — historical calendar foundation

- ajout du calendrier historique avec contraintes de domaine ;
- unicité partielle d’un calendrier actif par type ;
- références protectrices depuis les jours historiques ;
- aucune suppression ou mutation destructive de données existantes.

### 0015 — dataset versioning foundation

- ajout du schéma de versionnage, builds, lignes et métadonnées dataset ;
- contraintes, index et clés étrangères cohérents ;
- déclencheur interdisant la modification d’un dataset finalisé ;
- dépendances vers les fondations `0013` et `0014`.

Risque ouvert : le déclencheur d’immutabilité interdit `UPDATE`, mais pas
directement `DELETE`. Les clés étrangères protègent les datasets ayant des
dépendances et l’application n’expose pas de chemin de suppression ; une
suppression SQL directe d’un dataset finalisé sans dépendance reste cependant
possible.

### 0016 — model candidate registry

- registry séparée du modèle v1 ;
- statuts autorisés limités à `candidate` et `inactive` ;
- aucune valeur `active` possible ;
- aucune activation ou mutation de `human_model_versions`.

### 0017 — model candidate registry identity

- ajout de la graine et de l’identité logique unique ;
- valeur temporaire `0` utilisée pour remplir les lignes préexistantes, puis
  suppression du défaut ;
- aucune suppression destructive.

Risque ouvert : sur un environnement non maîtrisé contenant déjà plusieurs
lignes de même identité logique, la création de l’unicité peut échouer. La
migration ne contient pas de préflight explicite de doublons. Le remplissage de
toutes les anciennes lignes avec la graine `0` suppose également une
réconciliation applicative contrôlée.

Les scripts forward ne sont pas réexécutables directement comme du SQL
arbitraire ; l’idempotence attendue repose correctement sur le ledger SQLx.

## 9. Revue rollbacks

Les cinq rollbacks comportent un `BEGIN` et un `COMMIT` explicites, des gardes
de présence de données et des erreurs bloquantes. Le défaut historique où un
message de refus était affiché avant de poursuivre les `DROP` n’est pas
présent.

Tests sur bases PostGIS jetables :

- `0013` à `0015` : tests automatisés réussis sur base vide, refus réussi sur
  base peuplée, données conservées ;
- `0016` et `0017` : tests manuels réussis sur base vide ;
- `0016` et `0017` : refus non nul confirmé sur base contenant une entrée
  inactive, ligne et objets SQL conservés, message utile.

L’ordre inverse obligatoire est :

```text
0017 → 0016 → 0015 → 0014 → 0013
```

Les rollbacks `0016` et `0017` ne disposent toutefois pas d’une couverture
automatisée permanente dans le dépôt. Leur comportement a été validé
manuellement pendant cette revue, mais l’absence de test de non-régression
reste un risque ouvert.

Aucun rollback n’a été exécuté sur la production.

## 10. Revue scoring / modèles

Le diff n’apporte aucune modification aux composants opérationnels de scoring
v1, notamment au pipeline `/risk`, aux crates `risk` et `fwi`, au pipeline de
prévision ou à la migration `0008_human_model_versions`.

Constats :

- l’enregistrement candidat accepte seulement `candidate` ou `inactive` ;
- la contrainte SQL interdit `active` ;
- l’enregistrement écrit uniquement dans `ml.model_candidate_registry` ;
- la vérification de chargement candidat utilise une transaction read-only ;
- les commandes candidates sont des commandes CLI explicites ;
- l’API scientifique expose uniquement des routes GET ;
- le store de la console scientifique ne contient pas d’écriture SQL ;
- le flag de console ne fait que monter ou non ces routes ;
- aucune intégration de shadow scoring n’a été trouvée dans le serving ;
- aucune activation au démarrage ou substitution du modèle v1 n’a été trouvée.

Conclusion : la PR n’active pas le candidat, ne lance pas de shadow scoring et
ne remplace pas le serving v1.

## 11. Écart production / PR

Le diff exact entre `8490393` et `7f87dda` contient uniquement deux fichiers
Markdown ajoutés, pour 653 lignes :

```text
PHASE4A2_PRIVATE_VPS_DEPLOYMENT_REPORT.md
SCIENTIFIC_CONSOLE_DEPLOYMENT_RUNBOOK.md
```

Le diff hors `*.md` est vide. Il n’existe entre ces deux révisions :

- aucun changement Rust ;
- aucun changement SQL ou de migration ;
- aucun changement Docker ou Caddy ;
- aucun changement frontend ;
- aucun changement de configuration.

L’affirmation « même socle applicatif, documentation supplémentaire » est donc
confirmée par Git.

## 12. Résultats CI

### GitHub Actions sur `7f87dda`

Deux exécutions CI GitHub, une `push` et une `pull_request`, échouent au même
endroit avant les tests. Avec Rust stable 1.97.1 :

```text
crates/api/tests/science.rs:266
clippy::manual_assert_eq
```

L’expression suivante est refusée sous `-D warnings` :

```rust
assert!(system["migrations_failed"].as_i64().unwrap_or(-1) == 0);
```

### CI locale stricte

- `cargo fmt --all -- --check` : **réussi**
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
  avec Rust 1.97.1 : **échoué**, même diagnostic que GitHub
- la même commande Clippy sous Rust 1.94.1 : réussie, ce qui explique la
  divergence liée au `stable` courant non épinglé
- `cargo test --workspace --locked --no-fail-fast` : **échoué**

Deux tests ne sont pas autonomes sur la base CI neuve :

1. `api/tests/science.rs::system_summary_reports_exactly_one_active_model`
   attend exactement un modèle v1 actif, alors que la base jetable initialisée
   par les migrations en contient zéro ;
2. `store/tests/negative_sampling_experiment.rs::
   experimental_negative_sampling_window_comparison` attend des événements de
   cause naturelle qui ne sont pas créés par sa fixture.

Tous les autres groupes de tests exécutés ont réussi, notamment les tests de
fondation dataset, cohérence scientifique, candidate registry, API science,
ingestion, FWI, risk, quality et rollback guards `0013–0015`.

La CI GitHub n’atteint pas actuellement les tests, car Clippy échoue en premier.
Les trois échecs ci-dessus sont bloquants pour l’intégration.

Le workflow GitHub utilise Rust `stable` et exécute une commande Clippy moins
stricte que celle demandée pour cette revue. Il n’épingle pas précisément le
toolchain, ce qui rend les nouveaux lints susceptibles de casser la CI sans
changement du dépôt.

## 13. Reproductibilité du build

Vérifications statiques réussies :

- archive exacte du commit `8490393` et checksum concordant avec le rapport ;
- `Cargo.lock` présent et utilisé avec `--locked` ;
- contexte Docker limité aux sources versionnées nécessaires ;
- assets science inclus dans les sources versionnées ;
- SHA, phase et activation de console transmis par build args ;
- labels OCI prévus ;
- aucune dépendance à `.git` ou à un fichier local non versionné identifiée ;
- les images de base résolvaient aux mêmes digests que ceux consignés lors du
  déploiement.

La reconstruction locale `linux/amd64` a cependant été interrompue par une
défaillance interne de Docker Desktop :

```text
input/output error
containerd-overlayfs/metadata_v2.db
dpkg et rustc incapables d’écrire
```

Le moteur a ensuite signalé un blob containerd illisible. Il s’agit d’une
défaillance de l’environnement local de revue, pas d’une erreur de compilation
attribuable au projet. Aucun identifiant d’image ni hash de binaire local fiable
n’a donc pu être comparé aux valeurs de production :

```text
image production :
sha256:08f813aff1080169421c7d6ec46c3764b2409468588e309ed094cd5e0d95f6a1

binaire production :
0ef8bbe7eb65ff459e24ce70c7e90a752bc99dcbb067aa2cc6e57914381a8fb5
```

La reproductibilité bit-for-bit reste de toute façon limitée par :

- des tags d’images de base non épinglés par digest dans le Dockerfile ;
- `apt-get` et ses versions de paquets non épinglées ;
- les métadonnées BuildKit et l’architecture de construction.

Conclusion : la composition du build est cohérente, mais la reproduction
complète et la comparaison binaire ne sont **pas prouvées** par cette revue.

## 14. État GitHub de la PR

État observé en lecture seule :

- base : `main` à `2fca1d9` ;
- head : `agent/phase4a2-private-science-console` à `7f87dda` ;
- PR ouverte et draft ;
- 35 commits, 87 fichiers ;
- GitHub indique `MERGEABLE` ;
- `mergeStateStatus` : `UNSTABLE` ;
- deux checks CI en échec ;
- aucun conflit signalé ;
- aucune review, décision de review ou discussion relevée.

Aucun statut, commentaire, approbation ou autre élément de la PR n’a été
modifié.

## 15. Risques ouverts

### Bloquants

1. Clippy échoue sur le SHA exact avec le Rust stable actuel, localement et
   dans les deux runs GitHub.
2. Le test de résumé système suppose un modèle actif absent d’une base CI
   neuve.
3. Le test d’expérimentation de negative sampling dépend de données historiques
   non créées par sa fixture.
4. La reconstruction Docker comparative n’a pas pu être achevée à cause de
   l’environnement Docker local ; la concordance bit-for-bit n’est pas prouvée.

### Non bloquants à examiner lors d’une correction séparée

1. absence de tests automatisés des gardes de rollback `0016` et `0017` ;
2. immutabilité des datasets finalisés couvrant `UPDATE`, mais pas explicitement
   `DELETE` ;
3. hypothèses de `0017` sur les lignes préexistantes et les doublons logiques ;
4. toolchain CI `stable` et dépendances système/base Docker non épinglés ;
5. présence d’informations opérationnelles non secrètes dans la documentation.

## 16. Stratégie de fusion recommandée

Ne pas fusionner la tête actuelle.

La taille de la PR ne justifie pas une découpe : les datasets, migrations,
modèle candidat, registry, console et déploiement constituent une chaîne
cohérente, et aucun commit parasite ou défaut de traçabilité n’a été trouvé.
Conserver la PR et ses SHA est préférable.

Une intervention séparée, explicitement autorisée, doit :

1. corriger les échecs Clippy et les deux fixtures de test sans rebase ni
   squash, par un ou plusieurs nouveaux commits ;
2. ajouter idéalement les tests automatisés des rollbacks `0016–0017` ;
3. exécuter la CI complète sur la nouvelle tête ;
4. refaire au minimum les contrôles du delta entre `7f87dda` et cette nouvelle
   tête ;
5. reconstruire `8490393` dans un environnement Docker sain si la preuve
   binaire reste une exigence d’autorisation.

Après correction, un fast-forward restera préférable s’il est encore possible ;
sinon, utiliser un merge commit classique. Ne pas réécrire les 35 commits
existants.

Les tags prévus ne doivent pas être créés maintenant. En particulier, si la
correction ajoute des commits applicatifs ou CI, `v0.4.2` ne pourra plus
représenter à la fois la tête validée et `7f87dda` sans ambiguïté ; cette
décision devra être reprise après validation de la nouvelle tête.

## 17. Verdict

```text
PR #1 INTEGRATION REVIEW BLOCKED
NO MERGE AUTHORIZED
NO TAG AUTHORIZED
NO RELEASE AUTHORIZED
```

Les contrôles Git, secrets, artefacts, migrations, rollbacks, absence
d’activation candidate et écart documentaire avec la production sont
globalement satisfaisants. La PR reste néanmoins non intégrable tant que la CI
du SHA exact échoue. L’absence de preuve complète de reconstruction Docker doit
également être résolue ou explicitement acceptée avant publication.

```text
READ-ONLY REVIEW COMPLETED
INTEGRATION BLOCKED
NO MERGE PERFORMED
NO TAG CREATED
NO RELEASE CREATED
NO PUSH PERFORMED
AWAITING CORRECTION DECISION
```
