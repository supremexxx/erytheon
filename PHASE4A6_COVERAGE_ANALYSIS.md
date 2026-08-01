# Analyse de couverture

## Résultat de production

| Population | Cellules |
|---|---:|
| Registre statique total | 920 016 |
| Territoire opérationnel H3 r8 | 792 998 |
| Forecast nowcast observé | 792 998 |
| Hors territoire opérationnel | 127 018 |
| Manquants inattendus dans le territoire | 0 |

Le plan territorial construit depuis les 96 départements métropolitains produit exactement 792 998 H3 uniques, soit exactement les cellules du dernier forecast et les valeurs observées du snapshot.

Les 127 018 absences sont donc classées `outside_operational_aoi`. Parmi elles, 127 014 sont non combustibles et quatre sont marquées combustibles ; ces quatre cellules restent néanmoins hors AOI et ne constituent pas une panne de forecast.

## Décision

Le dénominateur attendu pour la modélisation est le masque territorial versionné (792 998 lors de l'audit), pas le registre statique global. La couverture statique totale reste exposée séparément. Toute absence future à l'intérieur du masque augmente `unexpected_missing_count` et bloque la complétude.
