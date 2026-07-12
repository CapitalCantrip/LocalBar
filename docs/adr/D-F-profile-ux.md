# ADR D-F — Profile UX location

**Status:** Accepted  
**Date:** 2026-07-12  
**Deciders:** Jay

---

## Context

`NamedProfile` is a global entity (shared across instances, optionally scoped to a `ServerType`). `ServerInstanceConfig.activeProfileID` references the active profile per instance. There's a design question about where profiles are created, edited, and deleted.

---

## Decision

### Milestone B: Separate "Profiles" tab (F2)

Profiles are a first-class object with their own tab in Settings, using the same HSplitView pattern as the Servers tab (list on left, detail on right).

- **Profile list:** name, server type badge (or "Any"), last modified
- **Profile detail:** full param panel (same controls as instance detail), name field, server type filter picker, Delete button

`InstanceDetailPanel` gains a **Profile** picker dropdown above the param panel:
- Options: "None (auto-memory)", then all compatible profiles
- Selecting a profile activates it on the instance (`activeProfileID`) and shows param values as overridden (greyed out / locked) with "From profile: <name>" label

### Milestone C later: Quick "Save as profile" (F3)

Add an inline "Save as profile…" button in `InstanceDetailPanel` that opens a name-entry popover and creates a new profile from the current param values. Deferred to Milestone C.

---

## Consequences

- Settings gains a third tab: Servers | Profiles | General
- No change to `NamedProfile` data model
- Profile tab is a new SwiftUI view (`ProfilesTab`) following the same structural pattern as `ServersTab`
- Instance detail panel gets a picker and readonly-overlay mode when a profile is active
