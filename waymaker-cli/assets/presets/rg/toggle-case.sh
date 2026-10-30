#!/bin/sh
# rg-toggle-case.sh — Toggles match-case mode for the wm rg preset.
# Reads WM_STORE / MM_STORE (ic / cs / ww / cs+ww), flips case-sensitivity,
# then emits Store / SetStyledPrompt / Reload actions to wm Transform.
#
# Called by the rg preset (Transform), next to which it is installed.

MODE="${WM_STORE:-${MM_STORE:-ic}}"
case "$MODE" in
  cs)    NEW="ic";    PROMPT="{cyan: rg> }" ;;
  ww)    NEW="cs+ww"; PROMPT="{yellow,bold: rg [Aa][W]> }" ;;
  cs+ww) NEW="ww";    PROMPT="{cyan: rg [W]> }" ;;
  *)     NEW="cs";    PROMPT="{yellow,bold: rg [Aa]> }" ;;
esac

printf 'Store(%s)\n' "$NEW"
printf 'SetStyledPrompt(%s)\n' "$PROMPT"
printf 'Reload\n'
