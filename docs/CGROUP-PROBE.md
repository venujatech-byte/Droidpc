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

## Guest cgroups: configuration work remains

The inspected images contain base cgroup and task-profile definitions,
plus API-specific files for levels 28, 29 and 30. The vendor advertises
first API level 28. Which definitions this patched build selects must be
verified; the presence of a file alone does not establish selection.

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
