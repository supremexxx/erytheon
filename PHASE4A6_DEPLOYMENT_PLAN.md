# Plan de déploiement 4A.6

Ce document est un plan ; cette PR ne déploie rien.

## Pré-déploiement

1. CI verte sur le commit exact de la PR.
2. Sauvegarde PostgreSQL et répétition de 0022 sur une copie PostGIS.
3. Relevé du checksum du snapshot scientifique v1 existant ; il doit rester identique après migration.
4. Injection explicite de `ERYTHEON_ENVIRONMENT`, `ERYTHEON_APPLICATION_REVISION`, `ERYTHEON_APPLICATION_IMAGE` et `ERYTHEON_APPLICATION_IMAGE_DIGEST`.

## Séquence contrôlée

1. appliquer 0022 ;
2. vérifier les lignes legacy et les nouvelles contraintes ;
3. construire le bundle via `snapshot-static-bundle` ;
4. publier le masque via `snapshot-coverage-mask` avec le même GeoJSON et H3 r8 que le forecast ;
5. lancer une capture opérationnelle manuelle, puis un replay et vérifier une ligne logique/deux tentatives ;
6. attendre et prouver un déclenchement horaire et le premier déclenchement hebdomadaire automatique ;
7. capturer un nouveau snapshot scientifique v2 seulement lorsque toute la provenance est présente ;
8. exécuter `snapshot-link-labels` sans `--apply` et examiner le rapport.

## Arrêt / retour

Avant toute donnée 4A.6, le rollback 0022 est possible. Après création d'une tentative, d'un masque, d'un bundle ou d'un manifeste v2, le rollback destructif refuse : retour applicatif uniquement, conservation des données et analyse manuelle.

Aucun tag, modèle, scoring, entraînement, shadow scoring, route `/risk`, moteur FWI, Caddy ou authentification n'est modifié.
