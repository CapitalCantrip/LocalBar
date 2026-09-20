import { useCallback, useEffect, useRef, useState } from 'react'
import { ipc, type ParamSchemaEntry } from '../ipc'
import {
  type DiscoveredModel,
  type DiscoveryConfig,
  type InstancePhase,
  type ModelMetadata,
  type ModelRef,
  type ParamValues,
  type ServerInstanceConfig,
  isActive,
  phaseColor,
  phaseLabel,
} from '../types'
import { useInstances } from '../useInstances'
import { startWithWarnings } from '../startWithWarnings'

// ─── Styles ───────────────────────────────────────────────────────────────────

const s = {
  root: {
    fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
    fontSize: 13, height: '100vh', display: 'flex',
    flexDirection: 'column' as const, background: '#f0f0f0', color: '#1a1a1a',
  },
  toolbar: {
    display: 'flex', alignItems: 'center', gap: 4,
    padding: '8px 12px', borderBottom: '1px solid #ddd', background: '#e8e8e8',
  },
  tabBtn: (active: boolean): React.CSSProperties => ({
    padding: '4px 14px', borderRadius: 6,
    border: '1px solid transparent',
    background: active ? 'white' : 'transparent',
    boxShadow: active ? '0 1px 3px rgba(0,0,0,0.15)' : 'none',
    cursor: 'pointer', fontSize: 13, fontFamily: 'inherit',
  }),
  body: { flex: 1, display: 'flex', overflow: 'hidden' },
  masterPane: {
    width: 220, borderRight: '1px solid #ddd', background: 'white',
    display: 'flex', flexDirection: 'column' as const, overflow: 'hidden',
  },
  masterHeader: {
    display: 'flex', justifyContent: 'space-between', alignItems: 'center',
    padding: '8px 12px', borderBottom: '1px solid #eee',
  },
  masterList: { flex: 1, overflow: 'auto' },
  masterRow: (selected: boolean): React.CSSProperties => ({
    display: 'flex', alignItems: 'center', gap: 8,
    padding: '8px 12px', cursor: 'pointer',
    background: selected ? '#e8f0fe' : 'transparent',
    borderLeft: selected ? '3px solid #1d4ed8' : '3px solid transparent',
  }),
  dot: (color: string): React.CSSProperties => ({
    width: 8, height: 8, borderRadius: '50%', background: color, flexShrink: 0,
  }),
  detailPane: {
    flex: 1, background: 'white', overflow: 'auto',
    display: 'flex', flexDirection: 'column' as const,
  },
  detailHeader: {
    padding: '14px 18px', borderBottom: '1px solid #eee',
    display: 'flex', alignItems: 'center', justifyContent: 'space-between',
  },
  detailTitle: { fontWeight: 600, fontSize: 15 },
  detailBody: { padding: '14px 18px', display: 'flex', flexDirection: 'column' as const, gap: 14 },
  field: { display: 'flex', flexDirection: 'column' as const, gap: 4 },
  fieldLabel: { fontSize: 11, fontWeight: 600, color: '#666', textTransform: 'uppercase' as const, letterSpacing: '0.05em' },
  fieldValue: { fontSize: 13 },
  phaseRow: { display: 'flex', alignItems: 'center', gap: 8 },
  btnRow: { display: 'flex', gap: 8 },
  btn: (danger?: boolean): React.CSSProperties => ({
    padding: '5px 12px', border: '1px solid #ccc', borderRadius: 6,
    background: danger ? '#fee2e2' : 'white',
    color: danger ? '#dc2626' : '#1a1a1a',
    cursor: 'pointer', fontSize: 12, fontFamily: 'inherit',
  }),
  primaryBtn: { padding: '5px 12px', border: '1px solid #1d4ed8', borderRadius: 6, background: '#1d4ed8', color: 'white', cursor: 'pointer', fontSize: 12, fontFamily: 'inherit' },
  placeholder: { color: '#aaa', fontSize: 12, textAlign: 'center' as const, padding: 40 },
  addBtn: {
    padding: '3px 10px', border: '1px solid #ccc', borderRadius: 4,
    background: 'white', cursor: 'pointer', fontSize: 12, fontFamily: 'inherit',
  },
  sheet: {
    position: 'absolute' as const, inset: 0, background: 'rgba(0,0,0,0.35)',
    display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 10,
  },
  sheetBox: {
    background: 'white', borderRadius: 10, padding: 20, width: 340,
    boxShadow: '0 8px 32px rgba(0,0,0,0.2)',
    display: 'flex', flexDirection: 'column' as const, gap: 12,
  },
  sheetTitle: { fontWeight: 600, fontSize: 15, margin: 0 },
  input: {
    padding: '6px 10px', border: '1px solid #ccc', borderRadius: 6,
    fontSize: 13, fontFamily: 'inherit', width: '100%', boxSizing: 'border-box' as const,
  },
  sheetBtns: { display: 'flex', gap: 8, justifyContent: 'flex-end', marginTop: 4 },
  checkbox: { display: 'flex', alignItems: 'center', gap: 8, cursor: 'pointer', fontSize: 13 },
  modelList: { display: 'flex', flexDirection: 'column' as const, gap: 2 },
  modelRow: (selected: boolean): React.CSSProperties => ({
    display: 'flex', alignItems: 'center', gap: 8,
    padding: '6px 10px', borderRadius: 6, cursor: 'pointer',
    background: selected ? '#e8f0fe' : 'transparent',
    border: selected ? '1px solid #93c5fd' : '1px solid transparent',
  }),
  modelName: { flex: 1, fontSize: 12, fontWeight: 500, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' as const },
  modelMeta: { fontSize: 11, color: '#888', flexShrink: 0 },
  refreshNote: { fontSize: 11, color: '#aaa', fontStyle: 'italic' as const },
}

// ─── Add-instance model picker ───────────────────────────────────────────────

const PICKER_TYPES: ServerTypeOption[] = ['mlx-lm', 'ollama']

function AddModelPicker({ serverType, selectedModelKey, onSelect }: {
  serverType: ServerTypeOption
  selectedModelKey: string | null
  onSelect: (key: string | null) => void
}) {
  const [models, setModels] = useState<ModelRef[] | null>(null)
  const [loading, setLoading] = useState(false)
  const [scanError, setScanError] = useState<string | null>(null)
  const [ollamaUnreachable, setOllamaUnreachable] = useState(false)
  const [freeText, setFreeText] = useState('')
  const scanRef = useRef(0)

  useEffect(() => {
    if (!PICKER_TYPES.includes(serverType)) {
      setModels(null); setScanError(null); setOllamaUnreachable(false); setFreeText(''); return
    }
    const seq = ++scanRef.current
    setLoading(true)
    setModels(null)
    setScanError(null)
    setOllamaUnreachable(false)
    setFreeText('')
    ipc.listModelsForType(serverType)
      .then(list => { if (seq === scanRef.current) { setModels(list); setLoading(false) } })
      .catch(e => {
        if (seq !== scanRef.current) return
        if (serverType === 'ollama' && String(e) === 'OLLAMA_UNREACHABLE') {
          setOllamaUnreachable(true)
        } else {
          setScanError(String(e))
        }
        setLoading(false)
      })
  }, [serverType])

  if (!PICKER_TYPES.includes(serverType)) return null

  if (loading) return (
    <div style={s.field}>
      <span style={s.fieldLabel}>Model</span>
      <span style={{ ...s.fieldValue, color: '#aaa', fontSize: 11 }}>Scanning…</span>
    </div>
  )

  if (scanError) return (
    <div style={s.field}>
      <span style={s.fieldLabel}>Model</span>
      <span style={{ ...s.fieldValue, color: '#ef4444', fontSize: 11 }}>Scan failed: {scanError}</span>
    </div>
  )

  if (ollamaUnreachable) return (
    <div style={s.field}>
      <span style={s.fieldLabel}>Model</span>
      <input
        style={s.input}
        value={freeText}
        onChange={e => { setFreeText(e.target.value); onSelect(e.target.value.trim() || null) }}
        placeholder="llama3:8b"
      />
      <span style={{ fontSize: 11, color: '#aaa' }}>Ollama not reachable — enter tag name manually</span>
    </div>
  )

  if (models !== null && models.length === 0) return (
    <div style={s.field}>
      <span style={s.fieldLabel}>Model</span>
      <span style={{ ...s.fieldValue, color: '#aaa', fontSize: 11 }}>
        {serverType === 'mlx-lm'
          ? 'No models found — configure model paths in Settings → Discovery.'
          : 'No models found'}
      </span>
    </div>
  )

  if (!models) return null

  return (
    <div style={s.field}>
      <span style={s.fieldLabel}>Model</span>
      <div style={s.modelList}>
        {models.map(m => (
          <div
            key={m.key}
            style={s.modelRow(m.key === selectedModelKey)}
            onClick={() => onSelect(m.key === selectedModelKey ? null : m.key)}
          >
            <span style={s.modelName} title={m.key}>{m.display_name}</span>
          </div>
        ))}
      </div>
    </div>
  )
}

// ─── Add-instance sheet ───────────────────────────────────────────────────────

type ServerTypeOption = 'ollama' | 'mlx-lm' | 'external'

const DEFAULTS: Record<ServerTypeOption, { port: string; execPath: string; namePlaceholder: string }> = {
  ollama: { port: '11434', execPath: '/usr/local/bin/ollama', namePlaceholder: 'My Ollama' },
  'mlx-lm': { port: '8080', execPath: 'uvx', namePlaceholder: 'My mlx-lm' },
  external: { port: '11434', execPath: '', namePlaceholder: 'My External Server' },
}

function AddInstanceSheet({ onAdd, onCancel }: {
  onAdd: (name: string, serverType: string, port: number, execPath: string, selectedModelKey: string | null) => Promise<void>
  onCancel: () => void
}) {
  const [name, setName] = useState('')
  const [serverType, setServerType] = useState<ServerTypeOption>('ollama')
  const [port, setPort] = useState(DEFAULTS.ollama.port)
  const [execPath, setExecPath] = useState(DEFAULTS.ollama.execPath)
  const [selectedModelKey, setSelectedModelKey] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const selectType = (t: ServerTypeOption) => {
    setServerType(t)
    setPort(DEFAULTS[t].port)
    setExecPath(DEFAULTS[t].execPath)
    setSelectedModelKey(null)
  }

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim() || (serverType !== 'external' && !execPath.trim())) return
    setBusy(true)
    try {
      await onAdd(name.trim(), serverType, parseInt(port, 10) || parseInt(DEFAULTS[serverType].port, 10), execPath.trim(), selectedModelKey)
    } finally { setBusy(false) }
  }

  return (
    <div style={s.sheet}>
      <form style={s.sheetBox} onSubmit={submit}>
        <p style={s.sheetTitle}>Add Instance</p>
        <div style={s.field}>
          <span style={s.fieldLabel}>Server type</span>
          <div style={{ display: 'flex', gap: 16, marginTop: 2 }}>
            {(['ollama', 'mlx-lm', 'external'] as const).map(t => (
              <label key={t} style={{ display: 'flex', alignItems: 'center', gap: 5, cursor: 'pointer', fontSize: 13 }}>
                <input type="radio" name="serverType" value={t} checked={serverType === t} onChange={() => selectType(t)} />
                {t}
              </label>
            ))}
          </div>
        </div>
        <div style={s.field}>
          <label style={s.fieldLabel}>Name</label>
          <input style={s.input} value={name} onChange={e => setName(e.target.value)} placeholder={DEFAULTS[serverType].namePlaceholder} autoFocus />
        </div>
        <div style={s.field}>
          <label style={s.fieldLabel}>Port</label>
          <input style={s.input} value={port} onChange={e => setPort(e.target.value)} />
        </div>
        {serverType !== 'external' && (
          <div style={s.field}>
            <label style={s.fieldLabel}>Executable path</label>
            <input style={s.input} value={execPath} onChange={e => setExecPath(e.target.value)} />
          </div>
        )}
        <AddModelPicker serverType={serverType} selectedModelKey={selectedModelKey} onSelect={setSelectedModelKey} />
        <div style={s.sheetBtns}>
          <button type="button" style={s.btn()} onClick={onCancel}>Cancel</button>
          <button type="submit" style={s.primaryBtn} disabled={busy || !name.trim()}>
            {busy ? 'Adding…' : 'Add'}
          </button>
        </div>
      </form>
    </div>
  )
}

