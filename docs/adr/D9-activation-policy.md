# ADR D9 — NSApplication activation policy: accessory vs regular

**Status:** Accepted  
**Date:** 2026-07-12  
**Deciders:** Jay

---

## Context

LocalBar uses `LSUIElement: YES` in Info.plist, which prevents a Dock icon and keeps the app out of Cmd+Tab by default. This is correct for a background menu bar utility — the user interacts via the menu bar popover, not via a standard app window.

However, when the Settings window is open, the app behaves like a regular windowed application. Without a Dock icon or Cmd+Tab presence, the window can be lost behind other apps with no obvious way to return to it. The user must click the menu bar icon again to bring it forward, which is unexpected for a settings window.

**Options considered:**

1. **Keep `.accessory` always** — simpler, but Settings window is effectively a second-class citizen. The user cannot Cmd+Tab to it.
2. **Switch to `.regular` always** — gives a permanent Dock icon, which contradicts the menu-bar-utility design.
3. **Switch to `.regular` while Settings is open; revert to `.accessory` on close** — dynamic policy matches user expectation: a settings window behaves like an app window while visible.

---

## Decision

**Dynamic activation policy**: `SettingsView.onAppear` calls `NSApp.setActivationPolicy(.regular)` and `onDisappear` reverts to `.accessory`.

This gives the Settings window a Dock icon and Cmd+Tab entry for its lifetime. When it closes, LocalBar vanishes from both, matching the menu-bar-utility mental model.

---

## Consequences

- The Dock icon appears briefly when Settings opens and disappears when it closes. This is slightly unusual but acceptable and used by other menu bar utilities (e.g. Bartender).
- `NSApp.setActivationPolicy` is not async-safe; calling it from SwiftUI `.onAppear` on the MainActor is correct.
- If the user opens multiple windows from LocalBar in future (e.g. a model detail window), the policy must remain `.regular` until all such windows are closed. A reference-count approach or window observation will be needed at that point.
- There is a brief flash (the Dock icon materialises) when Settings opens. This is a known quirk of dynamic policy switching on macOS and is not fixable without a more complex workaround.
