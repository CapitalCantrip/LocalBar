import { useEffect, useRef, useState } from 'react'
import { ipc } from '../ipc'
import {
  type InstancePhase,
  type ModelMetadata,
  type ModelRef,
  type ServerInstanceConfig,
  isActive,
} from '../types'
import { s } from './styles'

export function ModelList({ instance, phase, models, loading, error, onRefetch, onRefresh }: {
  instance: ServerInstanceConfig
  phase: InstancePhase | undefined
  models: ModelRef[]
  loading: boolean
  error: string | null
  onRefetch: () => Promise<void>
  onRefresh: () => void
}) {
  const [metaMap, setMetaMap] = useState<Record<string, ModelMetadata | null>>({})
  const fetchRef = useRef(0)

  useEffect(() => {
    const seq = ++fetchRef.current
    models.forEach(async m => {
      const meta = await ipc.fetchModelMetadata(instance.id, m.key).catch(() => null)
      if (seq !== fetchRef.current) return
      setMetaMap(prev => ({ ...prev, [m.key]: meta }))
    })
  }, [instance.id, models])

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
        <span style={{ ...s.fieldValue, color: '#ef4444', fontSize: 11 }}>
          {error === 'OLLAMA_UNREACHABLE' ? 'Ollama not reachable and no models found in its model folder' : error}
        </span>
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
            onClick={() => void onRefetch()}
          >↺</button>
        )}
      </div>
      {models.length === 0 && !loading && (
        <span style={{ ...s.fieldValue, color: '#aaa', fontSize: 11 }}>No models found</span>
      )}
      <div style={{ ...s.modelList, ...s.detailModelList }}>
        {models.map(m => (
          <div
            key={m.key}
            style={s.modelRow(m.key === instance.selected_model_key)}
            onClick={() => handleSelect(m.key)}
          >
            {m.publisher && <span style={{ ...s.modelMeta, marginRight: 4 }}>{m.publisher}</span>}
            <span style={s.modelName} title={m.key}>{m.display_name}</span>
            {metaLabel(m.key) && <span style={s.modelMeta}>{metaLabel(m.key)}</span>}
          </div>
        ))}
      </div>
    </div>
  )
}
