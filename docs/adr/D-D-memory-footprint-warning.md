# ADR D-D — Memory footprint warning

**Status:** Accepted  
**Date:** 2026-07-12  
**Deciders:** Jay

---

## Context

Apple Silicon has unified memory shared by CPU and GPU. Loading a large model (e.g. 27B 4-bit ~14 GB) plus a large KV cache on a 16 GB machine leaves almost no headroom for the OS, causing severe thrashing. Users may not know their model+context config exceeds their hardware.

The warning must account for both weight footprint and KV cache, since the user can independently increase context length (making KV cache the dominant factor at large context).

---

## Decision

### Trigger

When the user clicks **Start** on an instance where the memory estimate exceeds **70% of `hw.memsize`**, show a blocking confirmation sheet (E1) before starting. The user can override and proceed.

### Footprint formula

```
total_estimated = weight_bytes + kv_cache_bytes
```

**Weight bytes:**
- If `ModelRef.sizeBytes` is available (Ollama `/api/tags` returns it): use directly.
- Otherwise: estimate from `ModelMetadata.parameterCount` × `bitsPerParam(quantization)` / 8.
  - Known quant → bits mapping: 4bit/Q4_* → 4.5 bits, 8bit/Q8_* → 8 bits, f16/bf16/fp16 → 16 bits, unknown → 16 bits (conservative).

**KV cache bytes:**
```
kv_cache = 2 × num_layers × num_kv_heads × head_dim × context_length × bytes_per_element
```
Where:
- `num_layers`, `num_kv_heads`, `head_dim` come from `ModelMetadata` (see below)
- `context_length` = active `ParamValues[.contextLength]` ?? driver default
- `bytes_per_element`: mlx-lm default = 2 (bf16); reduced if `--kv-cache-bits 4` → 0.5, `--kv-cache-bits 8` → 1. Ollama = always 2 (no KV quant).
- If architecture fields are unavailable: skip KV estimate, warn user that estimate is weight-only.

### ModelMetadata enrichment

Add to `ModelMetadata`:
```swift
var numHiddenLayers: Int?
var numKVHeads: Int?      // num_key_value_heads (GQA); fallback: num_attention_heads
var headDim: Int?         // hidden_size / num_attention_heads if not explicit
```

`ModelMetadataParser` parses these from `config.json` keys:
`num_hidden_layers`, `num_key_value_heads` (fallback `num_attention_heads`), `hidden_size` + `num_attention_heads` → derive `head_dim`.

For Ollama: `OllamaDriver` gains `fetchModelInfo(modelKey:config:) async -> ModelMetadata?` calling `POST /api/show` and parsing `model_info` fields (`llama.block_count`, `llama.attention.head_count_kv`, etc.).

### KV cache quant — mlx-lm FlagDescriptor

`--kv-cache-bits` is added to `MLXLMDriver.flagSchema`:

```swift
FlagDescriptor(
    flagName: "--kv-cache-bits",
    displayName: "KV Cache Quantization",
    help: "Reduce KV cache memory: 8 = 8-bit (~half), 4 = 4-bit (~quarter). Default: bf16 (no quant).",
    valueType: .int(range: 4...8),   // only 4 and 8 are valid; UI uses a picker
    isEnvironmentVariable: false,
    defaultValue: nil
)
```

The memory warning reads this value from `config.advancedFlags["--kv-cache-bits"]` when computing `bytes_per_element`.

### Warning UI (E1)

Sheet attached to the Start button's view, triggered when estimate > 70% RAM:

```
⚠️ High memory usage estimated

Model weights:   ~11.2 GB
KV cache (32K):  ~ 2.8 GB  
────────────────────────────
Estimated total: ~14.0 GB
Available RAM:     16.0 GB  (87% utilisation)

This may cause system instability or swapping.

[Cancel]  [Start Anyway]
```

If architecture fields are missing, KV line reads "KV cache: unknown (architecture data unavailable)".

---

## Consequences

- `ModelMetadata` gains three new optional fields (non-breaking, all optional)
- `ModelMetadataParser` gains ~20 lines parsing `config.json`
- `OllamaDriver` gains one async method; called lazily at Start time (not on every model list)
- `MLXLMDriver.flagSchema` gains one entry
- Memory warning computation is async (needs `sysctl`, may need Ollama API call) — handled inside the existing `.task(id: startTrigger)` block before `controller.start()`
