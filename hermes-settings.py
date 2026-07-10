#!/usr/bin/env python3
"""
hermes-settings.py — Hermes inference stack settings panel.

4-tab tkinter window for managing generation params, server config, and system prompt.
Opens from SwiftBar "⚙️ Settings..." or directly: python3 ~/.hermes/bin/hermes-settings.py
"""

import json
import os
import re
import subprocess
import threading
import time
import urllib.request
import tkinter as tk
from tkinter import ttk, messagebox, scrolledtext

ENDPOINTS_FILE = os.path.expanduser("~/.hermes/endpoints.env")
SOUL_FILE      = os.path.expanduser("~/.hermes/SOUL.md")
# Absolute path — GUI processes have a minimal launchd PATH that omits /opt/homebrew/bin
HERMES_SWITCH  = os.path.expanduser("~/.hermes/an external switch script")

# ── Param definitions ─────────────────────────────────────────────────────────

PRESETS = {
    "35B (MLX)"   : "MLX",
    "9B Lite"     : "MLX_LITE",
}

GEN_PARAMS = [
    # (display_label, env_suffix, type, min, max, step, default)
    ("Temperature",        "TEMP",               "float", 0.0,   2.0,   0.05, "0.7"),
    ("Top-P",              "TOP_P",              "float", 0.0,   1.0,   0.05, "0.95"),
    ("Top-K",              "TOP_K",              "int",   0,     200,   1,    "20"),
    ("Min-P",              "MIN_P",              "float", 0.0,   1.0,   0.01, "0.0"),
    ("Max Tokens",         "MAX_TOKENS",         "int",   128,   32768, 128,  "2048"),
    ("Repetition Penalty", "REPETITION_PENALTY", "float", 1.0,   2.0,   0.05, "1.0"),
    ("Seed",               "SEED",               "str",   None,  None,  None, ""),
    ("Chat Template Args", "CHAT_TEMPLATE_ARGS", "str",   None,  None,  None, ""),
]

ADV_PARAMS = [
    # (display_label, env_key, type, choices, default)
    ("Log Level",          "MLX_LOG_LEVEL",          "choice", ["DEBUG","INFO","WARNING","ERROR","CRITICAL"], "WARNING"),
    ("Decode Concurrency", "MLX_DECODE_CONCURRENCY", "int",    None, ""),
    ("Prompt Concurrency", "MLX_PROMPT_CONCURRENCY", "int",    None, ""),
    ("Prefill Step Size",  "MLX_PREFILL_STEP_SIZE",  "int",    None, "2048"),
    ("Prompt Cache Size",  "MLX_PROMPT_CACHE_SIZE",  "int",    None, ""),
    ("Prompt Cache Bytes", "MLX_PROMPT_CACHE_BYTES", "str",    None, ""),
]

# ── endpoints.env helpers ─────────────────────────────────────────────────────

def read_env() -> dict:
    result = {}
    if not os.path.exists(ENDPOINTS_FILE):
        return result
    with open(ENDPOINTS_FILE) as f:
        for line in f:
            line = line.strip()
            if line and not line.startswith("#") and "=" in line:
                k, _, v = line.partition("=")
                result[k.strip()] = v.strip()
    return result

def write_env_key(key: str, value: str):
    """Atomic in-place update of KEY=value. Uses temp-file rename to avoid partial reads."""
    with open(ENDPOINTS_FILE, "r") as f:
        content = f.read()
    pattern = rf"^{re.escape(key)}=.*"
    # Use lambda so value is treated as a literal string, not a replacement pattern
    new_content, n = re.subn(pattern, lambda _: f"{key}={value}", content, flags=re.MULTILINE)
    if n == 0:
        new_content = content.rstrip("\n") + f"\n{key}={value}\n"
    tmp = ENDPOINTS_FILE + ".tmp"
    with open(tmp, "w") as f:
        f.write(new_content)
    os.replace(tmp, ENDPOINTS_FILE)

