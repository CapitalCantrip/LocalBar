# ADR D-H — No in-app chat panel

**Status:** Accepted  
**Date:** 2026-09-14  
**Deciders:** Jay

LocalBar does not include a chat or prompt interface. Users already have preferred frontends — Open WebUI, Continue, Cursor, custom scripts — and LocalBar's value is in keeping the server running and correctly configured, not in duplicating the interface layer.

**Why this matters:** Every LLM tool that targets developers includes a chat panel. The natural assumption is that LocalBar will too. It won't. The product is infrastructure, not a client.

**Consequence:** LocalBar exposes its server instances as standard OpenAI-compatible endpoints. Any tool that speaks that protocol works with LocalBar out of the box, which is a stronger compatibility story than a bespoke chat panel.
