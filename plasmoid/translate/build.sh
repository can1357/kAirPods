#!/usr/bin/env bash
# Compiles translate/<lang>.po into the catalogs Plasma loads for this widget:
# contents/locale/<lang>/LC_MESSAGES/plasma_applet_<KPlugin.Id>.mo
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(dirname "$DIR")"
PLUGIN_ID="$(sed -n 's/^[[:space:]]*"Id":[[:space:]]*"\([^"]*\)".*/\1/p' "$ROOT/metadata.json" | head -n 1)"
if [[ -z "$PLUGIN_ID" ]]; then
   echo "build.sh: could not read KPlugin.Id from $ROOT/metadata.json" >&2
   exit 1
fi

# contents/locale holds only generated catalogs; drop stale languages.
rm -rf "$ROOT/contents/locale"

shopt -s nullglob
for po in "$DIR"/*.po; do
   lang="$(basename "$po" .po)"
   out="$ROOT/contents/locale/$lang/LC_MESSAGES/plasma_applet_$PLUGIN_ID.mo"
   mkdir -p "$(dirname "$out")"
   msgfmt --check -o "$out" "$po"
   echo "built $lang"
done
