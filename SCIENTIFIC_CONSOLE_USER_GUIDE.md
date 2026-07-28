# Console scientifique ERYTHEON — Guide utilisateur (Phase 4A)

## Ce que c'est, ce que ce n'est pas

Cette console permet de **regarder** l'état réel du projet ERYTHEON : données ingérées, qualité,
features, datasets, modèle actif et candidat, intégrité système. Elle ne permet **aucune
action** : rien de ce qu'on peut cliquer ici ne modifie quoi que ce soit en base ou en
production. Le badge « Lecture seule — aucune action possible » dans la barre supérieure est
une garantie, pas une décoration.

## Accès

La console n'est disponible que si la variable d'environnement `SCIENCE_CONSOLE_ENABLED=true`
est positionnée au démarrage du service (défaut : `false`, désactivée). Ce n'est **pas** une
authentification — n'importe qui atteignant le service quand ce drapeau est actif peut consulter
la console. Ne l'activez jamais sur une instance exposée publiquement tant qu'aucune
authentification réelle n'a été ajoutée au projet.

Pour une prévisualisation locale sans toucher au service opérationnel :

```bash
SCIENCE_CONSOLE_ENABLED=true DATABASE_URL=... cargo run -p engine -- preview-science-console --bind 0.0.0.0:8081
```

Cette commande ne démarre ni le scheduler, ni l'ingestion FIRMS/météo, ni le chargement d'un
modèle de risque — c'est un serveur de lecture pure.

## Pages

- **Vue d'ensemble** (`/science/overview`) : état application/base, modèle actif, statut du
  candidat, compteurs de données, avertissements scientifiques ouverts (ex. snapshot de
  features appliqué uniformément à tout l'historique, vacances scolaires indisponibles).
- **Progression** (`/science/progress`) : chronologie des phases du projet, avec statut,
  environnement affecté, commits associés, risques ouverts le cas échéant.
- **Sources et pipelines** (`/science/sources`) : santé de chaque source (dernier succès,
  volume, dernière erreur), imports récents, exécutions de pipeline récentes.
- **Qualité des données** (`/science/data-quality`) : répartition par cause, classification des
  doublons, qualité géographique (8 catégories réelles, pas la valeur unique contrainte en
  base), combustibilité, table d'exploration événement par événement.
- **Features et snapshots** (`/science/features`) : snapshots de features avec classification
  temporelle (le cas `current_snapshot_applied_historically` est mis en évidence), calendrier
  historique avec vacances scolaires **connues** et **indisponibles** distinguées explicitement.
- **Datasets** (`/science/datasets` et `/science/datasets/:id`) : liste des versions de dataset
  candidates, détail avec répartition par split/label et par catégorie d'exclusion.
- **Modèles** (`/science/models`) : modèle actif v1 (métriques réelles de l'artefact), candidat
  le plus récent (statut, checksums, jamais présenté comme actif), comparaison v1/candidat issue
  de la phase 3B.8 (clairement sourcée, pas recalculée en direct), rappel explicite que le score
  du candidat est une propension relative, pas une probabilité absolue.
- **Système et intégrité** (`/science/system`) : migrations réussies/échouées, nombre de
  modèles actifs (doit être 1), cellules `cell_static`, événements d'ignition, dernier succès
  FIRMS/BDIFF, confirmation qu'aucune table de shadow scoring n'existe.

## Termes techniques

Les termes soulignés en pointillés (AP, ROC-AUC, Brier, ECE, lift, strict/inclusive, N2/N3,
snapshot, checksum, H3, propension relative, modèle actif/candidat, shadow scoring) affichent
une définition au survol ou au focus clavier (`tabindex="0"`).

## États d'affichage

Chaque page gère explicitement : chargement (« Chargement… »), succès, absence de données
(message « Aucune donnée pour ce filtre » plutôt qu'un tableau vide silencieux), erreur (message
d'erreur affiché, jamais une page blanche). La date d'actualisation, le fuseau (UTC) et la
source des données figurent en haut de chaque page pertinente. Aucune donnée manquante n'est
jamais convertie silencieusement en zéro — voir l'exemple des vacances scolaires ci-dessus.

## Limites connues de cette phase (4A)

- Cartographie avancée : non incluse (reportée à la phase 4B).
- Exploration d'événement individuel au-delà de la table paginée : non incluse.
- Shadow scoring : non déployé, aucune table n'existe pour cela.
- Comparaison temporelle en direct au-delà de la constante phase 3B.8 : non incluse.
- Export de données (CSV/Parquet) : non inclus.
- `row_count`/`positive_count`/etc. de certains datasets peuvent apparaître comme « — » : ce
  sont des colonnes réellement nulles en base pour ces versions, pas un défaut d'affichage (la
  page de détail calcule les répartitions réelles par agrégation directe malgré cela).
