#!/usr/bin/env bash
set -euo pipefail

# Download a prebuilt CLI. Deployment prerequisites are checked by `hku doctor`.
REPO=${HKU_REPO:-adlsdztony/hku-ssh-gateway}
VERSION=${HKU_VERSION:-latest}
BIN_DIR=${HKU_BIN_DIR:-${HOME:?HOME is required}/.local/bin}
usage() {
    printf 'Usage: bash install.sh [--version vX.Y.Z] [--bin-dir DIR]\n'
}
die() { printf 'Error: %s\n' "$*" >&2; exit 1; }
while (( $# )); do
    case $1 in
        --version|--bin-dir)
            (( $# >= 2 )) || die "Missing value for $1"
            if [[ $1 == --version ]]; then VERSION=$2; else BIN_DIR=$2; fi
            shift 2 ;;
        --help|-h) usage; exit 0 ;;
        *) die "Unknown option: $1" ;;
    esac
done
[[ $REPO =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || die 'Invalid HKU_REPO.'
SYSTEM=$(uname -s)
case "$SYSTEM/$(uname -m)" in
    Linux/x86_64|Linux/amd64) TARGET=x86_64-unknown-linux-musl ;;
    Linux/aarch64|Linux/arm64) TARGET=aarch64-unknown-linux-musl ;;
    Darwin/x86_64) TARGET=x86_64-apple-darwin ;;
    Darwin/arm64) TARGET=aarch64-apple-darwin ;;
    *) die "Unsupported architecture: $(uname -m)" ;;
esac
for required in curl tar mktemp install; do command -v "$required" >/dev/null || die "Missing command: $required"; done
if command -v sha256sum >/dev/null; then
    checksum() { sha256sum "$1" | cut -d ' ' -f 1; }
elif command -v shasum >/dev/null; then
    checksum() { shasum -a 256 "$1" | cut -d ' ' -f 1; }
else
    die 'Install sha256sum or shasum before continuing.'
fi
fetch() { curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 --retry 3 "$@"; }
if [[ $VERSION == latest ]]; then
    metadata=$(fetch "https://api.github.com/repos/$REPO/releases/latest")
    VERSION=$(printf '%s\n' "$metadata" | sed -n 's/^[[:space:]]*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p')
fi
[[ $VERSION =~ ^v[0-9]+\.[0-9]+\.[0-9]+([-.][A-Za-z0-9.-]+)?$ ]] || die 'Cannot determine a valid release version.'
ASSET="hku-$VERSION-$TARGET.tar.gz"
BASE="https://github.com/$REPO/releases/download/$VERSION"
TMP=$(mktemp -d)
trap 'rm -rf -- "$TMP"' EXIT
printf 'Downloading hku %s (%s)...\n' "$VERSION" "$TARGET"
fetch "$BASE/$ASSET" --output "$TMP/$ASSET"
fetch "$BASE/SHA256SUMS" --output "$TMP/SHA256SUMS"
expected=$(awk -v name="$ASSET" '$2 == name || $2 == "*"name {print $1}' "$TMP/SHA256SUMS")
[[ $expected =~ ^[0-9a-fA-F]{64}$ ]] || die 'Release checksum is missing or ambiguous.'
actual=$(checksum "$TMP/$ASSET")
[[ $(printf '%s' "$actual" | tr '[:upper:]' '[:lower:]') == "$(printf '%s' "$expected" | tr '[:upper:]' '[:lower:]')" ]] || die 'Checksum mismatch; refusing to install.'
# Extract only the expected member, never an arbitrary archive tree.
tar -xzf "$TMP/$ASSET" -C "$TMP" hku
[[ -f $TMP/hku && ! -L $TMP/hku ]] || die 'Release does not contain a regular hku binary.'
chmod 755 "$TMP/hku"
reported=$("$TMP/hku" --version)
[[ $reported == "hku ${VERSION#v}" ]] || die "Unexpected binary version: $reported"
mkdir -p -- "$BIN_DIR"
staging="$BIN_DIR/.hku-install-$$"
trap 'rm -f -- "${staging:-}"; rm -rf -- "$TMP"' EXIT
install -m 755 "$TMP/hku" "$staging"
mv -f -- "$staging" "$BIN_DIR/hku"
printf 'Installed %s/hku\n' "$BIN_DIR"
case ":${PATH:-}:" in
    *":$BIN_DIR:"*) ;;
    *) printf 'Add this directory to PATH: export PATH="%s:$PATH"\n' "$BIN_DIR" ;;
esac
if [[ $SYSTEM == Darwin ]]; then
    printf '\nNext: hku ssh -s YOUR_GATEWAY user@internal-host\n'
    printf 'macOS supports remote client mode only; configure the VPN on a Linux gateway.\n'
else
    printf '\nNext: hku doctor\nThen: hku setup --user YOUR_UID@connect.hku.hk\n'
    printf 'VPN authentication: hku connect (enter password and MFA interactively).\n'
fi
