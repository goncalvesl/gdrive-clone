#!/usr/bin/env bash
set -euo pipefail
project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_dir"
version="$(sed -n '/^name = "drive-desktop"/{n;s/^version = "\([^"]*\)"/\1/p;}' apps/drive-desktop/Cargo.toml)"
if [[ -z "$version" ]]; then
    version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' apps/drive-desktop/Cargo.toml | head -1)"
fi
if [[ -z "$version" ]]; then
    printf '%s\n' 'Unable to read application version' >&2
    exit 1
fi
mkdir -p dist
package_temp="$(mktemp -d)"
appdir=""
trap 'rm -rf -- "$package_temp"; if [[ -n "$appdir" ]]; then rm -rf -- "$appdir"; fi' EXIT
client_json=""
if [[ -n "${GDRIVE_OAUTH_CLIENT_FILE:-}" || -n "${GDRIVE_OAUTH_CLIENT_ID:-}" ]]; then
    command -v python3 >/dev/null || { printf '%s\n' 'python3 is required for OAuth packaging' >&2; exit 1; }
    client_json="$package_temp/oauth-client.json"
    python3 scripts/write-oauth-client.py "$client_json"
fi
case "${1:-}" in
    deb|rpm)
        command -v nfpm >/dev/null || { printf '%s\n' 'nfpm is required' >&2; exit 1; }
        cargo build --locked --release -p drive-desktop
        package_arch=amd64
        if [[ "$1" == rpm ]]; then
            package_arch=x86_64
        fi
        cp packaging/nfpm.yaml "$package_temp/nfpm.yaml"
        if [[ -n "$client_json" ]]; then
            printf '  - src: %s\n    dst: /usr/share/gdrive-clone/oauth-client.json\n    file_info:\n      mode: 0644\n' "$client_json" >> "$package_temp/nfpm.yaml"
        fi
        nfpm pkg --config "$package_temp/nfpm.yaml" --packager "$1" --target "dist/gdrive-clone_${version}_${package_arch}.$1"
        ;;
    appimage)
        command -v appimagetool >/dev/null || { printf '%s\n' 'appimagetool is required' >&2; exit 1; }
        cargo build --locked --release -p drive-desktop
        appdir="$(mktemp -d)"
        install -Dm755 target/release/drive-desktop "$appdir/usr/bin/drive-desktop"
        if [[ -n "$client_json" ]]; then
            install -Dm644 "$client_json" "$appdir/usr/share/gdrive-clone/oauth-client.json"
        fi
        install -Dm755 packaging/AppRun "$appdir/AppRun"
        install -Dm644 packaging/gdrive-clone.desktop "$appdir/gdrive-clone.desktop"
        install -Dm644 assets/drive-linux.svg "$appdir/gdrive-clone.svg"
        install -Dm644 assets/drive-linux.svg "$appdir/usr/share/icons/hicolor/scalable/apps/gdrive-clone.svg"
        install -Dm644 LICENSE "$appdir/usr/share/doc/gdrive-clone/LICENSE"
        install -Dm644 ATTRIBUTION.md "$appdir/usr/share/doc/gdrive-clone/ATTRIBUTION.md"
        install -Dm644 THIRD_PARTY_LICENSES.md "$appdir/usr/share/doc/gdrive-clone/THIRD_PARTY_LICENSES.md"
        install -Dm644 THIRD_PARTY_NOTICES.html "$appdir/usr/share/doc/gdrive-clone/THIRD_PARTY_NOTICES.html"
        for notice in packaging/licenses/*; do
            install -Dm644 "$notice" "$appdir/usr/share/doc/gdrive-clone/fonts/${notice##*/}"
        done
        ARCH=x86_64 appimagetool "$appdir" "dist/gdrive-clone_${version}_x86_64.AppImage"
        ;;
    *)
        printf 'Usage: %s {deb|rpm|appimage}\n' "$0" >&2
        exit 2
        ;;
esac
