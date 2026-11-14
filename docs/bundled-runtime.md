# Embedded OpenConnect runtime

Linux CI/release executables contain a compressed runtime: OpenConnect 9.12,
the musl loader, its transitive shared libraries, upstream vpnc-script and a CA
certificate fallback. The executable remains a single file for installation.
Windows/macOS builds contain only clients and do not include this payload.

## Deployment

Run `hku doctor`, `hku setup --user YOUR_UID@connect.hku.hk`, then `hku connect`.
No OpenConnect/vpnc-scripts packages, Python, Rust or Docker are required on the
deployment host. Host requirements are Linux/systemd, TUN, sudo, iproute2,
iptables/ip6tables and standard POSIX shell utilities used by the routing script.
Docker remains available as a separate backend.

Setup asks for sudo and verifies the embedded manifest, architecture, file types,
sizes and SHA-256 checksums. It rejects absolute paths, parent traversal, links,
duplicate entries and unlisted files. Extraction is staged in a root-owned private
directory and atomically renamed to an immutable digest-named runtime under the
instance's `/var/lib/hku/native/INSTANCE/runtime/` directory. A damaged existing
runtime is rejected before stopping the running service. Runtime files and older
versions remain with the root-owned installation after `down`.

The private musl loader is invoked directly with an explicit library directory,
so no host musl installation or changes to system library paths are needed.
OpenConnect runs as its normal external process; authentication prompts and MFA
handling are unchanged. Password and Microsoft OTP authentication, DTLS and the
AnyConnect protocol are enabled. Optional Kerberos, hardware-token and desktop
proxy integrations are omitted from this build.

TLS certificate verification remains enabled. The service prefers a root-owned
host CA bundle and uses its embedded CA snapshot if none is available. It passes
the selected CA file explicitly and disables implicit system-trust discovery, not
certificate verification. Update the CLI and run `hku up` to deploy updated
runtime libraries and fallback certificates; this restart requires VPN MFA again.
The CLI does not overwrite the host's VPN client, certificates or shared libraries.

## Source builds

To embed the runtime when compiling on Linux:

```sh
bash scripts/build-runtime.sh
HKU_OPENCONNECT_BUNDLE="$PWD/target/openconnect-runtime/runtime.tar.gz" cargo build --release --locked
```

Docker is used only by this build helper. Native x86_64 and aarch64 CI runners
build matching payloads. The helper builds checksum-pinned OpenConnect source
against Alpine musl/GnuTLS, includes the shared-library closure and records exact
package versions and recipe commits. It also creates a matching sources archive.

A bare `cargo build --release` intentionally retains the source-build fallback
using root-owned host OpenConnect/vpnc-script packages. It does not silently fetch
native dependencies during Cargo builds. `hku doctor` identifies bundled builds
with `BUNDLED` messages. To compile using an already-built matching payload, set
`HKU_OPENCONNECT_BUNDLE` to its absolute path; Docker is then unnecessary.

## Distribution and checks

Notices, license texts and a package manifest are included inside the runtime.
Each Linux CI build publishes separate executable and corresponding-source
artifacts. Tagged releases distribute `hku-VERSION-runtime-sources-TARGET.tar.gz`
beside the executable archives; source inputs, Alpine recipes/patches, checksums
and build scripts are preserved. See [third-party provenance](../third_party/README.md).

CI tests both architectures without installed host VPN packages. Before replacing
OpenConnect with a private test fixture, it executes the real bundled client and
checks rejection of an untrusted TLS certificate and acceptance of a supplied test
CA. Tests then cover persistent binary forwarding, namespace DNS, keepalive,
route restoration, permissions, restart recovery and cleanup. Cache corruption
must be rejected without stopping an existing service. No HKU credentials are
used in CI; real bundled HKU MFA remains a separate acceptance test.
