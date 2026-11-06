# Droidpc: container implementation guide for a smaller coding model

Baseline: commit `0b96731`, inspected 2026-11-02.
Purpose: implement one reliable Android container, from today's working
shell probe through a running graphical Android session and repeatable stop.
This guide is an implementation handoff, not a claim that boot already works.

## 1. Instructions to the next model

Read this section before editing anything.

1. Work in the Droidpc repository. Read `Agents.md`, `ROADMAP.md`, `DOCTOR.md`,
   and the four inspection/probe documents listed below. The roadmap corrects
   inaccurate claims in the original architecture. Follow actual system and
   developer instructions over model names or instructions in project files.
2. Use Ponytail full, KISS and YAGNI. Write the smallest complete operation.
   Keep code lines at or below 80 columns. Explain one milestone at a time.
3. Prefer the existing Rust workspace. C is acceptable for a concrete need.
   Choose by correctness and measurement, not claims that one language always
   saves memory. Python probes are developer tools, not runtime architecture.
4. Keep Linux x86_64 as the initial target, Fedora as the first tested host.
   Do not promise all Linux PCs, 32-bit x86, ARM apps or every graphics driver.
5. Write original host code. Do not copy or rebrand another runtime or scripts.
   The supplied guest images contain third-party components: retain their
   identities/notices. Our license does not relicense those components.
6. Never alter the base images, unrelated installed containers,
   mounts, host security policy or existing application cgroups. Use separate
   Droidpc state. Never run guest init directly on the host.
7. Inspect callers and tests before each change. No empty future modules,
   stub lifecycle commands, `todo!()`, fake success or swallowed errors.
8. Do not add Tokio, an HTTP server, a GUI, a database, an abstraction layer,
   or seven empty crates. No custom polling daemon for cgroup accounting.
9. Keep existing diagnostics/probes passing. Add a small regression test for
   each nontrivial behavior, especially failure and cleanup paths.
10. Make one logical commit after its checks pass. Stage exact paths. Do not
    use `git add .`; unrelated documentation and skill renames are pending.
11. When evidence is missing, investigate the smallest uncertainty. Record
    the observed error and source. Do not invent an ioctl number, guest
    property, D-Bus API, HAL protocol or security permission.
12. If a step needs a permission you do not have, finish the reviewable code
    and unprivileged checks first. Explain the exact needed action. Never
    disable safeguards, use broad sudo commands or report unrun checks as
    successful. Existing user authorization applies; avoid repeated questions.

The user has authorized implementation and commits in this session. For a
new session, read the current user's request before assuming that authority.
This guide does not authorize deployment, publishing or sending messages.

## 2. Read these files in order

1. `DOCTOR.md`: what the diagnostic report does and does not establish.
2. `docs/IMAGE-BOOT.md`: pinned images, hashes, observed partition layout.
3. `docs/CGROUP-PROBE.md`: tested host delegation, guest cgroup uncertainties.
4. `docs/SHELL-PROBE.md`: the working four-file shell experiment and limits.
5. `crates/droidpc-cli/src/main.rs`: current command handling.
6. `crates/droidpc-cli/src/doctor.rs`: current diagnostics.
7. `scripts/probe-cgroups.py`, `scripts/probe-shell.py` and their tests.

The workspace now contains an experimental `droidpc-container` image
verifier. There is no production start/stop command or Android framework
boot. A read-only FUSE staging experiment is documented separately.

Do not reread huge images or hash them every turn. Use the recorded evidence
unless images change or a verification gate explicitly requires a fresh hash.

## 3. What is proven, and what is not

| Proven on this host | Not yet proven |
| --- | --- |
| BinderFS and required namespace handles visible | Private Binder devices work with this guest |
| Cgroup delegation and limit write/readback | Limit enforcement under workload; guest task profiles |
| GPU render node can be opened | Guest graphics/HAL/display compatibility |
| ZIP hashes match upstream release IDs | Android boot, security integration, VINTF compatibility |
| Image content matches its ZIP | Full filesystem integrity or bootability |
| Isolated Android shell and loader run | Full image staging and generated linker configuration |
| Shell failure status and normal cleanup | SIGKILL recovery and persistent Android data |

