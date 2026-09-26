import { useEffect, useState } from 'react'
import { ipc, type ParamSchemaEntry } from '../ipc'
import { type ParamValues, type ServerInstanceConfig } from '../types'
import { s } from './styles'

export function ParamEditor({ instance, onRefresh }: {
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
