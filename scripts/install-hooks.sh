#!/usr/bin/env bash
# Installe un hook pre-commit qui rejoue la CI.
# La porte de sortie d'AGENTS.md devient mecanique au lieu d'etre une consigne
# qu'on peut oublier — ce qui est deja arrive une fois.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cat > "$ROOT/.git/hooks/pre-commit" <<'HOOK'
#!/usr/bin/env bash
exec "$(git rev-parse --show-toplevel)/scripts/ci.sh"
HOOK
chmod +x "$ROOT/.git/hooks/pre-commit"
echo "Hook pre-commit installe : la CI tourne avant chaque commit."
