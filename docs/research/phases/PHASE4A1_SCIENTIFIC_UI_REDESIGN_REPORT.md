# Phase 4A.1 — Refonte visuelle de la console scientifique — Rapport

## 1. Contexte

La phase 4A a livré une console `/science` fonctionnelle et testée, mais dont le langage visuel
(cartes arrondies, grille de KPI, timeline à cercles colorés, badges en pilule) évoquait un
dashboard SaaS générique plutôt qu'un outil scientifique. Cette phase 4A.1 ne touche ni aux
données, ni aux contrats `/api/science/*`, ni au comportement read-only, ni au modèle actif/
candidat — uniquement `crates/api/static/science/{index.html,science.css,science.js}`.

## 2. BEFORE — problèmes identifiés

Audit de l'interface livrée en phase 4A (captures prises alors, voir conversation précédente) :

- **Cartes KPI surdimensionnées** (`.sci-card`, valeurs à 22px, coins à 8-10px, une carte par
  métrique) sur la vue d'ensemble, les datasets et le système — pattern SaaS classique plutôt
  qu'une matrice dense.
- **Badge « lecture seule » en pilule** (`border-radius: 999px`) et badges de statut également en
  pilule pleine couleur (`.sci-badge-*` avec fond et texte coloré) — trop décoratif pour un statut
  scientifique.
- **Timeline de progression à cercles colorés** (`.sci-timeline` avec `::before` circulaire) —
  esthétique produit plutôt que journal de programme.
- **Police externe** : `index.html` chargeait Inter depuis Google Fonts (CDN), une dépendance
  externe évitable alors qu'une pile système suffit.
- **Palette neutre en gris/noir pur** (`--ink: #0d0d0d`, `--accent: #111111`) sans couleur
  d'accent scientifique dédiée — fonctionnelle mais sans identité de laboratoire.
- **Densité insuffisante** : grille de cartes en `auto-fit, minmax(180px, 1fr)` avec 12px de
  gap et beaucoup d'espace vide autour de chaque valeur, alors que les données (ex. la
  comparaison v1/candidat) auraient dû être en table dès la phase 4A.
- **Page modèles** : la comparaison v1/candidat manquait de colonne d'écart et d'interprétation
  par métrique, et il n'y avait pas de section « Limites scientifiques » séparée — les limites
  étaient reléguées à la vue d'ensemble uniquement.

## 3. Principes retenus

Voir [SCIENTIFIC_UI_STYLE_GUIDE.md](SCIENTIFIC_UI_STYLE_GUIDE.md) pour le détail complet des
tokens. Résumé des décisions :

- Fond légèrement grisé (`#f4f5f3`), jamais blanc pur ; séparation par lignes fines, pas
  d'ombres.
- Palette fonctionnelle restreinte (vert institutionnel `#244c3f` en accent, brun/ocre/bleu-gris
  pour les statuts), aucune couleur uniquement décorative.
- Rayon de coin réduit à 3-4px partout (contre 6-10px avant, 999px sur les badges).
- Police système uniquement (suppression de la dépendance Google Fonts), pile monospace dédiée
  pour tout identifiant technique (checksum, commit, seed, H3).
- Remplacement des grilles de cartes KPI par des grilles de métriques compactes
  (`.sci-metric-grid`, valeurs à 20px maximum) et par des tables partout où une comparaison
  ligne à ligne a du sens (progression, datasets, comparaison de modèles).
- Vue d'ensemble restructurée en quatre zones : bandeau de statut, état scientifique (table),
  indicateurs essentiels (grille compacte), risques ouverts (table priorisée) — remplace la
  double grille de cartes.
- Page modèles enrichie d'une colonne « écart » colorée (vert/rouge selon le sens de
  l'amélioration) et d'une section « Limites scientifiques » dédiée, séparée de l'interprétation
  du score.
- Page progression transformée en table chronologique (phase, intitulé, statut, commit,
  environnement, production affectée, résultat, risques en ligne secondaire) — remplace la
  timeline à cercles colorés.

## 4. Pages refondues (priorité 1)

- `/science/overview` — zones A (bandeau statut) / B (état scientifique) / C (indicateurs) / D
  (risques ouverts).
- `/science/progress` — table chronologique.
- `/science/data-quality` — table de synthèse avec part (%), quatre graphiques à barres annotés
  `n = ...`, table d'exploration inchangée dans sa densité.
- `/science/datasets` — table comparable ligne à ligne (nom logique, variante, statut, seed,
  positifs/négatifs/total/exclusions, checksum) ; détail restructuré en sections Identité /
  Population / Répartition par split / Exclusions.
- `/science/models` — bandeau de statut, table de comparaison avec écart et interprétation,
  artefact candidat en grille de définitions, section « Limites scientifiques » dédiée.

## 5. Pages harmonisées (priorité 2)

`/science/sources`, `/science/features`, `/science/system` : même fondation CSS (tables,
grilles de métriques, badges texte+pastille), sans restructuration de contenu supplémentaire —
leur densité était déjà correcte en phase 4A.

## 6. Vérification visuelle réelle