// ─── Param editor ────────────────────────────────────────────────────────────

function ParamEditor({ instance, onRefresh }: {
  instance: ServerInstanceConfig
  onRefresh: () => void
}) {
  const [params, setParams] = useState<ParamValues | null>(null)
  const [schema, setSchema] = useState<ParamSchemaEntry[]>([])
  const [systemPrompt, setSystemPrompt] = useState('')
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    ipc.getResolvedParams(instance.id).then(p => {
      setParams(p)
      setSystemPrompt(p.system_prompt ?? '')
    }).catch(() => {})
    ipc.getParamSchema(instance.server_type).then(setSchema).catch(() => {})
  }, [instance.id, instance.server_type])

  const setField = (key: string, raw: string, kind: 'double' | 'int') => {
    const num = kind === 'int' ? parseInt(raw, 10) : parseFloat(raw)
    if (raw !== '' && isNaN(num)) return
    setParams(prev => {
      if (!prev) return prev
      const values = { ...prev.values }
      if (raw === '') { delete values[key] } else { values[key] = { type: kind, value: num } }
      return { ...prev, values }
    })
  }

  const apply = async () => {
    if (!params) return
    setBusy(true)
    try {
      await ipc.updateInstanceParams(instance.id, { ...params, system_prompt: systemPrompt || null })
      onRefresh()
    } finally { setBusy(false) }
  }

  if (!params) return null

  return (
    <div style={s.field}>
      <span style={s.fieldLabel}>Parameters</span>
      {schema.map(f => (
        <div key={f.key} style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
          <label style={{ fontSize: 12, color: '#555', width: 110 }}>{f.label}</label>
          <input
            style={{ ...s.input, width: 90 }}
            type="number"
            step={f.kind === 'double' ? '0.01' : '1'}
            value={(params.values[f.key]?.value as number | undefined) ?? ''}
            onChange={e => setField(f.key, e.target.value, f.kind)}
          />
        </div>
      ))}
      <div style={{ marginBottom: 4 }}>
        <label style={{ fontSize: 12, color: '#555' }}>System prompt</label>
        <textarea
          style={{ ...s.input, height: 56, marginTop: 4, resize: 'vertical' as const, display: 'block' }}
          value={systemPrompt}
          onChange={e => setSystemPrompt(e.target.value)}
          placeholder="Optional system prompt…"
        />
      </div>
      <button style={s.btn()} onClick={apply} disabled={busy}>
        {busy ? 'Saving…' : 'Save params'}
      </button>
    </div>
  )
}

