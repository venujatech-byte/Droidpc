# Droidpc: start here

This is the current beginner guide for Droidpc. It records what we have
actually tested on the developer's Fedora PC and what we will do next.
Read it before the older experiment logs in this directory.

**Current status (2026-11-04): Android does not boot yet.** The Rust program
only has a read-only `doctor` command and image verification code. The
Android commands in `scripts/` are short, disposable development probes.
They are not the finished Droidpc container.

## 1. What are we building?

Droidpc aims to run Android apps on an x86_64 Linux desktop. Android user
space will share the Linux kernel but live in its own container. The host
controller is written in Rust. We are targeting Linux and Wayland, not
Windows or macOS. The first milestone is one Android session that boots,
stops cleanly, and runs an x86_64 app. Desktop windows, audio, game controls,
and optimization come later, after the base works.

The selected guest is **LineageOS 20 / Android 13 / x86_64**. These particular
images were built for a Android guest device. They contain third-party guest
components; Droidpc's host runtime is our own work. Keep image attribution
and licenses. Do not put the images in Git or change them to fix a probe.

“Every x86 PC” is a goal to investigate, not a present guarantee. The
current images and Rust target are x86_64; 32-bit x86 Linux has not been
supported or tested. Other Linux distributions and GPUs need their own
checks. The first supported machine is the developer's Fedora PC.

## 2. Words used in this guide

| Word | Plain meaning |
| --- | --- |
| Host | Your normal Fedora Linux system. |
| Guest | The Android system inside the container. |
| Image | A file holding an Android filesystem, such as `system.img`. |
| Namespace | A separate view of processes, mounts, users or network. |
| cgroup | Linux's way to group processes and control resources. |
| Binder | Android's main process-to-process communication device. |
| APEX | Android modules loaded early in startup. |
| init | Android's first process; it starts Android services. |
| Probe | A limited test of one requirement, not the finished app. |
| ueventd | Android's device manager; it creates device entries from kernel events. |
| Wayland | The display system used by the Linux desktop session. |
| HAL | An Android interface that connects a system service to hardware. |
| overlay | A writable layer placed over read-only files without editing them. |

## 3. Begin on the development PC

Open a terminal in the project directory:

```sh
cd /home/venujasilva/Documents/Coding/Droidpc
```

This guide assumes Fedora, a normal Wayland desktop session, and the
selected images already under `os_images/selected/`. Check the tools:

```sh
cargo --version
python3 --version
command -v bwrap debugfs unshare systemd-run
ls -l os_images/selected/system.img os_images/selected/vendor.img
```

On a fresh Fedora installation, the probe tools can be installed with:

```sh
sudo dnf install rust cargo python3 bubblewrap e2fsprogs \
  util-linux-core systemd
```

Do not download another Android image just to repeat these tests. The
selected pair and their exact hashes are recorded in
[IMAGE-BOOT.md](IMAGE-BOOT.md). If the images are missing, obtain a matched
system/vendor pair and inspect its version and licensing before selecting
it. Do not mix files from two releases.

Run the read-only host report without sudo:

```sh
cargo run -p droidpc-cli -- doctor \
  --images os_images/selected
```

`PASS` means that one check passed. `WARN` means more work may be needed.
Neither a zero exit status nor all green checks means Android boots.
See [DOCTOR.md](../DOCTOR.md) for the report's exact meanings.

## 4. What has been tested so far?

| Area | What passed on this Fedora PC | What it does not prove |
| --- | --- | --- |
| Host | Doctor saw kernel features, Wayland and GPU access. | Other PCs work. |
| Images | Matched hashes, version and layout inspected. | Android boots. |
| Guest shell | `/system/bin/sh` ran in an isolated, read-only view. | Android services run. |
| Full image mounts | System and vendor mounted read-only; cleanup left no mounts or loop devices. | Crash recovery or a production mount manager. |
| Properties | Android init loaded `ro.vndk.version=33` from the images. | Complete boot properties. |
| APEX/linker | Bootstrap tools exited successfully; linker config was generated. | All later Android services load. |
| User IDs | Guest UID/GID 1000 and `setgroups` worked in a disposable namespace. | All guest accounts map correctly. |
| Host cgroups | A temporary delegated service accepted CPU, memory and PID controls. | Android uses those controls. |
| Guest placement | A child cgroup namespace could use an `init` leaf and enable controllers. | Android init is placed there. |
| Android init | Stock second-stage init ran as PID 1, reached `late-init`, passed the 32-bit BoringSSL test, and opened private Binder devices. | Completed Android boot or app launch. |

