#!/usr/bin/env bash
# Extracts translatable strings from the plasmoid into template.pot and merges
# them into every translate/<lang>.po. Safe to re-run: files only change when
# the set of strings changes.
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(dirname "$DIR")"
PLUGIN_ID="$(sed -n 's/^[[:space:]]*"Id":[[:space:]]*"\([^"]*\)".*/\1/p' "$ROOT/metadata.json" | head -n 1)"
if [[ -z "$PLUGIN_ID" ]]; then
   echo "merge.sh: could not read KPlugin.Id from $ROOT/metadata.json" >&2
   exit 1
fi

POT="$DIR/template.pot"
NEW_POT="$(mktemp)"
trap 'rm -f "$NEW_POT" "$NEW_POT.raw"' EXIT

# QML is parsed with the C++ lexer, as KDE's own Messages.sh scripts do.
(
   cd "$ROOT"
   find contents -name '*.qml' -o -name '*.js' | LC_ALL=C sort | xargs xgettext \
      --from-code=UTF-8 -C --kde -ci18n \
      -ki18n:1 -ki18nc:1c,2 -ki18np:1,2 -ki18ncp:1c,2,3 \
      -kxi18n:1 -kxi18nc:1c,2 -kxi18np:1,2 -kxi18ncp:1c,2,3 \
      --add-location=file \
      --package-name="plasma_applet_$PLUGIN_ID" \
      --copyright-holder="Can Boluk" \
      --msgid-bugs-address="https://github.com/can1357/kAirPods/issues" \
      -o "$NEW_POT.raw"
)
sed -e 's/^# SOME DESCRIPTIVE TITLE\.$/# Translation template for the kAirPods Plasma widget./' \
   -e 's/charset=CHARSET/charset=UTF-8/' \
   "$NEW_POT.raw" >"$NEW_POT"

# Keep the old file (and its POT-Creation-Date) when no message changed.
if [[ -f "$POT" ]] && diff -q <(grep -v '^"POT-Creation-Date:' "$POT") \
   <(grep -v '^"POT-Creation-Date:' "$NEW_POT") >/dev/null; then
   echo "template.pot is up to date"
else
   cp "$NEW_POT" "$POT"
   echo "template.pot updated"
fi

shopt -s nullglob
for po in "$DIR"/*.po; do
   msgmerge --quiet --update --backup=none --add-location=file "$po" "$POT"
   echo "merged $(basename "$po")"
done
