# BLUE — workflow de preuves automatisées

## Objectif

BLUE conserve chaque jour l'archive scientifique complète de toutes les communes. Une sélection distincte et déterministe retient au maximum vingt communes uniques pour une lecture simple, une recherche de preuves et une démonstration ultérieure.

Les horizons +24 h et +48 h d'une même commune appartiennent au même dossier. Le classement utilise le plus haut indice des deux horizons, puis le nom et le code INSEE pour rendre les égalités reproductibles.

## Cycle

1. Le bulletin quotidien immuable est publié.
2. Les vingt communes au score le plus élevé sont sélectionnées sans modifier l'archive.
3. Une première recherche démarre trois heures après l'échéance +24 h et produit un constat provisoire.
4. Une seconde recherche démarre trois heures après l'échéance +48 h et produit le verdict final sur la fenêtre complète.
5. Le service effectue ces recherches avec `gpt-5.6-luna` par défaut.
6. La réponse brute, l'horizon contrôlé, le modèle, les jetons, le verdict et chaque URL citée sont archivés séparément.
7. Une recherche sans preuve est classée `no_evidence_found`, jamais « aucun incendie ».
8. Après +48 h seulement, ce résultat peut déclencher une seconde et dernière recherche 72 heures plus tard.

Une erreur technique peut être relancée une fois sur chaque horizon. Deux échecs à +24 h n'empêchent jamais la vérification finale à +48 h.

## Verdicts

- `confirmed` : une source directe datée et localisée concorde avec le dossier.
- `probable` : une source crédible existe, mais la concordance n'est pas complète.
- `signal_observed` : un signal sourcé existe, sans permettre une validation suffisante.
- `no_evidence_found` : la recherche n'a rien trouvé ; ce n'est pas une preuve d'absence.
- `inconclusive` : les éléments ne permettent aucune conclusion honnête.

Un résultat `confirmed` sans source directe est automatiquement rétrogradé. Un résultat `probable` ou `signal_observed` sans URL valide devient `inconclusive`.

## Configuration

```text
BLUE_CENTER_ENABLED=true
BLUE_AI_EVIDENCE_ENABLED=true
BLUE_OPENAI_MODEL=gpt-5.6-luna
OPENAI_API_KEY=...
```

La clé reste uniquement dans l'environnement de production. Elle n'est ni enregistrée en base, ni journalisée, ni exposée par l'API.

## Limite scientifique

Ce workflow mesure la capacité à retrouver des preuves pour les vingt dossiers sélectionnés. Il ne mesure pas encore tous les faux négatifs : une analyse inverse, partant de tous les incendies observés vers l'archive complète des prévisions, reste nécessaire avant toute affirmation globale de précision.
