#!/usr/bin/env bash
# Build-time Docker is optional for source users; deployment never invokes it.
set -euo pipefail
ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
OUTPUT=${1:-$ROOT/target/openconnect-runtime}
mkdir -p "$OUTPUT"
OUTPUT=$(cd "$OUTPUT" && pwd)
image="hku-runtime-build:$(uname -m)"
docker build --build-arg "ALPINE_IMAGE=${HKU_ALPINE_IMAGE:-alpine:3.23.7}" \
    -f "$ROOT/scripts/runtime/Dockerfile" -t "$image" "$ROOT"
container=$(docker create "$image" /bin/true)
trap 'docker rm "$container" >/dev/null' EXIT
docker cp "$container:/out/." "$OUTPUT/"
printf 'Runtime bundle: %s/runtime.tar.gz\n' "$OUTPUT"
printf 'Compile with: HKU_OPENCONNECT_BUNDLE=%q cargo build --release --locked\n' "$OUTPUT/runtime.tar.gz"
