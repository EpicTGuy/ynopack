# Règles de faisabilité

Chaque règle porte un identifiant stable, une sévérité, une preuve et une remédiation. Le catalogue
est versionné : ajouter une règle, c'est ajouter une entrée ici **et** un test positif plus un test
négatif dans `ynp-rules`.

## Sévérités

| Sévérité | Effet sur le score | Effet sur le verdict |
|---|---|---|
| `Blocker` | −100 | Un seul suffit : `NON FAISABLE` |
| `Major` | −20 | Dégrade, ne refuse pas |
| `Minor` | −5 | Dégrade à la marge |
| `Info` | 0 | Renseigne l'`AppSpec`, n'est pas un défaut |

Score = 100 − somme des pénalités, borné à 0. Il est indicatif : c'est **l'absence de bloquant** qui
décide. Le seuil (60 par défaut) ne sépare que « faisable » de « faisable avec travail ».

## Bloquants

### `LIC001` — Licence non libre ou absente
Le catalogue YunoHost n'accepte que du logiciel libre, ou éthique au cas par cas
(`docs/yunohost/90-policy.md`). Sans identifiant SPDX exploitable, on ne peut ni remplir
`upstream.license` ni trancher sur l'admissibilité.
**Preuve** : `license.spdx_id` de la forge, ou absence de `LICENSE`.
**Remédiation** : vérifier manuellement ; une licence non détectée automatiquement peut être valide.

### `SRC001` — Aucune source stable
Le manifest exige un `sha256` sur les sources. Sans tag, sans release et sans archive stable, il n'y
a rien à figer : le paquet ne serait pas reproductible.
**Remédiation** : utiliser `autoupdate.strategy = "latest_github_commit"`, qui fixe la version à la
date du commit — à défaut, attendre que l'upstream publie une release.

### `RUN001` — Le runtime exige un moteur de conteneurs
Une application YunoHost tourne en natif sur l'hôte. Une application distribuée **uniquement** sous
forme d'image, sans procédure d'installation native, sort du modèle.
**Nuance importante** : la présence d'un Dockerfile n'est pas un blocage — c'est au contraire notre
meilleure source d'information. Le blocage porte sur les applications dont le lancement suppose un
orchestrateur (services multiples interdépendants, réseau de conteneurs, `depends_on` sur des
composants non packageables).
**Remédiation** : vérifier si l'upstream documente une installation native.

### `DB002` — Base de données non supportée
`[resources.database]` ne provisionne que MySQL et PostgreSQL ; MongoDB et Redis passent par des
helpers. Elasticsearch, ClickHouse, Cassandra et consorts n'ont pas d'équivalent.
**Remédiation** : aucune dans le cadre du catalogue.

### `K8S001` — Déploiement uniquement Helm/Kubernetes
Variante de `RUN001` à une autre échelle : pas de chemin d'installation sur une machine unique.

## Majeurs

### `PY001` — Python autre que celui de bookworm
Il existe des `[resources]` pour nodejs, ruby, go et composer — **pas pour Python**. Une application
exigeant un Python différent du 3.11 de Debian bookworm impose un venv ou un pyenv écrit à la main.
**Remédiation** : vérifier si l'application tourne réellement sur 3.11 ; sinon, prévoir du travail manuel.

### `BUILD001` — Build gourmand en mémoire
Un build de frontal JavaScript dépasse fréquemment 1,5 Go. Beaucoup d'instances YunoHost tournent
sur ARM avec 1 à 2 Go : le build échouera chez une partie des utilisateurs.
**Remédiation** : privilégier une release contenant les assets déjà construits, et renseigner
`ram.build` honnêtement.

### `ARCH001` — Binaires spécifiques à une architecture
L'upstream ne publie que de l'amd64.
**Remédiation** : `architectures = ["amd64"]` — l'application sera masquée sur les autres plateformes.

### `PORT001` — Port fixe privilégié
L'application exige le 80 ou le 443, occupés par le nginx de YunoHost qui assure le reverse-proxy.
**Remédiation** : vérifier si le port est configurable ; sinon, incompatible.

## Mineurs

### `MAINT001` — Dépôt archivé ou inactif
Dépôt archivé, ou sans commit depuis plus de vingt-quatre mois. Le niveau 8 du catalogue exige une
maintenance effective ; un paquet abandonné devient une dette pour la communauté.

## Informatifs

Ces règles ne sont pas des défauts : elles transportent vers `plan` une information déduite.

| Id | Constat | Conséquence dans l'`AppSpec` |
|---|---|---|
| `DB001` | MySQL ou PostgreSQL détecté | `resources.database` |
| `SSO001` | Ni LDAP ni OIDC | `ldap = false`, `sso = false` |
| `PORT002` | `EXPOSE` trouvé | `resources.ports`, `runtime.port_env_var` |
| `NODE001` | Version de Node exigée | `resources.nodejs_version` |
| `ASSET001` | Release avec assets préconstruits | Source choisie, `BUILD001` évitée |

## Ajouter une règle

1. Choisir un identifiant dans une famille existante (`LIC`, `SRC`, `RUN`, `DB`, `BUILD`, `ARCH`,
   `PORT`, `MAINT`, `NODE`, `ASSET`) ou en ouvrir une, documentée ici.
2. Implémenter `Rule` dans `ynp-rules`, en citant systématiquement une preuve.
3. Écrire un test qui déclenche la règle et un test qui ne la déclenche pas. Le second est le plus
   important : c'est lui qui attrape les faux positifs.
4. Documenter la règle ici, avec sa remédiation.

Une règle sans remédiation actionnable n'a pas sa place dans le catalogue.