The observed guest cgroup2 mount was writable and reported `cpu io memory
pids` as available. Its `cgroup.subtree_control` was empty. The selected
Android configuration also tried old cgroup v1 mounts; they failed on this
host. `SetupCgroups` printed “succeeded” despite those failures, so that
message is **not** proof that Android service groups work.

The Android startup probe is bounded to 45 seconds. `INIT_PROBE_STATUS=124`
means the wrapper stopped the run; `129` means init exited after a fatal
shutdown. Neither status means Android completed boot. `INIT_EARLY_INIT_OK`
only confirms that the custom early action ran. Linker warnings before
linkerconfig is generated are expected in this probe.

All of these tests use temporary directories and namespaces. They do not
install Android, change the base images, or create a persistent container.
The host SELinux policy remains enforcing; effective labels on the mounted
images still need investigation.

### Why the checks happen in this order

Android startup depends on earlier pieces. A shell must load before init can
run; init needs properties and linker configuration; services need devices,
Binder and process groups; the display needs a working graphics HAL. Testing
the screen before the startup foundation works would hide the first failure
under many later ones.

The Rust workspace currently has a small CLI and a container crate for
checking image files. It does not yet own mount points, launch init, allocate
Binder devices, or show a window. Python and shell scripts are development
probes because they make it easier to test Linux behavior before adding
privileged Rust code.

An Android image is not the Linux kernel. It contains Android programs and
libraries. The guest uses the host kernel through namespaces and mounts.
That saves the cost of running a second kernel, but Android still needs a
compatible set of devices, system calls, properties and hardware interfaces.

Detailed evidence is in [SHELL-PROBE.md](SHELL-PROBE.md),
[STAGING-PROBE.md](STAGING-PROBE.md), and
[CGROUP-PROBE.md](CGROUP-PROBE.md). Candidate Android device paths are
listed in [DEVICE-NODES.md](DEVICE-NODES.md). Older next-step notes in those
logs describe what was next at that time; follow this file for the current
order.

## 5. Current Android boot blocker

The latest run reached `INIT_TRIGGER_OK` and `LATE_INIT_TRIGGERED`. The
32-bit BoringSSL self-test passed and both private Binder device paths open.
`zygote` started and accepted connections, but `ANDROID_BOOT_NOT_COMPLETED`
remains. The debug console's no-controlling-tty warning is expected; status
That run's status 124 was the probe's 45-second timeout. Logcat identifies
`lmkd`'s remaining restart cause: PSI monitor access returns `EACCES`, and
this kernel has no cgroup-v1 `memory.pressure_level` fallback. Linux restricts unprivileged PSI
monitors to 2-second-multiple windows, while this Android `lmkd` uses a
1-second window, so mapping its UID to the PSI file owner cannot solve the
failure. SurfaceFlinger and several hardware services also fail in this
headless container. Further progress requires host-level `CAP_SYS_RESOURCE`
or a different runtime/kernel boundary; the current user-namespace probe
cannot grant that capability.

An opt-in diagnostic can run Android init in the host user namespace while
keeping its mount, PID, IPC, UTS, network, and cgroup namespaces private. It
drops host reboot, module-loading, raw-I/O, time-setting, and ptrace
capabilities, but retains `CAP_SYS_ADMIN` for Android mounts and
`CAP_SYS_RESOURCE` for PSI. Run it only on a disposable host:

```sh
sudo env DROIDPC_PROBE_HOST_CAPS=1 ./scripts/probe-android-init-selinux.sh
```

