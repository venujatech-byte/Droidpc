---
name: droidpc-debug
description: Analyze Droidpc code, logs, desktop integration failures, and CPU or memory regressions while the owner applies fixes.
---

# Droidpc debugging

Follow the owner's coaching agreement in `ROADMAP.md`. Inspect code and
diagnostics; provide fixes for the owner to type. Do not silently edit source.
Keep Ponytail full active and code examples near 80 characters per line.

Read the failing path and every caller before suggesting a shared-code fix.
Identify whether failure belongs to the host, container setup, guest init,
Android framework/HAL, graphics protocol, or app. Explain evidence separately
from hypotheses; request only the next useful log or reproduction detail.

For lifecycle failures, inspect resource ownership, partial-start rollback,
persistent data, process reaping, and restart behavior. `Drop` is not crash
recovery. Never propose broad deletion or changing host security to hide a bug.

For graphics failures, inspect task mapping, buffer format/planes/modifiers,
descriptor lifetime, release/fences, configure acknowledgments, scale, and
input transforms. Do not assume a DMA-BUF descriptor alone proves zero-copy.

For input failures, check focus, coordinate spaces, touch slots, cancellation,
and release on disconnect before adding smoothing or higher polling rates.

For performance issues, require a reproducible workload and separate host
controller measurements from Android and GPU costs. Fix one measured cause;
avoid new threads, runtimes, caches, or dependencies without evidence.

Give the smallest complete correction and one meaningful regression check.
State what was actually checked, what remains unverified, and any risk.
