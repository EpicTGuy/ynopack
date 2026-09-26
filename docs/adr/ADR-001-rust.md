# ADR-001 — Rust pour l'implémentation

**Statut** : accepté · 23/09/2026

## Contexte

L'outillage YunoHost est en Python (`package_linter`, `apps_tools`, `appgenerator`) et en bash
(`package_check`). Écrire en Python nous placerait dans la continuité de l'écosystème.

## Décision

Rust, en workspace Cargo.

## Raisons

L'outil manipule des contrats de données stricts, qui traversent sept étages. Le système de types
transforme « ce champ peut manquer » en contrainte vérifiée à la compilation plutôt qu'en convention
à respecter — c'est exactement ce que fait `Known<T>`, et c'est le mécanisme central du projet.
En Python, le même mécanisme reposerait sur la discipline du développeur.

Un binaire unique se copie sur l'hôte de test sans installer d'interpréteur : `<machine-de-test>` n'a ni cargo,
ni shellcheck, et ne doit rien recevoir d'autre que le paquet à tester.

L'exhaustivité des `match` fait échouer la compilation quand une variante est ajoutée — ajouter une
technologie ou une base de données oblige à traiter le cas partout où il compte.

## Conséquences

- Nous n'héritons pas directement du code amont : les templates Jinja sont portés en Tera (ADR-004)
  et les contrôles du linter réimplémentés (tâche L4-2).
- Le test différentiel contre le vrai `package_linter` devient obligatoire — c'est la tâche L4-6.
- Un contributeur doit connaître Rust. Contrainte assumée : le projet est conçu pour être écrit par
  des agents, pas par une communauté large.
