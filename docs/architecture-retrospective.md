# Architecture Retrospective — LocalBar v0.1

**Date:** 2026-07-12

This document captures lessons from the first development cycle, specifically around the gap between early architectural assumptions and the reality discovered during implementation and testing.

---

## What went well

- The `ServerDriver` protocol and `InstancePhase` state machine proved solid. No significant rework was needed to the core abstractions once they were established.
- `@Observable` + `@MainActor` kept the concurrency model straightforward. Most bugs were at the edges (autoclosure + async, adoption timing) rather than in the state machine itself.
- ADRs D1–D5 (written early) held up. The decisions they captured were not revisited.

---

## What should have been designed earlier

### 1. Process ownership spectrum

The original design assumed a binary: LocalBar either owns a process (spawned it, holds `Process`) or doesn't know about it. Reality has a third state — **adopted**: a process LocalBar did not spawn but is now managing. This required retrofitting `adoptedPID`, `adoptIfRunning()`, `findListeningPID()`, and a modified `performStop()` path.

Had we modelled "externally-managed server" as a first-class concept from the start, several rework cycles would have been avoided. A proper early design would have asked: *who can start this server, and what does LocalBar own in each case?*

### 2. The launch environment problem

The assumption that a single executable path (python3 or uvx) would "just work" ignored the diversity of Python environments on a developer machine. The actual failure chain was:

- PathScanner finds Xcode Python → no mlx-lm installed → crash
- Switch to uvx → wrong arg format (`uvx mlx-lm server` vs `uvx --from mlx-lm mlx_lm.server`) → crash
- uvx version may differ from user's existing mlx-lm install

This entire class of problems could have been anticipated with a one-question design session: *how does the user's existing mlx-lm actually get launched, and can we replicate that exactly?* The answer (an external switch script uses a plain `python` binary from a specific venv, not uvx) reveals that LocalBar's auto-detection will always be approximate. The correct design is to make the executable path and launch arguments fully user-configurable with good defaults, not to make detection do all the work.

### 3. Startup error visibility

The `handleUnexpectedTermination` guard (`guard !phase.isTransitioning`) was correct for the running → crashed case but wrong for the starting → crashed case. The effect was that any process that died during startup produced a 120-second silent wait followed by a generic timeout error, hiding the actual stderr. This class of bug (crash during a transitioning phase is swallowed) should have been caught in a design review of the state machine's error paths.

### 4. Settings window lifecycle

`Cmd+Tab` presence, window resizability, and scan path persistence were all discovered as missing during testing rather than designed in. These are standard macOS windowed-app properties that a pre-implementation checklist would have caught.

---

## Recommended practice for future features

Before writing code for any non-trivial feature:

1. **Write a one-page design note** covering: what states can this thing be in, who owns it, what are the error paths, and what does the user see in each case.
2. **Ask "what already exists on the user's machine that we need to coexist with?"** — especially for process management, file paths, and network ports.
3. **Add the feature to the ADR list** even if the decision seems obvious. Obvious decisions are the ones most likely to be invisibly revisited later.
4. **Design the error path before the happy path.** The happy path is easy; the question is what the user sees when each step fails.
