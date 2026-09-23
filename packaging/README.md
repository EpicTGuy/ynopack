# Paquet YunoHost de ynopack

Produit par ynopack lui-même, à partir de sa propre release GitHub.

```bash
ynopack plan https://github.com/EpicTGuy/ynopack --out .ynopack
# renseigner runtime.execstart et resources.data_dir dans .ynopack/appspec.toml
ynopack generate --out .ynopack
ynopack verify  --out .ynopack
ynopack test    --host <hôte> --out .ynopack
```

`.appspec.toml` conserve les décisions prises, pour que la génération soit
rejouable à l'identique. Deux champs y ont été renseignés à la main :

- `runtime.execstart` : le dépôt n'a pas de Dockerfile d'où tirer la commande
  de démarrage ;
- `resources.data_dir` : l'application stocke sa clé SSH et les paquets
  produits, rien dans le dépôt ne permettait de le deviner.

## Installer

```bash
yunohost app install https://github.com/EpicTGuy/ynopack_ynh
```

Ce dépôt-là n'existe pas encore : le paquet doit d'abord être publié, puis
proposé au catalogue officiel.
