# ADR D1 — How to present per-request-overridable params on Ollama

**Status:** Accepted  
**Date:** 2026-07-12  
**Deciders:** Jay

---

## Context

LocalBar is not a proxy. When a user sets generation parameters (temperature, top-p, etc.) for an Ollama instance, LocalBar can only apply them as Ollama-side defaults — either via a Modelfile or Ollama's limited env-var config. External tools (Cursor, Continue.dev, any client that POSTs to the Ollama API) include their own `options` block in each request, which **silently overrides** anything LocalBar set as a server-side default.

This creates a UX trap: the user sets temperature in LocalBar, a client overrides it per-request, the user sees no effect and files it as a bug. The two obvious UI responses are:

1. **Grey out** these params (same treatment as unsupported params) — honest but makes the panel feel broken for Ollama.
2. **Show them normally with an advisory badge** — honest, actionable, keeps the panel useful.

A third option — omit the params entirely — was rejected because users still benefit from setting Modelfile defaults for tools that *don't* send options (e.g. raw API testing, simple scripts).

---

## Decision

Show per-request-overridable Ollama params in the panel **normally, with an advisory badge** on each affected control.

Badge text (hover/tooltip): *"External tools may override this per request. This sets the Ollama server default."*

`ParamApplication.serverSideDefault` is the signal: any param with this application mode on the active driver gets the badge. The badge is rendered by the UI layer by inspecting `ParamDescriptor.application` — no driver or controller changes needed when the badge copy is updated.

---

## Consequences

- Users understand why params may appear to have no effect when using third-party clients, reducing support noise.
- The param panel remains fully usable for Ollama — raw API testing and simple clients benefit from Modelfile defaults.
- `.serverSideDefault` in `ParamDescriptor.application` must be populated correctly for all Ollama params; incorrect classification produces misleading badges.
- If LocalBar later adds an optional transparent proxy mode, `.apiCall` params will no longer need the badge — `ParamApplication` already carves that space, so removing badges is an additive change.
- Badge copy should be reviewed before App Store submission for tone/length fit.