Effectuée dans un navigateur réel contre la base isolée `erytheon-3b3-deploy-20260727T203310Z`,
via un nouveau conteneur de build éphémère (`erytheon-4a1-build`) et la commande
`preview-science-console`, aux largeurs 1440px, 1280px, 1024px et mobile (375px, capture rendue
à 750px par mise à l'échelle Retina de l'outil de capture).

- **Vue d'ensemble** (1440/1280/1024/mobile) : les quatre zones (bandeau de statut, état
  scientifique, indicateurs essentiels, risques ouverts) s'affichent correctement à toutes les
  largeurs. À 1024px et en mobile, le bandeau de statut passe de 6 colonnes à un flux qui se
  replie sur plusieurs lignes sans perte d'information (`flex: 1 1 160px`). Données réelles
  confirmées : `bdiff_events_total: 15956`, `humains connus: 7094`, candidat `aucun` (le
  candidat de test de la phase 4A a été supprimé lors du nettoyage précédent, donc son absence
  ici est l'état réel et honnête de la base isolée).
- **Progression** : la timeline à cercles colorés a été remplacée par une table de 16 lignes
  (phase, intitulé, statut, commit, environnement, production affectée, résultat), avec deux
  lignes secondaires « Risques ouverts » insérées sous les phases 3B.7 et 3B.9. Commits réels
  affichés en monospace.
- **Qualité des données** : table de synthèse avec part en pourcentage, quatre graphiques à
  barres (causes, qualité géographique, doublons, combustibilité) chacun annoté `n = ...`, table
  d'exploration paginée inchangée. En mobile, les deux colonnes de graphiques repassent
  correctement en une seule colonne.
- **Datasets** : registre à 6 lignes comparables (nom logique, variante, statut, seed, positifs/
  négatifs/total/exclusions, checksum) ; page de détail restructurée en sections Identité/
  Population/Répartition par split/Exclusions, vérifiée sur le dataset
  `erytheon_human_ignition_cell_day_v1_pilot_inclusive`.
- **Modèles** : bandeau de statut (modèle actif, candidat, statut, promotion), table de
  comparaison v1/candidat avec colonne d'écart colorée (`+0.1928`, `+0.3468`, `+1.0500`, tous en
  vert car favorables au candidat), section « Artefact candidat » affichant honnêtement « Aucun
  candidat enregistré » (état réel après nettoyage), section « Limites scientifiques » dédiée
  avec six limites explicites.

Aucune régression visuelle ou fonctionnelle trouvée pendant cette vérification (contrairement à
la phase 4A, où un bug de double-échappement avait été détecté et corrigé — voir
[PHASE4A_SCIENTIFIC_CONSOLE_REPORT.md](PHASE4A_SCIENTIFIC_CONSOLE_REPORT.md) §10).

## 7. Limites de cette phase

- Aucune courbe ROC/precision-recall/calibration tracée : l'API ne fournit pas de points
  intermédiaires, seulement des métriques agrégées ; les tracer honnêtement nécessiterait un
  nouvel endpoint (reporté à la phase 4B).
- Aucune bibliothèque de composants n'a été introduite ; les « composants » listés dans le style
  guide sont des classes CSS et de petites fonctions JS (`statusLine`, `metricGrid`,
  `definitionGrid`, `barChart`, `badge`, `def`), cohérent avec le choix déjà fait en 4A de rester
  en vanilla JS sans framework.
- Pas de suite de tests frontend automatisée (toujours aucun outillage JS dans ce projet) ; la
  vérification reste manuelle, dans un navigateur réel, avec les données réelles de la base
  isolée.

## 8. Backlog (hors périmètre de cette phase)

- Courbes ROC / precision-recall / calibration réelles (nécessite un endpoint dédié).
- Petits multiples pour les distributions de features (min/médiane/max/taux manquant) au-delà
  du catalogue tabulaire actuel.
- Filtres persistants (URL query params) sur les tables paginées.

## 9. Tests

Conservés tous les tests backend existants (9 tests `science.rs` + suite complète du
workspace). Aucun contrat de données modifié par cette phase — seule la présentation change.

```
cargo fmt --all -- --check                                                     -> ok
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings  -> exit 0
cargo test --workspace --locked                                                -> exit 0, 0 échec
```

Exécuté dans un nouveau conteneur de build éphémère (`erytheon-4a1-build`), contre la même base
isolée que les phases précédentes. Aucun fichier Rust n'a été modifié par cette phase (seule la
couche statique `crates/api/static/science/*` a changé) ; la suite complète a néanmoins été
rejouée intégralement, conformément à la consigne de la mission.

## 10. Commits

Deux commits locaux, aucun push :
`refactor: redesign scientific console visual system`,
`docs: document scientific interface design`.

---

```
PHASE 4A.1 SCIENTIFIC UI REDESIGN COMPLETED
SCIENTIFIC CONSOLE VISUALLY VALIDATED
READ-ONLY BEHAVIOR UNCHANGED
V1 REMAINS ACTIVE
CANDIDATE REMAINS INACTIVE
NO SHADOW SCORING
NO PRODUCTION DEPLOYMENT
NO PUSH
```
