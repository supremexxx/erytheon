# BLUE Forecast & Evidence Center — contrat de prévision

## But

BLUE conserve chaque jour une preuve lisible de ce que le modèle v1 annonçait avant de connaître les événements observés. Le registre sert à mesurer les réussites, les échecs et les limites du modèle sans réécrire le passé.

## Bulletin quotidien

- Un seul bulletin logique est publié par date UTC.
- La première prévision complète calculée après 06:00 UTC alimente le bulletin.
- Les échéances sont `+24 h` et `+48 h`. Elles désignent l'heure de validité de la prévision, pas la garantie qu'un incendie surviendra pendant toute la fenêtre.
- Le bulletin contient une archive compacte de toutes les communes réellement évaluées et une liste lisible des communes placées en vigilance.
- Après publication, la prévision, ses scores, son empreinte et sa provenance ne peuvent plus être modifiés.

## Indice communal

Pour chaque commune et chaque échéance, BLUE agrège les cellules territoriales avec le percentile 95 (`p95`). Cette règle évite qu'une seule cellule extrême résume artificiellement toute une commune, tout en conservant les zones locales les plus préoccupantes.

Les seuils initiaux sont :

- vigilance élevée : indice communal supérieur ou égal à `0,65` ;
- vigilance critique : indice communal supérieur ou égal à `0,75`.

Ces seuils sont provisoires. L'indice BLUE est un indice relatif de vigilance issu du modèle actif v1. Ce n'est pas encore une probabilité calibrée de départ de feu.

## Données conservées

Chaque bulletin garde notamment :

- la date et l'heure exactes d'émission ;
- le lot de prévisions utilisé ;
- la version du modèle actif et la révision applicative ;
- le nombre de cellules prévues, rattachées et non rattachées ;
- les indices communaux `p95`, les maxima et les échéances ;
- les composantes physique et humaine au point culminant ;
- les principaux facteurs enregistrés par le moteur ;
- une empreinte SHA-256 du contenu publié.

La provenance technique complète reste disponible dans la base interne pour l'audit et la reproductibilité. Elle n'est pas exposée dans l'API ni dans l'interface destinée à la présentation de BLUE.

## Vérification ultérieure

La vérification est séparée de la prévision. Une fiche commence à l'état `pending`, puis pourra évoluer vers : recherche en cours, signal observé, probable, confirmé, aucun événement confirmé ou non concluant.

La Phase 1 ne fabrique aucun verdict automatique. Une future phase ajoutera des éléments de preuve datés et traçables. L'absence de preuve ne devra jamais être transformée automatiquement en preuve d'absence.

## Sécurité et activation

- Le centre BLUE est désactivé par défaut avec `BLUE_CENTER_ENABLED=false`.
- L'interface `/blue` et l'API `/api/blue/*` sont privées et protégées au niveau du proxy.
- Les routes sont en lecture seule.
- Le modèle v1 reste le seul modèle actif ; le candidat reste inactif.
- Le dispositif n'effectue aucun shadow scoring, aucun réentraînement et aucune activation de modèle.
