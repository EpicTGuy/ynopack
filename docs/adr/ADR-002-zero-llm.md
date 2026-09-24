# ADR-002 — Aucun appel à un LLM à l'exécution

**Statut** : accepté · 23/09/2026 · **Décision fondatrice du projet**

## Contexte

Packager une application demande de comprendre comment elle se construit et se lance. La réponse
réflexe est de confier cette compréhension à un modèle de langage : on lui donne le README et le
dépôt, il produit le manifest et les scripts.

## Décision

Aucun appel à un modèle dans le binaire livré. Ni clé d'API, ni réseau hors forge, ni résultat qui
varie d'une exécution à l'autre.

Des agents IA **construisent** l'outil ; l'outil, lui, tourne seul.

## Raisons

**Le problème n'est pas un problème de raisonnement.** Une application auto-hébergeable moderne
embarque déjà sa recette de construction sous forme lisible par une machine. Un `Dockerfile`, c'est
une image de base (donc une version de runtime), des `apt-get install` (donc des dépendances,
littéralement), des `RUN npm ci && npm run build` (donc des étapes de build), un `EXPOSE` (donc un
port), un `CMD` (donc un `ExecStart`). Ce qu'on confierait à un modèle — « lis et devine » — est en
réalité une transpilation. De l'analyse syntaxique.

**Un paquet est un artefact de sécurité.** Il crée un utilisateur système, ouvre un port, écrit une
configuration nginx et s'exécute en tant que service sur la machine de quelqu'un. Une valeur
plausible mais fausse y est plus dangereuse qu'une valeur absente.

**Un modèle comble les trous sans le dire.** C'est le comportement attendu d'un générateur de texte,
et c'est précisément ce qu'il ne faut pas ici. Notre mécanisme `Known<T>` fait l'inverse : il rend
l'absence visible, la propage jusqu'au fichier généré sous forme de `FIXME(yunopack)`, et fait
échouer `verify` tant qu'elle subsiste.

**Le déterminisme est testable.** Deux exécutions sur le même commit produisent des fichiers
identiques au bit près, donc le harnais d'évaluation mesure vraiment le progrès des détecteurs.
Avec un modèle dans la boucle, une régression et une variation d'échantillonnage sont
indiscernables.

## Ce que nous acceptons de perdre

Environ 20 à 25 % des dépôts n'ont ni Dockerfile ni système de build reconnaissable. Pour ceux-là,
l'outil produit un `appspec.toml` partiellement rempli, avec des champs non résolus, et refuse de
générer un paquet complet.

C'est un compromis assumé : ces cas remontent à un humain ou à un agent, qui complète `appspec.toml`
— donc un fichier de décision, pas six scripts bash.

## Ce qui invaliderait cette décision

Si le taux de couverture stagne bas **et** que les tables de connaissance plus les détecteurs ne le
font pas progresser sur plusieurs itérations mesurées par `yunopack eval`, alors l'hypothèse
« l'information est présente et structurée » est fausse pour la population visée, et il faudra
rouvrir la question.

Tant que la mesure progresse, la réponse à un cas mal couvert est d'enrichir un détecteur — jamais
d'ajouter un appel à un modèle.
