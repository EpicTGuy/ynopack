# ADR-005 — Deux sorties, pas un résumé de l'autre

**Statut** : accepté · 23/09/2026

## Contexte

Chaque étage du pipeline doit servir deux lecteurs : un humain qui décide, et
un programme qui enchaîne — script, agent, ou l'interface web. La tentation est
de n'avoir qu'un format et de le retailler.

## Décision

Chaque commande produit **deux sorties de même contenu** : un artefact JSON ou
TOML écrit sur disque, et un rapport lisible en terminal. `--json` remplace le
second par le premier sur la sortie standard.

Ni l'un ni l'autre n'est un résumé. Le rapport humain regroupe, hiérarchise et
enroule le texte ; le JSON porte les mêmes faits, sans mise en forme.

## Raisons

Un rapport qu'on ne peut pas actionner fait perdre du temps à qui le lit. Un
JSON qu'il faut compléter par une seconde exécution en fait perdre au programme
qui l'enchaîne. Les deux publics méritent la même information.

Les artefacts sur disque — `facts.json`, `report.json`, `appspec.toml`,
`lint.json`, `test.json` — permettent de reprendre le pipeline en cours de
route. Après un `analyze`, un `assess` ne retélécharge rien ; c'est ce qui rend
supportable le quota de soixante requêtes par heure de l'API GitHub.

## Le code de sortie fait partie du contrat

`0` quand tout passe, `10 + rang de la porte` quand une gate échoue, `20` quand
la spécification reste à compléter, `1` pour une panne de l'outil. Un script
appelant sait *où* ça a cassé sans analyser la sortie :

```bash
yunopack run "$url" --host=dell
case $? in
  0)  echo "publié" ;;
  11) echo "non packageable" ;;
  13) echo "ne s'installe pas" ;;
  20) echo "il reste des champs à renseigner" ;;
esac
```

## Conséquences

- Toute commande qui ajoute un constat doit l'ajouter aux deux sorties. Un test
  vérifie que le rapport humain n'omet aucune sévérité.
- Le rapport humain enroule le texte sans couper les mots : un rapport illisible
  en terminal n'est pas lu, et le constat est alors perdu quoi qu'il contienne.
