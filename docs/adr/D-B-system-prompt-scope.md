# ADR D-B — System prompt scope

**Status:** Accepted  
**Date:** 2026-07-12  
**Deciders:** Jay

---

## Context

`ParamValues.systemPrompt` and `CanonicalParam.systemPrompt` exist in the data model. The question is whether LocalBar *applies* the system prompt or only *stores* it.

The answer depends on the server type:
- **Ollama:** Modelfile supports a `SYSTEM` directive — LocalBar can inject it via the managed Modelfile (D-A).
- **mlx-lm:** No `--system-prompt` launch flag exists. The system prompt is per-request in the `/v1/chat/completions` body, sent by the *client* (Claude, Cursor, Open WebUI, etc.). LocalBar is a server manager, not a proxy, so it cannot inject it.

---

## Decision

- **Ollama:** system prompt in a profile/params → emitted as `SYSTEM "..."` in the generated Modelfile. Applied at model-selection time and on any param save.
- **mlx-lm:** system prompt field is stored and displayed in the param panel with a clear inline note: *"Not applied — mlx-lm servers receive the system prompt from the client per request."* The field remains editable so the user can copy it into their client config.

The `CanonicalParam.systemPrompt` case and `ParamValues.systemPrompt` field are retained as-is. `ParamValues.filtered(to:)` already handles the case where a param has no descriptor in the driver's schema — it will simply be omitted from the mlx-lm launch plan.

---

## Consequences

- No code change to `ParamValues` or `CanonicalParam`
- Ollama param panel: system prompt is a full-height `TextEditor`; applied via Modelfile
- mlx-lm param panel: same `TextEditor` with a muted advisory label beneath it
- No proxy layer required