The shell probe prints a linker-configuration warning. It uses an explicit
library path for four extracted files. Do not promote that workaround into
full Android boot without verifying linkerconfig/APEX requirements.

## 4. Scope and definition of done

Two checkpoints must be distinguished:

**Headless container:** Android init and framework run in an isolated guest,
`sys.boot_completed` is `1`, package manager responds, logs are available,
and stop/restart preserves a test file in `/data`.

**Running Android milestone:** the headless checkpoint plus one visible
Android session on Wayland, working keyboard/pointer input and one selected
bytecode-only or x86_64 app. Networking resolves DNS and reaches a controlled
test endpoint. Startup, stop, failed startup and restart cleanup are tested.

Individual host windows per app, audio, clipboard, notifications, ARM
translation, cameras, game mapping and macros are later milestones. Do not
silently include them in this container task. State unsupported behavior.

## 5. Baseline commands

Run from the repository root:

```sh
git status --short
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s scripts/tests -v
cargo run -p droidpc-cli -- doctor --images os_images/selected
```

Run doctor and host probes in a normal user session when checking host
capabilities. Development sandboxes can hide devices and make cgroups
read-only. A sandbox failure does not establish a host failure.

The cgroup and shell probe commands are in their documents. Do not repeat
privileged/resource experiments for documentation-only edits.

## 6. File map: add only at the stage that needs it

These are responsibilities, not instructions to create empty files.

| File or directory | Stage and purpose |
| --- | --- |
| `crates/droidpc-container/Cargo.toml` | A: small Linux runtime library |
| `crates/droidpc-container/src/lib.rs` | A: image verification; later exports |
| `crates/droidpc-container/examples/verify.rs` | A: runnable verification gate |
| `crates/droidpc-container/src/images.rs` | B: move verification here if needed |
| `crates/droidpc-container/src/mounts.rs` | B: private mounts and loop ownership |
| `crates/droidpc-container/src/sandbox.rs` | C: namespace setup and child entry |
| `crates/droidpc-container/src/binder.rs` | D: private BinderFS allocation |
| `crates/droidpc-container/src/cgroups.rs` | E: owned delegated hierarchy |
| `crates/droidpc-container/src/guest.rs` | F: verified init launch setup |
| `crates/droidpc-container/src/lifecycle.rs` | F/G: supervision and stop/recovery |
| `crates/droidpc-container/src/network.rs` | H: owned guest networking |
| `crates/droidpc-cli/src/main.rs` | F/G: real start/status/stop command wiring |
| `guest-config/` | E/F: original, documented guest overlay configuration |
| `configs/systemd/` | F: reviewed unit template if required |
| `crates/droidpc-container/tests/` | Each stage: relevant automated tests |
| `docs/CONTAINER-PROGRESS.md` | Short evidence ledger, created when work starts |

Do not create a daemon merely to keep a process alive: a foreground manager
supervised by systemd is a valid first lifecycle implementation.

## 7. Stage A — add a verified image input boundary

Outcome: Rust opens and verifies the selected pair with bounded memory, and
retains the verified file handles. No mounts or root permissions.

Add `crates/droidpc-container` to root workspace members. Keep resolver `3`.
The code below is a tested starter, not already part of the application.
Use these exact blocks for this stage, then run the listed checks.

### File: `crates/droidpc-container/Cargo.toml`

<!-- starter: Cargo.toml -->
```toml
[package]
name = "droidpc-container"
version = "0.1.0"
edition = "2024"
license = "Apache-2.0"

[dependencies]
libc = "0.2"
sha2 = "0.10.9"
```

`sha2` avoids inventing cryptography. `libc` supplies the Linux nonblocking
open flag, avoiding a hang if an input path names a FIFO. Use Cargo.lock for
resolved versions; do not add architecture-specific compiler flags.

### File: `crates/droidpc-container/src/lib.rs`

