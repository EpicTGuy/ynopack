# AppSpec — le format de décision

`appspec.toml` est le seul endroit du pipeline où une décision de packaging est prise. En amont,
`analyze` collecte des faits ; en aval, `generate` rend des templates sans rien décider.

Conséquence pratique : **on n'écrit jamais de bash à la main**. Pour corriger un paquet, on
édite `appspec.toml`, puis on relance `generate` et `verify`.

Exemple complet et à jour : [`tests/fixtures/appspec.example.toml`](../tests/fixtures/appspec.example.toml).
Un test le relit à chaque exécution de la suite, il ne peut donc pas diverger du code.

## Champs non résolus

Tout champ non trivial est un `Known<T>` : soit une valeur, soit la trace explicite de son absence.

```toml
execstart = "__NODEJS_DIR__/node __INSTALL_DIR__/server.js"            # résolu
execstart = { unknown = "aucun CMD dans le Dockerfile", \
              look_in = ["Procfile", "README.md"] }                    # à compléter
```

Pour compléter, on remplace la table par une valeur. Rien d'autre à faire : le type se relit dans
les deux formes.

Trois conséquences mécaniques :

1. `assess` liste les champs non résolus et refuse en nommant ce qui manque ;
2. `generate` dépose un `FIXME(ynopack): <champ> non déterminé — <raison> | chercher dans : <...>`
   dans le fichier concerné ;
3. `verify` échoue tant qu'il en reste un.

Détail volontaire : le défaut d'un champ textuel est *non résolu*, jamais la chaîne vide. Oublier de
renseigner un champ produit un FIXME visible plutôt qu'un paquet silencieusement incomplet.

## Sections

### `[app]` — identité

| Champ | Type | Note |
|---|---|---|
| `id` | chaîne | Minuscules, chiffres, tirets. Sert aussi d'utilisateur système, de nom de dossier et de préfixe de conf nginx |
| `name` | chaîne | Nom affiché. Le linter refuse au-delà de 23 caractères |
| `description_en` | `Known` | 150 caractères maximum, affichée dans le catalogue |
| `description_fr` | option | — |
| `version` | `Known` | Version **amont seule** : le suffixe `~ynh1` est ajouté à la génération |
| `maintainers` | liste | — |

### `[upstream]` — métadonnées du projet amont

`license` est le seul champ obligatoire côté YunoHost : identifiant SPDX, ex. `AGPL-3.0-or-later`.
Les autres (`website`, `demo`, `admindoc`, `userdoc`, `code`) ne sont renseignés que s'ils existent
réellement — le linter signale les URL laissées à leur valeur d'exemple.

### `[integration]` — relation à YunoHost

| Champ | Défaut | Note |
|---|---|---|
| `yunohost_min` | `>= 12.1.17` | — |
| `helpers_version` | `2.1` | Ne pas abaisser : les helpers ont été renommés en 2.1 |
| `architectures` | `"all"` | Ou une liste en nomenclature `dpkg --print-architecture` |
| `multi_instance` | `false` | Plusieurs installations sur la même machine |
| `ldap` / `sso` | `not_relevant` | `yes`, `no` ou `not_relevant` si l'app n'a pas de comptes |
| `disk`, `ram_build`, `ram_runtime` | `50M` | Estimations honnêtes : elles servent d'avertissement à l'admin |

`ldap` et `sso` ne se confondent pas : `ldap` dit que l'utilisateur *peut* se connecter avec ses
identifiants YunoHost, `sso` qu'il est connecté *automatiquement* depuis le portail.

### `[install]` — questions posées à l'administrateur

| Champ | Valeurs |
|---|---|
| `url_scheme` | `domain_and_path`, `full_domain` (domaine dédié), `no_url` (daemon sans web) |
| `init_main_permission` | `visitors` (public), `all_users` (privé), ou un groupe existant |
| `extra` | Questions supplémentaires, à n'ajouter qu'en dernier recours |

