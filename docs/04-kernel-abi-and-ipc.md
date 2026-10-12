# 04 — Kernel ABI and IPC

## BSD syscalls

- FreeBSD-derived sequential table: exit=1 fork=2 read=3 write=4 open=5:
  [syscalls.master](https://raw.githubusercontent.com/apple/darwin-xnu/main/bsd/kern/syscalls.master).
- Linux x86-64 differs (read=0 write=1 open=2 close=3), so 1:1 renumbering is
  required: [syscall_64.tbl](https://raw.githubusercontent.com/torvalds/linux/master/arch/x86/entry/syscalls/syscall_64.tbl).
- XNU has 3 tables: BSD, Mach (negative-numbered), machdep (int 0x82); BSD
  maps to Linux, Mach mostly in mol-server.
- Linux mapping: translation table to Linux syscalls; Darwin-only calls
  (e.g. `getattrlist`) need shims. Tier: M (Rust, `nix` crate per-header bindings).

## Mach traps + MIG

- Traps use negative numbers: `mach_vm_allocate`=-10,
  `mach_port_allocate`=-16, etc.:
  [syscall_sw.h](https://raw.githubusercontent.com/apple/darwin-xnu/main/osfmk/mach/syscall_sw.h).
- Core abstractions: tasks/threads/ports/address-space; trap-level msg API
  abstracted by MIG stubs:
  [KernelProgramming](https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/KernelProgramming/mach/mach.html).
- Complex messages transfer port rights and OOL data, flagged by
  `MACH_MSGH_BITS_COMPLEX`:
  [message.h](https://raw.githubusercontent.com/apple/darwin-xnu/main/osfmk/mach/message.h).
- MIG `.defs` define kernel RPC: subsystem task 3400 with
  task_create/task_terminate/task_threads:
  [task.defs](https://raw.githubusercontent.com/apple/darwin-xnu/main/osfmk/mach/task.defs).
- Ports are MPSC queues with receive/send/send-once/port-set/dead-name
  rights; `mach_msg` is the low-level trap:
  [Darling doc](https://github.com/darlinghq/darling-docs/blob/master/src/internals/macos-specifics/mach-ports.md).
- Linux mapping: AF_UNIX seqpacket + SCM_RIGHTS + userspace port-namespace
  server. Tier: XL (Rust, tokio reactor). Binder is a closer capability model
  but heavier than sockets.

## Exceptions

- Machine-independent EXC_BAD_ACCESS=1 … EXC_CORPSE_NOTIFY=13 delivered via
  exception ports:
  [exception_types.h](https://raw.githubusercontent.com/apple/darwin-xnu/main/osfmk/mach/exception_types.h).
- Linux mapping: pidfd/signalfd/ptrace delivery. Tier: L (Rust; C trampolines
  for signal/exception entry).

## VM

- XNU VM is a Mach-VM descendant: sparse spaces, per-map-entry
  protection+inheritance (shared/copy/none), VM objects vs pager-backed
  memory objects:
  [KernelProgramming](https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/KernelProgramming/vm/vm.html).
- Linux mapping: mmap/mprotect/memfd + userfaultfd for pager/COW nuance.
  Tier: L (Rust).

## IOKit

- User-kernel transport is Mach ports: `io_object_t`/`io_connect_t`/
  `io_service_t` are all `mach_port_t`:
  [IOKitLib](https://developer.apple.com/library/archive/documentation/DeviceDrivers/Conceptual/AccessingHardware/AH_IOKitLib_API/AH_IOKitLib_API.html).
- OO driver framework with I/O Registry, matching, work loops, power mgmt:
  [fundamentals](https://developer.apple.com/library/archive/documentation/DeviceDrivers/Conceptual/IOKitFundamentals/Introduction/Introduction.html).
- Linux mapping: ioctl char-devices + registry, per-service stubs. Tier: XL
  (unbounded per-driver surface; stub on demand).

## libdispatch / CFRunLoop

- libdispatch on Darwin is xnu-kernel + userspace lib; Linux port runs on
  userspace pthread primitives:
  [README](https://raw.githubusercontent.com/swiftlang/swift-corelibs-libdispatch/main/README.md).
  Mapping: reuse the Linux port on epoll/eventfd. Tier: S–M.
- CFRunLoop → epoll + eventfd + timerfd replacing `mach_msg` wait. Tier: M.
- CFMachPort integrates Mach ports into CFRunLoop via CreateRunLoopSource:
  [API](https://learn.microsoft.com/en-us/dotnet/api/corefoundation.cfmachport?view=xamarin-mac-sdk-14).

## XPC + launchd + notifyd

- XPC Services API is part of libSystem, integrated with GCD + launchd;
  services launched on demand, sandboxed, stateless:
  [Apple doc](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingXPCServices.html).
- launchd owns service ports/sockets, launch-on-demand, restarts daemons,
  passes registered sockets/fds:
  [Apple doc](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html).
- notifyd is system-wide pub/sub: tokens, 64-bit state values, delivery via
  check/dispatch/signal/mach-port/fd:
  [notify.3](https://keith.github.io/xcode-man-pages/notify.3.html).
- Linux mapping: notifyd → AF_UNIX pub/sub daemon + eventfd (S–M);
  XPC+launchd → AF_UNIX daemon with socket activation, systemd-like (L).

## Difficulty order (easy → hardest)

notifyd → BSD syscalls → libdispatch → CFRunLoop → vm_map → XPC+launchd →
IOKit user clients → Mach IPC+MIG+task/thread/exception ports.
