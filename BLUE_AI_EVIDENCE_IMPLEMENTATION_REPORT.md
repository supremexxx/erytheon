# BLUE — rapport d'implémentation de la preuve automatisée

## Périmètre livré

- archive complète des prévisions conservée sans modification ;
- sélection déterministe de vingt communes uniques au maximum par bulletin ;
- regroupement des horizons +24 h et +48 h dans un dossier unique ;
- API et interface limitées à ces dossiers lisibles ;
- recherche web automatique post-échéance avec réponse structurée ;
- conservation append-only des exécutions, réponses brutes, consommations et sources ;
- seconde recherche bornée pour les résultats sans preuve ;
- garde-fous contre les confirmations sans source ;
- aucune activation du candidat, aucun changement de scoring et aucune migration de modèle.

## Données créées

- `blue.evidence_cases` : sélection et verdict courant ;
- `blue.evidence_runs` : audit de chaque tentative ;
- `blue.evidence_sources` : liens vérifiables associés à une tentative.

## Sécurité et honnêteté

Le fournisseur des données de prévision reste absent de l'API BLUE. Les sources utilisées pour vérifier un événement restent visibles, car elles constituent précisément la preuve contrôlable. Une absence de résultat web ne produit jamais le statut historique `no_event_confirmed`.

## Validation

- formatage Rust : validé ;
- Clippy workspace, toutes cibles et fonctionnalités, avertissements interdits : validé ;
- tests unitaires du parseur et des rétrogradations de verdict : validés ;
- tests d'intégration locaux : bloqués par l'ancienne base PostgreSQL locale inaccessible, à rejouer dans une base isolée avant déploiement ;
- validation navigateur et production : à compléter pendant le déploiement contrôlé.

## Limite restante

La mesure des incendies observés qui ne figuraient pas dans le top 20 nécessite un futur rapprochement inverse avec l'archive complète. Elle ne doit pas être remplacée par une recherche web limitée aux alertes sélectionnées.
