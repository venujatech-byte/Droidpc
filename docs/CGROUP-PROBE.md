# Host cgroup delegation and guest boot findings

Checked 2026-11-02 on Fedora 44, systemd 259, kernel 7.2.13-200.fc44.x86_64.

## Host delegation: tested

The original developer probe in `scripts/probe-cgroups.py` verifies the
host's delegated cgroup v2 interface. It is not part of Droidpc's runtime
and adds no dependency to the Rust binary. Do not run it with sudo.

From the repository root, in a normal terminal outside a sandbox:

```sh
systemd-run --user --wait --pipe --collect \
  --unit=droidpc-cgroup-probe \
  --property='Delegate=cpu memory pids' \
  --property=MemoryMax=64M \
  --property=TasksMax=16 \
  --property=RuntimeMaxSec=15s \
  /usr/bin/python3 "$PWD/scripts/probe-cgroups.py"
```

The fixed unit name prevents concurrent probes from sharing the same unit.
The script refuses root execution, direct invocation outside that unit,
and a service with other processes. It requires the standard cgroup mount
at `/sys/fs/cgroup`, already established by doctor on this host.

The service's delegated root is kept free of processes: the probe moves
itself into a manager leaf before enabling controllers. It then creates
an empty guest leaf, writes and reads back:

| Control | Test value |
| --- | --- |
| `memory.max` | `33554432` (32 MiB) |
| `pids.max` | `16` |
| `cpu.max` | `20000 100000` (20% of one CPU) |

The probe removes the guest leaf. After the process exits, systemd removes
the remaining manager leaf and collects the service. The service has a
15-second maximum runtime so interrupted work is also bounded. No existing
application is moved or limited, and no Android process is started.

Observed result: all checks passed. A follow-up check found the service
inactive/not-found and no probe cgroup under the user's service hierarchy.
These are test values, not proposed Android resource budgets. The test
verifies configuration, not workload throttling, OOM behavior or PID-limit
failures. It is not an Android performance benchmark.

## Runtime design decision

For the first Fedora implementation, use systemd delegation for a subtree
owned by Droidpc, rather than writing to systemd-owned parent cgroups.
Keep a manager leaf separate from a guest subtree. The guest must not gain
write access to unrelated host cgroups. Other init systems need a separate
integration before we can claim support on them.

Host-level limits can constrain all Android descendants without a polling
loop or a custom resource daemon. This does not solve Android's internal
per-process and per-thread scheduling requirements.

This follows the [systemd delegation contract][delegation] and the kernel's
[cgroup v2 rules][cgroups]. It is not a copy of Android guest's host runtime.

The next runtime probe should start in its own delegated systemd unit. Move
the host supervisor into a `manager` leaf, then enable `cpu`, `memory` and
`pids` on the empty delegated parent. Create a separate empty `guest`
branch. Before starting Android, verify that the guest's cgroup namespace
is rooted at that branch and that Android init lives in a child leaf.
This placement is needed before the guest can enable domain controllers
for its own child groups. A writable cgroup v2 mount alone is insufficient.
Do not write to the current terminal scope or enable controllers there.

The placement check is in `scripts/probe-cgroup-placement.py`. Run it from
the repository root without sudo:

```sh
systemd-run --user --wait --pipe --collect \
  --unit=droidpc-cgroup-placement \
  --property='Delegate=cpu memory pids' \
  --property=RuntimeMaxSec=15s \
  /usr/bin/python3 "$PWD/scripts/probe-cgroup-placement.py"
```

It uses only that temporary delegated unit. A child enters a cgroup
namespace rooted at `guest`, moves to `guest/init`, and enables controllers
on the now-empty `guest` parent. The script removes its guest leaves;
systemd collects the unit. This checks placement mechanics without
starting Android or writing outside the assigned subtree.
Observed on this host: all three checks passed in 135 ms. The transient
service reported success and was collected. This proves the hierarchy
placement pattern; it does not yet prove Android's profiles or service
process placement.

## Guest cgroups: configuration work remains

The inspected images contain base cgroup and task-profile definitions,
plus API-specific files for levels 28, 29 and 30. The vendor advertises
first API level 28. Which definitions this patched build selects must be
verified for task profiles; the cgroup probe now confirms the API-28
`schedtune` descriptor was selected.

Read-only `debugfs` inspection of the selected images found:

- `/vendor/build.prop` sets `ro.product.first_api_level=28`.
- `/system/etc/cgroups.json` declares v1 `blkio`, `cpu`, `cpuset` and
  optional `memory`, plus a v2 freezer at `/sys/fs/cgroup`.
- `/system/etc/task_profiles/cgroups_28.json` adds non-optional v1
  `schedtune` at `/dev/stune`.
- `/system/etc/task_profiles/task_profiles_28.json` replaces several CPU
  performance profiles with `schedtune` actions.
- The selected vendor image has no `/etc/cgroups.json` or
  `/etc/task_profiles.json` override. Its `/android_guest.prop` contains only
  a comment, so it supplies no boot properties.