<!-- starter: src/lib.rs -->
```rust
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub struct ImageSpec {
    pub name: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

pub const SYSTEM: ImageSpec = ImageSpec {
    name: "system.img",
    size: 1_827_717_120,
    sha256: concat!(
        "30c87ee44412fee4f50173716a6bb7c6941d",
        "3021853f0adab4a249da8a9455ab",
    ),
};

pub const VENDOR: ImageSpec = ImageSpec {
    name: "vendor.img",
    size: 562_388_992,
    sha256: concat!(
        "5cc4267517ded28f1cf60a5d2b6e8b88b15d",
        "8520de1f55a39de7764e7929a877",
    ),
};

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn verify_bytes(mut input: impl Read, spec: &ImageSpec) -> io::Result<()> {
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut count = 0_u64;
    loop {
        let length = match input.read(&mut buffer) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                continue;
            }
            result => result?,
        };
        if length == 0 {
            break;
        }
        count = count
            .checked_add(length as u64)
            .ok_or_else(|| invalid("image size overflow"))?;
        if count > spec.size {
            return Err(invalid("image is larger than expected"));
        }
        hash.update(&buffer[..length]);
    }
    if count != spec.size {
        return Err(invalid("image size mismatch"));
    }
    if format!("{:x}", hash.finalize()) != spec.sha256 {
        return Err(invalid("image SHA-256 mismatch"));
    }
    Ok(())
}

pub fn open_verified(directory: &Path, spec: &ImageSpec) -> io::Result<File> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(directory.join(spec.name))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() != spec.size {
        return Err(invalid("expected a regular image of the pinned size"));
    }
    verify_bytes(&mut file, spec)?;
    file.rewind()?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ABC: ImageSpec = ImageSpec {
        name: "fixture",
        size: 3,
        sha256: concat!(
            "ba7816bf8f01cfea414140de5dae2223",
            "b00361a396177a9cb410ff61f20015ad",
        ),
    };

    #[test]
    fn accepts_known_bytes_and_rejects_corruption_and_wrong_sizes() {
        assert!(verify_bytes(&b"abc"[..], &ABC).is_ok());
        for bytes in [&b"abd"[..], &b"ab"[..], &b"abcd"[..]] {
            let error = verify_bytes(bytes, &ABC).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        }
    }

    #[test]
    fn preserves_read_errors() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            }
        }
        assert_eq!(
            verify_bytes(Broken, &ABC).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }
}
```

### File: `crates/droidpc-container/examples/verify.rs`

<!-- starter: examples/verify.rs -->
```rust
use droidpc_container::{SYSTEM, VENDOR, open_verified};
use std::{env, io, path::PathBuf};

fn main() -> io::Result<()> {
    let mut args = env::args_os().skip(1);
    let directory = args.next().map(PathBuf::from);
    let Some(directory) = directory.filter(|_| args.next().is_none()) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: verify IMAGE_DIRECTORY",
        ));
    };
    let _system = open_verified(&directory, &SYSTEM)?;
    let _vendor = open_verified(&directory, &VENDOR)?;
    println!("Verified system and vendor; no mounts created");
    Ok(())
}
```

Run after adding this crate to the workspace:

```sh
cargo fmt --all
cargo test -p droidpc-container
cargo run --release -p droidpc-container --example verify -- \
  os_images/selected
```

Gate: both images match; wrong size/hash and read errors are rejected.
Use a release build for full-image hashing on this low-power host. Unit tests
can remain debug builds. This is not an Android performance benchmark.
Keep handles open through later loop setup; do not verify one pathname and
then reopen another pathname for use. Hash verification alone cannot prevent
another writer modifying the same inode afterward. Before privileged use,
provision images into an owned, protected location; verify that copy and use
its handle. Define this trust boundary before adding a root helper.

The selected symlinks are accepted for local development. This starter is
not a setuid program or a privileged arbitrary-path mounting interface.
Do not hash images on every app launch; verify at import/update boundaries
once protected image storage exists. Until then, verify before experiments.

## 8. Stage B — prove full read-only image staging

Read `docs/IMAGE-BOOT.md` first. This is the next resource-owning operation.

Implement `mounts.rs` only after a disposable mount experiment succeeds.
Use existing Linux utilities for the experiment; no permanent host mounts.
Do not assume that ext4 or loop setup is allowed inside an unprivileged user
namespace just because the shell probe used user namespaces successfully.
Decide the privilege boundary from actual results. A rootless FUSE experiment
can establish layout, but it does not prove the eventual kernel mount path.

Required layout inside a private mount namespace:

