# Contrat de provenance scientifique v2

Une nouvelle capture ne peut atteindre `validated` ou `published` que si elle référence :

- environnement ;
- révision applicative ;
- tag d'image et digest d'image ;
- bundle statique actif ;
- batch forecast exact (`forecast_batch_computed_at`) ;
- instant forecast valide et horizon `nowcast` ;
- masque de couverture publié ;
- dénominateur modélisable et décomptes de couverture.

Le schéma impose ces champs aux manifestes `contract_version=2`. L'application refuse les chaînes vides avant insertion. Le checksum scientifique reste calculé sur les valeurs ordonnées par H3.

Ils sont injectés via `ERYTHEON_GIT_REVISION`, `ERYTHEON_IMAGE_REFERENCE`,
`ERYTHEON_IMAGE_DIGEST` et `ERYTHEON_ENVIRONMENT`, jamais par une commande Git dans le conteneur.

Le manifeste v1 existant reste inchangé avec `traceability_status=legacy_incomplete`. Cette classification est un avertissement, pas une réécriture rétroactive.

`complete=true` signifie en v2 : aucune cellule inattendue ne manque à l'intérieur du domaine modélisable. Les exclusions structurelles restent comptées séparément.
