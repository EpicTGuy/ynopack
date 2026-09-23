# Publication

Deux circuits, à employer dans cet ordre : la forge personnelle pour itérer vite, le catalogue
officiel pour diffuser quand le paquet est mûr.

## Circuit 1 — Forgejo + catalogue custom

Autonomie complète : aucune revue externe, l'application est installable dès le push.

### Dépôt

`ynp-publish` crée `<app>_ynh` via l'API Forgejo, puis pousse en SSH avec l'alias `forgejo` déjà
configuré — pas de dialogue de trousseau à chaque envoi.

Convention de nommage : `<app>_ynh`, comme dans l'écosystème YunoHost. Elle permet de reprendre tel
quel l'outillage amont (`package_check`, `autoupdate_app_sources`).

### Catalogue

YunoHost lit ses catalogues depuis `/etc/yunohost/apps_catalog.yml` :

```yaml
- id: default
  url: https://app.yunohost.org/default
- id: etg
  url: https://forge.local/catalog
```

Le catalogue est un `apps.json` servi en HTTP, produit depuis un `apps.toml` maison. La logique de
construction s'inspire de [`apps_tools/list_builder.py`](https://github.com/YunoHost/apps_tools/blob/main/list_builder.py),
qui est la référence amont.

Après ajout :

```bash
yunohost tools update apps
yunohost app install <app>
```

La gate G5 rejoue cette installation depuis le catalogue — pas seulement depuis un répertoire local.
C'est ce qui vérifie que le circuit complet fonctionne.

### README du paquet

**Il ne s'écrit pas à la main.** Le README d'une application YunoHost est généré depuis le manifest
et `doc/DESCRIPTION.md` par [`apps_tools/readme_generator`](https://github.com/YunoHost/apps_tools/tree/main/readme_generator).
Le template `assets/templates/README.md.tera` reproduit ce format ; s'en écarter fait remonter un
avertissement du linter sur les badges et la structure attendue.

## Circuit 2 — Catalogue officiel *(optionnel)*

Diffusion à l'ensemble des instances YunoHost, au prix d'une revue communautaire.

### Conditions

1. Gate G4 passée avec un niveau ≥ 4 — les applications de niveau ≤ 4 sont signalées comme de
   mauvaise qualité et l'installation en est découragée.
2. Conformité à la politique du catalogue (`docs/yunohost/90-policy.md`) : logiciel libre ou éthique
   au cas par cas, pas de cryptomonnaie, pas de cas d'usage ultra-niche.
3. Dépôt hébergé sur GitHub. Le niveau 6 exige même l'organisation `YunoHost-Apps`, pour que la
   communauté puisse reprendre la maintenance si l'auteur initial disparaît.

### Procédure

1. Pousser le dépôt sur GitHub.
2. Ouvrir une pull request ajoutant l'entrée dans
   [`YunoHost/apps/apps.toml`](https://github.com/YunoHost/apps/blob/master/apps.toml).
3. Commenter `!testme` sur la PR pour déclencher la CI officielle — c'est aussi le moyen d'obtenir
   un niveau de qualité **sans monter la VM Incus** du circuit local.
4. Attendre la revue.

Le champ `level` ne se renseigne jamais à la main : un bot ouvre une PR chaque vendredi soir avec
les résultats de la CI officielle.

Le catalogue réellement consommé par les serveurs est `https://app.yunohost.org/default/v3/apps.json`,
reconstruit toutes les quatre heures.

### Ce que fait `ynopack`

`ynopack publish --official` prépare la contribution : fourche, branche, entrée `apps.toml`, corps
de la pull request. **Il n'ouvre rien sans confirmation** — une PR vers un projet tiers engage
l'utilisateur, pas l'outil.

La commande refuse de s'exécuter si G4 n'a pas été passée : proposer au catalogue officiel un paquet
dont on ignore le niveau fait perdre du temps aux relecteurs.
