# Correction de l'historique horaire

## Défaut

L'identité par date convenait au quotidien mais pas à l'horaire. Toute exécution d'une même journée effectuait un `UPSERT` sur la même ligne.

## Contrat 4A.6

- `hourly` : fenêtre UTC `[date_trunc('hour', captured_at), +1 heure)` ;
- `daily` : fenêtre UTC `[00:00, +1 jour)` ;
- `event` : instant unique borné à une microseconde ;
- unicité : `(environment, cadence, capture_window_start)` ;
- replay d'une fenêtre : même snapshot logique, nouvelle tentative auditable ;
- heures différentes : lignes distinctes ;
- concurrence : numéro de tentative sérialisé par verrou transactionnel consultatif.

Les anciennes lignes sont conservées avec `provenance_status=legacy_last_state_only` ou `legacy_day_identity`. Elles ne sont jamais présentées comme un historique horaire complet.

`observability.snapshot_capture_attempts` conserve origine, statut, révision, image, checksum et erreur. Un échec n'efface donc plus la preuve de l'exécution.

La synthèse GET calcule les créneaux attendus entre la première et la dernière fenêtre réellement
capturées, les créneaux présents, les trous et les tentatives échouées. Les lignes legacy sont
explicitement exclues de ce calcul.
