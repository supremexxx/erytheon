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
- tests d'intégration locaux dépendant de l'ancienne base : indisponibles, car cette base locale ne répond pas ;
- migration 0026 et rollback à vide : validés sur une base temporaire isolée du VPS ;
- production : migration 26 appliquée, conteneur sain, vingt dossiers et vingt codes INSEE uniques ;
- navigateur réel : desktop et mobile validés, aucune erreur ni alerte console ;
- sécurité : `/blue` et `/api/blue/*` répondent `401` sans authentification, les mutations API répondent `405` ;
- sauvegarde pré-déploiement : dump restaurable et empreinte SHA-256 validée.

L'automatisation OpenAI reste volontairement désactivée en production tant qu'aucune clé API n'est installée. La sélection, l'archive, l'interface et le worker sont déployés et prêts à être activés uniquement par configuration.

## Limite restante

La mesure des incendies observés qui ne figuraient pas dans le top 20 nécessite un futur rapprochement inverse avec l'archive complète. Elle ne doit pas être remplacée par une recherche web limitée aux alertes sélectionnées.
