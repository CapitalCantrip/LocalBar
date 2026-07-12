# ADR D-A — Ollama param application: Modelfile generation

**Status:** Accepted  
**Date:** 2026-07-12  
**Deciders:** Jay

---

## Context

Ollama's `paramSchema` declares sampling params with `application: .serverSideDefault`. The server process (`ollama serve`) accepts no per-param flags — params are baked into a Modelfile artifact. Without a strategy for writing those params, the param panel would be display-only for Ollama instances.

Three options were considered:

| Option | Mechanism | Verdict |
|---|---|---|
| A1 — Advisory only | Store params, display with badge, never apply | Rejected — param panel has no teeth |
| A2 — Modelfile generation | `ollama create` a LocalBar-managed model tag | **Accepted** |
| A3 — Proxy injection | LocalBar proxy injects params per-request | Rejected — adds network component, scope creep |

---

## Decision

LocalBar generates and manages an Ollama Modelfile for each (instance, model) pair.

### Managed model tag format

```
localbar/<modelName>-<instanceId>
```

Example: `localbar/llama3.1:8b-3F2A1B`

The `instanceId` suffix disambiguates when the same base model appears on multiple instances.

### Lifecycle

| Event | Action |
|---|---|
| User selects a model on an Ollama instance | `ollama create localbar/<modelName>-<instanceId>` with minimal Modelfile (`FROM <model>` only) |
| User saves params / profile | Regenerate Modelfile with param directives; rerun `ollama create` (idempotent) |
| User switches to a different model | `ollama rm` old tag; create new tag for the new model |
| User deletes the instance | `ollama rm` the managed tag |

### Modelfile content

```
FROM <baseModelTag>
[PARAMETER temperature 0.7]
[PARAMETER top_p 0.9]
[PARAMETER num_ctx 8192]
[SYSTEM "..."]
```

Only params with non-default values are emitted. `systemPrompt` maps to `SYSTEM`. Chat template deferred (see D-C-deferred).

### Storage

`ServerInstanceConfig` gains:

```swift
var managedModelTag: String?
```

Set after successful `ollama create`; cleared on `ollama rm`. `OllamaDriver.makeLaunchPlan` passes the managed tag (not the base model key) when set.

`OllamaDriver` gains a new method:

```swift
func createManagedModel(baseTag: String, instanceId: String, params: ParamValues, executablePath: String) async throws -> String
```

Returns the managed tag on success.

---

## Consequences

- Users see a `localbar/` namespace in `ollama list` — document in UI tooltip
- `ollama create` ~1 s on first call; near-instant on subsequent calls for same base model
- Stale managed models cleaned up on instance delete; no manual cleanup needed for the user
