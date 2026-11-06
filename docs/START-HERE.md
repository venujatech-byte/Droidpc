# Droidpc: start here

This is the current beginner guide for Droidpc. It records what we have
actually tested on the developer's Fedora PC and what we will do next.
Read it before the older experiment logs in this directory.

**Current status (2026-11-06): Android can boot and render a captured screen
in a diagnostic probe.** The experimental window mode can show live frames
and preserve guest data, but still uses host capabilities. The Rust CLI only
has a read-only `doctor` command.

## 1. What are we building?

Droidpc aims to run Android apps on an x86_64 Linux desktop. Android user
space will share the Linux kernel but live in its own container. The host
controller is written in Rust. We are targeting Linux and Wayland, not
Windows or macOS. The first milestone is one Android session that boots,
stops cleanly, and runs an x86_64 app. Desktop windows, audio, game controls,
and optimization come later, after the base works.

The selected guest is **LineageOS 20 / Android 13 / x86_64**. The images
contain third-party guest components; Droidpc's host runtime is our own work.
Keep image attribution and licenses. Do not put the images in Git or change
them to fix a probe.

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
| Android init | A host-capability diagnostic reached boot completion; the captured lock screen confirms visible output. | Persistent session, interactive display, reliable lifecycle. |

The observed guest cgroup2 mount was writable and reported `cpu io memory
pids` as available. Its `cgroup.subtree_control` was empty. The selected
Android configuration also tried old cgroup v1 mounts; they failed on this
host. `SetupCgroups` printed “succeeded” despite those failures, so that
message is **not** proof that Android service groups work.

The Android startup probe is bounded to 180 seconds. `INIT_PROBE_STATUS=124`
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

## 5. Current Android boot and rendering status

The latest host-capability probe (2026-11-06) reached
`sys.boot_completed=1`; SurfaceFlinger, zygote, the graphics allocator and
HWC were running, and ActivityManager and PackageManager checks succeeded.
`INIT_PROBE_STATUS=124` is the outer 180-second timeout and occurred after
Android boot completion. An earlier run did not complete boot, so repeat
reliability remains to be established. The debug console's no-controlling-TTY
warning is expected.

In the latest run, Android `system` (UID 1000), launcher (10102) and SystemUI
(10108) each opened `/dev/dri/card1` and `/dev/dri/renderD128`. This verifies
device access. The captured screenshot from the subsequent probe shows the
Android lock screen with status and navigation bars, confirming visible frame
presentation. The guest saw
`cpu io memory pids` cgroup controllers, none enabled for children, and PID 1
remained at `0::/`; cgroup delegation and process placement remain unresolved.
Host SELinux AVCs for the temporary probe domain were `permissive=1`, so this
run does not validate an enforcing runtime policy.

This opt-in diagnostic runs init in the host user namespace while keeping
mount, PID, IPC, UTS, network, and cgroup namespaces private. It drops host
reboot, module-loading, raw-I/O, time-setting, and ptrace capabilities, while
retaining `CAP_SYS_ADMIN` and `CAP_SYS_RESOURCE`. It can affect host kernel
state; use it only for diagnostics on a disposable host:

```sh
sudo env DROIDPC_PROBE_HOST_CAPS=1 ./scripts/probe-android-init-selinux.sh
```

Build the Rust desktop launcher and GPU window as your regular user:

```sh
cargo build --release -j 1 -p droidpc-cli -p droidpc-window
./target/release/droidpc-cli launch
```

Run `launch` from the repository root. It starts the privileged diagnostic
through sudo and uses the existing persistent Android data. The viewer itself
runs as your desktop user. OpenGL handles scaling and presentation with vertical
sync. Raw pixels upload directly into a reusable OpenGL texture without making
an egui CPU image. The confirmed snapshot source remains the default. Enable
the continuous display source to avoid starting a capture process for every
frame:

```sh
DROIDPC_CAPTURE_SOURCE=display ./target/release/droidpc-cli launch
```

This uses an uncompressed virtual-display stream over a private FIFO, without
PNG encoding, image files, or video codecs. RGB pixels stay packed during host
upload. Only one pending frame is retained; old frames
are replaced rather than queued. The window title displays the observed rate.

