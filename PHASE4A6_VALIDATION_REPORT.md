# Rapport de validation 4A.6

## Périmètre vérifié

- compilation workspace et formatage ;
- identité horaire, replay et concurrence ;
- historique des tentatives ;
- bundle statique SHA-256, idempotence, activation et immutabilité ;
- masque déterministe et couverture v2 ;
- refus de publication sans provenance ;
- rattachement BDIFF en dry-run et historique de supersession ;
- API/console en lecture seule ;
- ordre et gardes de rollback.

## Limite locale

Le PostgreSQL local ne possède pas l'extension PostGIS, requise dès la migration 0001. Les tests SQL complets doivent donc être exécutés par la CI PostgreSQL/PostGIS du dépôt. Cette limite n'autorise aucun contournement sur la production.

Les résultats finaux de CI et les éventuelles validations navigateur sont consignés dans la PR avant passage en revue.
