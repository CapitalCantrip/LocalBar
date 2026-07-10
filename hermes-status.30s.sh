#!/usr/bin/env bash
# <swiftbar.title>Hermes Status</swiftbar.title>
# <swiftbar.version>1.0.0</swiftbar.version>
# <swiftbar.author>AgenticOS</swiftbar.author>
# <swiftbar.desc>Shows the active Hermes inference model and server health. Quick-switch models and open settings.</swiftbar.desc>
# <swiftbar.hideAbout>true</swiftbar.hideAbout>
# <swiftbar.hideRunInTerminal>true</swiftbar.hideRunInTerminal>
# <swiftbar.hideLastUpdated>false</swiftbar.hideLastUpdated>
# <swiftbar.hideDisablePlugin>false</swiftbar.hideDisablePlugin>
# <swiftbar.hideSwiftBar>false</swiftbar.hideSwiftBar>

ENDPOINTS="$HOME/.hermes/endpoints.env"
HERMES_SW="$HOME/.hermes/an external switch script"

[[ ! -f "$ENDPOINTS" ]] && echo "⬡ offline | color=gray" && exit 0

# Read individual keys — never source the file (avoids shell injection from GUI-written values)
_env() { grep "^${1}=" "$ENDPOINTS" 2>/dev/null | cut -d= -f2- ; }

HERMES_MODEL=$(_env HERMES_MODEL)
HERMES_MODEL_FAMILY=$(_env HERMES_MODEL_FAMILY)
HERMES_ENDPOINT=$(_env HERMES_ENDPOINT)
HERMES_TEMP=$(_env HERMES_TEMP)
HERMES_TOP_P=$(_env HERMES_TOP_P)
HERMES_TOP_K=$(_env HERMES_TOP_K)
HERMES_MAX_TOKENS=$(_env HERMES_MAX_TOKENS)
HERMES_REPETITION_PENALTY=$(_env HERMES_REPETITION_PENALTY)
HERMES_SEED=$(_env HERMES_SEED)
HERMES_MLX_MODEL_PATH=$(_env HERMES_MLX_MODEL_PATH)
MLX_LITE_MODEL_PATH=$(_env MLX_LITE_MODEL_PATH)

# ── Server status ─────────────────────────────────────────────────────────────
# Health check: process must exist AND port must respond
if pgrep -qf "mlx_lm" 2>/dev/null \
   && curl -s --max-time 2 http://127.0.0.1:8080/v1/models >/dev/null 2>&1; then
    SERVER_UP=true; SERVER_TYPE="mlx"
elif pgrep -qf "ollama" 2>/dev/null \
   && curl -s --max-time 1 http://localhost:11434/ >/dev/null 2>&1; then
    SERVER_UP=true; SERVER_TYPE="ollama"
else
    SERVER_UP=false; SERVER_TYPE="none"
fi

# Short model name for menu bar
SHORT=$(echo "${HERMES_MODEL:-?}" | sed 's/Qwen3-Heretic-//' | sed 's/Qwythos-//' | cut -c1-14)

# Determine "start current model" target (mlx vs lite) from current path
if [[ "${HERMES_MLX_MODEL_PATH:-}" == "${MLX_LITE_MODEL_PATH:-x}" ]]; then
    START_TARGET="lite"
else
    START_TARGET="mlx"
fi

# ── Menu bar line — filled brain when running, outline when stopped ────────────
# Check SF Symbols app for alternatives: search "brain" and look for fill variants
if $SERVER_UP; then
    echo ":brain.head.profile.fill: $SHORT | sfsize=14 templateImage=true"
else
    echo ":brain.head.profile: | sfsize=14 templateImage=true"
fi
echo "---"

# ── Status block ──────────────────────────────────────────────────────────────
echo "${HERMES_MODEL:-unknown}  [${HERMES_MODEL_FAMILY:-?}] | color=gray"
echo "Endpoint: ${HERMES_ENDPOINT:-?} | color=gray size=11"
echo "Temp ${HERMES_TEMP:-?}  ·  Top-P ${HERMES_TOP_P:-?}  ·  Top-K ${HERMES_TOP_K:-?}  ·  Max ${HERMES_MAX_TOKENS:-?} tok | color=gray size=11"
echo "Rep.Penalty ${HERMES_REPETITION_PENALTY:-1.0}  ·  Seed ${HERMES_SEED:-(random)} | color=gray size=11"
echo "---"

# ── Quick switch ──────────────────────────────────────────────────────────────
echo "🔄 Switch to 35B (mlx)  | bash=$HERMES_SW param1=mlx terminal=false refresh=true"
echo "🔄 Switch to 9B Lite    | bash=$HERMES_SW param1=lite terminal=false refresh=true"
echo "---"
echo "🟢 Start (${HERMES_MODEL:-current}) | bash=$HERMES_SW param1=$START_TARGET terminal=false refresh=true"
echo "⏹  Stop Server          | bash=$HERMES_SW param1=mlx-off terminal=false refresh=true"
echo "---"

# ── Utilities ─────────────────────────────────────────────────────────────────
echo "⚙️  Settings...          | bash=$HOME/.hermes/mlx-venv/bin/python3 param1=$HOME/.hermes/bin/hermes-settings.py terminal=false"
echo "📋 View Log              | bash=/usr/bin/open param1=-a param2=Console param3=$HOME/.hermes/logs/mlx-server.log terminal=false"
echo "🔁 Refresh               | refresh=true"
