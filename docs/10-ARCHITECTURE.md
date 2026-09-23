# Architecture

## Principe directeur

Le pipeline est une chaîne de transformations, chacune avec un contrat de données explicite. La
règle qui structure tout le reste :

> **Un seul étage décide.** Tout ce qui est en amont collecte, tout ce qui est en aval rend.

```
URL ──▶ RepoFacts ──▶ Feasibility ──▶ AppSpec ──▶ arbre du paquet ──▶ GateReport
        collecte      constat         DÉCISION    rendu               validation
        analyze       assess          plan        generate            verify/test
```

Pourquoi cette contrainte. D'abord, un détecteur qui ne décide pas se teste sur un arbre de fichiers
figé, sans réseau ni contexte : plusieurs agents peuvent travailler en parallèle sans se marcher
dessus. Ensuite, la relecture humaine se concentre sur un seul fichier, `appspec.toml`, au lieu
d'être éparpillée dans six scripts bash. Enfin, un bug se localise : une mauvaise valeur vient soit
d'un fait mal collecté, soit d'une décision mal prise — jamais des deux à la fois.

Un détecteur qui écrirait « puisque c'est du Node, le port est 3000 » viole cette séparation. Il
constate `EXPOSE 3000` ; c'est `plan` qui en tire une conséquence.

## Crates

| Crate | Rôle | Dépend de |
|---|---|---|
| `ynp-core` | Types du pipeline. Ne connaît ni le réseau, ni le disque, ni YunoHost | — |
| `ynp-forge` | GitHub (métadonnées, releases, tarball) et Forgejo (dépôts, push) | core |
| `ynp-dockerfile` | `Dockerfile` et `docker-compose.yml` → `BuildRecipe` | core |
| `ynp-analyze` | Détecteurs. Un module = un signal | core, forge, dockerfile |
| `ynp-rules` | Moteur de règles et scoring | core |
| `ynp-spec` | `RepoFacts` + `Feasibility` → `AppSpec`. **Le seul étage qui décide** | core |
| `ynp-gen` | Templates Tera → arbre de fichiers | core |
| `ynp-verify` | Schéma JSON, contrôles du linter, `shellcheck` | core |
| `ynp-runner` | Orchestration SSH de l'hôte de test | core |
| `ynp-publish` | Forgejo, catalogue custom, catalogue officiel | core, forge |
| `ynopack-cli` | Interface en ligne de commande | tous |
| `ynopack-server` | HTTP, file d'attente, SSE, interface web | tous |

`ynp-core` n'a volontairement aucune dépendance métier. C'est ce qui permet de le figer tôt et de
laisser les autres crates avancer indépendamment.

## Le mécanisme central : `Known<T>`

Un packager déterministe rencontre forcément des champs qu'il ne peut pas déduire. La réponse du
projet n'est pas de deviner, c'est de l'inscrire dans le type :

```rust
pub enum Known<T> {
    Value(T),
    Unresolved(Unresolved),   // raison + où chercher
}
```

En TOML, les deux formes se distinguent à l'œil :

```toml
execstart = "/var/www/foo/bin/server"                                  # résolu
execstart = { unknown = "aucun CMD", look_in = ["Procfile"] }          # à compléter
```

Trois conséquences mécaniques, et c'est ce qui rend l'absence de LLM tenable :

1. `assess` compte les champs non résolus et refuse en nommant précisément ce qui manque ;
2. `generate` dépose un marqueur `FIXME(ynopack)` dans le fichier concerné ;
3. `verify` échoue tant qu'il en reste un.

Détail volontaire : le `Default` d'un champ textuel est *non résolu*, jamais la chaîne vide. Oublier
de renseigner un champ produit ainsi un FIXME visible plutôt qu'un paquet silencieusement incomplet.

## Pourquoi le Dockerfile est la pièce maîtresse

Un `Dockerfile` est déjà une spécification de construction lisible par une machine :

| Instruction | Ce qu'on en tire |
|---|---|
| `FROM node:20-bookworm` | Technologie et version de runtime |
| `RUN apt-get install -y libvips` | Dépendances apt, littéralement |
| `RUN npm ci && npm run build` | Étapes de build |
| `EXPOSE 3000` | Port de reverse-proxy interne |
| `CMD ["node", "server.js"]` | `ExecStart` de l'unité systemd |
| `ENV DATABASE_URL=...` | Configuration à câbler |
| Dernière étape d'un multi-étage | Runtime réel, par opposition au builder |

D'où `BuildRecipe::runtime_stage()`, qui rend la dernière étape et non la première : dans un
multi-étage, l'image de build (`node:20`) n'est pas l'image de runtime (`nginx:alpine`), et se
tromper de cible fausse toute la détection de stack.

Le Dockerfile n'est **jamais exécuté ni embarqué** : une application YunoHost tourne en natif. Il
est lu comme documentation exécutable de ce que l'upstream fait pour construire son application.

## Gestion des erreurs

`Finding` plutôt qu'`Error` partout où le problème concerne l'application analysée et non l'outil.
Un `Finding` porte un identifiant stable, une sévérité, une preuve (fichier, ligne, extrait) et une
remédiation. Un rapport qu'on ne peut pas actionner fait perdre du temps à celui qui le lit.

Les `Result<_, Error>` sont réservés aux pannes de l'outil : réseau, disque, TOML invalide.

## Choix de dépendances, et ce qu'on a écarté

| Choix | Alternative écartée | Motif |
|---|---|---|
| Tarball via l'API de la forge | `git2` / `gix` | L'analyse n'a pas besoin de l'historique. Pas de libgit2 à compiler |
| `git` en sous-processus pour publier | crate git | Trois commandes, `git` est présent partout |
| `ssh`/`rsync` en sous-processus | `russh` | Jobs de 10-30 min ; `ControlMaster` gère le maintien de session mieux qu'une implémentation maison. ~800 lignes en moins |
| `tera` | `askama`, `minijinja` | Portage direct des templates Jinja de YoloGen, qui restent la référence amont |
| `jsonschema` sur le schéma officiel | validateur maison | Le schéma est maintenu en amont : on suit ses évolutions gratuitement |
| `indexmap` pour les variables d'environnement | `HashMap` | L'ordre de déclaration est signifiant dans un fichier de configuration |

## Format des artefacts

Chaque étage écrit un artefact relisible, ce qui permet de reprendre le pipeline en cours de route :

| Fichier | Format | Écrit par | Relu par |
|---|---|---|---|
| `facts.json` | JSON | `analyze` | `assess`, `plan` |
| `report.json` | JSON | `assess` | `plan`, gate G1 |
| `appspec.toml` | TOML | `plan` | **humain ou agent**, puis `generate` |
| `<app>_ynh/` | arbre | `generate` | `verify`, `test`, `publish` |
| `lint.json` | JSON | `verify` | gate G2 |

`appspec.toml` est en TOML et non en JSON précisément parce qu'il est fait pour être édité à la
main : commentaires, lisibilité, et cohérence avec le format des manifests YunoHost.
