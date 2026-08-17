# Console scientifique ERYTHEON — Guide de style (Phase 4A.1)

Ce document décrit le langage visuel de `/science`. Il s'applique uniquement à la couche de
présentation (`crates/api/static/science/{index.html,science.css,science.js}`) ; aucune donnée,
aucun contrat API, aucune permission n'est modifié par ce guide.

## 1. Intention

L'interface doit se lire comme un outil de laboratoire ou une console d'observatoire
scientifique — pas comme un tableau de bord SaaS. Priorité : densité d'information utile,
tables, matrices, méthode et limites explicites. Aucun élément décoratif sans fonction.

## 2. Fond et surfaces

```
--bg              #f4f5f3   fond de page (jamais blanc pur)
--bg-panel        #fafaf8   fond de zone de contenu léger
--bg-panel-raised #ffffff   panneaux, tables, en-tête
--bg-muted        #ecefed   survol de ligne, fond de piste de barre
```

Les séparations sont obtenues par des lignes fines (`--line`), pas par des ombres. Aucune ombre
portée n'est utilisée dans cette version.

## 3. Couleurs de texte et de structure

```
--text            #1b1d1c   texte principal
--text-secondary  #5c625f   texte secondaire, libellés
--text-faint      #7d827e   métadonnées, notes de graphique
--line            #d5d9d6   séparateurs de table, bordures de panneau
--line-strong     #b9beba   bordures d'en-tête de table, séparateurs de navigation active
```

## 4. Accents fonctionnels

Chaque couleur a un rôle unique — aucune n'est utilisée « pour faire joli ».

```
--accent            #244c3f   navigation active, identité de marque
--accent-secondary  #536b63   barres de graphique par défaut
--warn              #9a5a38   avertissements, badge lecture seule, statut draft/inactive
--error             #8b2d2d   échecs, statuts rejetés/bloqués
--success           #356447   statuts validés/terminés/actifs
--info              #3f5f73   statuts en production/en cours
```

## 5. Palette de séries (graphiques)

```
--series-v1          #64707c   gris bleu — modèle v1
--series-candidate    #2f5c46   vert sombre — modèle candidat
--series-positive     #7a4a35   brun rouge — positifs
--series-negative     #9a9d99   gris — négatifs
--series-unknown      #a8894f   ocre — inconnu
--series-natural      #59707c   bleu gris — naturel
--series-human        #6b4a37   brun profond — humain
```

Quatre couleurs suffisent pour la plupart des graphiques ; ne pas en ajouter sans besoin réel.

## 6. Rayon et bordures

```
--radius        3px   boutons, badges, champs de filtre
--radius-panel  4px   panneaux, tables, cellules de métrique
```

Aucun rayon supérieur à 6px. Bordures à 1px (`border: 1px solid var(--line)`), jamais d'ombre
lourde ni de glassmorphism.

## 7. Typographie

```
--sans  Inter, "IBM Plex Sans", "Source Sans 3", system-ui, -apple-system, "Segoe UI", sans-serif
--mono  "IBM Plex Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace
```

Aucune dépendance externe (pas de `<link>` Google Fonts) : la pile s'appuie uniquement sur les
polices déjà présentes sur le système, avec un empilement de secours cohérent.

`--mono` est utilisé pour : checksums, commits, identifiants, codes H3, seeds, logical IDs.

### Échelle de tailles

```
h1 (titre de page)      24px / 700
h2 (titre de section)   16px / 600
h3 (titre de panneau)   13px / 600, majuscules, +0.04em
texte normal            13px / 400
métadonnées             11–12px
valeur importante       20px maximum (cellules de métrique)
```

## 8. Composants

| Composant | Classe(s) CSS | Usage |
|---|---|---|
| Bandeau de statut | `.sci-status-line` / `.sci-status-item` | ligne compacte environnement/base/modèle en haut de page |
| Grille de métriques | `.sci-metric-grid` / `.sci-metric-cell` | remplace les grandes cartes KPI ; valeurs 20px max |
| Table | `table.sci-table` | élément central de presque toutes les pages |
| Badge de statut | `.sci-badge` (+ variante par statut) | texte coloré + pastille de 6px, jamais un pill plein |
| Barre horizontale | `.sci-bar-row` / `.sci-bar-track` / `.sci-bar-fill` | répartitions catégorielles, toujours annotées `n = ...` |
| Panneau d'avertissement | `.sci-warning-box` | bordure gauche `--warn`, jamais un fond plein coloré |
| Panneau de méthode | `.sci-method-panel` | bordure gauche `--info`, section « Limites scientifiques » |
| Grille de définitions | `.sci-definition-grid` (`<dl>`) | paires clé/valeur denses (identité de dataset, artefact) |
| Terme défini | `.sci-defterm` | soulignement pointillé + tooltip au survol/focus |
| Valeur monospace | `.sci-mono` | checksums, commits, IDs, H3 |
| Diff positif/négatif | `.sci-diff-pos` / `.sci-diff-neg` | écarts de métriques, coloration minimale |

## 9. Navigation

Colonne fixe de 230px, fond `--bg-panel`, item actif signalé par une bordure gauche de 2px
`--accent` et un fond `--accent-bg` léger — jamais un bloc plein arrondi.

## 10. Graphiques

- Barres horizontales uniquement pour cette phase (pas de donut, pas de jauge, pas de 3D).
- Chaque graphique affiche sa taille d'échantillon (`n = ...`) sous les barres.
- Pas d'animation d'entrée, pas de lissage.
- Courbes ROC/precision-recall/calibration : reportées à la phase 4B (aucune donnée point-par-
  point n'est actuellement exposée par l'API pour les tracer honnêtement).

## 11. Wording

Utiliser : statut, mesure, source, méthode, population, échantillon, résultat, limite,
avertissement, version, période.

Éviter : insights, smart analytics, next-gen, AI-powered, performance boost, powerful, enhanced,
health score.

## 12. Responsive

Trois points de rupture : 1440px (confort), 1280px (standard), 1024px (compact, `sci-two-col`
repasse en une colonne). Sous 900px : navigation latérale devient horizontale scrollable,
tables scrollables horizontalement sans perte de colonne.

## 13. Accessibilité

- Contraste texte/fond vérifié AA (`--text` sur `--bg`/`--bg-panel-raised` : ratio > 7:1).
- Statuts jamais codés uniquement par la couleur : chaque badge porte aussi le texte du statut.
- Navigation clavier : liens de navigation et termes définis (`tabindex="0"`) restent focusables ;
  le focus visible par défaut du navigateur n'est pas supprimé (aucun `outline: none` global).
- Tables avec `<th>` explicites sur toutes les pages.
