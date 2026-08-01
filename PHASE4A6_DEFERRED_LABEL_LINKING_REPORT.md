# Rattachement différé BDIFF

La Phase 4A.6 prépare un linker borné, rejouable et inactif par défaut.

Règle `bdiff-exact-h3-week-v1` : événement BDIFF actif, même H3, date comprise dans `[snapshot.valid_at, +7 jours)`, et `updated_at` antérieur au seuil de maturité fourni. FIRMS n'est jamais lu et aucun négatif synthétique n'est créé.

La CLI est en dry-run par défaut, bornée à 100 événements (`--limit`). Elle rapporte les classes,
les H3 trouvés/absents, les liens proposés et les exclusions supervisées. L'écriture exige
`--apply`. Un changement de cause ne modifie pas l'ancien lien : il le marque supersédé et crée
une nouvelle version avec `supersedes_link_id`. Les causes `unknown` et `indeterminate` restent
leurs propres classes d'audit, sont exclues des labels supervisés et ne deviennent jamais
`no_event`.

État de production audité : zéro lien. Aucun rattachement n'a été écrit pendant cette phase de développement.
