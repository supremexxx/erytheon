# Phase 4A.5 — Politique de rétention (aucune suppression automatique activée)

Cette politique est **définie** par cette phase. Aucune suppression automatique n'est activée :
`erytheon snapshot-retention` n'accepte que `--dry-run` (par défaut et seule valeur possible ;
`crates/engine/src/main.rs` refuse explicitement toute tentative de désactiver le dry-run).

## 1. Principes

- Un snapshot `published` n'est jamais supprimé automatiquement dans cette phase.
- Toute suppression future devra être un mécanisme séparé, explicitement autorisé, avec simulation
  préalable, rapport de volume et sauvegarde vérifiée (spec §25).
- Les manifestes et alertes sont peu coûteux (quelques centaines d'octets par ligne) : leur
  rétention par défaut est indéfinie tant qu'aucune contrainte de volume réelle ne l'impose.

## 2. Politique proposée par famille

| Famille | Rétention proposée | Statut d'activation |
|---|---|---|
| `observability.system_snapshots` (cadence `hourly`) | 90 jours | Non activée (dry-run seulement) |
| `observability.system_snapshots` (cadence `daily`) | indéfinie | Non activée |
| `observability.system_snapshots` (cadence `event`) | indéfinie | Non activée |
| `observability.scientific_snapshots` (manifestes) | indéfinie (permanente) | N/A — pas de suppression prévue |
| `observability.scientific_snapshot_values` (pilote hebdo/nowcast) | indéfinie pour l'instant ; à réévaluer après mesure réelle du volume (~10 Go/an estimés) | Non activée |
| `observability.snapshot_alerts` | indéfinie | N/A |
| `ml.snapshot_label_links` | indéfinie (labels différés, jamais destructifs) | N/A |

## 3. Commande de vérification

```sh
erytheon snapshot-retention
```

Retourne un rapport JSON : compte actuel par famille, politique en vigueur, `would_delete: 0`,
`dry_run: true`. Aucune ligne n'est jamais supprimée par cette commande dans cette phase — voir
`crates/engine/src/snapshot_pipeline.rs::run_retention_dry_run`.

## 4. Avant toute activation future

1. Mesurer le volume réel sur au moins 4 semaines de production (§25 de la commande de phase).
2. Produire un rapport de volume comparant l'estimation `PHASE4A5_SNAPSHOT_ARCHITECTURE_DECISION.md`
   à la réalité observée.
3. Vérifier qu'une sauvegarde couvrant les données à supprimer existe et a été testée en restauration.
4. Ajouter des tests de non-régression prouvant qu'aucune ligne `published`/liée à un modèle ou un
   événement n'est supprimée par erreur.
5. Obtenir une autorisation séparée et explicite — cette politique ne vaut pas autorisation
   implicite d'implémenter la suppression.
