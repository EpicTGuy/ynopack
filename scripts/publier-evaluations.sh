#!/usr/bin/env bash
# Publie les evaluations d'une instance pour que les autres n'aient pas a les
# refaire.
#
# Une evaluation est le meme calcul, sur le meme depot, avec les memes regles :
# la rejouer sur chaque instance depense du quota de forge et du courant pour
# arriver au meme resultat. Elle est verifiable — n'importe qui peut la
# rejouer — donc la reprendre ne demande de faire confiance a personne.
#
#   scripts/publier-evaluations.sh https://exemple.fr/yunopack
set -euo pipefail

INSTANCE="${1:-${YUNOPACK_INSTANCE:-}}"
[ -n "$INSTANCE" ] || { echo "usage : $0 <adresse de l'instance yunopack>" >&2; exit 2; }

RACINE="$(cd "$(dirname "$0")/.." && pwd)"
CIBLE="$RACINE/assets/evaluations.json"
TEMPO="$(mktemp)"
trap 'rm -f "$TEMPO"' EXIT

echo "Recuperation depuis ${INSTANCE%/}/evaluations/export …"
curl -sSf --max-time 120 "${INSTANCE%/}/evaluations/export" -o "$TEMPO"

# Un fichier tronque ou vide remplacerait un lot utile par rien.
python3 - "$TEMPO" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
if not isinstance(d, list) or not d:
    sys.exit("le lot recupere est vide ou mal forme")
print(f"{len(d)} evaluation(s)")
PY

python3 -c "
import json,sys
d = json.load(open('$TEMPO'))
d.sort(key=lambda f: f.get('depot',''))
json.dump(d, open('$CIBLE','w'), ensure_ascii=False, indent=1)
open('$CIBLE','a').write('\n')
"
echo "Ecrit dans ${CIBLE#"$RACINE/"} — a committer pour le rendre disponible."