| Guest path | Source | Access/lifetime |
| --- | --- | --- |
| `/` including `/system` | system image root | Read-only base |
| `/vendor` | vendor image root | Read-only base |
| `/data` | separate Droidpc data directory | Persistent, never rollback-delete |
| `/dev`, `/run`, `/tmp` | private temporary filesystems | Per launch |
| `/proc` | guest PID namespace procfs | Restricted, private |
| `/sys` | minimum verified guest view | Host writes blocked |
| `/apex` and `/linkerconfig` | verified activation/generated state | Per launch |

Do not merge vendor and system as competing overlay lower directories. Use
separate narrowly scoped overlays only for files the selected guest needs
to change. A read-only ext filesystem may still attempt journal recovery:
verify safe read-only loop/mount options for this image and kernel.

Ownership rules for implementation:

- Start private mount propagation before any mount operation.
- Acquire each resource, record its handle/identity, then acquire the next.
- Keep loop backing read-only; never detach a loop device by guessed number.
- Prefer autoclear plus explicit cleanup; verify behavior with open handles.
- Roll back in reverse order on every failure. Report cleanup errors too.
- Keep an explicit cleanup method. Drop is best effort, not crash recovery.
- Do not delete `/data` when cleanup fails. Never use recursive deletion on
  an unverified path or remove a directory while it is still a mount point.
- Never use lazy unmount to hide a leak during normal completion.

Gate: staged files are visible at correct guest paths; writes to image
mounts fail; success and failure leave no owned mounts/loops. Inject failure
after system staging and after vendor staging. Record host mount state before
and after. Raw-image hashes must remain unchanged for this first experiment.

**Host experiment (2026-11-02):** read-only `fuse2fs` mounts with journal
recovery disabled exposed both images; a separate read-only `/vendor` bind
and `/apex/com.android.runtime` view let the guest shell run. Guest writes to
both image mounts failed. `fuse2fs` needed its `fakeroot` option for this
rootless check because Android properties include root-only files. All FUSE
processes and temporary directories were removed afterward. This proves a
read-only staging experiment and the guest path layout only. It does not prove
kernel loop mounts, init startup, graphics or a production mount owner. See
`docs/STAGING-PROBE.md` for the exact result and limitation.

## 9. Stage C — execute a shell from the staged images

Implement namespace/child setup in `sandbox.rs`. Reuse evidence from the
shell probe, but do not treat its four extracted files as the full rootfs.

**Host experiment (2026-11-02):** `/system/bin/sh` has now run with the full
read-only FUSE-backed system root, separate vendor view, and runtime APEX
symlink, inside Bubblewrap namespaces. See `docs/STAGING-PROBE.md`. The
linker still warns that `/linkerconfig/ld.config.txt` is absent. Do not
promote this shell run to an Android-init boot result.

- Make a private user/UID mapping decision appropriate to the privilege
  model. A single host-UID mapping cannot represent all Android service UIDs.
- Establish mount, PID, IPC, UTS and network isolation before guest execution.
- Check every namespace and mount syscall result; abort on partial setup.
- Enter the guest root correctly, change working directory to `/`, close
  inherited host descriptors and detach access to the old root.
- Mount procfs for the new PID namespace. Restrict host-sensitive proc/sysfs
  interfaces; a new PID namespace does not isolate every sysctl.
- First run only the fixed shell test, with no GPU/network/Binder access.
- Establish APEX/linker paths from the image layout. Do not use a blanket
  LD_LIBRARY_PATH workaround as evidence that Android service loading works.

Gate: the shell loads from full staged images, cannot see host home or alter
host settings, and failure/timeout reap all children and release resources.
This gate still does not start Android init.

## 10. Stage D — private Binder devices

Use kernel UAPI headers and [BinderFS documentation][binder]. Do not copy a
constant from memory, hardcode major/minor numbers or reuse other runtime devices.

Implement `binder.rs` to own a private BinderFS instance. Its initial
`binder-control` device is an allocator. Issue the real `BINDER_CTL_ADD`
ioctl for the guest's binder, hwbinder and vndbinder devices. Confirm returned
names/device nodes and expose only those nodes at the guest's expected paths.

Use a maintained ioctl binding or a small C shim if it makes ABI layout
safer. Check struct size/alignment and errno handling. No wholesale framework
is needed for three allocations. Preserve security checks and ownership.

