# ADR D-I — Windows support via CLI-first, not GUI-first

**Status:** Accepted  
**Date:** 2026-09-14  
**Deciders:** Jay

When LocalBar expands to Windows, the first deliverable is a **CLI binary**, not a Tauri GUI. The Rust core is platform-agnostic; the CLI is a thin shell around it. This means Windows users get real functionality (full server lifecycle management via commands) before we invest in a Windows GUI port.

**Why CLI first:** Validating Windows demand is cheap this way — the CLI binary costs almost nothing once the Rust core exists. A full Windows GUI (system tray, native window chrome, installer) is a meaningful effort that should only happen once there is evidence of Windows demand.

**Why not just wait:** A CLI tool on Windows also serves automation and scripting use cases (CI, headless servers, dotfiles) that a GUI never would. It's genuinely useful to a segment of Windows users regardless of whether a GUI follows.
