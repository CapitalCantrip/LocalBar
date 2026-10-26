# ADR D19 — Instance params stay per instance; one rule for effective params

**Status:** Accepted
**Date:** 2026-10-22
**Deciders:** Jay

---

## Context

Three places decided which params an instance runs with, and they disagreed:

- The Servers tab param editor showed `ParamValues::resolve(profile, memory)` but saved to `instance_params`.
- mlx-lm launched with raw `instance_params`, ignoring Auto-memory and the Active profile.
- Ollama baked the Modelfile from `resolve(profile, memory)`, ignoring `instance_params`, so a saved edit never reached the server.
- After a successful start, ModelMemory re-recorded `resolve(profile, memory)`, so it never learned the values that ran, and the editor reverted to them after a refresh.

An architecture review (2026-10-22) proposed folding `instance_params` into ModelMemory, so that saved edits live per model, as the Auto-memory definition suggests.

## Decision

### Instance params stay per instance

Instance params are not merged into ModelMemory. The same model is often run with different settings for different uses (for example a low-temperature coding instance and a high-temperature writing instance on one model), and that needs the settings to belong to the instance. Future reviews should not re-suggest per-model storage for saved edits.

### One precedence rule

The effective params for an instance are computed in one core module, layering, each over the last:

1. the driver's param defaults
2. the ModelMemory of the selected model
3. the instance params
4. the Active profile

Every consumer uses that module: mlx-lm launch, Ollama Modelfile baking, the param editor's display, and the ModelMemory write after a successful start.

### Saving and remembering

- **Save params** stores every field as shown, so what the editor shows is what runs. A cleared field falls back to the lower layers.
- After a successful start, ModelMemory records the effective params that ran, so a new instance of that model starts from the last working setup.
- Saving params on a running mlx-lm instance shows that the change takes effect on the next start; mlx-lm reads params only at launch.

## Consequences

- No migration: `instance_params` keeps its shape in `state.json`.
- An instance's saved params carry over when its selected model changes, by design.
- #106's InstanceManager calls the effective-params module rather than moving three copies of the rule.

## Alternatives considered

**Per-model storage (saved edits written to ModelMemory):** one store instead of two and closer to the Auto-memory wording, but it rules out tuning one model differently on two instances. Rejected.

**Instance params above the Active profile:** rejected; choosing a profile is a deliberate act, and GLOSSARY already says the Active profile overrides the instance's own params.