// ─── Model list ───────────────────────────────────────────────────────────────

function ModelList({ instance, phase, onRefresh }: {
  instance: ServerInstanceConfig
  phase: InstancePhase | undefined
  onRefresh: () => void
}) {
  const [models, setModels] = useState<ModelRef[]>([])
  const [metaMap, setMetaMap] = useState<Record<string, ModelMetadata | null>>({})
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const fetchRef = useRef(0)

  const fetchModels = useCallback(async () => {
    const seq = ++fetchRef.current
    setLoading(true)
    setError(null)
    try {
      const list = await ipc.listModels(instance.id)
      if (seq !== fetchRef.current) return
      setModels(list)
      // Fetch metadata for each model in the background.
      list.forEach(async m => {
        const meta = await ipc.fetchModelMetadata(instance.id, m.key).catch(() => null)
        if (seq !== fetchRef.current) return
        setMetaMap(prev => ({ ...prev, [m.key]: meta }))
      })
    } catch (e) {
      if (seq === fetchRef.current) setError(String(e))
    } finally {
      if (seq === fetchRef.current) setLoading(false)
    }
  }, [instance.id])

  // Re-scan whenever the panel opens for this instance (instance.id change) or on phase change.
  useEffect(() => { fetchModels() }, [fetchModels])

  const handleSelect = async (key: string) => {
    if (key === instance.selected_model_key) return
    if (isActive(phase) && phase?.type !== 'switchingModel') {
      await ipc.switchModel(instance.id, key)
    } else {
      await ipc.setSelectedModel(instance.id, key)
    }
    onRefresh()
  }

  const metaLabel = (key: string): string => {
    const m = metaMap[key]
    if (!m) return ''
    const parts = [m.parameter_count, m.quantization].filter(Boolean)
    return parts.join(' · ')
  }

  if (error) {
    return (
      <div style={s.field}>
        <span style={s.fieldLabel}>Models <span style={s.refreshNote}>(unavailable)</span></span>
        <span style={{ ...s.fieldValue, color: '#ef4444', fontSize: 11 }}>{error}</span>
      </div>
    )
  }

  return (
    <div style={s.field}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
        <span style={s.fieldLabel}>Models</span>
        {loading && <span style={s.refreshNote}>refreshing…</span>}
        {!loading && (
          <button
            style={{ ...s.btn(), padding: '1px 6px', fontSize: 11 }}
            onClick={fetchModels}
          >↺</button>
        )}
      </div>
      {models.length === 0 && !loading && (
        <span style={{ ...s.fieldValue, color: '#aaa', fontSize: 11 }}>No models found</span>
      )}
      <div style={s.modelList}>
        {models.map(m => (
          <div
            key={m.key}
            style={s.modelRow(m.key === instance.selected_model_key)}
            onClick={() => handleSelect(m.key)}
          >
            <span style={s.modelName} title={m.key}>{m.display_name}</span>
            {metaLabel(m.key) && <span style={s.modelMeta}>{metaLabel(m.key)}</span>}
          </div>
        ))}
      </div>
    </div>
  )
}