Gate: allocate three devices in a disposable namespace, check partial failure
and cleanup, and demonstrate that another running Android instance is not
affected. No host Binder node deletion or global permission broadening.

## 11. Stage E — resolve guest cgroups, identity and properties

This is an investigation gate, not a ready-made boot configuration.
Use exact guest files and authoritative Android documentation.

Fill this table in the progress file before starting init:

| Question | Required evidence |
| --- | --- |
| Which cgroup JSON files are loaded? | Exact loader/version behavior or logs |
| Are API-28 overrides selected? | Verify, do not infer from filenames alone |
| Which task profiles are required during startup? | Guest services plus logs |
| How are v1 actions represented on v2? | Working semantics, not fake files |
| What is the guest `host` UID/GID? | Definition and a tested mapping |
| What does the vendor property file supply? | Exact read path/order and needed keys |
| Which init entry mode applies? | Verified selected-binary/source behavior |
| How are APEX/linkerconfig initialized? | Selected release's startup sequence |
| What security permissions are required? | Narrow policy and observed denials |

Host cgroup delegation is already demonstrated by our original probe.
Prefer a systemd-owned service with a delegated subtree. Respect the empty
inner-node rule and single-writer ownership. Keep manager and guest branches
separate. Guest access must stop at its assigned subtree.

Android supports vendor configuration overrides, but that does not prove
all existing profiles can be remapped by a tiny JSON file. Implement only
verified overrides in `guest-config/`, with a source/reason for every entry.
Test per-thread versus per-process placement, freezer use and memory handling.
Do not disable LMKD or make all profile actions no-ops to silence failures.

Do not assume `second_stage` from a binary string is the correct host launch
argument. Do not falsify verified-boot properties. Do not make SELinux
permissive to get past a denial. Identify the required guest/host boundary.

Gate: a reviewed launch contract exists, with no guessed mandatory values.
If the selected guest requires changes that cannot be configured safely,
record that fact and request a focused design review. Do not hide the blocker.

## 12. Stage F — supervised headless Android boot

Implement `guest.rs` and the needed parts of `lifecycle.rs`. Add a foreground
CLI `start` only when it performs a real operation. Keep doctor unchanged.

The foreground manager must:

1. Acquire an exclusive lock for this instance; reject a second start.
2. Verify/provision protected images and prepare owned state directories.
3. Enter the delegated resource arrangement and create isolated resources.
4. Start the correctly prepared guest init as PID 1 in its PID namespace.
5. Capture bounded logs without holding unlimited output in RAM.
6. Wait on child/process readiness events; no hot polling loop.
7. Check real Android readiness with a deadline and cancellation.
8. On error or stop, terminate/reap the guest and release owned resources.

Keep desktop-session credentials and sockets out of a privileged manager's
ambient environment. Use a trusted executable path for privileged units.
Do not install a root service that executes a user-writable repository binary.

Initial command design, to implement only as functionality becomes real:

```text
droidpc-cli doctor [--images DIR]
droidpc-cli start [--images DIR]
droidpc-cli status
droidpc-cli stop
```

Choose persistent data/state paths before implementing start. Do not add a
config framework: one instance and one documented image pair are enough.
Production `start` must not mount arbitrary user-selected data as root.
Provision trusted inputs through a separate validated boundary if needed.

Wire the new library into `crates/droidpc-cli/Cargo.toml` when first called:

```toml
droidpc-container = { path = "../droidpc-container" }
```

Place it under `[dependencies]`. Refactor argument parsing with tests for
missing paths, extra arguments, non-UTF-8 inputs and exit codes. Existing
CLI argument tests must be updated deliberately, not deleted.

Guest readiness observations, executed through the eventual isolated shell:

```sh
getprop sys.boot_completed
service check activity
service check package
pm list packages
```

These are guest commands, not commands to run against the host. A live init
PID alone is not readiness. `sys.boot_completed=1` plus responding framework
services is the headless checkpoint. Record boot timeout and service errors.

## 13. Stage G — stop, status and recovery before graphics

Implement one lifecycle path used by normal stop and startup rollback.

- Use process handles/pidfds or verified unit ownership. A stored PID alone
  is unsafe because it can be reused by an unrelated process.
- Request a verified guest shutdown mechanism, then use bounded termination
  and kill escalation if the deadline expires. Reap children.