This mode can still affect host kernel state; it is a diagnostic, not a safe
daily runtime. It has been run once. `system_server` started, with no `lmkd`
restart reported, but the 45-second run ended before boot completion.
`system_server` logged PSI trigger `EINVAL`, and SurfaceFlinger crashed. The
probe now allows 120 seconds and includes those tags in the log summary.
The probe supplies `ro.sf.lcd_density=160` through a temporary build-property
overlay. The latest run confirms that property is read, but SurfaceFlinger
still crashes in vendor ANGLE (`libGLESv2_angle.so`) after a null EGL context;
the Android guest composer service also exits. The probe now selects the Android 13
ANGLE plus Pastel software-rendering path and reports the effective graphics
properties. This host has no `/dev/dri` device, so the probe now exposes the
active Wayland session socket to the Android guest composer and captures its errors.
One run reported `HOST_WAYLAND_SOCKET_MISSING`; detection now falls back to a
socket under `/run/user/<invoking-uid>`. The latest run mounted it, but HWC
looked under `/run/user/1000` while the socket was under `/tmp`. The probe now
mounts it at HWC's runtime path using a staged `/run` tree. The next run exited
before init because Bubblewrap could not create `/run/user` in the read-only
image. Re-run to check whether the staged mount lets composer register.
The next attempt stopped before init because that branch omitted the `/tmp`
tmpfs used by probe helpers; the probe now mounts it in both branches.
The previous run confirmed HWC connects and SurfaceFlinger receives a display
hotplug event, then aborts with `output buffer not gpu writeable`; SwiftShader
rejects an AHardwareBuffer format and `gralloc.default` reports ashmem
allocation failures. The latest run confirms `sys.use_memfd=true` and shows
SystemServer progressing into `StartDropBoxManager`, then repeated ActivityManager
stack dumps about every 45 seconds. At 110 seconds, zygote, SurfaceFlinger and
boot animation were still running, while `sys.boot_completed` remained empty.
The host's Android guest config and earlier startup log show a Mesa/GBM render-node
path; the probe now passes that node through when present. It also captures
the SystemServer process and latest ANR trace at 110 seconds.

The probe is diagnostic, not the production container. The installed Android guest
LXC config binds persistent `/data`, stages Wayland and Pulse under
`/run/xdg`, and binds the host-generated `/vendor/android_guest.prop`. This probe
uses disposable `/data`, stages Wayland under `/run/user/<uid>`, and supplies
the vendor Sensors HAL's `android_guest.stub_sensors_hal=1` setting. An older
Android guest ANR dump shows
`system_server`'s main thread idle in its normal looper, but it does not record
`sys.boot_completed`; it cannot establish a successful boot. The current probe
therefore does not establish a working Android container. The next runtime
milestone is to implement Droidpc's own isolated mounts and lifecycle with
persistent data, then validate boot through `sys.boot_completed=1` and a
responsive package manager. Repeating this init probe alone cannot complete
that milestone.

The latest GPU-enabled run selects Mesa/GBM, but Mesa reports that it cannot
open a DRM device. SurfaceFlinger then aborts with `output buffer not gpu
writeable`. The probe originally exposed only `/dev/dri/renderD128`; the
current host's sysfs maps that node to `/dev/dri/card1`. The probe now matches
and passes the associated primary card. This fix still needs a run on the host
to verify that SurfaceFlinger remains up.

The plain probe process uses the host's `unconfined_t` SELinux domain;
Android's SELinux policy is not loaded into the shared host kernel. Init also
logs permission failures while setting ownership and mode on
`/proc/pressure/memory`. The default diagnostic wrapper temporarily runs
`lmkd` as Android root inside the user namespace.
The `console:/ $` prompt and “No controlling tty” warning come from the image's
debug console service; they do not mean `sys.boot_completed=1`.

The temporary SELinux domain lets Binder-dependent init commands run, and the
1 GiB memlock limit lets `bpfloader` start. BPF map creation still fails with
`Operation not permitted` in the user namespace. The latest bounded run
reached the 45-second timeout with `lmkd` fatal reboot suppression enabled.
ART started and accepted zygote socket connections, but
`sys.boot_completed=1` was not reached. PSI monitoring needs host-level
`CAP_SYS_RESOURCE` for Android's 1-second monitor window; the user-namespace
probe cannot provide it. SurfaceFlinger also crashes in this headless setup.
The temporary SELinux module is removed on exit. These settings diagnose boot
in this environment; they are not a production security configuration or a
complete boot fix.

Common output meanings:

| Output | Meaning |
| --- | --- |
| `INIT_EARLY_INIT_MISSING` | Init stopped before the custom early action. Check the first Android error in the log. |
| `INIT_TRIGGER_MISSING` | Init did not reach `on init`; inspect ueventd and coldboot logs. |
| `INIT_PROBE_STATUS=124` | The 120 second timer stopped the probe as designed. |
| `INIT_PROBE_STATUS=129` | Init exited during fatal shutdown; inspect the critical service failures in the log. |
| `INIT_PROBE_STATUS=0` | The command ended normally; this alone does not mean Android booted. |
| `TASK_PROFILE_ACTION_FAILED` | Init reported failure applying `HighPerformance`; inspect the first related log line. |
| `TASK_PROFILE_ACTION_NO_FAILURE_REPORTED` | No command failure was logged; this does not prove cgroup movement. |
| `CHARGER_TRIGGERED` | Init selected charger mode instead of normal startup. |
| `LATE_INIT_NOT_TRIGGERED` | The normal Android startup sequence has not started. |
| `ANDROID_BOOT_NOT_COMPLETED` | Android did not set its boot-complete property before timeout. |