// ─── Model path override field ────────────────────────────────────────────────

function ModelPathOverrideField({ instance, onRefresh }: {
  instance: ServerInstanceConfig
  onRefresh: () => void
}) {
  const [value, setValue] = useState(instance.model_search_path_override ?? '')
  const [busy, setBusy] = useState(false)
  const [saveError, setSaveError] = useState<string | null>(null)

  useEffect(() => {
    setValue(instance.model_search_path_override ?? '')
  }, [instance.id, instance.model_search_path_override])

  const save = async () => {
    setBusy(true)
    setSaveError(null)
    try {
      await ipc.setModelSearchPathOverride(instance.id, value.trim() || null)
      onRefresh()
    } catch (e) {
      setSaveError(String(e))
    } finally { setBusy(false) }
  }

  return (
    <div style={s.field}>
      <span style={s.fieldLabel}>Model path override</span>
      <div style={{ display: 'flex', gap: 6 }}>
        <input
          style={{ ...s.input, flex: 1 }}
          value={value}
          onChange={e => setValue(e.target.value)}
          placeholder="Optional — overrides global search paths"
        />
        <button style={s.btn()} onClick={() => void save()} disabled={busy}>Save</button>
      </div>
      {saveError && <span style={{ fontSize: 11, color: '#ef4444' }}>{saveError}</span>}
      {!saveError && !value.trim() && (
        <span style={{ fontSize: 11, color: '#aaa' }}>Using global discovery paths</span>
      )}
    </div>
  )
}