- `status` must examine real unit/process state, not just a stale text file.
- Keep logs and persistent `/data`. Remove only ephemeral owned state.
- Recover after manager death using ownership evidence. Do not kill or
  unmount unrelated resources just because names look similar.
- Keep data-wipe/reset out of ordinary stop and out of this milestone.

Required tests on disposable Droidpc state:

1. Start/stop three times; resources do not accumulate.
2. Start twice; second start fails without disturbing the first.
3. Fail after a mount, after Binder setup and after guest spawn.
4. Cancel a slow boot; all owned processes and mounts disappear.
5. Create a guest `/data` test file; verify it after restart.
6. Kill the manager in a controlled test; recovery identifies its own state
   without acting on an unrelated PID, mount, device or cgroup.

Gate: cleanup and data preservation pass before a graphical session starts.

## 14. Stage H — owned networking

Implement only what the selected guest needs: an isolated interface,
addressing, routing and DNS. Use one documented mechanism appropriate for
the host. Read current NetworkManager/firewalld/nftables state first.

Never flush host firewall rules, edit global DNS or claim an existing bridge
as ours. Record and remove only our interface/rules. Subnet selection must
check for conflicts. Test DNS separately from raw IP reachability.

Do not expose insecure ADB on host or LAN. The image has development ADB
settings; use an isolated local control channel or keep ADB disabled until
its scope and authentication are established. Android properties must reflect
working networking; a property alone does not create connectivity.

Gate: guest DNS and a controlled network request work; stop restores the
owned network state without disrupting the host connection.

## 15. Stage I — one visible Android session

First establish the actual guest composer/allocator contract. The image has
vendor guest components; do not rename them or invent a replacement host
protocol. Determine needed properties, socket paths, credentials, formats
and synchronization from sources/observations and record them.

Add only required render-node and Wayland-session access. Keep graphical
session work unprivileged. Do not mount all of `/dev`, expose host input
devices indiscriminately, or run the compositor as root.

Start with one Android session/window. Test display, resize, focus,
keyboard and pointer with the chosen app. Log actual graphics errors.
A successful render-node open is not GPU rendering proof. Software rendering
can be an explicitly measured fallback, not a claim of low CPU performance.

Gate: the launcher and one selected app are visible and interactive, followed
by a successful stop/restart. Individual host windows remain a later feature.

## 16. Stage J — measure, then tune

Record versions and identical workload settings. Measure:

- Stopped: no leftover Droidpc processes or Android resources.
- Headless boot and graphical boot time.
- Android idle, one active app and hidden/minimized session.
- Whole guest cgroup memory, CPU time and process count.
- Host manager RSS/PSS separately; GPU memory where measurable.
- Repeated starts/stops and persistent growth.

Use cgroup `memory.current`, `memory.peak`, `memory.events`, `cpu.stat` and
`pids.current` where available. CPU utilization is a delta over a measured
interval. Explain whether percentages refer to one CPU or the entire machine.
Do not add up process RSS and call it unique physical RAM.

Do not apply the probe's 32 MiB/16-process limits to Android. Choose bounded
experimental limits from observed boot needs and available host memory, with
clear OOM diagnosis. Increasing limits is not an optimization; reducing limits
until Android thrashes is not an optimization either.

Optimize a measured cost one change at a time: idle wakeups, unnecessary
services/redraws, pixel copies, unbounded logs or needless workers. Preserve
app compatibility. Keep both zygotes until the chosen app set justifies a
64-bit-only configuration. Never promise a RAM/FPS target before measuring.

## 17. Failure diagnosis table

| Symptom | Next smallest useful check |
| --- | --- |
| Shell reports missing executable that exists | ELF interpreter and its APEX path |
| `printf` unavailable in shell probe | It is external here; use built-in echo |
| Mount denied | Namespace, capabilities, filesystem support and policy |
| BinderFS has only binder-control | Device allocation ioctl has not completed |
| Cgroup write denied | Delegation, mount read-only state and owned subtree |
| Cgroup enable returns busy | Processes in a domain-controller parent |
| Init loops/exits | Exact entry mode, first error, properties and mounts |
| Services fail changing UID | Guest account definition and UID/GID mappings |
| Task profile errors | Effective JSON definitions and actual v2 semantics |
| APEX or linker errors | Activation order, paths and generated linkerconfig |
| SELinux denial | Read denial; fix narrow policy, never disable enforcement |
| Black window | Guest composer, allocator, socket access and buffer support |
| Stop hangs | Owned process tree, open mount references and shutdown deadline |
| Host RAM looks unexpectedly high | Page cache versus guest PSS/cgroup accounting |

