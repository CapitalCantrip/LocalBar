# ADR D-G — Tauri rewrite: abandon Swift, target cross-platform

**Status:** Accepted  
**Date:** 2026-09-14  
**Deciders:** Jay

The Swift/SwiftUI prototype demonstrated the UX and validated the driver architecture, but macOS-only reach is no longer acceptable: self-hosting power users increasingly run Linux, and Windows is a meaningful future market. We are rewriting LocalBar in **Tauri** (Rust core + web frontend).

The Rust core is the platform-agnostic heart — server lifecycle, model management, persistence, driver protocol — and serves both the GUI (via Tauri) and a CLI binary. The Swift app is feature-frozen immediately after three critical bug fixes (documented in `docs/review-2026-09-14.md`) and will not receive new features.

**Why Tauri over Electron:** Rust eliminates the memory overhead of a Node.js runtime; native system tray and process-management APIs are first-class; the binary is significantly smaller.

**Why Tauri over a pure web app / cloud tool:** LocalBar manages local processes on the user's machine. A browser page cannot spawn, monitor, or kill OS processes. Native is non-negotiable.

**Why not extend the Swift app:** SwiftUI is macOS/iOS only. Porting to Linux would require rewriting the entire UI layer anyway. Rewriting in Rust+web once is cheaper than maintaining two codebases.