// ─── Discovered models section ────────────────────────────────────────────────

function fmtBytes(b: number | null): string {
  if (b === null) return '—'
  if (b >= 1e9) return `${(b / 1e9).toFixed(1)} GB`
  if (b >= 1e6) return `${(b / 1e6).toFixed(0)} MB`
  if (b >= 1e3) return `${(b / 1e3).toFixed(0)} KB`
  return `${b} B`
}

function hfLink(serverType: string, key: string): { href: string; label: string } {
  if (serverType === 'mlx-lm') return { href: `https://huggingface.co/${key}`, label: 'HuggingFace ↗' }
  const name = key.split(':')[0]
  return { href: `https://huggingface.co/models?search=${encodeURIComponent(name)}`, label: 'Search HF ↗' }
}

function DiscoveredModelsSection() {
  const [models, setModels] = useState<DiscoveredModel[] | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const seqRef = useRef(0)

  const scan = useCallback(async () => {
    const seq = ++seqRef.current
    setLoading(true)
    setError(null)
    try {
      const list = await ipc.listAllDiscoveredModels()
      if (seq === seqRef.current) setModels(list)
    } catch (e) {
      if (seq === seqRef.current) setError(String(e))
    } finally {
      if (seq === seqRef.current) setLoading(false)
    }
  }, [])

  useEffect(() => { void scan() }, [scan])

  const grouped = (models ?? []).reduce<Record<string, DiscoveredModel[]>>((acc, m) => {
    ;(acc[m.server_type] ??= []).push(m)
    return acc
  }, {})

  return (
    <div style={s.field}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
        <span style={s.fieldLabel}>Discovered Models</span>
        {loading && <span style={s.refreshNote}>scanning…</span>}
        {!loading && (
          <button style={{ ...s.btn(), padding: '1px 6px', fontSize: 11 }} onClick={() => void scan()}>↺</button>
        )}
      </div>
      {error && <span style={{ fontSize: 11, color: '#ef4444' }}>{error}</span>}
      {models !== null && models.length === 0 && !loading && (
        <span style={{ fontSize: 11, color: '#aaa' }}>No models found</span>
      )}
      {Object.entries(grouped).map(([stype, rows]) => (
        <div key={stype} style={{ marginTop: 8 }}>
          <span style={{ ...s.fieldLabel, marginBottom: 4, display: 'block' }}>{stype}</span>
          {rows.map(m => {
            const link = hfLink(m.server_type, m.key)
            const meta = [m.parameter_count, m.quantization].filter(Boolean).join(' · ') || null
            return (
              <div key={m.key} style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '4px 0', borderBottom: '1px solid #f0f0f0' }}>
                <span style={{ flex: 1, fontSize: 12, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' as const }} title={m.key}>
                  {m.display_name}
                </span>
                {meta && <span style={s.modelMeta}>{meta}</span>}
                <span style={s.modelMeta}>{fmtBytes(m.size_bytes)}</span>
                <a href={link.href} target="_blank" rel="noreferrer" style={{ fontSize: 11, color: '#1d4ed8', textDecoration: 'none', flexShrink: 0 }}>
                  {link.label}
                </a>
              </div>
            )
          })}
        </div>
      ))}
    </div>
  )
}

