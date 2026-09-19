import { useCallback, useState } from 'react'
import { ipc } from '../ipc'
import {
  type InstancePhase,
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
}

// ─── Add-instance sheet ───────────────────────────────────────────────────────

function AddInstanceSheet({ onAdd, onCancel }: {
  onAdd: (name: string, serverType: string, port: number, execPath: string) => Promise<void>
  onCancel: () => void
}) {
  const [name, setName] = useState('')
  const [port, setPort] = useState('11434')
  const [execPath, setExecPath] = useState('/usr/local/bin/ollama')
  const [busy, setBusy] = useState(false)

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim() || !execPath.trim()) return
    setBusy(true)
    try { await onAdd(name.trim(), 'ollama', parseInt(port, 10) || 11434, execPath.trim()) }
    finally { setBusy(false) }
  }

  return (
    <div style={s.sheet}>
      <form style={s.sheetBox} onSubmit={submit}>
        <p style={s.sheetTitle}>Add Ollama Instance</p>
        <div style={s.field}>
          <label style={s.fieldLabel}>Name</label>
          <input style={s.input} value={name} onChange={e => setName(e.target.value)} placeholder="My Ollama" autoFocus />
        </div>
        <div style={s.field}>
          <label style={s.fieldLabel}>Port</label>
          <input style={s.input} value={port} onChange={e => setPort(e.target.value)} />
        </div>
        <div style={s.field}>
          <label style={s.fieldLabel}>Executable path</label>
          <input style={s.input} value={execPath} onChange={e => setExecPath(e.target.value)} />
        </div>
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

// ─── Detail panel ─────────────────────────────────────────────────────────────

function DetailPanel({ instance, phase, onRefresh }: {
  instance: ServerInstanceConfig
  phase: InstancePhase | undefined
  onRefresh: () => void
}) {
  const active = isActive(phase)
  const transitioning = phase?.type === 'starting' || phase?.type === 'stopping'

  const handleStart = async () => {
    await startWithWarnings(instance.id, async msg => confirm(msg + '\n\nStart anyway?') ?? false)
    onRefresh()
  }

  const handleStop = async () => {
    await ipc.stopInstance(instance.id)
    onRefresh()
  }

  const handleRemove = async () => {
    if (!confirm(`Remove "${instance.name}"? This cannot be undone.`)) return
    await ipc.removeInstance(instance.id)
  }

  return (
    <div style={s.detailPane}>
      <div style={s.detailHeader}>
        <span style={s.detailTitle}>{instance.name}</span>
        <button style={s.btn(true)} onClick={handleRemove}>Remove</button>
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
            <button style={s.primaryBtn} onClick={handleStart}>Start</button>
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
        {instance.selected_model_key && (
          <div style={s.field}>
            <span style={s.fieldLabel}>Model</span>
            <span style={s.fieldValue}>{instance.selected_model_key}</span>
          </div>
        )}
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

export default function SettingsApp() {
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const clearSelected = useCallback(() => setSelectedId(null), [])
  const { instances, phases, refresh } = useInstances(clearSelected)
  const [showAddSheet, setShowAddSheet] = useState(false)

  const handleAdd = async (name: string, serverType: string, port: number, execPath: string) => {
    const id = await ipc.addInstance(name, serverType, port, execPath)
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
        <button style={s.tabBtn(true)}>Servers</button>
      </div>
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
    </div>
  )
}