Capture targets 60 frames per second by default. For a faster display, use
`DROIDPC_WINDOW_FPS=144 ./target/release/droidpc-cli launch` (valid range 1–240).
This is a capture ceiling, not a promise of that rate: Android capture readback,
resolution, CPU and GPU determine the actual result. The continuous producer
waits for new display frames rather than spawning a process for each capture.
Unchanged pixels skip GPU uploads. If continuous capture fails between complete
packets, it falls back to snapshots and logs the reason. Snapshot mode slows to
four samples per second after an unchanged second; input wakes capture.
This path still reads GPU pixels through CPU memory; it is not DMA-BUF sharing.
To select the confirmed raw snapshot source explicitly:

```sh
DROIDPC_CAPTURE_SOURCE=snapshot ./target/release/droidpc-cli launch
```

Clicks become taps, drags become swipes, the wheel scrolls, and right click or
Escape goes back. Basic keys and printable ASCII text are supported; arbitrary
Unicode input is not implemented. Android soft input methods remain disabled.
The guest graphics composer uses the host Wayland socket. Closing the viewer
or pressing Ctrl+C stops the guest. Data remains under
`~/.local/share/droidpc-probe/data`. Startup prints viewer and capture log paths;
the window shows a waiting message or capture error instead of an empty black
screen. Capture starts when SurfaceFlinger runs, so boot animation and startup
frames can appear before Android completes boot. This remains the host-capability diagnostic, not the production runtime.

The previous screenshot viewer is available for recovery:

```sh
sudo env DROIDPC_PROBE_HOST_CAPS=1 DROIDPC_PROBE_WINDOW=1 \
  DROIDPC_WINDOW_BACKEND=screenshots ./scripts/probe-android-init-selinux.sh
```

This fallback needs Python GTK 3 bindings and captures approximately twice per
second. The Rust window needs a working OpenGL driver and Wayland desktop; it
fails explicitly if GPU window creation fails.

The run repeatedly logged `incidentd` exits because it requests
`CAP_SYS_PTRACE`, which this diagnostic intentionally drops. BPF setup also
reports missing guest bpffs paths. Android nevertheless reached boot
completion. The probe writes an
initial boot/service snapshot immediately, then records another after waiting
up to 90 seconds for boot completion. This preserves framework evidence if
the outer 180-second timeout stops the run. After boot completion it attempts
`screencap`; a successful image is copied to a mode-0600 `/tmp/droidpc-screen.*.png`
file and its path is printed. A captured lock screen now confirms visible frame
presentation. The probe checks
ActivityManager and package services without dumping unbounded SystemServer
stacks. If boot remains incomplete and SystemServer is running, it captures a
backtrace with Android's `debuggerd -b`; the host output is capped at 160
lines.

The host DRM node maps to `card1`; the probe passes that card and a private
sysfs discovery snapshot alongside `renderD128`. The guest SELinux policy is
not loaded into the shared host kernel. The temporary host diagnostic domain
is permissive and removed after the run; it is not a production SELinux
configuration. Boot completion, framework service checks, and visible frame
presentation now pass in supplied runs. Reliable repeated boots, enforcing
SELinux, cgroup delegation, and a production start/stop lifecycle remain
required before calling this a working container.

If Android stalls while services repeatedly wait for SurfaceFlinger, leave the
window open and collect the graphics startup messages and native stacks:

```sh
sudo ./scripts/diagnose-android-graphics.sh
```

The command identifies the guest by its Droidpc helper file, excludes repeated
service lookups, and bounds each native stack request to 15 seconds. It does
not restart graphics services or alter Android data. A running process alone
does not establish that SurfaceFlinger has registered its Binder service.

Common output meanings:

| Output | Meaning |
| --- | --- |
| `INIT_EARLY_INIT_MISSING` | Init stopped before the custom early action. Check the first Android error in the log. |
| `INIT_TRIGGER_MISSING` | Init did not reach `on init`; inspect ueventd and coldboot logs. |
| `INIT_PROBE_STATUS=124` | The 180 second timer stopped the probe as designed. |
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

1. **Repeat Android boot completion.** The latest bounded probe reached the
   marker; repeat it and confirm guest resources clean up afterward. Keep
   images read-only and all mounts private.
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

