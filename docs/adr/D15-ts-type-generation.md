# ADR D15 — TS type generation with ts-rs

**Status:** Accepted
**Date:** 2026-09-27
**Deciders:** Jay

---

## Context

`localbar-tauri/ui/src/types.ts` was hand-maintained, starting with a comment claiming it "must stay in sync with localbar-core/src/types.rs." That comment was the only thing enforcing the claim; nothing checked it. The Rust↔React boundary has two parts: **data shapes** (structs like `ServerInstanceConfig`, hand-copied into `types.ts`) and **command calls** (`invoke('add_instance', { name, serverType, port, executablePath })`: 26 string-keyed calls in `ipc.ts` against 28 `#[tauri::command]` functions). Before this ADR, nothing checked either part at compile time.

Two crates generate TypeScript from Rust types: ts-rs and tauri-specta.

| | ts-rs | tauri-specta |
|---|---|---|
| Generates | Data shapes only | Data shapes **and** typed command wrappers |
| Mechanism | `#[derive(TS)]` on structs; running `cargo test` writes `.ts` files | `#[specta::specta]` on commands plus derives; the app writes `bindings.ts` with `commands.addInstance(…)` |
| Compile error on | A renamed or removed field | That, plus a renamed command, a wrong argument name or type, or a wrong return type |
| Replaces | `types.ts` | `types.ts` and most of `ipc.ts` |
| Maturity (2026-09-27) | Stable: 12.0.1, about 16M downloads | Tauri 2 line still `2.0.0-rc.25`; the only stable is 1.0.2, for Tauri 1 |
| Effort | Small | Medium: every command and every `ipc.ts` call site |

---

## Decision

Use **ts-rs** now. Migrating to tauri-specta is tracked in #46, to do once tauri-specta 2.0 is stable — that is the trigger for revisiting this decision. ts-rs avoids depending on a release-candidate crate; the command-call half of the Rust↔React boundary stays untyped until then.

`ts-rs` (12.0.1) is an optional dependency behind a `ts` Cargo feature in both `localbar-core` and `localbar-tauri`, so release builds of either crate stay free of the generation machinery. `#[cfg_attr(feature = "ts", derive(ts_rs::TS))]` and `#[cfg_attr(feature = "ts", ts(export))]` are added to every type mirrored in `types.ts` — `ServerType`, `ModelRef`, `ModelMetadata`, `ParamValue`, `ParamValues`, `ServerInstanceConfig`, `DiscoveryConfig` — plus the wire DTOs defined in `localbar-tauri` itself (`InstancePhaseDto`, `ErrorKindDto`, `DiscoveredModel`).

Two workspace-wide settings make the generated output deterministic and independent of where the export command is run, set in `.cargo/config.toml`:

- `TS_RS_EXPORT_DIR = { value = "localbar-tauri/ui/src/generated", relative = true }` — pins the output directory to one place regardless of the current working directory.
- `TS_RS_LARGE_INT = "number"` — ts-rs maps `i64`/`u64`/`i128`/`u128` to `bigint` by default, but `serde_json` serializes them as plain JSON numbers with no `bigint` involved anywhere on this boundary. This one setting makes every such field (`size_bytes`, `modified_secs`, and any future ones) come out as `number`, which is what the wire format actually is, without annotating individual fields.

The `uuid-impl` ts-rs feature maps `Uuid` fields (`id`, `active_profile_id`) to `string`, matching how `serde` serializes them. The default `serde-compat` feature (on by default in 12.x) reads `#[serde(tag = …, content = …, rename_all = …)]` and produces bindings that match what `serde` actually emits — verified against `ParamValue`'s `{ type, value }` tagged shape and `ServerType`'s kebab-case strings.

`types.ts` is now a thin file: it re-exports the generated types from `generated/` (so every existing import keeps working) and keeps the runtime helpers (`phaseLabel`, `phaseColor`, `isActive`) that have no Rust equivalent. The "must stay in sync" comment, and the header above it, are gone — the generated files are the sync mechanism now.

### Differences between the old hand-written types and the generated ones

- **`InstancePhase`'s `error` variant gained a `kind: ErrorKindDto` field.** The hand-written type only had `{ type: 'error'; message: string }`; the actual DTO Rust always sends (`InstancePhaseDto::Error { kind, message }`) includes the error kind. The hand-written type was simply incomplete. No frontend call site read `.kind`, so nothing broke; the extra field is now available.
- **`ParamValue` became a discriminated union of four shapes** (`{ type: 'double'; value: number } | { type: 'int'; value: number } | { type: 'string'; value: string } | { type: 'bool'; value: boolean }`) instead of one shape with `value: number | string | boolean`. This is what `serde`'s internally-tagged representation actually guarantees. `ParamEditor.tsx` constructs a `ParamValue` from a `kind: 'double' | 'int'` union rather than a literal, which the stricter type can no longer verify is exhaustively one member; that one construction site now asserts `as ParamValue`.
- **`ParamValues.values` became `{ [key in CanonicalParam]?: ParamValue }`** instead of `Record<string, ParamValue>`. `CanonicalParam` (`temperature`, `topP`, `topK`, …) was not previously exported to TypeScript at all; it is now a dependency of `ParamValues` and is exported alongside it. `ParamSchemaEntry.key` in `ipc.ts` (and `ParamEditor.tsx`'s `setField`) changed from `string` to `CanonicalParam` to match, since indexing the stricter map type with an arbitrary `string` no longer type-checks — this is also more accurate, since the key was always one of the canonical param names.
- **`ErrorKindDto` is now exported** as its own type; it did not exist in the hand-written `types.ts` at all.
- Every other mirrored type (`ServerType`, `ModelRef`, `ModelMetadata`, `ServerInstanceConfig`, `DiscoveryConfig`, `DiscoveredModel`) generated structurally identical to its hand-written counterpart and was replaced outright.

### CI

`.github/workflows/ci.yml` regenerates the bindings on the Linux job only (deterministic output, one OS is enough to catch drift) and fails if `git diff --exit-code localbar-tauri/ui/src/generated` is non-empty or if `git status --porcelain localbar-tauri/ui/src/generated` reports untracked files.

### D13 interaction

D13 bans comments in `.rs`, `.ts` and `.tsx` files. `ts-rs` writes a header comment ("// This file was generated by ts-rs…") into every file under `generated/`. This is a generated-file header, not agent-authored rationale, so `generated/` is excluded from the D13 policy. #43, which gates D13 in CI, is expected to skip `generated/` for the same reason.

---

## Consequences

- Running `cargo test -p localbar-core -p localbar-tauri --features ts export_bindings` from any directory regenerates `localbar-tauri/ui/src/generated/`; CI fails if that output doesn't match what's committed.
- The data-shape half of the Rust↔React boundary is now generated and checked; the command-call half (`ipc.ts`) is not, until #46 lands tauri-specta.
- `generated/` is exempt from D13's no-comments rule and from whatever enforces it in #43.
- A future field rename or type change in a mirrored Rust type will fail CI via the staleness check rather than silently drifting.
