# 40 — Sub-phase P0.3: Prefix Boot + Packaging

Parent: [30 — Phase P0](30-phase-P0-foundation.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: empty prefix boots PID1; offline PPA pipeline ships debs.
Milestone: `mol-loader --help` runs and the empty prefix boots on
both arches; debs install.
Proven by: boot test + deb install from a network-off build.

Criteria:

- [x] P0.3.1 `mol-loader --help` runs; empty prefix boots PID1 in
  mount+PID namespaces with overlayfs and `DYLD_SHARED_CACHE_DIR`
  override on both arches (docs 09, 12 L0).
- [x] P0.3.2 cargo-deb + PPA source .dsc pipeline produces
  installable debs for both arches from a network-off build
  (docs 07, 28).

Tests:

- [x] P0.3.V1 Empty-prefix boot test asserts PID1 + cache override
  (doc 12).

## Verification procedure

P0.3 proofs run on Ubuntu 24.04, natively or in docker
(`--privileged` supplies the boot test's mount powers):

- Boot (both arches): `cargo test -p mol-loader` on linux/arm64
  and linux/amd64 — `prefix_boot` asserts PID 1, cache env, and
  overlay content. Prefix dirs must live on an overlay-capable fs
  (ext4, tmpfs — never a docker container rootfs); in docker run
  the test with `TMPDIR` on a tmpfs mount.
- Deb: `cargo deb -p mol-loader`, `dpkg -i` the artifact,
  `mol-loader --help` exits 0. Offline: the zero-dependency
  workspace builds under `cargo build --offline`.
- Source package: `debuild -S -d -I.git -Itarget` produces the .dsc
  (`-d` skips the dpkg build-dep check since the toolchain comes
  from rustup; archive builders resolve Build-Depends normally);
  `debuild -b -d` produces an installable .deb.

Follow-up (not gating P0.3): upload the .dsc to a Launchpad PPA
(`dput ppa:<user>/<ppa>`) — needs the maintainer's Launchpad
account; sbuild clean-chroot builds come with the first release.
