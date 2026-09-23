#!/usr/bin/env bash
# Rejoue localement ce que fait .github/workflows/ci.yml.
# A lancer avant chaque commit : c'est la porte de sortie definie dans AGENTS.md.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

fail=0
step() { # <nom> <commande...>
  local nom="$1"; shift
  printf '%-28s' "$nom"
  if out=$("$@" 2>&1); then
    echo "ok"
  else
    echo "ECHEC"
    echo "$out" | tail -25 | sed 's/^/    /'
    fail=1
  fi
}

step "Formatage"  cargo fmt --all --check
step "Clippy"     cargo clippy --workspace --all-targets -- -D warnings
step "Tests"      cargo test --workspace

# Garde-fou de l'ADR-002 : aucun client de modele de langage dans l'arbre de
# dependances du binaire livre.
printf '%-28s' "Aucune dependance LLM"
if cargo tree --workspace --prefix none 2>/dev/null \
   | grep -Eiq '^(async-openai|openai|anthropic|llm-chain|langchain|ollama-rs|genai)\b'; then
  echo "ECHEC — cf. docs/adr/ADR-002-zero-llm.md"; fail=1
else
  echo "ok"
fi

echo
[ $fail -eq 0 ] && echo "CI verte." || echo "CI rouge."
exit $fail
