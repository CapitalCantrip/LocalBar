# ADR D2 — Param memory keying: separate per runtime for the "same" model

**Status:** Accepted  
**Date:** 2026-07-12  
**Deciders:** Jay

---

## Context

The same underlying model weights can be served by different runtimes. For example:

- `llama3.1:8b` via Ollama (identified as `"llama3.1:8b"`)
- `mlx-community/Llama-3.1-8B-Instruct-4bit` via mlx-lm (identified by its HF path)

These are related but not identical: mlx-lm applies temperature and other sampling params at launch time as CLI flags; Ollama applies them per-request or via a Modelfile. Optimal param values tuned on one runtime may produce different (sometimes noticeably different) results on the other.

The question is whether `ModelMemory` should be keyed so that the same model across runtimes shares one memory entry, or whether each runtime gets its own.

**Option A — Shared key (normalize to a canonical model name)**  
Pros: params carry over when switching runtimes. Cons: params optimised for one runtime may produce worse results on another; the normalization mapping (Ollama tag → HF repo name) is fragile and incomplete.

**Option B — Separate keys per runtime**  
Pros: clean separation; no normalization problem; params always reflect what worked on the actual runtime. Cons: switching runtimes starts fresh on memory (user re-tunes from scratch).

---

## Decision

**Option B — separate per-runtime memory keys.**

`ModelMemory.id` is `ModelRef.key`, which is runtime-scoped: the Ollama tag string for Ollama instances, the HF repo path/dir name for mlx-lm instances. No cross-runtime normalization is attempted.

Rationale: the tuning-per-runtime difference is real, not cosmetic. A user who tunes temperature on mlx-lm and then tries the "same" model on Ollama should start from Ollama's param defaults, not carry over mlx-lm-optimised values that may behave differently. The fresh-start cost is low; the silent-wrong-params cost is higher.

---

## Consequences

- `ModelMemory` stores are keyed by `ModelRef.key`; no cross-runtime lookup is needed or performed.
- A user switching from mlx-lm to Ollama for the first time on a given model starts from driver defaults — expected and correct.
- If a future release adds cross-runtime param sharing as a UX feature, the `ModelRef.key` structure may need a canonical ID field separate from the runtime-specific key. `ModelRef` is already a value type — this is additive.
- The architecture doc note *"confirm this is acceptable for MVP"* is hereby confirmed.
