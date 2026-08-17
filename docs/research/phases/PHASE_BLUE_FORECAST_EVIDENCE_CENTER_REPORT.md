# BLUE Forecast & Evidence Center — rapport Phase 1

## Résultat

La fondation du registre quotidien de prévisions communales BLUE est implémentée localement. Elle archive les échéances `+24 h` et `+48 h`, expose une interface privée lisible et prépare une vérification ultérieure distincte des prévisions originales.

## Périmètre livré

- migration `0025` avec bulletins, archive communale compacte, vigilances et statuts d'évaluation ;
- verrouillage SQL des prévisions dès leur publication ;
- catalogue versionné des limites communales et correspondance H3 ;
- commande contrôlée d'import du catalogue communal ;
- capture automatique après une prévision complète, sous feature flag ;
- API BLUE en lecture seule, sans exposition de la provenance amont ;
- interface privée filtrable par échéance, commune et niveau ;
- protection HTTP Basic au niveau Caddy ;
- rollback refusé dès que des bulletins ou correspondances communales existent.

## Choix scientifiques

- agrégation communale par percentile 95 ;
- seuils provisoires `0,65` et `0,75`, choisis pour une première liste lisible et à valider sur les archives ;
- conservation de l'intégralité de l'index communal, même sous le seuil d'alerte ;
- présentation explicite de l'indice comme vigilance relative et non comme probabilité calibrée ;
- séparation stricte entre prévision immuable et évaluation ultérieure mutable.

## Ce qui n'a pas changé

- aucun changement du calcul du score v1 ;
- aucun changement de modèle ou de candidat ;
- aucun réentraînement ;
- aucun shadow scoring ;
- aucune écriture depuis les routes HTTP ;
- aucun verdict IA ou événement simulé.

## Validation réalisée

Un test d'intégration sur une base temporaire applique toutes les migrations, publie un bulletin à deux horizons, vérifie son empreinte et son archive compacte, rejoue la capture sans doublon, confirme que la provenance amont n'est pas sérialisée et prouve qu'une prévision publiée ne peut plus être modifiée.

Le rollback `0025` a également été exécuté sur une base temporaire vide avant les rollbacks antérieurs, dans l'ordre inverse des migrations.

## Activation encore requise

Cette branche n'est pas déployée. Avant activation en production, il reste à :

1. charger et vérifier le catalogue communal officiel ;
2. mesurer la couverture des cellules de production ;
3. construire et vérifier l'image ;
4. sauvegarder la base, appliquer la migration puis activer `BLUE_CENTER_ENABLED` ;
5. contrôler le premier bulletin réel et son affichage navigateur.

La future recherche automatisée de preuves constitue une phase séparée. Elle ne doit démarrer qu'après accumulation et revue des premiers bulletins réels.
