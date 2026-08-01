# ERYTHEON — Phase 4A.6 — Audit de production

Audit en lecture seule réalisé le 1er août 2026 avant toute implémentation.

## Conclusions

- Les captures opérationnelles sont exécutées : 53 succès horaires consécutifs ont été observés entre le 30 juillet 08:51 UTC et le 1er août 12:51 UTC, sans erreur de job.
- L'historique horaire n'était pas conservé : la contrainte `(environment, capture_date, cadence)` ramenait ces 53 exécutions à trois lignes, la dernière heure de chaque jour écrasant les précédentes.
- Trois lignes quotidiennes existent. Les replays restent idempotents mais les tentatives n'étaient pas historisées.
- Le snapshot scientifique publié `277efb46-6c03-4dbb-a512-cc4624d4c336` est intact et son SHA-256 recalculé correspond à `ad4ed3e46c007a37fd116d67307061a826d9f876407e5df1912ef7a238f21d14`.
- Ce snapshot est un manifeste v1 legacy : `static_snapshot_id`, révision, image, périodes et versions sources sont absents. Son manifeste annonce H3 r9 alors que les cellules de production et le territoire sont H3 r8.
- Les valeurs scientifiques contiennent 920 016 cellules : 792 998 observées et 127 018 marquées manquantes. Aucun composant FWI n'est absent parmi les cellules observées.
- Aucun lien différé BDIFF n'existe encore.
- Le premier déclenchement hebdomadaire automatique attendu est le lundi 3 août 2026 à 03:00 UTC ; il ne peut donc pas être déclaré prouvé à la date de l'audit.

## Risques confirmés

1. Perte d'identité temporelle des captures horaires.
2. Alertes liées à une ligne horaire ensuite écrasée : leur valeur observée ne correspond plus nécessairement à l'état courant de la ligne.
3. Snapshot scientifique non reproductible sans bundle statique immuable.
4. Provenance de déploiement et de batch forecast insuffisante.
5. Confusion entre couverture statique totale et domaine réellement modélisé.

## Garde-fous

Le snapshot publié n'est ni modifié, ni recalculé, ni supprimé. Le modèle v1 reste actif ; candidat, entraînement et shadow scoring restent hors périmètre.
