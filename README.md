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

## Comment il procède

Une application auto-hébergeable moderne décrit déjà sa construction quelque
part : un `Dockerfile` indique son image de base, donc sa version de runtime ;
ses lignes `apt-get install` donnent ses dépendances ; son `EXPOSE` donne son
port ; son `CMD` donne sa commande de démarrage. Un `docker-compose.yml` ajoute
la base de données, les volumes et la configuration.

ynopack lit ces fichiers et en tire un `appspec.toml` — le document où toutes
les décisions de packaging sont réunies, et le seul qu'on relise. Le paquet est
ensuite produit mécaniquement à partir de là.

Quand un champ ne se déduit pas, l'outil l'écrit au lieu de choisir à votre
place :

```toml
# appspec.toml — extrait
execstart = { unknown = "aucun CMD dans le Dockerfile", look_in = ["Procfile", "README.md"] }
```

`verify` refuse tout paquet où subsiste un champ de ce genre. Vous obtenez donc
soit un paquet complet, soit la liste précise de ce qu'il reste à renseigner —
jamais un paquet qui a l'air fini alors qu'il ne l'est pas.

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

Deux commandes servent à trouver quoi packager plutôt qu'à le faire :
`ynopack wishlist` liste ce que la communauté YunoHost attend, et
`ynopack alternatives <nom>` propose les logiciels auto-hébergeables voisins
qui ne sont pas encore au catalogue.

Il existe aussi `ynopack-server`, qui expose le même pipeline dans un
navigateur : on colle une URL, on suit la progression en direct.

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
  écarts restants sont des arbitrages, pas des erreurs ;
- ynopack se package lui-même, et le paquet obtenu passe le même cycle sur
  l'instance de test.

La gate G4 (`package_check`, niveau 0-8) reste optionnelle : elle demande une VM dédiée, et la CI
publique de YunoHost mesure le même niveau gratuitement sur une pull request.

## Licence

AGPL-3.0-or-later, comme YunoHost.
