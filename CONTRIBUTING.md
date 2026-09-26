# Contribuer

Ce fichier fait autorité pour toute contribution au dépôt. Il est court exprès.

## Les trois règles qui ne se discutent pas

### 1. Les décisions viennent de règles, pas d'inférence

Aucun appel réseau hors GitHub et Forgejo. Si un champ ne peut pas être déduit, on le marque non
résolu — on ne le devine pas.

C'est la décision fondatrice du projet ([ADR-002](docs/adr/ADR-002-zero-llm.md)). Si un détecteur
couvre mal un cas, la réponse est d'enrichir le détecteur ou les tables de `assets/knowledge/`.

### 2. On n'écrit pas de bash à la main

Les scripts d'un paquet YunoHost sont **entièrement** produits par les templates Tera de
`assets/templates/`. Un correctif sur un paquet généré se fait dans le template ou dans
l'`AppSpec`, jamais dans le fichier de sortie.

Corollaire pour l'exploitation : un paquet incomplet se corrige **dans `appspec.toml`**, puis on
relance `generate` et `verify`. On ne touche pas à `scripts/install` directement.

### 3. Toute décision passe par `AppSpec`

La séparation des étages est le socle de la testabilité :

| Étage | Peut décider ? |
|---|---|
| `analyze` → `RepoFacts` | Non. Collecte uniquement, aucune interprétation |
| `assess` → `Feasibility` | Non. Constate et classe |
| `plan` → **`AppSpec`** | **Oui. Le seul.** |
| `generate` → arbre | Non. Rendu mécanique |

Un détecteur qui écrit « puisque c'est du Node, je mets le port à 3000 » viole cette séparation :
il constate `EXPOSE 3000`, et c'est `plan` qui en tire une conséquence.

## Avant de commencer une tâche

1. Lire la tâche dans [BACKLOG.md](BACKLOG.md) — chacune tient dans un crate ou un détecteur.
2. Lire la doc YunoHost concernée dans `docs/yunohost/`. **Ne jamais travailler de mémoire** : le
   format de packaging a changé plusieurs fois, et les helpers ont été renommés en 2.1.
3. Vérifier dans `crates/ynp-core/src/` les types qu'on va manipuler. Ils sont figés : les modifier
   impacte tous les autres crates et demande de mettre à jour ce document.

## Avant de rendre une tâche

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Les trois doivent passer. Aucune exception, aucun `#[allow]` ajouté pour faire taire clippy sans
justification écrite en commentaire.

## Conventions

**Tests.** Chaque détecteur, chaque règle, chaque template a ses tests. Les noms de tests sont des
phrases en français qui énoncent la propriété vérifiée — `un_seul_bloquant_suffit_a_refuser` se lit
dans un rapport d'échec sans avoir à ouvrir le fichier.

**Commentaires.** On explique *pourquoi*, pas *quoi*. « Python n'a pas de resource officielle, d'où
le traitement particulier » est utile ; « incrémente le compteur » ne l'est pas.

**Langue.** Documentation, commentaires et noms de tests en français. Identifiants de code en
anglais, sans accents ni caractères non-ASCII dans le code source.

**Messages d'erreur.** Un `Finding` de sévérité supérieure à `Info` porte toujours une remédiation
et une preuve. Un rapport qu'on ne peut pas actionner fait perdre du temps à celui qui le lit.

**Commits.** Une tâche du backlog par commit, message à l'impératif décrivant l'effet.

## Pièges spécifiques à YunoHost

Chacun coûte un aller-retour avec le linter officiel si on l'ignore :

- **Helpers 2.1 uniquement.** `ynh_config_add_nginx`, pas `ynh_add_nginx_config`. La liste complète
  est dans `docs/yunohost/21-helpers-2.1.md`.
- **Une app YunoHost ne tourne pas dans Docker.** Le Dockerfile sert de source d'information sur le
  build ; il n'est jamais exécuté ni embarqué.
- **Pas de `[resources.python]`.** Node, Ruby, Go et Composer en ont une, Python non. Une app
  exigeant un Python autre que le 3.11 de bookworm demande un traitement manuel : c'est la règle `PY001`.
- **`sha256` obligatoire** sur les sources. Sans tag ni release, pas de somme stable : règle `SRC001`.
- **Les scripts tournent en `set -eu`**, sauf `remove`. Une variable non définie fait échouer l'install.
- **Le `README.md` d'un paquet est généré**, pas écrit. Il suit le format de
  `apps_tools/readme_generator` ; s'en écarter fait remonter un avertissement du linter.

## Machines de test

Déclarées dans `~/.ssh/config`, jamais d'identifiants en dur dans le code.

| Alias | Rôle | À savoir |
|---|---|---|
| `forgejo` | Accès SSH à la forge pour les push | Clé déjà autorisée |