La philosophie YunoHost est explicite : « ne pas noyer l'administrateur sous des questions
techniques ». Toute question supplémentaire doit être justifiée — si la valeur peut être déduite ou
générée, elle ne doit pas être demandée.

### `[resources]` — ce que le cœur provisionne

Ces ressources sont créées **avant** le script d'installation et supprimées **après** le script de
suppression. C'est le cœur de YunoHost qui fait le travail, pas nos scripts.

| Champ | Effet |
|---|---|
| `system_user` | Utilisateur système portant l'`id` de l'app |
| `install_dir` | `/var/www/<app>`, exposé comme `$install_dir` |
| `data_dir` | `/home/yunohost.app/<app>`, préservé à la désinstallation sauf purge |
| `main_permission_url` | Chemin de la permission principale, typiquement `/` |
| `ports` | Réserve un port libre pour le reverse-proxy interne, exposé comme `$port` |
| `apt_packages` | Paquets Debian |
| `database` | `mysql`, `postgresql`, `sqlite`, `mongodb`, `none`, `unsupported` |
| `nodejs_version`, `ruby_version`, `go_version`, `composer_version` | Runtimes provisionnés par le cœur |

**Il n'y a pas de `python_version`** : contrairement à Node, Ruby, Go et Composer, Python n'a pas de
ressource officielle. Une app exigeant autre chose que le Python 3.11 de bookworm demande un venv
écrit à la main — c'est l'objet de la règle `PY001`.

### `[resources.sources]` — quoi télécharger

`url` et `sha256` sont obligatoires : la somme sert à la fois de contrôle d'intégrité et de
protection contre une archive amont modifiée après coup.

`autoupdate_strategy` déclenche la maintenance automatique côté infrastructure YunoHost, qui ouvrira
des pull requests quand l'upstream publie :

| Stratégie | Quand l'employer |
|---|---|
| `latest_github_release` | L'upstream publie des releases. **À préférer** : seule stratégie qui fournit le lien de changelog |
| `latest_github_tag` | Tags sans releases |
| `latest_github_commit` | Ni l'un ni l'autre. La version devient la date du commit |

`in_subdir = false` quand l'archive n'a pas de répertoire intermédiaire.

### `[runtime]` — ce qui pilote les scripts

La section sans équivalent direct dans le manifest : elle détermine le contenu des scripts.

| Champ | Provenance habituelle |
|---|---|
| `technology` | Image de base du Dockerfile, fichiers de projet |
| `build_steps` | Lignes `RUN` du Dockerfile, `scripts.build` de `package.json` |
| `execstart` | `CMD`/`ENTRYPOINT`, avec `__NODEJS_DIR__` et `__INSTALL_DIR__` substitués à la génération |
| `port_env_var` | Variable par laquelle l'app apprend son port ; câblée sur `$port` |
| `config_file` | Fichier de conf à générer dans `$install_dir` |
| `[runtime.env]` | Variables d'environnement, avec placeholders `__MAJUSCULES__` |

Les placeholders `__FOO__` sont remplacés par la valeur de `$foo` au moment où les helpers installent
le fichier. C'est le mécanisme de templating natif de YunoHost, pas une invention du projet.

### `[features]` — briques à câbler

Chaque drapeau conditionne des blocs dans **plusieurs scripts à la fois**. Activer `systemd` ajoute
du code dans install, remove, upgrade, backup, restore et change_url — d'où l'intérêt de le décider
ici une fois plutôt que six.

`nginx`, `systemd`, `phpfpm`, `logrotate`, `fail2ban`, `cron`, `change_url`, `service_integration`.

### `[docs]`

Contenu de `doc/DESCRIPTION.md` et des notices affichées avant et après installation. Le repli
déterministe est la description de la forge, puis le premier paragraphe du README amont.

Le `README.md` du paquet, lui, n'est **pas** dans cette section : il est généré depuis le manifest,
selon le format de `apps_tools/readme_generator`.
