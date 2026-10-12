# 19 — Ubuntu VM Test Agent

The Linux-side leg of the harness (doc 17): runs each test under mol
inside a hermetic Ubuntu VM and captures outputs for the compare job.
QEMU+KVM via libvirt is the primary Linux-host stack. Tiers use the
doc-README legend.

## VM host stacks

| Stack | Role | Tier | Source |
|-------|------|------|--------|
| QEMU+KVM via libvirt | Primary: `virsh` domains, snapshots, guest agent | S | [virsh](https://libvirt.org/manpages/virsh.html) |
| QEMU snapshots | `savevm/delvm/info snapshots` on writable qcow2 disks | S | [images](https://www.qemu.org/docs/master/system/images.html) |
| `virsh snapshot-revert` | Restore named/current snapshot, running/paused option — the per-run hermetic reset | S | [virsh](https://libvirt.org/manpages/virsh.html) |
| Tart | macOS+Linux VMs on Apple Silicon via Virtualization.Framework, OCI-registry images; `brew install`, Linux-from-scratch, OCI push/pull | S–M | [Tart](https://tart.run/), [quick-start](https://tart.run/quick-start/) |
| UTM | QEMU-based host for macOS/iOS (QEMU + Apple backends); Shortcuts intents, AppleScript bridge, CLI automation | M | [UTM](https://docs.getutm.app/), [scripting](https://docs.getutm.app/scripting/scripting/) |
| VirtualBox | Snapshots take/restore/delete + cloning (ARM host/guest parity + Guest Additions ARM status unverified — doc 10) | M | [manual](https://www.virtualbox.org/manual/ch01.html) |

Caveats: Tart sits on Virtualization.Framework, not QEMU — the
qemu-ga guest-exec path below is unverified inside Tart VMs (SSH may
be the only exec channel; doc 10). Apple
Virtualization.Framework nested-virt is unverifiable (JS-gated), and
ARM64 KVM nested-virt requirements are unverified — never plan nested
KVM (doc 10).

## Guest control

| Channel | Use | Tier | Source |
|---------|-----|------|--------|
| qemu-guest-agent | In-guest daemon: host file IO, time sync, fs freeze, suspend, password; `guest-exec/guest-exec-status` for host-driven commands | S | [qemu-ga](https://www.qemu.org/docs/master/interop/qemu-ga.html), [ref](https://www.qemu.org/docs/master/interop/qemu-ga-ref.html) |
| QMP | JSON machine-level protocol for operating QEMU (harness-driven) | M | [QMP spec](https://www.qemu.org/docs/master/interop/qmp-spec.html) |
| QEMU monitor | Freeze/unfreeze VM, save/restore state to disk | S | [monitor](https://www.qemu.org/docs/master/system/monitor.html) |
| Test-agent service | Small in-guest service for file staging + result upload alongside qemu-ga | M | harness-authored |
| SPICE / VNC | Remote-display + agent protocols for capture/input; os-autoinst can autostart a VNC viewer per test VM | S–M | [SPICE docs](https://www.spice-space.org/documentation.html), [os-autoinst](https://github.com/os-autoinst/os-autoinst) |

## Capture strategy: prefer hypervisor-level

Capture at the hypervisor (VNC/SPICE/QMP screendump) so the guest's
Wayland-vs-X11 choice stops mattering. In-guest capture is fallback only:

| API | Scope | Tier | Source |
|-----|-------|------|--------|
| xdg Screenshot portal | Sanctioned Wayland capture API | M | [portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Screenshot.html) |
| xdg RemoteDesktop portal | Remote input + screen on Wayland | M | [portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.RemoteDesktop.html) |
| openQA needles | Match screen against needle images, needle editor + OCR areas | M–L | [openQA](https://open.qa/docs/) |

OS-version cliff: Ubuntu 26.04 removed the GNOME X11 session scripts
([release notes](https://documentation.ubuntu.com/release-notes/26.04/summary-for-lts-users/)).
Pin the test image to 24.04/Xorg while X11-only tooling is in use, and
budget the Wayland-native path (portals + libei) before moving the
image to 26.04.

## Input injection

| Tool | Scope | Tier | Source |
|------|-------|------|--------|
| xdotool | X11 XTEST/Xlib key/mouse/type/window ops — Xorg-only | S | [xdotool](https://github.com/jordansissel/xdotool) |
| ydotool | X11 + Wayland via kernel uinput virtual device; needs `ydotoold` daemon + `/dev/uinput` access (usually root) | M | [ydotool](https://github.com/ReimuNotMoe/ydotool) |
| libei | Wayland emulated-input lib: EI client + compositor EIS server — the long-term path | M–L | [libei](https://libinput.pages.freedesktop.org/libei/) |

Note: libei is input-only; there is no Wayland replacement for
xdotool/wmctrl window ops — window placement must move to
hypervisor-level or compositor-scripted control on Wayland guests.

## Recommended setup per host arch

- AMD64 Linux host: (S) QEMU/KVM+libvirt, qcow2 golden image, `virsh
  snapshot-revert` per run, VNC/SPICE capture, qemu-ga guest-exec,
  X11 guest session for xdotool; (M) QMP-driven harness + openQA-style
  needle assertions; (L) os-autoinst/openQA worker farm.
- ARM64 Linux host: same stack stays first choice
  (`qemu-system-aarch64`, same virsh/QMP APIs); avoid x86 guests (TCG
  too slow) and avoid nested KVM.
- Apple Silicon host: (S) Tart Linux VMs + OCI image versioning +
  SSH exec, host-side capture over VNC/screen share; (M) UTM QEMU
  backend where Tart display-capture gaps hurt, scripted via
  AppleScript/CLI; (L) Orchard cluster for parallel hermetic runs
  (within the 2-VM-per-host SLA cap for macOS guests, doc 18).

## Hermetic run protocol + determinism limits

Revert to the golden snapshot (libvirt) or a fresh Tart clone/UTM
snapshot before each case; never reuse mutable state. Snapshot revert
alone does not give determinism — required pre-run steps:

- Time sync + fsfreeze-consistent snapshots (qemu-ga) before capture.
- Reseed guest RNG; reset MAC/IP/hostname/cookies that leak across reverts.
- Pin the renderer: virtio-gpu vs llvmpipe vs paravirtualized
  graphics, font set, DPR — unpinned rendering breaks even
  same-platform baselines (doc 21).
- Pin the guest-image digest into `corpus.lock` (doc 20): OCI-versioned
  images otherwise drift undetectably against CAS-pinned workloads.
- Unmeasured: QEMU savevm/HMP vs libvirt snapshot-path performance
  and qcow2 internal-snapshot locking under parallel jobs (doc 10).

## Checklist

- [ ] Golden image boots; per-run revert/clone verified hermetic
- [ ] qemu-ga (or SSH on Tart) executes compat-layer runs from the host
- [ ] Hypervisor-level capture green; in-guest portals as fallback only
- [ ] Input path matches guest session (xdotool/Xorg, ydotool/libei/Wayland)
- [ ] Renderer + fonts + DPR pinned; image digest pinned in corpus.lock
- [ ] Clock/RNG/network-identity reset steps run before every case
