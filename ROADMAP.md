# ERYTHEON — Roadmap

État au 29 juillet 2026.

## Principes de progression

- v1 reste actif tant qu'une décision de promotion séparée n'est pas validée.
- Le candidat reste inactif ; aucune phase documentaire ne peut changer ce statut.
- Chaque évolution de modèle doit être réversible, mesurée et indépendante du service v1.
- Les phases d'interface et de documentation ne doivent déclencher ni scoring, ni import, ni migration en production.
- Les tags publiés `v0.4.2-app` et `v0.4.2` ne doivent pas être déplacés.

La séquence **4A.3 → 4B → P3** est la recommandation actuelle. Elle pourra être réévaluée à partir des observations de 4A.3 ; cet ordre n'est pas une obligation irréversible et ne remplace pas les validations propres à chaque phase.

## Terminé et intégré

### Socle opérationnel historique

- Workspace Rust, API Axum et PostgreSQL/PostGIS.
- FWI, grille H3 et ingestion NASA FIRMS.
- Observations Météo-France et prévisions AROME/ARPEGE.
- Features OSM, BDIFF, Prométhée, CORINE, INSEE et calendrier.
- Scoring v1 explicable, API GeoJSON, alertes et dashboard opérationnel.
- Déploiement Docker/Caddy et traitement territorial France métropolitaine.

Les anciennes appellations « phases 0–9C » décrivent ce socle historique. Elles ne constituent plus la roadmap active du programme scientifique.

### Phases 0 à 2 — plateforme de données

- **Phase 0** — sauvegarde et restauration PostgreSQL validées.
- **Phase 1** — schémas `raw`, `staging`, `fire`, validation et opérations.
- **Phase 2** — ingestion FIRMS traçable et opérationnelle.

### Phase 3A — spécification scientifique

- Population, labels, fenêtres temporelles et règles de construction du dataset humain spécifiés.

### Phases 3B.1 à 3B.6 — datasets

- Fondation et audits de qualité BDIFF.
- Versioning des features et des datasets.
- Stratégies de negative sampling N2/N3.
- Variantes strict/inclusive.
- Revue scientifique des biais, dérives et règles de combustibilité.

### Phases 3B.7 à 3B.9 — candidat

- Baselines et candidat GBM avec calibration isotonic.
- Comparaison appariée v1/candidat sur la population historique commune.
- Artefact versionné, checksums, parité entraînement/inférence et plan de promotion.

### Phases 3B.10 et 3B.11 — garde-fous de production

- **P1** — candidat enregistré avec le statut `inactive`.
- **P2** — chargement et validation en lecture seule.
- Aucun scoring candidat et aucune activation.

### Phases 4A à 4A.2 — console scientifique

- API scientifique read-only.
- Console privée, responsive et sans chaîne de build frontend.
- Présentation des sources, imports, qualité, features, datasets, modèles et intégrité.
- Déploiement VPS derrière une protection Caddy.
- Intégration GitHub v0.4.2, CI stricte et tags séparant application et état intégré.

## Prochaine phase — 4A.3 Stabilisation

Phase courte, limitée à l'observation et à la correction des défauts réels.

Objectifs :

- utiliser la console sur desktop et mobile ;
- vérifier la cohérence entre SQL, API et UI ;
- clarifier les métriques difficiles à interpréter ;
- corriger les états vides, erreurs et régressions d'ergonomie ;
- mesurer les lenteurs des endpoints scientifiques ;
- surveiller les erreurs de scheduler et les rate limits Open-Meteo ;
- consolider le runbook et le monitoring.

Hors périmètre :

- nouveau modèle ;
- modification du scoring v1 ;
- activation du candidat ;
- shadow scoring ;
- migration de données non indispensable ;
- visualisation scientifique majeure.

Critères de sortie :

- aucune incohérence connue entre chiffres affichés et sources ;
- erreurs API observables et documentées ;
- parcours principaux utilisables sur mobile et desktop ;
- limites scientifiques visibles ;
- procédures d'exploitation vérifiées.

## Phase 4B — visualisations scientifiques

À ouvrir après stabilisation de 4A.3.

Périmètre envisagé :

- cartes H3 et exploration géographique BDIFF ;
- ROC, precision-recall et calibration ;
- distributions des features ;
- comparaison strict/inclusive et N2/N3 ;
- détails des exclusions ;
- historique des imports et erreurs ;
- filtres temporels et territoriaux.

Cette phase reste read-only et ne modifie aucun statut de modèle.

## P3 — shadow scoring limité

À ouvrir seulement après stabilisation de la console et validation du protocole.

Principes :

- v1 continue de répondre seul ;
- le candidat reçoit les mêmes cas en arrière-plan ;
- aucun score candidat n'est servi aux utilisateurs ;
- les écarts sont persistés dans un stockage dédié ;
- la console présente la comparaison live ;
- toute erreur candidat est isolée du chemin v1.

La phase doit définir avant implémentation :

- population et fréquence de scoring ;
- schéma de stockage et rétention ;
- métriques de dérive et seuils d'alerte ;
- budget de calcul ;
- arrêt d'urgence et rollback ;
- critères de passage ou d'abandon.

## Après P3

1. Observer une fenêtre live suffisante.
2. Comparer performance, calibration, dérive et stabilité opérationnelle.
3. Documenter les écarts et incidents.
4. Décider explicitement entre abandon, nouvel entraînement, prolongation du shadow ou proposition de promotion.
5. Traiter toute activation comme une phase distincte avec validation humaine.

## État des modèles

| Modèle | Registry | Scoring servi | Shadow scoring | Décision |
|---|---|---:|---:|---|
| v1 | actif | oui | n/a | référence opérationnelle |
| `gbm_isotonic_v2` | inactive | non | non | en attente de P3 |
