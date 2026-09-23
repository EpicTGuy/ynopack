# Tables de connaissance

De la donnée, pas de l'intelligence. C'est ce qui remplace le « bon sens » qu'on attendrait d'un
modèle : versionné, relisible, testable, et corrigeable par n'importe qui.

| Fichier | Contenu | Tâche |
|---|---|---|
| `apk-to-deb.toml` | Correspondance paquets Alpine → Debian, pour les Dockerfiles à base Alpine | L1-9 |
| `env-vars.toml` | Noms canoniques de variables (`PORT`, `DATABASE_URL`, `APP_URL`…) → rôle | L1-9 |
| `runtime-versions.toml` | Ce que bookworm fournit nativement (PHP 8.2, Python 3.11…), pour `PY001` | L1-9 |
| `unsupported-services.toml` | Services sans équivalent YunoHost, pour `DB002` | L1-9 |
| `npm-native-deps.toml` | Paquets npm exigeant des `-dev` apt (`sharp` → `libvips-dev`…) | L1-9 |

Toute entrée ajoutée doit être accompagnée du cas réel qui l'a motivée, en commentaire.