// ─── Discovery tab ────────────────────────────────────────────────────────────

function MlxPathList({ paths, onRemove, busy }: {
  paths: string[]
  onRemove: (i: number) => void
  busy: boolean
}) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column' as const, gap: 4 }}>
      {paths.length === 0 && (
        <span style={{ fontSize: 11, color: '#aaa' }}>HF cache default (~/.cache/huggingface/hub)</span>
      )}
      {paths.map((p, i) => (
        <div key={i} style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <span style={{ flex: 1, fontSize: 11, wordBreak: 'break-all' as const }}>{p}</span>
          <button
            style={{ ...s.btn(true), padding: '2px 8px', fontSize: 11 }}
            onClick={() => onRemove(i)}
            disabled={busy}
          >✕</button>
        </div>
      ))}
    </div>
  )
}

function DiscoveryTab() {
  const [config, setConfig] = useState<DiscoveryConfig | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [newPath, setNewPath] = useState('')
  const [ollamaExe, setOllamaExe] = useState('')
  const [busy, setBusy] = useState(false)
  const [saveError, setSaveError] = useState<string | null>(null)

  const loadConfig = () => {
    setLoadError(null)
    ipc.getDiscoveryConfig().then(c => {
      setConfig(c)
      setOllamaExe(c.ollama_executable_path ?? '')
    }).catch(e => setLoadError(String(e)))
  }

  useEffect(() => { loadConfig() }, [])

  const persist = async (updated: DiscoveryConfig): Promise<boolean> => {
    setBusy(true)
    setSaveError(null)
    try { await ipc.setDiscoveryConfig(updated); setConfig(updated); return true }
    catch (e) { setSaveError(String(e)); return false }
    finally { setBusy(false) }
  }

  const addPath = async () => {
    if (!config || !newPath.trim()) return
    const ok = await persist({ ...config, mlx_lm_search_paths: [...config.mlx_lm_search_paths, newPath.trim()] })
    if (ok) setNewPath('')
  }

  const removePath = async (i: number) => {
    if (!config) return
    await persist({ ...config, mlx_lm_search_paths: config.mlx_lm_search_paths.filter((_, idx) => idx !== i) })
  }

  const saveOllamaExe = async () => {
    if (!config) return
    await persist({ ...config, ollama_executable_path: ollamaExe.trim() || null })
  }

  if (loadError) return (
    <div style={{ padding: '14px 18px', display: 'flex', flexDirection: 'column' as const, gap: 8 }}>
      <span style={{ fontSize: 12, color: '#ef4444' }}>Failed to load discovery config: {loadError}</span>
      <button style={s.btn()} onClick={loadConfig}>Retry</button>
    </div>
  )

  if (!config) return <p style={s.placeholder}>Loading…</p>

  return (
    <div style={{ padding: '14px 18px', display: 'flex', flexDirection: 'column' as const, gap: 20 }}>
      {saveError && <span style={{ fontSize: 11, color: '#ef4444' }}>{saveError}</span>}
      <div style={s.field}>
        <span style={s.fieldLabel}>mlx-lm — Model search directories</span>
        <MlxPathList paths={config.mlx_lm_search_paths} onRemove={i => void removePath(i)} busy={busy} />
        <div style={{ display: 'flex', gap: 6, marginTop: 6 }}>
          <input
            style={{ ...s.input, flex: 1 }}
            value={newPath}
            onChange={e => setNewPath(e.target.value)}
            placeholder="/path/to/models"
            onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); void addPath() } }}
          />
          <button style={s.btn()} onClick={() => void addPath()} disabled={busy || !newPath.trim()}>Add</button>
        </div>
      </div>
      <div style={s.field}>
        <span style={s.fieldLabel}>Ollama — Executable path</span>
        <div style={{ display: 'flex', gap: 6 }}>
          <input
            style={{ ...s.input, flex: 1 }}
            value={ollamaExe}
            onChange={e => setOllamaExe(e.target.value)}
            placeholder="ollama (uses system PATH)"
          />
          <button style={s.btn()} onClick={() => void saveOllamaExe()} disabled={busy}>Save</button>
        </div>
      </div>
      <DiscoveredModelsSection />
    </div>
  )
}

