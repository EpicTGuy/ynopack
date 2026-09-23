# Backlog

Tâches dimensionnées pour un agent : un crate, un détecteur ou une règle chacune. Lire
[AGENTS.md](AGENTS.md) avant de commencer.

**Convention de sortie** : `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
&& cargo test --workspace` passe.

---

## L0 — Socle · **livré**

- [x] Workspace Cargo, douze crates, profil de release
- [x] `scripts/refresh-docs.sh` — corpus YunoHost + schémas + `example_ynh` + templates YoloGen
- [x] `ynp-core` : `Known<T>`, `RepoFacts`, `AppSpec`, `Finding`, `GateReport` — **types figés**
- [x] Cahier des charges, README, AGENTS.md, ADR
- [x] **L0-1** · Intégration continue GitHub Actions : fmt, clippy, test
- [x] **L0-2** · `docs/adr/ADR-005` sur le format de sortie du CLI (humain vs `--json`)

---

## L1 — Analyse · **livré**

Dépend de L0. Les tâches L1-2 à L1-8 sont **parallélisables** : chaque détecteur est une fonction
pure sur un arbre de fichiers, testable seule.

- [x] **L1-1** · `ynp-forge` : client GitHub (métadonnées, releases, tags, tarball), traits pour
      accueillir GitLab/Gitea. Tests avec `wiremock`, jamais d'appel réseau en test. → EXG-F-01