The PID 1 init probe attempted v1 `blkio`, `cpu`, `cpuset`, `memory` and
`schedtune` mounts. All failed. It mounted cgroup v2 at `/sys/fs/cgroup`.
After linkerconfig and APEX bootstrap, the guest shell read available
controllers as `cpu io memory pids`; none were enabled for child groups.
The probe keeps `/sys` private and does not expose host device events. The
image requests a 16 MiB uevent socket receive buffer; `SO_RCVBUFFORCE` fails
with `EPERM` in this sandbox, while a 256 KiB request succeeds. The probe
overlays a temporary copy of `ueventd.rc` with that smaller value.

**Init trigger and `/dev` result (2026-11-04):** with that temporary
override, the bounded probe printed `INIT_TRIGGER_OK`. Ueventd completed
coldboot on the private empty `/sys`, and init processed the custom `on init`
action. The `/dev` listing contained standard nodes and Android's
cgroup-related directories, but no Binder, DRM, input or sound nodes. This
is expected because the probe passes no host device tree. It proves the
ueventd and init-trigger path, not physical devices or a full Android boot.
See [the device inventory](DEVICE-NODES.md).

Read-only inspection of `/system/etc/cgroups.json` shows required v1
`blkio`, `cpu` and `cpuset` descriptors; v1 `memory` is marked optional.
Its v2 descriptor only declares the freezer at `/sys/fs/cgroup`. The
API-28 `cgroups_28.json` adds required v1 `schedtune`. The corresponding
API-28 task profiles move processes among `schedtune` groups and set
`schedtune.boost` and `schedtune.prefer_idle`. The guest has no vendor
cgroup or task-profile override. A v2-compatible override must preserve
process placement where possible; do not map `schedtune` onto a v2
controller because Linux cgroup v2 has no equivalent controller. First
verify how this image's process-group library applies v1 and v2 actions.
Then test the smallest viable override in the bounded init probe before
running Android's stock service sequence.
Do not enable them in the current terminal's cgroup. The runtime needs an
owned delegated parent with no processes before enabling domain controllers
for guest children. The earlier host systemd probe established that an
empty delegated parent can enable `cpu`, `memory` and `pids`; it did not
test Android's placement or `io` delegation.
This confirms the base and API-28 cgroup descriptors were selected in the
tested init path. It does not confirm which task-profile actions run later.
The host's cgroup v2 mount cannot provide `schedtune` behavior. Establish
which Android actions can be represented with v2 controls before changing
guest configuration.
The [AOSP descriptor loader][aosp-descriptors] and
[task-profile loader][aosp-profiles] read base, API-specific, then vendor
files in that order. This is evidence for the expected behavior, not yet
proof that the selected LineageOS binaries use those exact revisions.

The base configuration includes older `cpu`, `cpuset`, `blkio` and memory
paths, and a v2 freezer. Some task profiles also reference older schedtune
interfaces. Symlinking every old path to one v2 directory does not reproduce
these semantics. Empty fake control files would hide errors, not enforce
limits, and will not be used.

AOSP documents vendor overrides at `/vendor/etc/cgroups.json` and
`/vendor/etc/task_profiles.json`. The guest's setup library contains the
vendor cgroup path, and its process-group library contains the vendor task
profile path. These observations support investigating an overlay-based
configuration; they do not prove the exact load order of this patched build.

Before starting Android init, determine which mappings the guest actually
uses, which actions are required during startup, and whether safe vendor
overrides suffice. Preserve working process/thread placement and memory
management rather than disabling profiles to make logs quiet. Reference:
[AOSP cgroup abstraction][android-cgroups].

## Guest init and shell preflight

Additional read-only observations from the selected system image:

- `/system/bin/init` is x86-64 ELF, Android API 33, with interpreter
  `/system/bin/bootstrap/linker64`. That bootstrap linker and its libraries
  are present.
- `/system/bin/linker64` points to
  `/apex/com.android.runtime/bin/linker64`.
- APEX modules, including `com.android.runtime`, are unpacked directories
  under `/system/apex`. Init still needs to expose them at runtime paths.
- The init binary contains `second_stage`, `/vendor/android_guest.prop` and
  `android_guest.host.uid`. Strings are clues, not proof of argument behavior or
  property initialization order. The exact entry mode remains unverified.
- Guest graphics/audio services use a `host` account. Its UID/GID and
  permissions must be established before those services start.
- The selected zygote configuration starts 64-bit and 32-bit processes.
  Keep this compatibility initially; any memory-saving change requires a
  defined app set and measurements.

The isolated shell preflight now passes using four extracted guest files;
see [SHELL-PROBE.md](SHELL-PROBE.md). Full image mounting is still untested.
A shell preflight is not an Android boot. Do not execute guest init directly
on the host or expose writable host `/proc/sys`, `/sys` or debugfs trees.

Unresolved for Android init: exact entry mode, properties, UID mapping,
guest cgroup configuration, SELinux integration and restart recovery. No
Android startup code has been added until those details are established.

[delegation]: https://systemd.io/CGROUP_DELEGATION/
[cgroups]: https://docs.kernel.org/admin-guide/cgroup-v2.html
[android-cgroups]: https://source.android.com/docs/core/perf/cgroups
[aosp-descriptors]: https://android.googlesource.com/platform/system/core/%2B/1d02727c47b64cf377b1376d8c9ee4e840b2c526/libprocessgroup/util/util.cpp
[aosp-profiles]: https://android.googlesource.com/platform/system/core.git/%2B/b65b8835e509fc14407c8263da69abd3fb03e54f/libprocessgroup/task_profiles.cpp
