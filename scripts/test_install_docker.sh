#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "⚡ Starting Waymaker isolated Docker installation test (local build)..."

docker run --rm \
  -v "${SCRIPT_DIR}:/workspace:ro" \
  debian:sid-slim bash -c '
    set -euo pipefail
    apt-get update -qq && apt-get install -y -qq zsh sudo >/dev/null

    useradd -m -s /bin/zsh testuser

    su - testuser << "EOF"
      set -euo pipefail
      sh /workspace/install.sh -s

      # Verify that ~/.zshrc contains PATH export and eval wm init zsh
      grep -F ".local/bin" "$HOME/.zshrc" >/dev/null
      grep -F "wm init zsh" "$HOME/.zshrc" >/dev/null

      # Test Zsh shell invocation: ~/.zshrc must automatically load PATH and Smart Tab
      zsh -i -c "
        which wm >/dev/null
        echo -n \"Installed binary version: \"
        wm --version

        which z >/dev/null
        echo \"'z' function: OK\"

        # Verify Tab binding is now _wm_smart_tab
        tab_binding=\$(bindkey '^I')
        echo \"Tab binding: \$tab_binding\"
        echo \"\$tab_binding\" | grep -q \"_wm_smart_tab\"
        echo \"Smart Tab keybinding: OK\"
      "

      test -f "$HOME/.config/waymaker/config.toml"
      test -f "$HOME/.config/waymaker/session.toml"
      test -f "$HOME/.config/waymaker/presets/jump.toml"
      test -f "$HOME/.config/waymaker/presets/rg.toml"
      echo "Preset verification: OK"
EOF
    echo "✨ All isolated Docker tests PASSED successfully!"
'