- [x] **L1-2** · `ynp-dockerfile` : parseur Dockerfile → `BuildRecipe`. Gérer le multi-étage, les
      continuations `\`, `apt-get install` sur plusieurs lignes, les formes shell et exec de
      `CMD`/`ENTRYPOINT`. **La tâche la plus structurante du lot** : elle porte le pari zéro-LLM. → EXG-F-02
- [x] **L1-3** · `ynp-dockerfile` : parseur `docker-compose.yml` → `ComposeFacts`. Distinguer le
      service applicatif (`build:`) des services d'infrastructure (`image: postgres`). → EXG-F-03
- [x] **L1-4** · Détecteur `stack` : techno principale et version de runtime. → EXG-F-04
- [x] **L1-5** · Détecteur `config` : `.env.example` → variables classées par rôle, via
      `assets/knowledge/env-vars.toml`. → EXG-F-05
- [x] **L1-6** · Détecteur `assets` : choisir la source (release, tag, commit), calculer le
      `sha256`, déduire la stratégie d'autoupdate. → EXG-F-06
- [x] **L1-7** · Détecteur `services` : base de données, Redis, services non supportés
- [x] **L1-8** · Détecteur `health` : archivé, inactivité, absence de release
- [x] **L1-9** · Tables `assets/knowledge/` : `apk-to-deb.toml`, `env-vars.toml`,
      `runtime-versions.toml`, `unsupported-services.toml`
- [x] **L1-10** · `ynopack analyze <url>` → `facts.json`
- [x] **L1-11** · **Sélection des binaires préconstruits par architecture.** Constaté en comparant
      aux paquets officiels : `gotify_ynh`, `memos_ynh` et `miniflux_ynh` téléchargent tous des
      assets de release (`amd64.url`, `arm64.url`, …) plutôt que l'archive des sources, ce qui
      évite de compiler sur la machine cible. Notre sélection ne prend que le tarball source.
      Implique `autoupdate.asset.$arch` et le rapprochement asset ↔ architecture. **C'est ce qui
      désamorce `BUILD001` dans la majorité des cas.** → EXG-F-06

## L2 — Faisabilité · **livré**

- [x] **L2-1** · `ynp-rules` : moteur (trait `Rule`, registre, application), scoring
- [x] **L2-2** · Règles bloquantes : `LIC001`, `SRC001`, `RUN001`, `DB002`, `K8S001`. Un test
      positif et un négatif chacune. → EXG-F-10, EXG-F-12
- [x] **L2-3** · Règles majeures et mineures : `PY001`, `BUILD001`, `ARCH001`, `PORT001`, `MAINT001`
- [x] **L2-4** · Règles informatives : `DB001`, `SSO001`
- [x] **L2-5** · Gates G0 et G1
- [x] **L2-6** · `ynopack assess` → rapport lisible en terminal et `--json`

## L3 — Génération · **livré**

- [x] **L3-1** · `ynp-spec` : `RepoFacts` + `Feasibility` → `AppSpec`. **Le seul étage qui décide.**
      Tout ce qui ne se déduit pas devient `Known::unresolved`. → EXG-F-20, EXG-F-21
- [x] **L3-2** · `ynp-gen` : moteur Tera, écriture de l'arbre, permissions des scripts
- [x] **L3-3** · Template `manifest.toml.tera`, porté de `tests/fixtures/yologen/manifest.j2`
- [x] **L3-4** · Templates des scripts : install, remove, upgrade, backup, restore, change_url,
      `_common.sh`. **Helpers 2.1 exclusivement.** → EXG-F-22, EXG-F-23
- [x] **L3-5** · Templates `conf/` : nginx, systemd, fichier de conf de l'app
- [x] **L3-6** · Templates `doc/` et `tests.toml`
- [x] **L3-7** · `README.md.tera` aligné sur `apps_tools/readme_generator` → EXG-F-24
- [x] **L3-8** · Injection des marqueurs `FIXME(ynopack)` dans les fichiers concernés
- [x] **L3-9** · `ynopack plan` et `ynopack generate`, snapshots `insta`

## L4 — Vérification statique · **livré**

- [x] **L4-1** · Validation du manifest contre `assets/schemas/manifest.v2.schema.json` → EXG-F-30
- [x] **L4-2** · Portage des contrôles critiques de `package_linter` : helpers obsolètes,
      placeholders restants, `sudo`, `chown root`, cohérence des dépendances apt → EXG-F-31
- [x] **L4-3** · `bash -n` et `shellcheck` quand il est disponible
- [x] **L4-4** · Échec sur `FIXME(ynopack)` résiduel → EXG-F-32
- [x] **L4-5** · Gate G2, `ynopack verify`
- [x] **L4-6** · Test différentiel : faire tourner le vrai `package_linter` et comparer les verdicts

## L5 — Validation dynamique · **livré**

- [x] **L5-1** · `ynp-runner` : exécution SSH/rsync via `tokio::process`, `ControlMaster`, journaux
- [x] **L5-2** · Cycle G3 sur `dell` : install, `curl` sur l'endpoint, backup/restore, remove → EXG-F-33
- [x] **L5-3** · Contrôle des résidus après remove : utilisateur système, `$install_dir`, conf
      nginx, base → EXG-F-34
- [x] **L5-4** · `ynopack test --host=<alias>`, gate G3
- [x] **L5-5** *(optionnel)* · `scripts/provision-runner.sh` : VM Debian + Incus sur `hom-e`.
      **Écrit mais jamais exécuté** — à relire avant de s'y fier.
- [ ] **L5-6** *(optionnel)* · Pilotage de `package_check`, analyse du niveau 0-8, gate G4 → EXG-F-35

## L6 — Publication · **livré**

- [x] **L6-1** · `ynp-publish` : création du dépôt Forgejo et push → EXG-F-40
- [x] **L6-2** · Catalogue custom : `apps.toml` → `apps.json`, inspiré de `apps_tools/list_builder.py` → EXG-F-41
- [x] **L6-3** · Gate G5, `ynopack publish`
- [x] **L6-4** *(optionnel)* · Préparation de PR vers le catalogue officiel, sous condition de G4 → EXG-F-42
- [x] **L6-5** · `ynopack run` : pipeline complet, arrêt à la première gate en échec → EXG-F-50, EXG-F-51

## L7 — Interface web · **livré**

- [x] **L7-1** · `ynopack-server` : `POST /jobs`, file d'attente, état persistant
- [x] **L7-2** · Progression en SSE
- [x] **L7-3** · Page « coller un lien » → rapport → paquet → lien du dépôt → EXG-F-52

## Transverse

- [x] **T-1** · `ynopack eval --corpus` : comparaison aux paquets YunoHost existants, matrice de
      précision par champ. **À démarrer dès L1** — sans cette mesure, on ne sait pas si les
      détecteurs progressent. → EXG-F-53
- [x] **T-2** · `tests/corpus.toml` : échelle de canaris, une difficulté nouvelle à chaque barreau.
      Tous déjà au catalogue officiel sauf le dernier, donc avec une vérité terrain à comparer :
      `gotify` (binaire préconstruit, `architectures = "all"`, pas de base) →
      `memos` (binaires par architecture) →
      `miniflux` (PostgreSQL + `sso = true`) →
      `whoogle` (Python, cf. `PY001`) →
      `buzz` (cas dur : Rust + frontal pnpm, absent du catalogue)
- [x] **T-3** · Corpus de Dockerfiles réels avec la `BuildRecipe` attendue, pour L1-2