// ─── Detail panel ─────────────────────────────────────────────────────────────

function DetailPanel({ instance, phase, onRefresh }: {
  instance: ServerInstanceConfig
  phase: InstancePhase | undefined
  onRefresh: () => void
}) {
  const active = isActive(phase)
  const transitioning = phase?.type === 'starting' || phase?.type === 'stopping'
  const [confirmingRemove, setConfirmingRemove] = useState(false)

  const handleStart = async () => {
    await startWithWarnings(instance.id, async msg => {
      setConfirmingRemove(false)
      return new Promise(resolve => {
        // Use inline state for start warnings too — avoids confirm() suppression
        const ok = window.confirm(msg + '\n\nStart anyway?')
        resolve(ok)
      })
    })
    onRefresh()
  }

  const handleStop = async () => {
    await ipc.stopInstance(instance.id)
    onRefresh()
  }

  const handleRemove = async () => {
    setConfirmingRemove(false)
    await ipc.removeInstance(instance.id)
  }

  return (
    <div style={s.detailPane}>
      <div style={s.detailHeader}>
        <span style={s.detailTitle}>{instance.name}</span>
        {confirmingRemove ? (
          <div style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
            <span style={{ fontSize: 12, color: '#c00' }}>Remove?</span>
            <button style={s.btn(true)} onClick={handleRemove}>Yes</button>
            <button style={s.btn()} onClick={() => setConfirmingRemove(false)}>No</button>
          </div>
        ) : (
          <button style={s.btn(true)} onClick={() => setConfirmingRemove(true)}>Remove</button>
        )}
      </div>
      <div style={s.detailBody}>
        <div style={s.field}>
          <span style={s.fieldLabel}>Phase</span>
          <div style={s.phaseRow}>
            <div style={s.dot(phaseColor(phase))} />
            <span style={s.fieldValue}>{phaseLabel(phase)}</span>
          </div>
        </div>
        <div style={s.btnRow}>
          {active && !transitioning && (
            <button style={s.btn()} onClick={handleStop}>Stop</button>
          )}
          {!active && !transitioning && (
            <button
              style={s.primaryBtn}
              onClick={handleStart}
              disabled={(instance.server_type === 'mlx-lm' || instance.server_type === 'ollama') && instance.selected_model_key === null}
            >Start</button>
          )}
        </div>
        <div style={s.field}>
          <span style={s.fieldLabel}>Server type</span>
          <span style={s.fieldValue}>{instance.server_type}</span>
        </div>
        <div style={s.field}>
          <span style={s.fieldLabel}>Port</span>
          <span style={s.fieldValue}>{instance.port}</span>
        </div>
        <div style={s.field}>
          <span style={s.fieldLabel}>Executable</span>
          <span style={{ ...s.fieldValue, wordBreak: 'break-all', fontSize: 11 }}>{instance.executable_path}</span>
        </div>
        {instance.server_type === 'mlx-lm' && (
          <ModelPathOverrideField instance={instance} onRefresh={onRefresh} />
        )}
        <ModelList instance={instance} phase={phase} onRefresh={onRefresh} />
        <ParamEditor instance={instance} onRefresh={onRefresh} />
        <label style={s.checkbox}>
          <input
            type="checkbox"
            checked={instance.start_on_launch}
            onChange={async e => {
              await ipc.setStartOnLaunch(instance.id, e.target.checked)
              onRefresh()
            }}
          />
          Start on app launch
        </label>
      </div>
    </div>
  )
}

