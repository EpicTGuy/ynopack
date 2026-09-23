# Runbook de validation

## Les machines

Relevé effectué le 23 septembre 2026. Les alias sont ceux de `~/.ssh/config` : le code ne manipule
jamais d'identifiants en dur, seulement un alias.

| Alias | Adresse | OS | CPU / RAM | Disque libre | Rôle |
|---|---|---|---|---|---|
| `dell` | 192.168.8.132 | Debian 12 · **YunoHost 12.1.26** | 2 c / 3,7 Go | 300 Go | **Cible des gates G3** |
| `hom-e` | 192.168.8.24 | Debian 12 · YunoHost + Forgejo + Docker | 16 c / 31 Go | 220 Go | Forge, et hôte de la VM pour G4 |
| `vps` | Infomaniak | Debian 12 | 1 c / 1,9 Go | 17 Go | Trop petit pour servir de runner |

Le Mac ne peut rien valider : YunoHost ne tourne que sur Debian.

## Pourquoi deux étages

`package_check` exige Incus ou LXD. Or son bridge réseau veut dnsmasq sur le port 53, **déjà occupé
par YunoHost** sur `dell` comme sur `hom-e`. Docker sur `hom-e` entre en conflit supplémentaire avec
le réseau Incus.

Installer Incus sur `dell` casserait l'environnement de test YunoHost. On sépare donc :

- **G3 sur `dell`, sans Incus** — l'instance est déjà exactement la cible (bookworm, amd64,
  YunoHost 12.1.26, format v2, helpers 2.1). Rien à installer.
- **G4 dans une VM isolée sur `hom-e`** — seule façon propre d'avoir Incus sans toucher à l'hôte.

## Étage 1 — G3 sur `dell`

### Prérequis

Aucun. La machine est prête. À titre indicatif, l'état relevé :

```
Debian 12.12 · amd64 · YunoHost 12.1.26 (stable)
git ✓   python3 ✓   incus ✗   docker ✗   shellcheck ✗   cargo ✗
```

`shellcheck` et `cargo` sont absents : c'est voulu. Le build se fait sur le Mac, on ne transfère
que le paquet à tester.

`python3` est présent, ce qui permet d'y faire tourner le vrai `package_linter` pour le test
différentiel de la tâche L4-6.

### Cycle exécuté par `ynopack test --host=dell`

```bash
rsync -a ./<app>_ynh/ dell:/tmp/ynopack/<app>_ynh/

ssh dell yunohost app install /tmp/ynopack/<app>_ynh \
      --debug --force \
      -a "domain=test.local&path=/<app>&init_main_permission=visitors"

ssh dell curl -sI https://test.local/<app>/          # 200 ou 30x attendu
ssh dell yunohost backup create --apps <app>
ssh dell yunohost backup restore <archive> --apps <app> --force
ssh dell yunohost app remove <app>
```

### Contrôle des résidus

Exécuté après `remove`. C'est le contrôle qui attrape les scripts `remove` incomplets — le défaut le
plus fréquent des paquets faits main :

| Résidu | Commande |
|---|---|
| Utilisateur système | `getent passwd <app>` |
| Répertoire d'installation | `test -d /var/www/<app>` |
| Configuration nginx | `ls /etc/nginx/conf.d/*.d/<app>.conf` |
| Unité systemd | `systemctl list-unit-files \| grep <app>` |
| Base de données | `mysql -e "SHOW DATABASES"` / `psql -l` |
| Réglages | `test -d /etc/yunohost/apps/<app>` |

Tout résidu fait échouer la gate G3.

### Réversibilité

L'installation est réversible par `yunohost app remove`. En cas d'échec au milieu du cycle :

```bash
ssh dell yunohost app remove <app> --purge
```

`ynopack test --snapshot` prend en plus un `yunohost backup create --system` avant la campagne. Sur
une machine déclarée environnement de test, ce n'est pas indispensable ; sur toute autre, c'est obligatoire.

### Dépannage

| Symptôme | Cause habituelle |
|---|---|
| `app already installed` | Cycle précédent interrompu. `yunohost app remove <app> --purge` |
| Install en échec, aucun journal | `--debug` absent. Les journaux complets : `yunohost log list` |
| 502 sur l'endpoint | Le service ne démarre pas. `journalctl -u <app> -n 50` |
| 404 sur l'endpoint | Conf nginx absente ou `main.url` mal renseigné dans le manifest |
| Restauration en échec | Le script `backup` ne déclare pas tout ce que `restore` attend |

## Étage 2 — G4 sur `hom-e` *(optionnel)*

Requis uniquement pour une contribution au catalogue officiel, qui publie un niveau de qualité 0-8.

### Principe

Une VM Debian sous QEMU/libvirt sur `hom-e`, avec Incus **à l'intérieur**. L'isolation résout d'un
coup les deux conflits : le dnsmasq de la VM n'entre pas en concurrence avec celui de YunoHost, et
le réseau Docker de l'hôte reste à l'écart.

On n'installe **rien** sur `hom-e` lui-même : c'est une machine de production qui héberge Forgejo.

### Provisionnement

`scripts/provision-runner.sh` (tâche L5-5) est idempotent et enchaîne :

1. VM Debian bookworm via cloud-init, 4 vCPU, 8 Go, 60 Go ;
2. `apt install lynx jq btrfs-progs` ;
3. installation et initialisation d'Incus (`incus admin init --minimal`) ;
4. ajout du dépôt d'images YunoHost ;
5. clone de `package_check`.

### Alternative sans infrastructure

Pour le chemin vers le catalogue officiel, la CI publique du projet fait le même travail
gratuitement : pousser le dépôt sur GitHub, ouvrir la pull request, commenter `!testme`. Le niveau
est calculé par `ci-apps-dev.yunohost.org` et affiché sur la PR.

C'est souvent le meilleur rapport effort/résultat : G3 en local pour itérer vite, CI officielle pour
la mesure qui fait foi.
