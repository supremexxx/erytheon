# Console scientifique ERYTHEON — Architecture (Phase 4A)

## 1. Objectif et périmètre

La console scientifique est une interface **strictement en lecture seule** exposant l'état réel
du projet ERYTHEON : sources de données, qualité, features, datasets, modèles (v1 actif et
candidat), et intégrité système. Elle ne fournit **aucune** action opérationnelle : pas
d'activation de modèle, pas de désactivation de v1, pas d'enregistrement de candidat, pas
d'entraînement, pas de reconstruction de dataset, pas d'import, pas de migration, pas de
déclenchement de scheduler, pas de shadow scoring, pas d'édition de table, pas de SQL libre.

Tout endpoint de `/api/science/*` est un `SELECT` (ou un agrégat en lecture) exécuté via
`sqlx::query_as`/`query_scalar`. Aucun ne contient d'`INSERT`/`UPDATE`/`DELETE`.

## 2. Audit de l'existant (préalable à toute décision)

- **Frontend existant** : aucun outillage de build (pas de `package.json`, pas de npm/pnpm/yarn,
  pas de TypeScript/JSX). Une unique page HTML/CSS/JS vanilla
  (`crates/api/static/{index.html,dashboard.css,dashboard.js}`), embarquée dans le binaire via
  `include_str!` et servie directement par des routes `axum`. Design system en variables CSS
  (`--paper/--ink/--accent/--amber/--green/--danger`), tooltips via `data-tooltip`, accessibilité
  (`aria-*`, skip-link), état géré en JS natif (`querySelector` + `fetch`), carte Leaflet (CDN).
  → **Décision** : la console scientifique réutilise exactement ce pattern (vanilla JS, mêmes
  variables CSS, même conventions), sans introduire de framework ni de dépendance de build.
- **API existante** : un seul fichier `crates/api/src/lib.rs`, routeur `axum::Router`, enveloppe
  d'erreur typée `ApiError{status,code,message}` → `{"error":{"code","message"}}`,
  `AppState` porteur du `Store`/`H3Grid`/canal de diffusion/AOI. **Aucune authentification
  n'existe nulle part dans cette API.**
- **Conventions de schéma** : toutes les colonnes UUID sont castées `::text` et représentées en
  `String` côté Rust (le crate `uuid` n'est jamais utilisé, il n'est même pas une dépendance du
  workspace) — confirmé par grep sur `crates/store/src/dataset.rs`. La console suit cette
  convention à l'identique.
- **Corrections apportées aux hypothèses initiales de la mission**, après audit direct du
  schéma (17 migrations) :
  - `reference.data_sources` (migration 0009) existe mais les données réelles de santé des
    sources (`Store::source_statuses()`) proviennent d'une table **différente**,
    `public.source_status` (migration 0005). La console fait un `LEFT JOIN` explicite entre les
    deux plutôt que de supposer une seule source de vérité.
  - `fire.ignition_events.geographic_quality` est contraint en base (`CHECK`) à une unique
    valeur (`precision_undocumented`). La classification enrichie à 8 catégories que la mission
    voulait afficher vit en réalité dans `validation.event_geographic_quality.geographic_category`
    — c'est cette table que la page « Qualité des données » interroge.
  - `ml.dataset_versions` porte un trigger `BEFORE UPDATE` (`dataset_versions_finalized_immutable`)
    qui refuse toute modification une fois `status='finalized'` — renforce la garantie de
    lecture seule côté base, indépendamment de la console.

## 3. Architecture retenue

```
crates/store/src/science.rs   <- couche requêtes (Store::science_*), DTOs Serialize
crates/api/src/science.rs     <- handlers HTTP minces, /api/science/* router
crates/api/static/science/    <- index.html, science.css, science.js, phases.json
crates/engine/src/config.rs   <- SCIENCE_CONSOLE_ENABLED (déploiement, pas une auth)
crates/engine/src/main.rs     <- commande PreviewScienceConsole (sans scheduler)
```

Séparation stricte : toute la logique SQL vit dans `store::science` (jamais dans un handler
HTTP), suivant le pattern `Store` déjà en place dans le projet (`model_candidate.rs`,
`dataset.rs`, etc.). Les handlers de `api::science` ne font qu'appeler une méthode `Store` et
sérialiser le résultat en JSON, en réutilisant l'enveloppe d'erreur existante.

## 4. Exposition et contrôle d'accès

`AppState` porte un champ `science_console_enabled: bool`, piloté par la variable d'environnement
`SCIENCE_CONSOLE_ENABLED` (défaut `"false"`, voir `crates/engine/src/config.rs`). Quand ce
drapeau est faux, **aucune route `/science*` ni `/api/science/*` n'est montée dans le routeur**
(`crates/api/src/lib.rs`, `pub fn router`) — pas un contrôle d'accès qui répond 403/401 après
coup, mais une absence pure et simple des routes.

**Avertissement explicite et documenté** : ce drapeau est un **verrou de déploiement**, pas une
authentification réelle. Aucun mécanisme d'authentification n'existe ailleurs dans cette API
(confirmé par l'audit du point 2). Tant qu'aucune authentification n'est ajoutée au projet, la
console ne doit jamais être activée sur une instance exposée publiquement. Cette phase ne
déploie rien en production : validation faite uniquement via une commande de prévisualisation
dédiée (`PreviewScienceConsole`, voir §6) contre la base isolée de test.

## 5. Stockage de l'historique de progression

`/science/progress` lit `crates/api/static/science/phases.json`, un fichier **versionné dans le
dépôt**, pas une table de base de données. Alternative rejetée : créer une table
`project_phases` aurait nécessité de rétro-dater artificiellement des phases déjà terminées
(hash de commit connu mais horodatage de complétion non tracé nativement) — un choix jugé moins
honnête qu'un fichier JSON explicitement daté par les commits Git réels qu'il référence.

## 6. Prévisualisation sans effets de bord

`Command::Run` (le binaire opérationnel) démarre `scheduler::spawn(...)`, qui interroge FIRMS et
la météo — explicitement interdit pour cette phase. Une commande dédiée
`Command::PreviewScienceConsole { bind }` a donc été ajoutée : elle construit le même `AppState`
et le même routeur, mais **ne démarre jamais le scheduler et ne charge aucun modèle de risque**.
C'est un serveur de prévisualisation en lecture seule, pas le service opérationnel.

## 7. Comparaison v1 / candidat (phase 3B.8)

Les métriques appariées v1-vs-candidat (AP, ROC-AUC, lift, IC 95 %) ont été calculées une seule
fois en phase 3B.8 et publiées dans `PHASE3B8_PROMOTION_GAP_REPORT.md` ; aucune table ne les
stocke. Plutôt que de les re-dériver silencieusement (risque de dérive) ou de les inventer, elles
sont exposées via une constante Rust explicite et versionnée
(`api::science::phase_3b8_comparison()`), avec un champ `source` indiquant clairement qu'il ne
s'agit pas d'une requête en direct. Voir `SCIENTIFIC_CONSOLE_DATA_CONTRACTS.md`.

## 8. Ce qui est délibérément hors périmètre (reporté à la phase 4B)

- Cartographie avancée (au-delà d'un lien vers le dashboard opérationnel existant).
- Exploration détaillée événement par événement au-delà de la table paginée actuelle.
- Shadow scoring (P3, non commencé).
- Comparaison temporelle en direct (au-delà de la constante phase 3B.8).
- Export de données (CSV/Parquet) depuis la console.
