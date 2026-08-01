# Matrice migrations / rollback

| Objet 0022 | Forward | Rollback vide | Rollback avec données |
|---|---|---|---|
| Fenêtres des snapshots système | backfill legacy puis nouvelle unicité | restaure l'identité journalière | refus si tentatives 4A.6 |
| Tentatives de capture | création additive | suppression | refus si une tentative existe |
| Valeurs bundle statique | création additive | suppression | refus si une valeur existe |
| Activations bundle | création additive | suppression | refus indirect via valeurs |
| Masques de couverture | création additive | suppression | refus si un masque existe |
| Provenance scientifique v2 | colonnes nullables pour legacy, contraintes v2 | suppression | refus si un manifeste v2 existe |
| Maturité des labels | colonnes additives | restauration de l'unicité 0021 | rollback uniquement avant données 4A.6 |

Le rollback doit suivre `0022 → 0021 → 0020 → 0019 → 0018`. La migration 0022 est conçue pour une copie de base avant production. Aucun rollback destructif automatique n'est prévu.
