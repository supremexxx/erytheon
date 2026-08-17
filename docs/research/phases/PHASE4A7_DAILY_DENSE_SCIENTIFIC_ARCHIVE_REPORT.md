# ERYTHEON — Archive scientifique quotidienne compacte

## Résultat

ERYTHEON peut conserver automatiquement un état scientifique quotidien des six composantes FWI
(`FFMC`, `DMC`, `DC`, `ISI`, `BUI`, `FWI`) pour toutes les cellules H3 modélisables, sans changer
le modèle v1, les scores servis, les routes API ou le candidat inactif.

## Déclenchement

Après chaque prévision météo réussie, le service tente de publier l'archive de la date UTC de la
prévision. La première prévision complète de cette date crée l'archive ; les tentatives suivantes
sont idempotentes. Le pilote hebdomadaire en lignes PostgreSQL n'est plus planifié automatiquement,
mais sa commande manuelle et ses données historiques restent disponibles.

## Garanties de publication

La publication est refusée si l'une des conditions suivantes manque :

- provenance complète du déploiement (révision, image, digest, environnement) ;
- identifiant de la source météo ;
- prévision nowcast complète et vieille d'au plus six heures ;
- bundle statique actif et masque de couverture publié ;
- présence exacte d'une valeur FWI pour chaque cellule modélisable.

Les six tableaux de nombres sont stockés en `float32` réseau, dans l'ordre H3 du masque immuable.
Le checksum couvre l'ensemble du contenu. Une archive publiée ne peut plus être insérée, modifiée
ou supprimée silencieusement.

## Volumétrie

Avec les 792 998 cellules modélisables mesurées en production :

- environ 18 MiB par jour hors compression PostgreSQL ;
- environ 6,5 GiB par an hors compression ;
- contre environ 200 MiB par capture pour le format historique en lignes.

Cette cadence quotidienne apporte beaucoup plus de diversité temporelle tout en restant compatible
avec la capacité actuelle du VPS.

## Labels et entraînement

Le rattachement différé BDIFF sait lire une archive dense et utilise une fenêtre d'un jour. Il
reste volontairement manuel : le délai officiel de maturité BDIFF n'est pas encore défini. FIRMS
n'est jamais transformé en label humain et aucune cellule sans événement n'est automatiquement
déclarée négative.

Cette phase améliore la récolte. Elle ne lance aucun entraînement et n'active aucun candidat.

## Validation

- compilation de tout le workspace ;
- migration appliquée sur une base PostgreSQL/PostGIS isolée ;
- test d'intégration de création, idempotence, checksum et immutabilité ;
- base isolée supprimée après le test ;
- aucune donnée de production modifiée pendant la validation.
