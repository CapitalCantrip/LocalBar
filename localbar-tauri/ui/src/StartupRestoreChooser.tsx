import { useState } from 'react'
import { ipc } from './ipc'
import { modelLabelForKey } from './modelLabel'
import { startSelected, usePendingStartup } from './startupRestore'
import type { ServerInstanceConfig, ServerType } from './types'

const SERVER_TYPE_LABELS: Record<ServerType, string> = {
  ollama: 'Ollama',
  'mlx-lm': 'mlx-lm',
  external: 'External',
}

const NO_MODEL_LABEL = 'No model selected'

const s = {
  overlay: {
    position: 'absolute' as const, inset: 0,
    background: 'rgba(0,0,0,0.4)', display: 'flex',
    alignItems: 'center', justifyContent: 'center', zIndex: 10,
  },
  box: {
    background: 'white', borderRadius: 8, padding: 16, width: 260,
    boxShadow: '0 4px 20px rgba(0,0,0,0.25)', color: '#1a1a1a',
  },
  title: { fontWeight: 600, fontSize: 13, margin: '0 0 4px' },
  hint: { fontSize: 11, color: '#666', lineHeight: 1.4, margin: '0 0 10px' },
  list: { display: 'flex', flexDirection: 'column' as const, gap: 8, marginBottom: 12, maxHeight: 220, overflowY: 'auto' as const },
  item: { display: 'flex', alignItems: 'flex-start', gap: 8, cursor: 'pointer', fontSize: 12 },
  name: { fontWeight: 500 },
  detail: { fontSize: 11, color: '#666', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' as const, maxWidth: 210 },
  row: { display: 'flex', gap: 8, justifyContent: 'flex-end' },
  btn: (primary?: boolean): React.CSSProperties => ({
    padding: '3px 8px', border: '1px solid #ccc', borderRadius: 4,
    background: primary ? '#1d4ed8' : 'white',
    color: primary ? 'white' : '#1a1a1a',
    cursor: 'pointer', fontSize: 11, fontFamily: 'inherit',
  }),
}

export function StartupRestoreChooser({ instances, onWarning, onDone }: {
  instances: ServerInstanceConfig[]
  onWarning: (id: string, message: string) => Promise<boolean>
  onDone: () => void
}) {
  const pending = usePendingStartup()
  const [unticked, setUnticked] = useState<Set<string>>(new Set())
  const [busy, setBusy] = useState(false)

  const candidates = pending
    .map(id => instances.find(i => i.id === id))
    .filter((i): i is ServerInstanceConfig => i !== undefined)
  if (busy || candidates.length === 0) return null

  const toggle = (id: string) => setUnticked(current => {
    const next = new Set(current)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    return next
  })

  const handleStart = async () => {
    setBusy(true)
    try {
      const ids = candidates.map(c => c.id)
      await startSelected(ids.filter(id => !unticked.has(id)), ids.filter(id => unticked.has(id)), onWarning)
    } finally {
      setBusy(false)
      onDone()
    }
  }

  const handleSkip = async () => {
    await ipc.skipStartupInstances(candidates.map(c => c.id))
    onDone()
  }

  return (
    <div style={s.overlay}>
      <div style={s.box} role="dialog" aria-label="Restore servers">
        <p style={s.title}>Restore servers?</p>
        <p style={s.hint}>These servers were set to start with LocalBar. Choose which to start now.</p>
        <div style={s.list}>
          {candidates.map(c => (
            <label key={c.id} style={s.item}>
              <input type="checkbox" checked={!unticked.has(c.id)} onChange={() => toggle(c.id)} />
              <span>
                <div style={s.name}>{c.name}</div>
                <div style={s.detail}>
                  {SERVER_TYPE_LABELS[c.server_type]} · {c.selected_model_key ? modelLabelForKey(c.selected_model_key) : NO_MODEL_LABEL}
                </div>
              </span>
            </label>
          ))}
        </div>
        <div style={s.row}>
          <button style={s.btn()} onClick={handleSkip}>Skip</button>
          <button style={s.btn(true)} onClick={handleStart} disabled={candidates.every(c => unticked.has(c.id))}>Start Selected</button>
        </div>
      </div>
    </div>
  )
}
