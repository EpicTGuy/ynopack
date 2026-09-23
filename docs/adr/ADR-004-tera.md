# ADR-004 — Tera pour les templates

**Statut** : accepté · 23/09/2026

## Contexte

Les scripts d'un paquet YunoHost sont fortement répétitifs et conditionnels : activer systemd ajoute
des blocs dans six fichiers. `appgenerator` (YoloGen) a déjà résolu ce problème avec des templates
Jinja, maintenus en amont et alignés sur `example_ynh`.

## Décision

Tera, et portage direct des templates de YoloGen, conservés comme référence dans
`tests/fixtures/yologen/`.

## Raisons

La syntaxe de Tera est celle de Jinja. Le portage est une traduction ligne à ligne, pas une
réécriture : les évolutions amont restent lisibles et reportables.

YoloGen est maintenu par le projet YunoHost, donc suit les changements de format et les renommages
de helpers. Partir de ses templates, c'est hériter de ce travail plutôt que redécouvrir les
conventions une par une.

Les alternatives compilées (`askama`) obligeraient à réécrire dans une autre syntaxe, ce qui casse
la correspondance avec l'amont.

## Conséquences

- `scripts/refresh-docs.sh` récupère les `.j2` de YoloGen : on peut diffuser nos templates contre
  les leurs après chaque évolution amont.
- Le rendu se fait à l'exécution, pas à la compilation : une erreur de template se voit au test, pas
  au build. Compensé par les snapshots `insta` de la tâche L3-9.
- YoloGen porte un mode « tutoriel » qui injecte des commentaires pédagogiques. Nous ne le reprenons
  pas : nos paquets sont générés pour être installés, et les commentaires `###` seraient du bruit.
