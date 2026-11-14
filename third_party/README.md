# Bundled upstream sources

`vpnc-scripts-ce9e961.tar.gz` is an unmodified `git archive` of the official
[vpnc-scripts repository](https://gitlab.com/openconnect/vpnc-scripts), commit
`ce9e961bd0f6b867e1c7c35f78f6fb973f6ff101`, compressed with `gzip -n`.
SHA-256: `0b5906377890f41c7d07e3353d2f97c8d28ed1aaf25668d5f6dc80294f241a84`.
It contains the upstream script, copyright notices and COPYING.

The runtime builder downloads checksum-pinned OpenConnect 9.12 source and builds
it with GnuTLS. It collects the transitive musl library dependencies, matching
Alpine package recipes, checksum-verified source inputs and license texts.
The runtime embeds notices and licenses. Each Linux build also produces a
`runtime-sources.tar.gz` containing corresponding sources and build recipes;
release workflows distribute it alongside the executable. See
[`scripts/runtime/Dockerfile`](../scripts/runtime/Dockerfile) and
[`collect.py`](../scripts/runtime/collect.py).