**Current handoff:** the latest probe reached `sys.boot_completed=1`, framework
services responded, all three tested Android UIDs opened the DRM nodes, and a
captured lock screen confirms visible output. Next, repeat boot and cleanup,
validate the Rust stage/BinderFS implementations, and resolve enforcing
SELinux and cgroup delegation before production lifecycle work.

### Input, display sizing and devices

The Rust window opens maximized and reports its client size and display DPI
before Android starts. The guest composer uses that size rather than enlarging
a fixed low resolution image. Later window resizing scales the existing output;
restart on another monitor to recalibrate resolution and density.

The Rust window sends native Android wheel events through a persistent Android
input bridge. Mouse notches target about three text lines, while touchpads retain
fractional horizontal and vertical movement; events coalesce at up to 60 Hz.
Scrolling does not synthesize touch swipes or flings. Building the bridge requires
a JDK and Android SDK platform/build-tools (`ANDROID_HOME`, or `~/Android/Sdk`);
`scripts/build-android-input.sh` produces `target/droidpc-input.jar`. Startup builds
it if missing or older than its source. Runtime injection errors go to
`~/.local/share/droidpc-probe/data/droidpc-input.log`.
Camera nodes use Android's camera group (1006), matching the external provider.
Desktop sessions require `pasta` from the `passt` package for a private Ethernet
link and host DNS forwarding. Its helper retains UID 0 to enter the root-owned
guest network namespace, then applies its own capability and syscall sandbox.
The launcher pins the namespace with a private bind mount before starting the
helper, avoiding restricted process namespace paths; cleanup releases the mount.
Android also receives live, read-only network interface metadata and writable
network sysctls mounted from inside its own network namespace. Global host kernel
sysctls remain hidden.
With the SELinux wrapper, the helper enters the same temporary diagnostic domain
as Android so the host helper policy does not block namespace entry. Set `DROIDPC_PROBE_NETWORK=0` for offline operation.
No host routes or inbound port forwards are created. Networking logs are stored
in the printed temporary log directory as `network.log`.

After restart, use `sudo ./scripts/diagnose-android-graphics.sh --devices` once
Android has finished booting to check framework networking and camera services.
An Ethernet transport alone does not verify Android DNS, network validation or
camera capture; those require a running guest check.

### Verify an Android app can use internet and camera

After Android finishes booting, run:

```sh
sudo ./scripts/diagnose-android-graphics.sh --verify
```

This builds and installs a small diagnostic APK with Internet and camera
permissions, briefly opens it, and checks HTTPS through Android's active network.
It then opens a camera through Camera2 and requests one JPEG frame. The frame is
checked and discarded; no photo is saved. The utility closes after the checks,
and the helper stops it if the check times out. Results are written to
`~/.local/share/droidpc-probe/data/droidpc-device-check.log`.
`HTTPS_OK` and `CAMERA_FRAME_OK` are the success markers. Merely seeing `eth0` or
a running camera provider is insufficient. Failure includes network properties,
camera IDs, and provider initialization logs. External camera IDs may not be
supported by apps that require front/back phone cameras; the check reports facing.

The desktop transport uses the private IPv4 subnet `10.0.2.0/24`, guest address
`10.0.2.15`, gateway `10.0.2.2`, and DNS proxy `10.0.2.3`. DHCP advertises these
values rather than inheriting addresses or a missing gateway from a host VPN.
The helper follows host outbound connectivity without changing host routes.
Camera and network startup messages are preserved separately in the bounded
`droidpc-devices.log` files so unrelated framework logs cannot immediately erase
the initialization failure.

The host verification on 2026-11-06 confirmed an Android app received a validated
Ethernet default network, the expected DHCP gateway/DNS, and `HTTPS_OK 204`.
Camera initially reported `Permission denied` opening the passed video nodes,
even for guest root with correct ownership. The nodes inherited `nodev` from the
host temporary directory. Camera nodes now use a dedicated tmpfs mounted with
`dev,nosuid,noexec`, preserving `root:camera` ownership and `0660` permissions.
The mount is private to the launcher and released during cleanup. Host verification
confirmed the new camera mounts omit `nodev` and guest root can open both video
nodes. Run `--verify` to check HAL registration and actual capture. `--camera-access`
prints provider credentials, device permissions and device mount flags.
