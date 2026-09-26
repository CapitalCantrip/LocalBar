import { useState } from 'react'
import { s } from './styles'
import { AddModelPicker } from './AddModelPicker'

export type ServerTypeOption = 'ollama' | 'mlx-lm' | 'external'

export const DEFAULTS: Record<ServerTypeOption, { port: string; execPath: string; namePlaceholder: string }> = {
  ollama: { port: '11434', execPath: '/usr/local/bin/ollama', namePlaceholder: 'My Ollama' },
  'mlx-lm': { port: '8080', execPath: 'uvx', namePlaceholder: 'My mlx-lm' },
  external: { port: '11434', execPath: '', namePlaceholder: 'My External Server' },
}

export function AddInstanceSheet({ onAdd, onCancel }: {
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