def write_env_batch(pairs: list[tuple[str, str]]):
    """Atomically write multiple key=value pairs in a single file replace."""
    with open(ENDPOINTS_FILE, "r") as f:
        content = f.read()
    for key, value in pairs:
        pattern = rf"^{re.escape(key)}=.*"
        content, n = re.subn(pattern, lambda _, k=key, v=value: f"{k}={v}", content, flags=re.MULTILINE)
        if n == 0:
            content = content.rstrip("\n") + f"\n{key}={value}\n"
    tmp = ENDPOINTS_FILE + ".tmp"
    with open(tmp, "w") as f:
        f.write(content)
    os.replace(tmp, ENDPOINTS_FILE)

def run_cmd(args: list, timeout=60) -> tuple:
    """Run a command, return (returncode, stdout, stderr)."""
    try:
        r = subprocess.run(args, capture_output=True, text=True, timeout=timeout)
        return r.returncode, r.stdout, r.stderr
    except Exception as e:
        return -1, "", str(e)

def validate_gen_values(vals: dict) -> str | None:
    """Return an error message if any value is invalid, else None."""
    seed = vals.get("SEED", "")
    if seed and not re.fullmatch(r"[0-9]+", seed):
        return f"Seed must be a positive integer or empty (got: {seed!r})"
    tmpl = vals.get("CHAT_TEMPLATE_ARGS", "")
    if tmpl:
        try:
            json.loads(tmpl)
        except json.JSONDecodeError as e:
            return f"Chat Template Args must be valid JSON or empty:\n{e}"
    return None

def active_mlx_switch_target(env: dict) -> str:
    """Return 'mlx' or 'lite' based on which model path is currently active."""
    active = env.get("HERMES_MLX_MODEL_PATH", "")
    lite   = env.get("MLX_LITE_MODEL_PATH", "")
    return "lite" if (active and active == lite) else "mlx"

def preset_is_active(prefix: str, env: dict) -> bool:
    """Return True if the given preset (MLX or MLX_LITE) is the currently loaded model."""
    active = env.get("HERMES_MLX_MODEL_PATH", "")
    if prefix == "MLX_LITE":
        return active == env.get("MLX_LITE_MODEL_PATH", "")
    else:  # MLX
        return active == env.get("MLX_MODEL_PATH", "")

# ── Main window ───────────────────────────────────────────────────────────────

