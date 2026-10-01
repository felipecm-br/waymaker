#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "⚡ Starting Waymaker isolated Docker installation test..."

docker run --rm \
  -v "${SCRIPT_DIR}/install.sh:/tmp/install.sh:ro" \
  ubuntu:24.04 bash -c '
    set -euo pipefail
    apt-get update -qq && apt-get install -y -qq curl zsh sudo >/dev/null

    useradd -m -s /bin/zsh testuser

    su - testuser << "EOF"
      set -euo pipefail
      sh /tmp/install.sh -s

      export PATH="$HOME/.local/bin:$PATH"
      echo -n "Installed binary version: "
      wm --version

      test -f "$HOME/.config/waymaker/config.toml"
      test -f "$HOME/.config/waymaker/session.toml"
      test -f "$HOME/.config/waymaker/presets/jump.toml"
      test -f "$HOME/.config/waymaker/presets/rg.toml"
      echo "Preset verification: OK"

      zsh -c "source ~/.zshrc && which z >/dev/null"
      echo "Zsh integration verification: OK"
EOF
    echo "✨ All isolated Docker tests PASSED successfully!"
'
