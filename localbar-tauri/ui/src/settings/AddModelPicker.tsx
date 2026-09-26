import { useEffect, useRef, useState } from 'react'
import { ipc } from '../ipc'
import { type ModelRef } from '../types'
import { s } from './styles'
import type { ServerTypeOption } from './AddInstanceSheet'

export const PICKER_TYPES: ServerTypeOption[] = ['mlx-lm', 'ollama']

export function AddModelPicker({ serverType, selectedModelKey, onSelect }: {
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
            {m.publisher && <span style={{ ...s.modelMeta, marginRight: 4 }}>{m.publisher}</span>}
            <span style={s.modelName} title={m.key}>{m.display_name}</span>
          </div>
        ))}
      </div>
    </div>
  )
}
