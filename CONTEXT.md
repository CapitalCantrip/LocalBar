# LocalBar

A macOS (and future cross-platform) menu bar app for managing local LLM inference servers. It starts, stops, and configures servers on the user's own hardware, handling model selection and parameter tuning.

## Language

### Servers and instances

**Instance**: A single configured inference server — one server type, one port, one set of parameters. A user may have multiple instances (e.g. an mlx-lm instance on 8080 and an Ollama instance on 11434).
_Avoid_: server, node, endpoint

**Server type**: The inference backend that an instance runs. Each type has a corresponding Driver. Current types: mlx-lm, Ollama, llama.cpp.
_Avoid_: backend, engine, runtime

**Driver**: The code responsible for a single server type's lifecycle — launching, health-checking, model listing, parameter schema, and teardown. Full drivers own the process; external drivers connect to an already-running process.
_Avoid_: adapter, plugin, integration

**Full driver**: A Driver that spawns and owns the server process. Responsible for the full lifecycle from launch to shutdown.

**External driver**: A Driver that connects to a server process started outside LocalBar. The server exposes an OpenAI-compatible API; LocalBar provides configuration UI and status tracking but does not own the process.
_Avoid_: generic driver, pass-through driver

### Models

**Model**: A specific weights file or tag loadable by an instance. Identified by a key (driver-specific — HuggingFace repo path for mlx-lm, tag name for Ollama).
_Avoid_: checkpoint, weight, engine

**Model key**: The canonical string identifier for a model within a given server type. Used for persistence and switching.

**Managed model tag**: An Ollama-specific concept. A tag in the local Ollama registry in the form `localbar/<model>-<instanceId>` that LocalBar creates by running `ollama create` with a baked Modelfile.
_Avoid_: custom tag, derived model

**Modelfile**: An Ollama configuration file declaring a base model (`FROM`) and baked-in parameters (`PARAMETER` directives, `SYSTEM` block). LocalBar generates this file and passes it to `ollama create` to produce a managed model tag.

### Parameters and profiles

**Param**: A single tunable inference parameter (e.g. temperature, context length, top-p). Each param has a canonical identity (`CanonicalParam`) that is driver-agnostic where possible.
_Avoid_: setting, option, config field

**ParamValues**: The full bag of params for an instance at a point in time — a map of CanonicalParam → ParamValue, plus a separate system prompt.
_Avoid_: parameter set, config snapshot

**Profile**: A named, saved snapshot of ParamValues that can be applied to an instance. Profiles are global (not tied to one instance) and optionally scoped to a server type.
_Avoid_: preset, template, configuration

**Active profile**: The profile currently applied to an instance. When a profile is active, its ParamValues override the instance's own params for the purpose of Modelfile baking and inference.

**Auto-memory**: The default mode when no profile is active. LocalBar remembers the last-used ParamValues per model (via ModelMemory) and restores them automatically on next use.
_Avoid_: default params, remembered params

**ModelMemory**: A persisted record of the last-used ParamValues and restart-duration samples for a given model key. Drives auto-memory and adaptive timing estimates.

### UX surfaces

**Tray popover (A-mode)**: The menu bar popover — the operational surface for users whose configuration is settled. Start/stop instances, switch models, view status.
_Avoid_: menu, dropdown, popover

**Settings window (B-mode)**: The dedicated Settings window — the configuration surface for active experimentation. Add/remove instances, tune params, manage profiles.
_Avoid_: preferences, config window

**Phase**: The current lifecycle state of an instance: stopped, starting, running, stopping, switchingModel, error.
_Avoid_: state, status, mode

### Monetisation

**Personal use**: Use of LocalBar by an individual on their own hardware for their own work or learning. Personal use is free.

**Commercial use**: Use of LocalBar within an organisation to support business operations. Requires a licence.

**Fleet management**: A commercial-tier feature. Central visibility and control of instances running across multiple machines in an org.
