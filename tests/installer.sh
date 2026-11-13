#!/usr/bin/env bash
# Test installer behavior without network access or installing to the user's home.
set -euo pipefail
ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
TMP=$(mktemp -d)
trap 'rm -rf -- "$TMP"' EXIT
mkdir -p "$TMP/mock" "$TMP/fixtures" "$TMP/build" "$TMP/bin"
cat > "$TMP/build/hku" <<'BINARY'
#!/bin/sh
printf 'hku 0.1.0\n'
BINARY
chmod 755 "$TMP/build/hku"
asset=hku-v0.1.0-x86_64-unknown-linux-musl.tar.gz
tar -czf "$TMP/fixtures/$asset" -C "$TMP/build" hku
(cd "$TMP/fixtures" && sha256sum "$asset" > SHA256SUMS)
cat > "$TMP/mock/uname" <<'UNAME'
#!/bin/sh
case "$1" in -s) echo Linux ;; -m) echo x86_64 ;; esac
UNAME
cat > "$TMP/mock/curl" <<'CURL'
#!/bin/bash
set -eu
output=
url=
while (( $# )); do
    case $1 in
        --output) output=$2; shift 2 ;;
        https://*) url=$1; shift ;;
        *) shift ;;
    esac
done
if [[ $url == */releases/latest ]]; then
    printf '{\n  "tag_name": "v0.1.0"\n}\n'
else
    cp "$HKU_TEST_FIXTURES/${url##*/}" "$output"
fi
CURL
chmod 755 "$TMP/mock/"*
export PATH="$TMP/mock:$PATH" HKU_TEST_FIXTURES="$TMP/fixtures"
bash "$ROOT/install.sh" --bin-dir "$TMP/bin"
test "$("$TMP/bin/hku" --version)" = 'hku 0.1.0'
before=$(sha256sum "$TMP/bin/hku")
printf 'corrupted archive' >> "$TMP/fixtures/$asset"
if bash "$ROOT/install.sh" --version v0.1.0 --bin-dir "$TMP/bin"; then
    echo 'FAIL: accepted a corrupted release' >&2; exit 1
fi
test "$before" = "$(sha256sum "$TMP/bin/hku")"
if bash "$ROOT/install.sh" --version '../invalid' --bin-dir "$TMP/bin"; then
    echo 'FAIL: accepted an invalid version' >&2; exit 1
fi
printf 'PASS: latest resolution, installation, checksum rejection, preservation of existing binary and version validation.\n'
