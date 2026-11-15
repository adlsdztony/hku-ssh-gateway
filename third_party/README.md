# Bundled upstream sources

`vpnc-scripts-ce9e961.tar.gz` is an unmodified `git archive` of the official
[vpnc-scripts repository](https://gitlab.com/openconnect/vpnc-scripts), commit
`ce9e961bd0f6b867e1c7c35f78f6fb973f6ff101`, compressed with `gzip -n`.
SHA-256: `0b5906377890f41c7d07e3353d2f97c8d28ed1aaf25668d5f6dc80294f241a84`.
It contains the upstream script, copyright notices and COPYING.

The runtime builder downloads checksum-pinned OpenConnect 9.21 source and builds
it with GnuTLS. It collects the transitive musl library dependencies, matching
Alpine package recipes, checksum-verified source inputs and license texts.
The runtime embeds notices and licenses. Each Linux build also produces a
`runtime-sources.tar.gz` containing corresponding sources and build recipes;
release workflows distribute it alongside the executable. See
[`scripts/runtime/Dockerfile`](../scripts/runtime/Dockerfile) and
[`collect.py`](../scripts/runtime/collect.py).

OpenConnect source SHA-256:
`5b32369467db6e5f317aa1ed12cfcbb81ed00bdbc765450b6bfcbdc300944a58`.
The included detached release signature is verified against the public release
key with fingerprint `BE07D9FD54809AB2C4B0FF5F63762CDA67E2F359`, as published on
the [official download page](https://www.infradead.org/openconnect/download.html).
The exported key is public and contains only the release signing identity and
its self-certifications. These verification inputs accompany the sources archive.
