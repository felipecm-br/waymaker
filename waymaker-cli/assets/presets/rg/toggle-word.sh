#!/bin/sh
# rg-toggle-word.sh — Toggles whole-word mode for the wm rg preset.
# Reads WM_STORE / MM_STORE (ic / cs / ww / cs+ww), flips word-regexp,
# then emits Store / SetStyledPrompt / Reload actions to wm Transform.
#
# Called by the rg preset (Transform), next to which it is installed.

MODE="${WM_STORE:-${MM_STORE:-ic}}"
case "$MODE" in
  ww)    NEW="ic";    PROMPT="{cyan: rg> }" ;;
  cs)    NEW="cs+ww"; PROMPT="{yellow,bold: rg [Aa][W]> }" ;;
  cs+ww) NEW="cs";    PROMPT="{yellow,bold: rg [Aa]> }" ;;
  *)     NEW="ww";    PROMPT="{cyan: rg [W]> }" ;;
esac

printf 'Store(%s)\n' "$NEW"
printf 'SetStyledPrompt(%s)\n' "$PROMPT"
printf 'Reload\n'
