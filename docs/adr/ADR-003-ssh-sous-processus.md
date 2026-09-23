# ADR-003 — SSH en sous-processus plutôt qu'une bibliothèque

**Statut** : accepté · 23/09/2026

## Contexte

La validation dynamique pilote un hôte Debian distant : copier un paquet, lancer `yunohost app
install`, lire les journaux. Deux voies : une bibliothèque SSH native (`russh`, `thrussh`), ou les
binaires `ssh` et `rsync` en sous-processus.

## Décision

Sous-processus, via `tokio::process`.

## Raisons

Les jobs durent de deux à trente minutes. Le multiplexage `ControlMaster` d'OpenSSH gère le maintien
de session, la reconnexion et les délais d'attente mieux que ce que nous écririons — et c'est du
code éprouvé depuis vingt ans.

Les hôtes sont déjà déclarés dans `~/.ssh/config`. L'outil prend un alias (`--host=dell`), jamais
des identifiants : aucun secret ne transite par la configuration de l'application, et l'utilisateur
garde la maîtrise de ses clés.

`rsync` fait du transfert incrémental correctement. Le réimplémenter n'apporterait rien.

Ordre de grandeur : environ 800 lignes de crate en moins, et autant de surface de bug.

## Conséquences

- `ssh` et `rsync` doivent être présents sur la machine qui pilote. Acquis sur macOS comme sur Linux.
- Les erreurs arrivent sous forme de code de sortie et de sortie standard, à analyser. Acceptable :
  on analyse déjà la sortie de `yunohost` et de `package_check`.
- Le débogage est direct : chaque commande émise est journalisée telle quelle et se rejoue à la main.
