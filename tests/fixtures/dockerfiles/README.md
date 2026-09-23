# Dockerfiles réels

Prélevés sur des applications auto-hébergées, et conservés tels quels. Ce sont
eux qui ont révélé la plupart des défauts du parseur : héritage d'étape,
options BuildKit, listes de paquets rangées dans un `ARG`, Dockerfiles de
conditionnement pris pour ceux du produit.

Les cas d'école ne montrent aucun de ces problèmes.

| Fichier | Ce qu'il met à l'épreuve |
|---|---|
| `grist.Dockerfile` | Six étapes, blocs shell découpés, `yarn` en plusieurs passes |
| `linkwarden.Dockerfile` | Options BuildKit `--mount`, monorepo, `corepack` |
| `miniflux.Dockerfile` | Base Alpine, paquets `apk` à traduire |
| `paperless.Dockerfile` | Listes de paquets dans des `ARG`, alias d'étape pris pour une image |
| `vikunja.Dockerfile` | Runtime `scratch`, binaire Go statique |

Pour en ajouter un : le déposer ici, puis écrire le test correspondant dans
`crates/ynp-dockerfile/tests/`, en citant l'application d'origine.
