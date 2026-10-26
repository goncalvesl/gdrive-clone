#!/usr/bin/env bash
set -euo pipefail
project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ "${1:-}" != "--no-build" ]]; then
    cargo build --manifest-path "$project_dir/Cargo.toml" --release -p drive-desktop
fi
app_install_root="${GDRIVE_INSTALL_HOME:-$HOME}"
install -Dm755 "$project_dir/target/release/drive-desktop" "$app_install_root/.local/bin/drive-desktop"
app_data="${XDG_DATA_HOME:-$app_install_root/.local/share}"
app_config="${XDG_CONFIG_HOME:-$app_install_root/.config}"
# Optional distributor-provided Desktop client; never committed to the source tree.
if [[ -n "${GDRIVE_OAUTH_CLIENT_FILE:-}" ]]; then
    install -Dm600 "$GDRIVE_OAUTH_CLIENT_FILE" "$app_install_root/.local/share/gdrive-clone/oauth-client.json"
fi
install -Dm644 "$project_dir/assets/drive-linux.svg" "$app_data/icons/hicolor/scalable/apps/gdrive-clone.svg"
install -Dm644 "$project_dir/packaging/gdrive-clone.service" "$app_config/systemd/user/gdrive-clone.service"
mkdir -p "$app_data/applications"
# Desktop-entry quoting: escape backslashes, quotes, dollar and backticks.
app_binary="$app_install_root/.local/bin/drive-desktop"
app_binary="${app_binary//%/%%}"
app_binary="${app_binary//\\/\\\\}"
app_binary="${app_binary//\"/\\\"}"
app_binary="${app_binary//\$/\\\$}"
app_binary="${app_binary//\`/\\\`}"
cat > "$app_data/applications/gdrive-clone.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Drive para Linux
Comment=Espelhamento independente do Google Drive
Exec="$app_binary"
Icon=gdrive-clone
Terminal=false
Categories=Network;FileTransfer;
StartupNotify=true
DESKTOP
install -Dm644 "$project_dir/packaging/gdrive-links.xml" "$app_data/mime/packages/gdrive-clone.xml"
cat > "$app_data/applications/gdrive-clone-reference.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Abrir documento Google Drive
Exec="$app_binary" open-document %f
Icon=gdrive-clone
Terminal=false
NoDisplay=true
MimeType=application/x-gdrive-clone-reference;
DESKTOP
if command -v update-mime-database >/dev/null 2>&1; then
    update-mime-database "$app_data/mime"
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$app_data/applications"
fi
printf '%s\n' 'Instalado em ~/.local/bin/drive-desktop. Serviço preparado, sem ativação automática.' 'Abra pelo menu ou execute drive-desktop. Para segundo plano, consulte docs/user-guide.md.'
