# YunoPackage

**Colle une URL GitHub, récupère une application YunoHost installable.**

YunoPackage analyse un dépôt, décide s'il est packageable pour YunoHost — avec des raisons
explicites quand il refuse —, génère le paquet, le valide jusqu'à l'installation réelle sur un
serveur, puis le publie.

```console
$ ynopack run https://github.com/foo/bar --host=dell

G0 Legal & policy .................. ok   AGPL-3.0, dépôt actif
G1 Faisabilité ..................... ok   score 85/100 — 1 majeur, 2 infos
G2 Conformité statique ............. ok   manifest valide, 0 erreur linter
G3 Installabilité .................. ok   install, HTTP 200, backup/restore, remove propre
G5 Publication ..................... ok   https://forge.local/etg/bar_ynh

   yunohost app install bar
```

## Le parti pris : aucun LLM à l'exécution

Des agents IA **construisent** l'outil. L'outil, lui, ne fait appel à aucun modèle : ni clé d'API,
ni réseau hors la forge, ni résultat qui change d'une exécution à l'autre.

Ce n'est pas un choix idéologique, c'est ce que permet la nature du problème. Une application
auto-hébergeable moderne embarque déjà sa recette de construction sous forme lisible par une
machine. Un `Dockerfile`, c'est une image de base (donc une version de runtime), des `apt-get
install` (donc des dépendances, littéralement), des `RUN npm ci && npm run build` (donc des étapes
de build), un `EXPOSE` (donc un port), un `CMD` (donc un `ExecStart` systemd). Le travail qu'on
confierait d'ordinaire à un modèle — « lis le README et devine » — est en réalité une
**transpilation** : de l'analyse syntaxique, pas du raisonnement.

Reste ce qui ne se déduit pas. La réponse n'est pas de deviner, c'est de le dire :

```toml
# appspec.toml — extrait
execstart = { unknown = "aucun CMD dans le Dockerfile", look_in = ["Procfile", "README.md"] }
```

`verify` refuse tout paquet où subsiste un marqueur de ce genre. **L'outil ne rend jamais un paquet
qui a l'air fini alors qu'il est deviné** — et c'est plus sûr qu'un modèle, qui comble les trous
sans le signaler.

## Le pipeline

| Commande | Produit | Rôle |
|---|---|---|
| `ynopack analyze <url>` | `facts.json` | Ce qui **est** dans le dépôt, sans interprétation |
| `ynopack assess` | `report.json` | Règles de faisabilité, verdict, score |
| `ynopack plan` | `appspec.toml` | **Le seul point de décision** — relu par un humain ou un agent |
| `ynopack generate` | `<app>_ynh/` | Rendu de templates, aucune décision |
| `ynopack verify` | `lint.json` | Schéma officiel, règles du linter, `bash -n`, jetons de configuration |
| `ynopack test --host=dell` | `test.json` | Install réelle, service, endpoint, backup/restore, remove sans résidu |
| `ynopack publish` | URL du dépôt | Forgejo + entrée de catalogue |
| `ynopack run <url>` | tout | Enchaîne les étapes, s'arrête à la première gate en échec |
| `ynopack eval` | matrice | Compare les paquets produits à ceux du catalogue officiel |

Il existe aussi `ynopack-server`, qui expose le même pipeline dans un navigateur :
on colle une URL, on suit la progression en direct.

Un agent n'écrit jamais de bash : il édite `appspec.toml`, relance `generate`, puis `verify`.

## Les six gates

L'exigence « une fois toutes les validations passées, il package » se matérialise en six portes.
Le pipeline s'arrête à la première qui échoue, et le code de sortie désigne laquelle.

| | Gate | Critère |
|---|---|---|
| G0 | Légal & policy | Licence SPDX libre, pas de cryptomonnaie, dépôt vivant |
| G1 | Faisabilité | Aucun constat bloquant |
| G2 | Conformité statique | Manifest valide, linter propre, aucun `FIXME` résiduel |
| G3 | Installabilité | Install → HTTP 200 → backup/restore → remove sans résidu |
| G4 | Qualité *(optionnelle)* | `package_check` niveau ≥ 4, en VM isolée |
| G5 | Publication | Poussé, catalogué, réinstallable depuis le catalogue |

## Installation

```bash
git clone <ce dépôt> && cd yunopackage
cargo build --release
./target/release/ynopack --help
```

Pour la validation dynamique, un hôte Debian avec YunoHost accessible en SSH est nécessaire — le
Mac ne peut pas faire tourner YunoHost. Voir [docs/50-RUNBOOK-VALIDATION.md](docs/50-RUNBOOK-VALIDATION.md).

## Documentation

| | |
|---|---|
| [Cahier des charges](docs/00-CAHIER-DES-CHARGES.md) | Exigences numérotées, périmètre, critères d'acceptation |
| [Architecture](docs/10-ARCHITECTURE.md) | Pipeline, crates, flux de données |
| [AppSpec](docs/20-APPSPEC.md) | Le format de décision, champ par champ |
| [Règles de faisabilité](docs/30-REGLES-FAISABILITE.md) | Catalogue des règles |
| [Gates](docs/40-GATES.md) | Critères et codes de sortie |
| [Runbook de validation](docs/50-RUNBOOK-VALIDATION.md) | Machines, cycle G3, VM pour G4 |
| [Publication](docs/60-PUBLICATION.md) | Forgejo, catalogue custom, catalogue officiel |
| [AGENTS.md](AGENTS.md) | Règles de travail pour les agents IA |
| [BACKLOG.md](BACKLOG.md) | Tâches découpées, prêtes à distribuer |
| [docs/yunohost/](docs/yunohost/) | Documentation YunoHost de référence, versionnée |

## État

Les huit lots sont livrés : analyse, faisabilité, génération, vérification, installation réelle,
publication, interface web, et le harnais d'évaluation.

Ce que la validation sur des applications réelles établit aujourd'hui :

- le manifest produit pour **miniflux** est **identique** à celui du paquet officiel sur les onze
  champs comparés ;
- le linter officiel de YunoHost rend « *Not even a warning! This app qualifies for level 7!* » ;
- le cycle complet sur l'instance de test — installation, service actif, migrations de base,
  sauvegarde, restauration, désinstallation sans le moindre résidu — passe en une centaine de secondes ;
- l'accord avec les paquets écrits à la main est de **85 %** sur le corpus d'évaluation, et les
  écarts restants sont des arbitrages, pas des erreurs.

La gate G4 (`package_check`, niveau 0-8) reste optionnelle : elle demande une VM dédiée, et la CI
publique de YunoHost mesure le même niveau gratuitement sur une pull request.

## Licence

AGPL-3.0-or-later, comme YunoHost.
