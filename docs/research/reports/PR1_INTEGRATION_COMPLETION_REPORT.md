# ERYTHEON — PR #1 Integration Completion Report

Date de clôture : 2026-07-28
PR : [#1 — Deploy the private scientific console and candidate validation pipeline](https://github.com/supremexxx/erytheon/pull/1)

## 1. État initial

- `origin/main` était à `2fca1d920fa9d87674e1574a88633fcc88eb4cb1`.
- La tête de revue de la PR était `7f87ddaef4dbe177c476e02432b63d8c83994d31`.
- La PR contenait 35 commits historiques entre ces deux révisions.
- La production applicative exécutait la révision `849039385a14f95df0a95cca69e5987d3b311478`.
- La PR était ouverte en draft et les contrôles stricts demandés n'étaient pas tous reproductibles.

## 2. Bloquants corrigés

- Tests scientifiques rendus autonomes avec une base PostgreSQL/PostGIS jetable et des fixtures déterministes.
- Test du modèle actif rendu indépendant de l'état préalable de la base.
- Expérience de negative sampling rendue autonome et représentative des causes attendues.
- Couverture des rollbacks `0016` et `0017` étendue aux cas vide, peuplé et ordre invalide.
- Rollback `0016` protégé contre une exécution avant `0017`.
- Toolchain Rust épinglée à `1.97.1`.
- CI alignée sur les commandes strictes de formatage, Clippy et tests workspace.
- Installation et contrôle de disponibilité du client PostgreSQL ajoutés à la CI.

## 3. Commits correctifs ajoutés

Les corrections ont été ajoutées au-dessus de la tête historique, sans réécriture :

1. `b42d6bc471d1b2d38906daecd71909c03e89aca9` — `fix: make scientific integration tests self-contained`
2. `168f5e994487d7110f221ec43fe5a406788dcdc9` — `test: cover candidate registry rollback guards`
3. `d4730fda09571db491d0611e3308f96d1ee03ebb` — `ci: pin rust toolchain and enforce full quality gate`

## 4. Preuve de préservation des 35 commits

- Le nombre de commits entre `2fca1d9` et `7f87dda` reste exactement 35.
- La liste ordonnée des 35 couples SHA/message a été enregistrée avant correction puis comparée après intégration : aucune différence.
- Les 35 SHA historiques sont tous ancêtres de `main`.
- Aucun rebase, squash, amend ou force-push n'a été utilisé.
- Les trois corrections sont les seuls commits ajoutés après `7f87dda`.

## 5. CI locale

Exécutée avec Rust `1.97.1` et une instance PostGIS jetable :

- `cargo fmt --all -- --check` : succès.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` : succès.
- `cargo test --workspace --locked --no-fail-fast` : succès.
- Environ 200 tests ont été exécutés, sans échec et sans ajout de test ignoré.
- Les tests ciblés du modèle actif, du negative sampling et des rollbacks ont également été exécutés séparément avec succès.

## 6. CI GitHub

Deux exécutions CI sur le SHA final exact `d4730fd` sont terminées avec succès :

- [Run 30401857075](https://github.com/supremexxx/erytheon/actions/runs/30401857075) — succès.
- [Run 30401859232](https://github.com/supremexxx/erytheon/actions/runs/30401859232) — succès.

Les contrôles GitHub couvrent le formatage, Clippy strict et les tests complets du workspace.

## 7. Rollbacks

- Six scénarios de sécurité ont réussi.
- La séquence inverse `0017` puis `0016` fonctionne sur une base vide.
- Les données peuplées empêchent les rollbacks destructifs et restent préservées.
- Une tentative de rollback `0016` avant `0017` est explicitement refusée.
- Les migrations forward `0013` à `0017` n'ont pas été modifiées.

## 8. Toolchain

- Rust est épinglé à `1.97.1` dans `rust-toolchain.toml`.
- Le profil est `minimal`.
- `rustfmt` et `clippy` sont déclarés explicitement.
- La CI consomme ce fichier au lieu d'une toolchain flottante.

## 9. Rebuild applicatif

- L'archive source de `849039385a14f95df0a95cca69e5987d3b311478` a été reconstruite séparément pour `linux/amd64`.
- L'archive source correspond au hash déjà documenté pour le déploiement.
- Les labels d'image indiquent la révision exacte, `phase4A.2` et l'activation de la console scientifique.
- L'image reconstruite démarre correctement avec la console désactivée et activée.
- `/health` répond `200` dans les deux modes ; `/science` répond respectivement `404` puis `200`.
- La base jetable atteint exactement 17 migrations.
- La reproductibilité fonctionnelle est démontrée. Le binaire et l'image ne sont pas bit-à-bit identiques à la production, les paquets APT et la chaîne BuildKit n'étant pas intégralement figés.

## 10. Revue du delta correctif

Le delta après `7f87dda` est limité à huit fichiers :

- workflow CI ;
- toolchain Rust ;
- lockfile et dépendance SQLx de test de l'API ;
- tests scientifiques API ;
- test de negative sampling ;
- tests de sécurité des rollbacks ;
- garde du rollback `0016`.

Aucun changement n'a été apporté au scoring v1, à l'algorithme candidat, aux migrations forward, au frontend scientifique ou à la configuration de production. Gitleaks `8.30.1` n'a détecté aucun secret dans les trois commits correctifs.

## 11. Stratégie de merge

- La PR a été passée de draft à ready après validation complète.
- L'historique permettait un fast-forward direct.
- Une anomalie locale macOS de lecture mémoire a interrompu une première commande `git merge --ff-only` après l'avancement partiel du worktree.
- L'état distant a été contrôlé immédiatement, puis `main` a été avancée directement et uniquement par fast-forward depuis la branche PR.
- Aucun commit de merge, squash, rebase ou force-push n'a été créé.
- Le dépôt local a ensuite été vérifié et réaligné exactement sur `origin/main`.

## 12. SHA final de `main`

`main` et `origin/main` pointent sur :

`d4730fda09571db491d0611e3308f96d1ee03ebb`

## 13. Tags

- Tag annoté `v0.4.2-app` → `849039385a14f95df0a95cca69e5987d3b311478`.
- Tag annoté `v0.4.2` → `d4730fda09571db491d0611e3308f96d1ee03ebb`.
- Les deux tags ont été poussés séparément et leur cible pelée a été vérifiée.

## 14. Release GitHub

La release publique [ERYTHEON v0.4.2 — Scientific candidate foundation and private console](https://github.com/supremexxx/erytheon/releases/tag/v0.4.2) a été publiée sur le tag `v0.4.2`. Elle n'est ni draft ni prerelease.

## 15. État final de la PR

- PR #1 : `MERGED`.
- Draft : non.
- Tête finale : `d4730fda09571db491d0611e3308f96d1ee03ebb`.
- Révision de merge : `d4730fda09571db491d0611e3308f96d1ee03ebb`.
- Date de merge GitHub : `2026-07-28T21:52:14Z`.

## 16. Contrôle de production en lecture seule

- Conteneur applicatif : running et healthy.
- Révision déclarée : `849039385a14f95df0a95cca69e5987d3b311478`.
- PostgreSQL/PostGIS et Caddy : running.
- Base : 17 migrations appliquées.
- Modèle v1 actif : exactement un.
- Candidat : présent dans la registry et inactif.
- Aucun événement récent de candidate scoring ou shadow scoring.
- Console scientifique et API privées : `401` sans authentification, `200` avec authentification.
- `/health` public : `200`.
- Aucun redémarrage, déploiement, changement de configuration ou write SQL n'a été effectué.

## 17. Risques résiduels

- Les builds Docker ne sont pas bit-à-bit reproductibles tant que toutes les dépendances système et l'environnement BuildKit ne sont pas figés.
- La taille historique de la PR rend les futures revues plus coûteuses ; les prochaines phases doivent rester plus petites.
- La console doit encore être observée en usage réel pour détecter incohérences d'affichage, lenteurs et erreurs opérationnelles.
- `main` contient maintenant le socle déployé, mais la production doit rester volontairement sur `8490393` jusqu'à une décision de déploiement séparée.

## 18. Prochaine étape

Ouvrir une phase séparée **4A.3 — stabilisation** limitée aux erreurs réelles, à l'ergonomie, à la cohérence des chiffres, au monitoring, aux rate limits Open-Meteo et à la documentation d'exploitation. Ne pas activer le candidat, ne pas lancer de shadow scoring et ne pas commencer la phase 4B dans cette intégration.
