## Accès aux hôtes de test

ynopack valide les paquets qu'il produit en les installant réellement sur un
serveur YunoHost. Il lui faut donc une identité SSH propre — jamais la vôtre.

Créez-la une fois, sous l'utilisateur de l'application :

```bash
sudo -u ynopack ssh-keygen -t ed25519 -N "" \
  -f /home/yunohost.app/ynopack/ssh/id_ed25519
sudo cat /home/yunohost.app/ynopack/ssh/id_ed25519.pub
```

Déposez la clé publique dans le `~/.ssh/authorized_keys` de l'hôte visé, puis
déclarez-y un alias dans `/home/yunohost.app/ynopack/ssh/config`. Tant que vous
ne l'avez pas fait, la clé n'autorise rien.

## Jetons

`GITHUB_TOKEN` relève le quota de l'API GitHub de 60 à 5000 requêtes par heure.
Sans lui l'outil fonctionne, mais s'arrête vite sur les analyses en série.

`FORGEJO_URL`, `FORGEJO_OWNER` et `FORGEJO_TOKEN` servent à publier sur une
forge Forgejo.

À placer dans `/home/yunohost.app/ynopack/env`, lisible par le seul utilisateur
de l'application.