// ─── SettingsApp ──────────────────────────────────────────────────────────────

type SettingsTab = 'servers' | 'discovery'

export default function SettingsApp() {
  const [activeTab, setActiveTab] = useState<SettingsTab>('servers')
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const clearSelected = useCallback(() => setSelectedId(null), [])
  const { instances, phases, refresh } = useInstances(clearSelected)
  const [showAddSheet, setShowAddSheet] = useState(false)

  const handleAdd = async (name: string, serverType: string, port: number, execPath: string, selectedModelKey: string | null) => {
    const id = await ipc.addInstance(name, serverType, port, execPath)
    try {
      if (selectedModelKey) await ipc.setSelectedModel(id, selectedModelKey)
    } catch {
      // Instance created; model selection failed. Continue — user can pick in the detail panel.
    }
    setShowAddSheet(false)
    await refresh()
    setSelectedId(id)
  }

  const selected = instances.find(i => i.id === selectedId)

  return (
    <div style={{ ...s.root, position: 'relative' }}>
      {showAddSheet && (
        <AddInstanceSheet onAdd={handleAdd} onCancel={() => setShowAddSheet(false)} />
      )}
      <div style={s.toolbar}>
        <button style={s.tabBtn(activeTab === 'servers')} onClick={() => setActiveTab('servers')}>Servers</button>
        <button style={s.tabBtn(activeTab === 'discovery')} onClick={() => setActiveTab('discovery')}>Discovery</button>
      </div>
      {activeTab === 'discovery' ? (
        <DiscoveryTab />
      ) : (
        <div style={s.body}>
          <div style={s.masterPane}>
            <div style={s.masterHeader}>
              <span style={{ fontWeight: 600, fontSize: 12, color: '#444' }}>Instances</span>
              <button style={s.addBtn} onClick={() => setShowAddSheet(true)}>+ Add</button>
            </div>
            <div style={s.masterList}>
              {instances.length === 0 && (
                <p style={s.placeholder}>No servers configured</p>
              )}
              {instances.map(inst => (
                <div
                  key={inst.id}
                  style={s.masterRow(inst.id === selectedId)}
                  onClick={() => setSelectedId(inst.id)}
                >
                  <div style={s.dot(phaseColor(phases[inst.id]))} />
                  <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {inst.name}
                  </span>
                </div>
              ))}
            </div>
          </div>
          <div style={s.detailPane}>
            {selected ? (
              <DetailPanel instance={selected} phase={phases[selected.id]} onRefresh={refresh} />
            ) : (
              <p style={s.placeholder}>Select a server to configure it</p>
            )}
          </div>
        </div>
      )}
    </div>
  )
}
