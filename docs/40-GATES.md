# Les six gates

« Une fois toutes les étapes de validation validées, il le package » : chaque étape est une porte
qui rend `Pass`, `Fail` ou `Skipped`. Le pipeline s'arrête à la première qui échoue, sauf `--force`.

## Tableau

| Gate | Nom | Critère | Où ça tourne | Coût |
|---|---|---|---|---|
| G0 | Légal & policy | Licence SPDX libre, pas de cryptomonnaie, dépôt non archivé | Local | Quelques secondes |
| G1 | Faisabilité | Aucun constat bloquant, score ≥ seuil | Local | Quelques secondes |
| G2 | Conformité statique | Manifest valide, linter propre, `bash -n`, aucun `FIXME` résiduel | Local | Quelques secondes |
| G3 | Installabilité | Install → HTTP 200 → backup/restore → remove sans résidu | `<machine-de-test>` en SSH | 2 à 10 min |
| G4 | Qualité *(optionnelle)* | `package_check` niveau ≥ 4 | VM sur `<machine-hote>` | 10 à 30 min |
| G5 | Publication | Poussé, catalogué, réinstallable depuis le catalogue | Forgejo + `<machine-de-test>` | 1 à 2 min |

L'ordre suit le coût croissant : on refuse pour une question de licence avant de dépenser trente
minutes de CPU.

## Détail

### G0 — Légal & policy
Se vérifie sans télécharger le dépôt, à partir des seules métadonnées de la forge. Porte les règles
`LIC001` et `MAINT001`. Échouer ici coûte une requête HTTP.

### G1 — Faisabilité
Applique le catalogue complet de [docs/30-REGLES-FAISABILITE.md](30-REGLES-FAISABILITE.md). Un seul
bloquant suffit. C'est la gate qui porte la promesse du projet : **refuser explicitement plutôt que
produire un paquet deviné**.

### G2 — Conformité statique
Quatre contrôles :
1. `manifest.toml` valide contre `assets/schemas/manifest.v2.schema.json` ;
2. contrôles portés de `package_linter` — helpers obsolètes, placeholders, `sudo`, `chown root` ;
3. `bash -n` sur chaque script, et `shellcheck` si disponible ;
4. **aucun `FIXME(yunopack)` résiduel.**

Le quatrième point est le garde-fou anti-bluff : un champ non déterminé fait échouer la gate au lieu
de passer inaperçu.

### G3 — Installabilité
Le cœur de la validation, sur l'instance YunoHost de test. Cycle complet :

```
install → curl sur l'endpoint → upgrade → backup → restore → remove → contrôle des résidus
```

Le contrôle des résidus vérifie qu'après désinstallation il ne reste ni utilisateur système, ni
`$install_dir`, ni configuration nginx, ni base de données. Un script `remove` incomplet fait
échouer la gate : c'est précisément ce qu'on veut détecter avant publication.

### G4 — Qualité *(optionnelle)*
`package_check` officiel en VM isolée, qui calcule le niveau 0-8. Requis uniquement pour une
contribution au catalogue officiel. Pour le circuit Forgejo interne, G3 suffit.

Sautée par défaut faute de VM : `Skipped` n'est pas un échec et ne bloque pas le pipeline.

### G5 — Publication
Création du dépôt sur la forge, push, génération de l'entrée de catalogue, puis **réinstallation
depuis le catalogue** — ce dernier point vérifie que le circuit complet fonctionne, pas seulement
l'installation depuis un répertoire local.

## Codes de sortie

| Code | Signification |
|---|---|
| 0 | Toutes les gates exécutées ont passé |
| 10 | G0 en échec |
| 11 | G1 en échec |
| 12 | G2 en échec |
| 13 | G3 en échec |
| 14 | G4 en échec |
| 15 | G5 en échec |
| 1 | Panne de l'outil (réseau, disque, arguments) |

Un script appelant sait ainsi *où* ça a cassé sans analyser la sortie :

```bash
yunopack run "$url" --host=<machine-de-test>
case $? in
  0)  echo "publié" ;;
  11) echo "non packageable — voir report.json" ;;
  13) echo "ne s'installe pas — voir les journaux" ;;
esac
```

## `--force`

`--force` poursuit malgré une gate en échec. Réservé au développement : il permet d'atteindre G3
pour comprendre *pourquoi* un paquet imparfait ne s'installe pas. Il ne doit jamais servir à
publier — G5 refuse de s'exécuter si une gate antérieure a échoué, même avec `--force`.
