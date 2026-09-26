# Cahier des charges — yunopack

> Statut : approuvé · Version 1.0 · Cible YunoHost 12.1.x / Debian bookworm

## 1. Problème

Packager une application pour YunoHost est un travail artisanal : cloner `example_ynh`, remplir un
`manifest.toml`, écrire six scripts bash, deviner les dépendances apt, choisir un port, puis itérer
des heures sur `package_check`. Le seul outil d'automatisation existant, [appgenerator
(« YoloGen »)](https://github.com/YunoHost/appgenerator), est un formulaire : **l'humain remplit les
quarante champs, l'outil ne fait que rendre des templates**.

Tout le travail difficile — comprendre ce que l'application exige — reste donc manuel.

## 2. Objectif

Un outil autonome qui, à partir d'une seule URL de dépôt, décide si l'application est packageable,
produit le paquet, le valide jusqu'à l'installation réelle, et le publie installable.

```
yunopack run https://github.com/foo/bar --host=<machine-de-test>
```

## 3. Périmètre

### Inclus

- Dépôts GitHub publics (GitLab, Gitea et Forgejo en extension du même code).
- Format de packaging v2, helpers 2.1, YunoHost ≥ 12.1.17.
- Applications web et daemons : PHP, Node.js, Go, Ruby, Python, statique.
- Publication sur forge Forgejo auto-hébergée + catalogue custom.
- Promotion optionnelle vers le catalogue officiel YunoHost.

### Exclu, et pourquoi

| Hors périmètre | Raison |
|---|---|
| Applications dont le **runtime** exige Docker | Une app YunoHost tourne en natif sur l'hôte. C'est un refus, pas un avertissement |
| Déploiements uniquement Helm / Kubernetes | Même raison, à une échelle différente |
| Bases hors MySQL / PostgreSQL / MongoDB / Redis | Le cœur de YunoHost ne sait pas les provisionner |
| Migration d'un paquet v1 vers v2 | Problème distinct, outillé en amont par `apps_tools/autopatches` |
| Maintenance à long terme du paquet | Prise en charge par `autoupdate.strategy` côté YunoHost |
| Tout appel à un LLM à l'exécution | Décision d'architecture, cf. ADR-002 |

## 4. Exigences fonctionnelles

### Analyse

| Id | Exigence | Critère d'acceptation |
|---|---|---|
| EXG-F-01 | Résoudre une URL de dépôt en métadonnées : licence SPDX, description, archivé, date du dernier push, tags, releases et leurs assets | `analyze` produit un `facts.json` valide sur les 5 apps du corpus de référence |
| EXG-F-02 | Extraire du `Dockerfile` une recette de build : images de base, paquets apt/apk, étapes de build, `EXPOSE`, `ENTRYPOINT`/`CMD`, `ENV`, `WORKDIR` | Sur le corpus de Dockerfiles de test, la `BuildRecipe` attendue est produite au champ près |
| EXG-F-03 | Extraire de `docker-compose.yml` les services, ports, volumes et variables, et distinguer le service applicatif des services d'infrastructure | Un compose avec app + postgres + redis donne `database = postgresql` et `needs_redis = true` |
| EXG-F-04 | Identifier la stack principale et la version de runtime exigée | Précision ≥ 90 % sur le corpus d'évaluation |
| EXG-F-05 | Classer chaque variable de configuration par rôle canonique (port, URL de base, identifiants de base, secret…) | `PORT`, `DATABASE_URL`, `APP_URL`, `SECRET_KEY` sont reconnues sans configuration |
| EXG-F-06 | Choisir la source à télécharger et calculer son `sha256` | Le `sha256` produit est identique à celui calculé par `sha256sum` sur l'archive |

### Faisabilité

| Id | Exigence | Critère d'acceptation |
|---|---|---|
| EXG-F-10 | Évaluer le dépôt contre un catalogue de règles versionnées, chacune avec identifiant stable, sévérité, preuve et remédiation | Chaque règle du catalogue a au moins un test unitaire positif et un négatif |
| EXG-F-11 | Rendre un verdict `FAISABLE` / `FAISABLE AVEC TRAVAIL` / `NON FAISABLE` et un score 0-100 | Un seul constat bloquant suffit à produire `NON FAISABLE` |
| EXG-F-12 | **Refuser explicitement plutôt que produire un paquet deviné** | Une app sans source stable est refusée avec la règle `SRC001` nommée, pas packagée à moitié |

### Génération

| Id | Exigence | Critère d'acceptation |
|---|---|---|
| EXG-F-20 | Produire un `appspec.toml` relisible et éditable, seul point de décision du pipeline | Le fichier fait un aller-retour TOML sans perte |
| EXG-F-21 | Marquer tout champ non déductible comme non résolu, avec sa raison et où chercher | Une app opaque produit des `FIXME(yunopack)`, jamais une valeur inventée |
| EXG-F-22 | Générer l'arborescence complète du paquet : `manifest.toml`, `scripts/` (install, remove, upgrade, backup, restore, change_url, `_common.sh`), `conf/`, `doc/`, `tests.toml`, `README.md` | Le paquet généré pour l'app canonique est structurellement identique à `example_ynh` |
| EXG-F-23 | N'employer que les helpers 2.1 | Aucun helper obsolète détecté par le linter officiel |
| EXG-F-24 | Générer le `README.md` selon le générateur officiel, pas à la main | Le rendu est aligné avec `apps_tools/readme_generator` |

### Vérification et test

| Id | Exigence | Critère d'acceptation |
|---|---|---|
| EXG-F-30 | Valider le `manifest.toml` contre le schéma JSON officiel | Un manifest volontairement invalide est rejeté avec le chemin fautif |
| EXG-F-31 | Reproduire les contrôles critiques de `package_linter` | Aucun désaccord avec le linter officiel sur le corpus |
| EXG-F-32 | Refuser tout paquet contenant un `FIXME(yunopack)` résiduel | La gate G2 échoue et nomme chaque champ manquant |
| EXG-F-33 | Installer réellement le paquet sur un hôte YunoHost distant, vérifier l'endpoint HTTP, la sauvegarde/restauration, puis la désinstallation sans résidu | Le cycle complet passe sur `<machine-de-test>` pour l'app canari |
| EXG-F-34 | Contrôler l'absence de résidus après désinstallation : utilisateur système, `$install_dir`, conf nginx, base de données | Un paquet qui laisse une trace fait échouer la gate G3 |
| EXG-F-35 | Piloter `package_check` en VM isolée et remonter le niveau 0-8 *(optionnel)* | Le niveau remonté correspond à celui du journal de `package_check` |

### Publication

| Id | Exigence | Critère d'acceptation |
|---|---|---|
| EXG-F-40 | Créer le dépôt sur Forgejo et y pousser le paquet | Le dépôt `<app>_ynh` existe et contient l'arborescence |
| EXG-F-41 | Produire et publier un catalogue custom consommable par `/etc/yunohost/apps_catalog.yml` | `yunohost app install <app>` depuis le catalogue aboutit |
| EXG-F-42 | Préparer une contribution au catalogue officiel, sous condition de niveau ≥ 4 *(optionnel)* | La PR n'est proposée que si la gate G4 est passée |

### Interfaces

| Id | Exigence | Critère d'acceptation |
|---|---|---|
| EXG-F-50 | CLI couvrant chaque étage séparément, plus un `run` qui les enchaîne | Chaque sous-commande s'exécute seule à partir de l'artefact de l'étage précédent |
| EXG-F-51 | Code de sortie désignant la gate en échec | Un script appelant sait où ça a cassé sans analyser la sortie |
| EXG-F-52 | Serveur HTTP : coller une URL, suivre la progression, récupérer le résultat | Une URL collée aboutit au rapport puis au lien du dépôt publié |
| EXG-F-53 | Harnais d'évaluation comparant les paquets générés aux paquets YunoHost existants | `yunopack eval` sort une matrice de précision par champ |

## 5. Exigences non fonctionnelles

| Id | Exigence | Critère d'acceptation |
|---|---|---|
| EXG-NF-01 | **Aucun appel à un LLM à l'exécution.** Aucune clé d'API, aucun réseau hors forge | `grep` sur les dépendances : aucun client LLM. Le binaire tourne hors ligne hors appels forge |
| EXG-NF-02 | Déterminisme : deux exécutions sur le même commit produisent des artefacts identiques | Deux `generate` successifs donnent des fichiers identiques au bit près |
| EXG-NF-03 | Traçabilité : tout constat cite le fichier et, quand c'est pertinent, la ligne | Chaque `Finding` de sévérité ≥ Mineur porte une preuve |
| EXG-NF-04 | Analyse d'un dépôt de taille moyenne en moins de 30 secondes | Mesuré sur le corpus |
| EXG-NF-05 | Aucune écriture hors du répertoire de sortie et du cache | Vérifié en test d'intégration |
| EXG-NF-06 | Les tests sur l'hôte YunoHost sont réversibles | Après échec, `yunohost app remove` restaure l'état antérieur |
| EXG-NF-07 | La doc de référence YunoHost est versionnée dans le dépôt et resynchronisable | `scripts/refresh-docs.sh` est idempotent |
| EXG-NF-08 | Qualité : `cargo fmt --check`, `cargo clippy -- -D warnings` et `cargo test` passent | Vérifié en intégration continue |

## 6. Contraintes imposées par YunoHost

Relevées dans la documentation officielle versionnée sous `docs/yunohost/`. Non négociables.

1. Format `packaging_format = 2`, `helpers_version = "2.1"`. Les helpers ont été renommés en 2.1
   (`ynh_config_add_nginx`, `ynh_config_add_systemd`, `ynh_systemctl`, `ynh_replace`…) : produire les
   anciens noms déclenche une erreur du linter.
2. Le `sha256` des sources est obligatoire — d'où `SRC001`.
3. Il existe des `[resources]` pour nodejs, ruby, go et composer, **mais pas pour Python** — d'où `PY001`.
4. `[resources.database]` ne provisionne que `mysql` et `postgresql`.
5. `package_check` calcule un niveau 0-8 ; ≥ 4 signifie installable, upgradable, sauvegardable.
6. Politique du catalogue : logiciel libre ou éthique au cas par cas, pas de cryptomonnaie, pas de niche extrême.

## 7. Critères d'acceptation du projet

Le projet est considéré livré quand :

1. `yunopack run <url> --host=<machine-de-test>` produit, pour au moins **trois applications réelles de stacks
   différentes**, un paquet passant les gates G0 à G3 et installé sur l'instance de test.
2. `yunopack assess` refuse, avec la règle nommée, au moins **trois applications réellement non
   packageables** (runtime Docker, base non supportée, absence de source stable).
3. `yunopack eval` publie une matrice de précision par champ sur le corpus de référence.
4. Un paquet publié s'installe depuis le catalogue custom sur une instance vierge.
5. `cargo fmt --check && cargo clippy -- -D warnings && cargo test` passe.

## 8. Ce qui définirait un échec

À énoncer explicitement, pour qu'on puisse le constater plutôt que le contourner :

- l'outil produit un paquet qui *paraît* complet mais dont les valeurs ont été devinées ;
- le taux de couverture stagne et la réponse envisagée est de rajouter un LLM plutôt que d'enrichir
  les détecteurs (cf. ADR-002) ;
- la validation dynamique est abandonnée et l'outil ne garantit plus que le paquet s'installe.
