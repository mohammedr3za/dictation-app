#!/usr/bin/env bash
# Installs the latest Dictation AppImage for Linux.
# Usage: curl -fsSL https://raw.githubusercontent.com/mohammedr3za/dictation-app/main/install.sh | bash
set -euo pipefail

REPO="mohammedr3za/dictation-app"
APP="dictation"
SHARE_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/dictation"
BIN_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
DESKTOP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"

if [ "$(uname -s)" != "Linux" ]; then
  echo "This installer is for Linux. On macOS, download the .dmg from:"
  echo "  https://github.com/$REPO/releases/latest"
  exit 1
fi

case "$(uname -m)" in
  x86_64|amd64)  ARCH_TAG="amd64" ;;
  aarch64|arm64) ARCH_TAG="aarch64" ;;
  *) echo "Unsupported architecture: $(uname -m)"; exit 1 ;;
esac

echo "Fetching the latest release for $ARCH_TAG…"

# Preferred: the stable asset name, which redirects to the latest release and
# never touches the rate-limited GitHub API.
URL="https://github.com/$REPO/releases/latest/download/Dictation-linux-$ARCH_TAG.AppImage"
if ! curl -fsIL "$URL" >/dev/null 2>&1; then
  # Fallback for releases that only carry versioned asset names.
  URL="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null \
    | grep -oE '"browser_download_url"[[:space:]]*:[[:space:]]*"[^"]+\.AppImage"' \
    | sed -E 's/.*"(https[^"]+)"/\1/' \
    | grep -i "$ARCH_TAG" \
    | head -n 1)" || true
fi

if [ -z "${URL:-}" ]; then
  echo "No $ARCH_TAG AppImage found in the latest release yet."
  echo "Build from source instead:"
  echo "  git clone https://github.com/$REPO.git && cd dictation-app"
  echo "  npm install && npm run tauri build"
  exit 1
fi

mkdir -p "$SHARE_DIR" "$BIN_DIR" "$DESKTOP_DIR"
echo "Downloading $(basename "$URL")…"
curl -fL --progress-bar "$URL" -o "$SHARE_DIR/$APP.AppImage"
chmod +x "$SHARE_DIR/$APP.AppImage"

# Wrapper: APPIMAGE_EXTRACT_AND_RUN avoids requiring libfuse2 on newer distros.
cat > "$BIN_DIR/$APP" <<EOF
#!/bin/sh
exec env APPIMAGE_EXTRACT_AND_RUN=1 "$SHARE_DIR/$APP.AppImage" "\$@"
EOF
chmod +x "$BIN_DIR/$APP"

cat > "$DESKTOP_DIR/$APP.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Dictation
Comment=Local, on-device dictation
Exec=$BIN_DIR/$APP
Terminal=false
Categories=Utility;AudioVideo;
EOF

echo
echo "Installed: $BIN_DIR/$APP"
case ":$PATH:" in
  *":$BIN_DIR:"*) echo "Run it with: $APP" ;;
  *) echo "Add this to your shell profile, then restart the terminal:"
     echo "  export PATH=\"$BIN_DIR:\$PATH\"" ;;
esac

if [ "${XDG_SESSION_TYPE:-}" = "wayland" ]; then
  echo
  echo "Note: you are on Wayland. The global shortcut and automatic paste are"
  echo "limited there; use the in-app Start/Stop button and the clipboard mode."
fi