When reporting a failure, copy the command and all output from the status
line through the Android log. Do not paste only the final linker warning;
look above it for the first failed action. The probe is designed to clean up
on normal exit and timeout. Afterward, the mount checks below can confirm
whether any image mounts remain.

```sh
findmnt -rn -t ext4 | grep droidpc-mount \
  || echo "No Droidpc mounts remain"
losetup -a | grep -E 'system.img|vendor.img' \
  || echo "No image loop devices remain"
```

These checks only inspect mount and loop-device listings. They do not alter
them. Do not unmount an entry unless you have confirmed it belongs to this
probe; never detach a loop device by guessing its number.

## 6. What we will build next, in order

1. **Reach Android boot completion.** Run the bounded boot probe and find
   the first fatal service or missing resource if the marker is absent.
   Keep images read-only and all mounts private.
2. **Make a safe guest cgroup layout.** Put Android init in a child group
   of Droidpc's own delegated hierarchy. Determine which Android task
   profiles need cgroup v2 replacements. Do not fake old control files.
3. **Complete device and process setup.** Provide the required Binder and
   graphics devices. Verify Android process placement and keep host sysfs,
   proc sysctls, cgroups and SELinux protected.
4. **Implement a small Rust lifecycle.** Verify pinned images, mount them
   read-only, own all temporary resources, launch a guest shell, stop it,
   and recover after a failed start. Add one piece at a time.
5. **Install one app.** Verify the package manager installs one x86_64 APK.
6. **Add a persistent `/data` directory.** Verify app state survives
   container restarts without changing the source images.
7. **Verify desktop display.** Validate the selected guest graphics path
   on this Intel GPU, then launch the installed APK.
8. **Add desktop features one at a time.** Windows, keyboard and mouse,
   audio, clipboard, networking and shared files each need a real test.
9. **Measure and optimize.** Record idle and active CPU, memory, GPU and
   power before choosing a change. Keep the controller event-driven and
   avoid extra daemons or polling unless measurements require them.

### What the next milestones will look like

| Milestone | Evidence that it is done |
| --- | --- |
| Init trigger | Probe prints `INIT_TRIGGER_OK`; its log and cleanup are recorded. |
| Guest cgroups | Android's actual process groups appear under Droidpc's delegated subtree, with real v2 controls. |
| Device startup | ueventd creates only guest devices; host devices and sysctls stay isolated. |
| Rust lifecycle | `start` and `stop` work; failed starts clean up mounts, loops and processes. |
| Android shell | A guest shell runs after init, properties and core services start. |
| Binder | Android processes can communicate over private Binder devices. |
| First app | An x86_64 APK installs and opens on the selected guest image. |
| Desktop display | The app is visible through a verified Linux graphics path. |
| Desktop workflows | Input, audio, clipboard and files work and have individual checks. |
| Optimization | Repeated measurements show a specific CPU or memory improvement. |

We will not call the container complete merely because init stays alive or
prints a success line. A usable milestone needs the feature, a repeatable
check, and cleanup after both success and failure.

Each step is finished only when its own test passes, cleanup is checked,
and the result is documented. The larger design and later feature phases
are in [ROADMAP.md](../ROADMAP.md). The more technical implementation
guide is [CONTAINER-IMPLEMENTATION-GUIDE.md](CONTAINER-IMPLEMENTATION-GUIDE.md).

## 7. How to work on the code

Use one small, readable change per commit. Keep code lines short, around
80 characters where practical. Prefer existing Rust and Linux facilities;
do not add a framework for a future feature. Before committing code, run:

```sh
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The repository's [Agents.md](../Agents.md) requires atomic Conventional
Commits. Do not stage `os_images/`, unrelated edits, or generated files.
When a probe fails, record the exact command, first meaningful error, and
whether temporary resources remained. Never “fix” an error by disabling
SELinux or changing host-wide cgroups.

**Current handoff:** run the command in section 5 once and report whether
`ANDROID_BOOT_COMPLETED` appears. If it does not, send the full output so
we can fix the earliest startup failure before adding more services.
