# Contrat du bundle statique immuable

Le registre existant `features.feature_snapshots` reste l'autorité. La Phase 4A.6 lui ajoute une matérialisation dans `features.feature_snapshot_values` et un journal d'activation.

## Contenu obligatoire

Chaque cellule contient les neuf clés réelles : `hist`, `wui`, `road`, `agri`, `combustible`, `population`, `poi`, `power_line`, `school_zone`.

## Construction

1. lecture ordonnée de `public.cell_static` par H3 ;
2. SHA-256 déterministe de `h3:jsonb` ;
3. copie dans un manifeste `draft` ;
4. recomptage et recalcul du SHA-256 sur la copie ;
5. activation atomique et journalisée ;
6. rejet des mises à jour/suppressions de valeurs lorsque le manifeste est actif ou supersédé.

Un checksum identique rend la commande idempotente. Une nouvelle empreinte crée une nouvelle identité et supersède l'activation précédente sans réécrire son contenu.

La valeur `school_zone=C` reste une limitation documentée ; elle n'est ni corrigée ni interprétée dans cette phase.
