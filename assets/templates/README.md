# Templates Tera

Les scripts d'un paquet YunoHost sont **entièrement** produits ici. On ne corrige jamais un fichier
généré : on corrige le template ou l'`AppSpec`.

Portés de [`appgenerator`](https://github.com/YunoHost/appgenerator) (YoloGen), dont les `.j2`
d'origine sont conservés dans `tests/fixtures/yologen/` pour pouvoir diffuser après chaque évolution
amont. Voir [ADR-004](../../docs/adr/ADR-004-tera.md).

| Template | Tâche |
|---|---|
| `manifest.toml.tera` | L3-3 |
| `scripts/{install,remove,upgrade,backup,restore,change_url,_common.sh}.tera` | L3-4 |
| `conf/{nginx,systemd,app-config}.tera` | L3-5 |
| `doc/*.tera`, `tests.toml.tera` | L3-6 |
| `README.md.tera` — aligné sur `apps_tools/readme_generator` | L3-7 |

**Helpers 2.1 exclusivement** : `ynh_config_add_nginx`, pas `ynh_add_nginx_config`. La liste est
dans `docs/yunohost/21-helpers-2.1.md`.
