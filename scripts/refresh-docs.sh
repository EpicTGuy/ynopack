#!/usr/bin/env bash
# Resynchronise le corpus de reference YunoHost depuis les depots amont.
# Ce corpus est versionne volontairement : les agents doivent travailler sur la
# doc reelle, pas de memoire. Relancer apres chaque release majeure de YunoHost.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DOC_BASE="https://raw.githubusercontent.com/YunoHost/doc/master/pages/06.contribute/10.packaging_apps"
SCHEMA_BASE="https://raw.githubusercontent.com/YunoHost/apps/main/schemas"

fetch() { # <url> <dest>
  printf '  %-34s' "$(basename "$2")"
  if curl -sfL "$1" -o "$2"; then echo "ok ($(wc -c <"$2" | tr -d ' ') o)"; else echo "ECHEC"; return 1; fi
}

echo "== Documentation packaging =="
fetch "$DOC_BASE/packaging_apps.md"                                              "$ROOT/docs/yunohost/00-packaging-apps.md"
fetch "$DOC_BASE/10.manifest/docs.md"                                            "$ROOT/docs/yunohost/10-manifest.md"
fetch "$DOC_BASE/10.manifest/10.appresources/packaging_app_manifest_resources.md" "$ROOT/docs/yunohost/11-resources.md"
fetch "$DOC_BASE/20.scripts/scripts.md"                                          "$ROOT/docs/yunohost/20-scripts.md"
fetch "$DOC_BASE/20.scripts/12.helpers21/packaging_app_scripts_helpers_v21.md"   "$ROOT/docs/yunohost/21-helpers-2.1.md"
fetch "$DOC_BASE/40.testing/testing.md"                                          "$ROOT/docs/yunohost/40-testing.md"
fetch "$DOC_BASE/50.publishing/publishing.md"                                    "$ROOT/docs/yunohost/50-publishing.md"
fetch "$DOC_BASE/60.advanced/20.config_panels/config_panels.md"                  "$ROOT/docs/yunohost/60-config-panels.md"
fetch "$DOC_BASE/90.policy/policy.md"                                            "$ROOT/docs/yunohost/90-policy.md"

echo "== Schemas officiels (utilises par ynp-verify) =="
fetch "$SCHEMA_BASE/manifest.v2.schema.json" "$ROOT/assets/schemas/manifest.v2.schema.json"
fetch "$SCHEMA_BASE/tests.v1.schema.json"    "$ROOT/assets/schemas/tests.v1.schema.json"

echo "== Paquet canonique de reference (example_ynh) =="
EX="https://raw.githubusercontent.com/YunoHost/example_ynh/main"
mkdir -p "$ROOT/tests/fixtures/example_ynh/"{scripts,conf}
for f in manifest.toml tests.toml conf/nginx.conf conf/systemd.service \
         scripts/_common.sh scripts/install scripts/remove scripts/upgrade \
         scripts/backup scripts/restore scripts/change_url; do
  fetch "$EX/$f" "$ROOT/tests/fixtures/example_ynh/$f"
done

echo "== Templates YoloGen (a porter en Tera, cf. ADR-004) =="
YG="https://raw.githubusercontent.com/YunoHost/appgenerator/main/templates"
mkdir -p "$ROOT/tests/fixtures/yologen"
for f in manifest install remove upgrade backup restore change_url _common.sh nginx systemd tests \
         DESCRIPTION ADMIN PRE_INSTALL POST_INSTALL PRE_UPGRADE POST_UPGRADE; do
  fetch "$YG/$f.j2" "$ROOT/tests/fixtures/yologen/$f.j2" || true
done

echo
echo "Corpus a jour."
