# ADR D18 — Showing the version and checking for new releases

**Status:** Accepted
**Date:** 2026-10-04
**Deciders:** Jay

---

## Context

LocalBar showed its version nowhere, so a tester couldn't tell v0.3.1 from v0.3.2 without Finder's Get Info (#97). There was also no way to learn that a newer release exists.

## Decision

### Version display

The running version is read at runtime from Tauri's package info (`app.package_info().version`), which the release workflow syncs from `localbar-tauri/Cargo.toml`. It is never hard-coded. It appears as "LocalBar 0.3.2" in the tray tooltip, the popover footer, and the bottom of Settings → General.

### Update check: notify only

LocalBar checks `GET https://api.github.com/repos/CapitalCantrip/LocalBar/releases/latest` and, when the release is newer, shows "v0.3.3 is available" with a link that opens the release page through `open_url`. It never downloads or installs anything. Tauri's updater was rejected because the build is unsigned (#93): an auto-installed bundle would still be blocked by Gatekeeper, so the user has to download and approve it by hand anyway.

### Cadence

- The first check runs shortly after startup, then the app re-evaluates every hour whether a check is due.
- A check is due when none has succeeded in the last 24 hours (`update_check::is_check_due`), or when the clock has moved backwards. The time of the last successful check is stored in `state.json` (`last_update_check_secs`), so restarting does not trigger another request within 24 hours.
- A failed check is logged to stderr and otherwise silent; it does not record a time, so the next hourly evaluation retries.
- **Check now** in Settings → General runs a check on demand and reports "You're up to date", the new version, or "Couldn't check".

### Opt-out, default on

`AppSettings.check_for_updates` defaults to `true` (`serde(default = "default_true")`, the same pattern as #82's `restore_running_servers_on_launch` in D3), so existing `state.json` files load with checking on. The General tab checkbox reads "Check GitHub for new versions once a day". When it is off, no scheduled request is made and no notice is shown; **Check now** still works because the user asked for it.

### Privacy

Each check is one anonymous HTTPS GET to api.github.com with a `User-Agent: LocalBar/<version>` header and a 10-second timeout. No identifier, instance, model or usage data is sent. GitHub sees the requesting IP address, as with any request.

### Where the code lives (D12)

Version parsing and comparison, the due-check, release-JSON parsing (`tag_name`, `html_url`) and the outcome mapping are pure functions in `localbar-core::update_check`, tested with an injected fetch. The HTTP call, the timer and the `update-status-changed` event live in `localbar-tauri/src/update.rs`; the request runs on a blocking worker, never on the main thread and never while the registry lock is held.

Comparison is semver-like: an optional `v` prefix, up to three numeric parts (missing parts count as zero), and a pre-release (`-beta.1`) is never reported as newer. `releases/latest` already excludes drafts and pre-releases.

## Consequences

- A newer release is noticed within about a day of publishing, or immediately with **Check now**.
- If the app is ever signed, the notice could be replaced by Tauri's updater; that would need a new ADR.