After two identical unsuccessful experiments, change the hypothesis or get a
focused review. Do not repeat privileged commands blindly. This is a debugging
rule, not a reason to abandon independent work or request redundant approval.

## 18. Per-stage work loop and handoff

For each stage:

1. State the one outcome, touched files and evidence still missing.
2. Read related code and verify required APIs against primary sources.
3. Implement only that stage; do not copy future skeletons into the repo.
4. Run focused tests plus workspace checks; inspect the diff.
5. Test the real operation in disposable owned state where required.
6. Record exact results, failures, cleanup and limitations.
7. Commit only completed, verified work. Never claim a gate passed from mocks.
8. Proceed to the next gate if authorized; otherwise explain the concrete
   blocker and continue any independent, safe work.

Use this format in `docs/CONTAINER-PROGRESS.md`:

```text
Current stage:
Last completed commit:
Verified image IDs:
Files changed:
Commands actually run:
Observed result:
Resources created and cleanup result:
Known blockers:
Next single action:
```

Final acceptance checklist:

- [ ] Stage A: protected, pinned image inputs and verified handles.
- [ ] B/C: full image staging, isolated shell and cleanup.
- [ ] D: private Binder devices, allocation and rollback.
- [ ] E: verified guest cgroup/identity/property/init contract.
- [ ] F: framework ready, package manager responds.
- [ ] G: stop, failure rollback, persistence and crash recovery.
- [ ] H: guest networking/DNS with owned cleanup.
- [ ] I: visible interactive Android session and selected app.
- [ ] J: measured CPU/RAM baseline and no persistent resource growth.

Do not tick all boxes because the shell prints a success marker.

### Validation of this guide's starter code

The three Stage A code blocks were extracted into a temporary standalone
crate, formatted, compiled and checked with Clippy. Both unit tests passed.
The example verified the actual selected system and vendor image sizes and
SHA-256 values in both debug and release builds. No images were modified.
All fenced code lines are at most 80 characters.

The starter is only in this document; the application workspace has not been
expanded. Later stages are implementation instructions with explicit gates,
not prevalidated full-container code. Re-run their checks as implemented.

## 19. Copyable prompt for the next model

> Read docs/CONTAINER-IMPLEMENTATION-GUIDE.md and its linked local documents.
> Inspect current Git status and progress before changing files. Follow
> Ponytail full, KISS/YAGNI and the user's current authorization. Start at the
> first unfinished gate; implement and verify one complete stage at a time,
> with atomic commits. Keep existing tests and unrelated edits intact. Use
> original host code and preserve third-party attribution. Never invent boot
> properties or weaken host security to pass a test. Continue through the
> container milestones while authorized, reporting measured results and
> concrete blockers. First action: validate Stage A against the current tree.

## 20. Primary references

Use references to understand interfaces, not to paste another runtime.
Recheck versions against the installed host and selected Android release.

- [BinderFS allocation][binder]
- [Linux cgroup v2][cgroup]
- [systemd delegation][delegation]
- [Android cgroup abstraction][android-cgroup]
- [Android 13 init language][init]
- [Linux mount namespaces][mount-ns]
- [Linux loop devices][loop]
- [RustCrypto SHA-2 API][sha2]

[binder]: https://docs.kernel.org/admin-guide/binderfs.html
[cgroup]: https://docs.kernel.org/admin-guide/cgroup-v2.html
[delegation]: https://systemd.io/CGROUP_DELEGATION/
[android-cgroup]: https://source.android.com/docs/core/perf/cgroups
[init]: https://android.googlesource.com/platform/system/core/+/refs/heads/android13-release/init/README.md
[mount-ns]: https://man7.org/linux/man-pages/man7/mount_namespaces.7.html
[loop]: https://man7.org/linux/man-pages/man4/loop.4.html
[sha2]: https://docs.rs/sha2/0.10.9/sha2/