class HermesSettings(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title("Hermes Settings")
        self.resizable(False, False)
        self.configure(padx=8, pady=8)

        self.env = read_env()

        nb = ttk.Notebook(self)
        nb.pack(fill="both", expand=True)

        self.tab_status   = StatusTab(nb, self)
        self.tab_gen      = GenerationTab(nb, self)
        self.tab_advanced = AdvancedTab(nb, self)
        self.tab_soul     = SoulTab(nb, self)

        nb.add(self.tab_status,   text="Status / Model")
        nb.add(self.tab_gen,      text="Generation")
        nb.add(self.tab_advanced, text="Advanced")
        nb.add(self.tab_soul,     text="System Prompt")

        self._refresh_status()

    def _refresh_status(self):
        self.env = read_env()
        self.tab_status.refresh(self.env)
        self.after(5000, self._refresh_status)

    def reload_env(self):
        self.env = read_env()


# ── Tab 1: Status / Model ─────────────────────────────────────────────────────

class StatusTab(ttk.Frame):
    def __init__(self, parent, app):
        super().__init__(parent, padding=12)
        self.app = app
        self._build()

    def _build(self):
        f = self
        ttk.Label(f, text="Server status", font=("", 11, "bold")).grid(row=0, column=0, sticky="w", pady=(0,4))
        self.status_dot = ttk.Label(f, text="●  checking…", foreground="gray")
        self.status_dot.grid(row=0, column=1, sticky="w", padx=8)

        ttk.Separator(f, orient="horizontal").grid(row=1, column=0, columnspan=3, sticky="ew", pady=6)

        ttk.Label(f, text="Active model").grid(row=2, column=0, sticky="w", pady=2)
        self.lbl_model = ttk.Label(f, text="—", foreground="#555")
        self.lbl_model.grid(row=2, column=1, columnspan=2, sticky="w", padx=8)

        ttk.Label(f, text="Model family").grid(row=3, column=0, sticky="w", pady=2)
        self.lbl_family = ttk.Label(f, text="—", foreground="#555")
        self.lbl_family.grid(row=3, column=1, columnspan=2, sticky="w", padx=8)

        ttk.Label(f, text="Endpoint").grid(row=4, column=0, sticky="w", pady=2)
        self.lbl_endpoint = ttk.Label(f, text="—", foreground="#555")
        self.lbl_endpoint.grid(row=4, column=1, columnspan=2, sticky="w", padx=8)

        ttk.Separator(f, orient="horizontal").grid(row=5, column=0, columnspan=3, sticky="ew", pady=6)

        ttk.Label(f, text="Switch model", font=("", 11, "bold")).grid(row=6, column=0, sticky="w", pady=(0,4))

        btn_frame = ttk.Frame(f)
        btn_frame.grid(row=7, column=0, columnspan=3, sticky="w")
        ttk.Button(btn_frame, text="35B (mlx)",  command=lambda: self._switch("mlx")).pack(side="left", padx=(0,4))
        ttk.Button(btn_frame, text="9B Lite",    command=lambda: self._switch("lite")).pack(side="left", padx=4)
        ttk.Button(btn_frame, text="Ollama 27B", command=lambda: self._switch("ollama")).pack(side="left", padx=4)
        ttk.Button(btn_frame, text="Gemma 12B",  command=lambda: self._switch("gemma")).pack(side="left", padx=4)

        ttk.Separator(f, orient="horizontal").grid(row=8, column=0, columnspan=3, sticky="ew", pady=6)

        stop_frame = ttk.Frame(f)
        stop_frame.grid(row=9, column=0, columnspan=3, sticky="w")
        ttk.Button(stop_frame, text="⏹  Stop Server", command=self._stop).pack(side="left")
        self.lbl_action = ttk.Label(stop_frame, text="", foreground="gray")
        self.lbl_action.pack(side="left", padx=8)

        f.columnconfigure(1, weight=1)

    def refresh(self, env: dict):
        # Health check via HTTP rather than just pgrep (avoids false positives)
        try:
            urllib.request.urlopen("http://127.0.0.1:8080/v1/models", timeout=1)
            self.status_dot.configure(text="●  mlx-lm running", foreground="green")
        except Exception:
            try:
                r = subprocess.run(["curl", "-s", "--max-time", "1", "http://localhost:11434/"],
                                   capture_output=True, timeout=2)
                if r.returncode == 0:
                    self.status_dot.configure(text="●  Ollama running", foreground="#4477cc")
                else:
                    self.status_dot.configure(text="○  no server running", foreground="red")
            except Exception:
                self.status_dot.configure(text="○  no server running", foreground="red")

        self.lbl_model.configure(text=env.get("HERMES_MODEL", "—"))
        self.lbl_family.configure(text=env.get("HERMES_MODEL_FAMILY", "—"))
        self.lbl_endpoint.configure(text=env.get("HERMES_ENDPOINT", "—"))

    def _switch(self, target: str):
        self.lbl_action.configure(text=f"Switching to {target}…")
        self.update()
        def _run():
            rc, out, err = run_cmd([HERMES_SWITCH, target], timeout=300)
            msg = "Done." if rc == 0 else f"Error (rc={rc}): {(err or out).strip()[:80]}"
            self.after(0, lambda: self.lbl_action.configure(text=msg))
            self.after(0, self.app.reload_env)
        threading.Thread(target=_run, daemon=True).start()

    def _stop(self):
        self.lbl_action.configure(text="Stopping…")
        self.update()
        def _run():
            rc, out, err = run_cmd([HERMES_SWITCH, "mlx-off"], timeout=60)
            msg = "Stopped." if rc == 0 else f"Error: {err.strip()[:80]}"
            self.after(0, lambda: self.lbl_action.configure(text=msg))
        threading.Thread(target=_run, daemon=True).start()


# ── Tab 2: Generation params ──────────────────────────────────────────────────

class GenerationTab(ttk.Frame):
    def __init__(self, parent, app):
        super().__init__(parent, padding=12)
        self.app = app
        self._vars: dict[str, tk.Variable] = {}
        self._build()
        self.load_preset("MLX_LITE")

    def _build(self):
        f = self

        sel_frame = ttk.Frame(f)
        sel_frame.grid(row=0, column=0, columnspan=3, sticky="ew", pady=(0, 8))
        ttk.Label(sel_frame, text="Editing preset:").pack(side="left")
        self._preset_var = tk.StringVar(value="9B Lite")
        preset_cb = ttk.Combobox(sel_frame, textvariable=self._preset_var,
                                  values=list(PRESETS.keys()), state="readonly", width=14)
        preset_cb.pack(side="left", padx=6)
        preset_cb.bind("<<ComboboxSelected>>", lambda e: self._on_preset_change())
        ttk.Label(sel_frame, text="(Apply & Restart uses the active model's preset)",
                  foreground="gray").pack(side="left", padx=4)

        ttk.Separator(f, orient="horizontal").grid(row=1, column=0, columnspan=3, sticky="ew", pady=4)

        for i, (label, suffix, typ, lo, hi, step, default) in enumerate(GEN_PARAMS):
            row = i + 2
            ttk.Label(f, text=label, width=20, anchor="w").grid(row=row, column=0, sticky="w", pady=3)
            if typ == "float":
                var = tk.DoubleVar(value=float(default))
                ttk.Scale(f, from_=lo, to=hi, variable=var, orient="horizontal", length=180,
                          command=lambda v, s=suffix, vr=var: self._on_scale(s, vr)
                          ).grid(row=row, column=1, sticky="w", padx=6)
                val_lbl = ttk.Label(f, text=default, width=7)
                val_lbl.grid(row=row, column=2, sticky="w")
                self._vars[suffix] = var
                self._vars[f"_lbl_{suffix}"] = val_lbl  # type: ignore
            elif typ == "int":
                var = tk.IntVar(value=int(default) if default else 0)
                ttk.Spinbox(f, from_=lo, to=hi, textvariable=var, width=8
                            ).grid(row=row, column=1, sticky="w", padx=6)
                self._vars[suffix] = var
            else:
                var = tk.StringVar(value=default)
                ttk.Entry(f, textvariable=var, width=26
                          ).grid(row=row, column=1, columnspan=2, sticky="w", padx=6)
                self._vars[suffix] = var

        last_row = len(GEN_PARAMS) + 2
        ttk.Separator(f, orient="horizontal").grid(row=last_row, column=0, columnspan=3, sticky="ew", pady=8)

        btn_frame = ttk.Frame(f)
        btn_frame.grid(row=last_row+1, column=0, columnspan=3, sticky="w")
        ttk.Button(btn_frame, text="Reset to Defaults",        command=self._reset).pack(side="left", padx=(0,4))
        ttk.Button(btn_frame, text="Apply",                    command=self._apply).pack(side="left", padx=4)
        ttk.Button(btn_frame, text="Apply & Restart Server ↺", command=self._apply_restart).pack(side="left", padx=4)
        self.lbl_status = ttk.Label(btn_frame, text="", foreground="gray")
        self.lbl_status.pack(side="left", padx=8)

        f.columnconfigure(1, weight=1)

    def _on_scale(self, suffix, var):
        lbl = self._vars.get(f"_lbl_{suffix}")
        if lbl:
            lbl.configure(text=f"{var.get():.2f}")

    def _on_preset_change(self):
        self.load_preset(PRESETS[self._preset_var.get()])

    def load_preset(self, prefix: str):
        env = self.app.env
        for label, suffix, typ, lo, hi, step, default in GEN_PARAMS:
            raw = env.get(f"{prefix}_{suffix}", default)
            var = self._vars.get(suffix)
            if var is None:
                continue
            try:
                if typ == "float":
                    var.set(float(raw) if raw else float(default))
                    lbl = self._vars.get(f"_lbl_{suffix}")
                    if lbl:
                        lbl.configure(text=f"{var.get():.2f}")
                elif typ == "int":
                    var.set(int(raw) if raw else (int(default) if default else 0))
                else:
                    var.set(raw if raw is not None else default)
            except (ValueError, TypeError):
                pass

    def _get_values(self) -> dict:
        result = {}
        for label, suffix, typ, lo, hi, step, default in GEN_PARAMS:
            var = self._vars.get(suffix)
            if var is None:
                continue
            if typ == "float":
                s = f"{var.get():.4f}".rstrip("0").rstrip(".")
                result[suffix] = s if "." in s else s + ".0"
            else:
                result[suffix] = str(var.get())
        return result

    def _reset(self):
        for label, suffix, typ, lo, hi, step, default in GEN_PARAMS:
            var = self._vars.get(suffix)
            if var is None:
                continue
            try:
                if typ == "float":
                    var.set(float(default))
                    lbl = self._vars.get(f"_lbl_{suffix}")
                    if lbl:
                        lbl.configure(text=f"{float(default):.2f}")
                elif typ == "int":
                    var.set(int(default) if default else 0)
                else:
                    var.set(default)
            except ValueError:
                pass

    def _apply(self):
        vals = self._get_values()
        err = validate_gen_values(vals)
        if err:
            messagebox.showerror("Validation error", err)
            return
        self._write_preset_values(vals)
        self.lbl_status.configure(text="Saved.")
        self.after(3000, lambda: self.lbl_status.configure(text=""))

    def _write_preset_values(self, vals: dict):
        prefix = PRESETS[self._preset_var.get()]
        env = self.app.env
        is_active = preset_is_active(prefix, env)

        pairs = []
        for suffix, value in vals.items():
            pairs.append((f"{prefix}_{suffix}", value))
            # Only propagate to HERMES_* when this preset is actually running.
            # Editing the 35B preset while lite is loaded must not change live params.
            if is_active:
                pairs.append((f"HERMES_{suffix}", value))

        write_env_batch(pairs)
        self.app.reload_env()

    def _apply_restart(self):
        vals = self._get_values()
        err = validate_gen_values(vals)
        if err:
            messagebox.showerror("Validation error", err)
            return
        # For restart, always write HERMES_* regardless of which preset is open —
        # the user explicitly asked to restart with these params.
        prefix = PRESETS[self._preset_var.get()]
        pairs = []
        for suffix, value in vals.items():
            pairs.append((f"{prefix}_{suffix}", value))
            pairs.append((f"HERMES_{suffix}", value))
        write_env_batch(pairs)
        self.app.reload_env()

        env = self.app.env
        endpoint = env.get("HERMES_ENDPOINT", "")
        if "11434" in endpoint:
            messagebox.showinfo("Not applicable",
                                "Active endpoint is Ollama — restart not applicable.\n"
                                "Use the Status tab to switch to an mlx model first.")
            return

        target = active_mlx_switch_target(self.app.env)

        self.lbl_status.configure(text=f"Restarting ({target})…")
        def _run():
            rc, out, err_s = run_cmd([HERMES_SWITCH, target], timeout=300)
            msg = "Ready ✓" if rc == 0 else f"Error (rc={rc}): {(err_s or out).strip()[:80]}"
            self.after(0, lambda: self.lbl_status.configure(text=msg))
            self.after(0, self.app.reload_env)
        threading.Thread(target=_run, daemon=True).start()


# ── Tab 3: Advanced ───────────────────────────────────────────────────────────

class AdvancedTab(ttk.Frame):
    def __init__(self, parent, app):
        super().__init__(parent, padding=12)
        self.app = app
        self._vars: dict[str, tk.Variable] = {}
        self._build()
        self._load()

    def _build(self):
        f = self
        ttk.Label(f, text="mlx-lm server startup flags", font=("", 11, "bold")).grid(
            row=0, column=0, columnspan=3, sticky="w", pady=(0, 6))

        for i, (label, key, typ, choices, default) in enumerate(ADV_PARAMS):
            row = i + 1
            ttk.Label(f, text=label, width=22, anchor="w").grid(row=row, column=0, sticky="w", pady=3)
            if typ == "choice":
                var = tk.StringVar(value=default)
                ttk.Combobox(f, textvariable=var, values=choices, state="readonly", width=12
                             ).grid(row=row, column=1, sticky="w", padx=6)
            elif typ == "int":
                var = tk.StringVar(value=default)
                ttk.Entry(f, textvariable=var, width=10).grid(row=row, column=1, sticky="w", padx=6)
                ttk.Label(f, text="(blank = auto)", foreground="gray").grid(row=row, column=2, sticky="w")
            else:
                var = tk.StringVar(value=default)
                ttk.Entry(f, textvariable=var, width=16).grid(row=row, column=1, columnspan=2, sticky="w", padx=6)
            self._vars[key] = var

        last_row = len(ADV_PARAMS) + 1
        ttk.Separator(f, orient="horizontal").grid(row=last_row, column=0, columnspan=3, sticky="ew", pady=8)
        ttk.Label(f, text="Changes take effect on next server restart.",
                  foreground="gray", font=("", 10, "italic")).grid(
            row=last_row+1, column=0, columnspan=3, sticky="w")

        btn_frame = ttk.Frame(f)
        btn_frame.grid(row=last_row+2, column=0, columnspan=3, sticky="w", pady=(6,0))
        ttk.Button(btn_frame, text="Apply", command=self._apply).pack(side="left")
        self.lbl_status = ttk.Label(btn_frame, text="", foreground="gray")
        self.lbl_status.pack(side="left", padx=8)

        f.columnconfigure(1, weight=1)

    def _load(self):
        env = self.app.env
        for label, key, typ, choices, default in ADV_PARAMS:
            var = self._vars.get(key)
            if var:
                var.set(env.get(key, default) or default)

    def _apply(self):
        pairs = [(key, var.get()) for key, var in self._vars.items()]
        write_env_batch(pairs)
        self.app.reload_env()
        self.lbl_status.configure(text="Saved.")
        self.after(3000, lambda: self.lbl_status.configure(text=""))


# ── Tab 4: System Prompt ──────────────────────────────────────────────────────

class SoulTab(ttk.Frame):
    def __init__(self, parent, app):
        super().__init__(parent, padding=12)
        self.app = app
        self._build()
        self._load()

    def _build(self):
        f = self
        ttk.Label(f, text="System Prompt (SOUL.md)", font=("", 11, "bold")).pack(anchor="w")
        ttk.Label(f, text=SOUL_FILE, foreground="gray", font=("", 10)).pack(anchor="w", pady=(0,6))

        btn_frame = ttk.Frame(f)
        btn_frame.pack(anchor="w", pady=(0, 6))
        ttk.Button(btn_frame, text="Open in VS Code",        command=self._open_vscode).pack(side="left", padx=(0,4))
        ttk.Button(btn_frame, text="Open in Default Editor", command=self._open_default).pack(side="left", padx=4)
        ttk.Button(btn_frame, text="Refresh Preview",        command=self._load).pack(side="left", padx=4)

        self.text = scrolledtext.ScrolledText(f, wrap="word", width=64, height=26,
                                               state="disabled", font=("Menlo", 10))
        self.text.pack(fill="both", expand=True)
        self.lbl_status = ttk.Label(f, text="", foreground="gray")
        self.lbl_status.pack(anchor="w", pady=(4,0))

    def _load(self):
        if not os.path.exists(SOUL_FILE):
            self._set_text(f"(SOUL.md not found at {SOUL_FILE})")
            return
        with open(SOUL_FILE) as fh:
            self._set_text(fh.read())

    def _set_text(self, content: str):
        self.text.configure(state="normal")
        self.text.delete("1.0", "end")
        self.text.insert("1.0", content)
        self.text.configure(state="disabled")

    def _open_vscode(self):
        try:
            subprocess.Popen(["code", SOUL_FILE])
            self.lbl_status.configure(text="Opened in VS Code.")
        except FileNotFoundError:
            messagebox.showerror("VS Code not found",
                                 "'code' not found in PATH.\n"
                                 "VS Code → Command Palette → 'Install code command in PATH'.")

    def _open_default(self):
        subprocess.Popen(["open", SOUL_FILE])
        self.lbl_status.configure(text="Opened in default editor.")


# ── Entry point ───────────────────────────────────────────────────────────────

if __name__ == "__main__":
    app = HermesSettings()
    app.update_idletasks()
    w, h = 580, 640
    sw = app.winfo_screenwidth()
    sh = app.winfo_screenheight()
    app.geometry(f"{w}x{h}+{(sw-w)//2}+{(sh-h)//2}")
    # Force window to front when launched from SwiftBar (background process)
    app.lift()
    app.attributes("-topmost", True)
    app.after(200, lambda: app.attributes("-topmost", False))
    subprocess.Popen(["osascript", "-e",
        'tell application "System Events" to set frontmost of '
        'first process whose name starts with "Python" to true'])
    app.mainloop()
