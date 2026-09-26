import { useEffect, useState } from 'react'
import { ipc } from '../ipc'
import { type ServerInstanceConfig } from '../types'
import { s } from './styles'

export function ModelPathOverrideField({ instance, onRefresh }: {
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
