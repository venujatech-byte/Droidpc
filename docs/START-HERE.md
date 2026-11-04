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
| Android init | Second-stage init ran as PID 1, reached `early-init`, and ran a guest shell. | Home screen or app launch. |

The observed guest cgroup2 mount was writable and reported `cpu io memory
pids` as available. Its `cgroup.subtree_control` was empty. The selected
Android configuration also tried old cgroup v1 mounts; they failed on this
host. `SetupCgroups` printed “succeeded” despite those failures, so that
message is **not** proof that Android service groups work.

The init probe deliberately stops after about 12 seconds. Its
`INIT_PROBE_STATUS=124` means the wrapper timed out; it is expected for the
bounded probe, but it is **not** a boot success result. `INIT_EARLY_INIT_OK`
means only that the custom early action ran. A warning from the linker
before linkerconfig is generated is expected in this probe.

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
[CGROUP-PROBE.md](CGROUP-PROBE.md). Older “next step” paragraphs in those
logs describe what was next *at that time*; follow this file for the current
order.

## 5. The one new check to run now

The newest init probe preloads a coldboot marker in private `/dev` **only
inside the test** and checks whether Android's `on init` action runs. Normal Android boot must
start `ueventd` and wait for real device discovery. This test cannot replace
that work.

Run this complete command from the repository root:

```sh
sudo ./scripts/probe-android-init.sh
```

Enter your sudo password at your own terminal. The coding agent cannot enter
it. Look for these lines:

```text
INIT_PROBE_STATUS=124
INIT_EARLY_INIT_OK
INIT_TRIGGER_OK
```

If `INIT_TRIGGER_OK` is missing, send the full output so the first new error
can be fixed. If it appears, the next checkpoint is to inspect the `on init`
work and design real device discovery. Do not keep rerunning this same probe
after it passes; each later checkpoint will have its own focused test.

Common output meanings:

| Output | Meaning |
| --- | --- |
| `INIT_EARLY_INIT_MISSING` | Init stopped before the custom early action. Check the first Android error in the log. |
| `INIT_TRIGGER_MISSING` | Init reached early-init but did not run the later `init` trigger. |
| `INIT_PROBE_STATUS=124` | The 12 second timer stopped the probe as designed. |
| `INIT_PROBE_STATUS=129` | Init or a child ended from signal 1; inspect the log for its earlier fatal error. |
| `INIT_PROBE_STATUS=0` | The command ended normally; this alone does not mean Android booted. |

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

1. **Finish the bounded init check.** Confirm `INIT_TRIGGER_OK`. Keep the
   system and vendor images read-only and all test mounts private.
2. **Make a safe guest cgroup layout.** Put Android init in a child group
   of Droidpc's own delegated hierarchy. Determine which Android task
   profiles need cgroup v2 replacements. Do not fake old control files.
3. **Provide real startup prerequisites.** Start `ueventd`, create only the
   required private devices, and verify Android's next startup actions.
   Keep host `/proc/sys`, `/sys`, cgroups and SELinux protected.
4. **Implement a small Rust lifecycle.** Verify pinned images, mount them
   read-only, own all temporary resources, launch a guest shell, stop it,
   and recover after a failed start. Add one piece at a time.
5. **Allocate private Binder devices.** Prove Binder works inside the
   guest without taking devices from another Android instance.
6. **Reach a basic Android boot.** Add persistent `/data`, required
   properties and services; check logs and a working Android shell.
7. **Add one display and one app.** Validate the selected guest graphics
   path on this Intel GPU, then install and launch one x86_64 APK.
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
`INIT_TRIGGER_OK` appears. Then continue with step 2 above.
