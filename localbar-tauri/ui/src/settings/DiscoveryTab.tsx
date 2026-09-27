import { useEffect, useState } from 'react'
import { ipc, pickPath } from '../ipc'
import { type DiscoveryConfig } from '../types'
import { s } from './styles'
import { DiscoveredModelsSection } from './DiscoveredModelsSection'

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

export function DiscoveryTab() {
  const [config, setConfig] = useState<DiscoveryConfig | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [newPath, setNewPath] = useState('')
  const [busy, setBusy] = useState(false)
  const [saveError, setSaveError] = useState<string | null>(null)

  const loadConfig = () => {
    setLoadError(null)
    ipc.getDiscoveryConfig().then(c => {
      setConfig(c)
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

  const addPath = async (path?: string) => {
    if (!config) return
    const value = (path ?? newPath).trim()
    if (!value) return
    const ok = await persist({ ...config, mlx_lm_search_paths: [...config.mlx_lm_search_paths, value] })
    if (ok) setNewPath('')
  }

  const choosePath = async () => {
    const picked = await pickPath({ directory: true })
    if (picked) await addPath(picked)
  }

  const removePath = async (i: number) => {
    if (!config) return
    await persist({ ...config, mlx_lm_search_paths: config.mlx_lm_search_paths.filter((_, idx) => idx !== i) })
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
          <button style={s.btn()} onClick={() => void choosePath()} disabled={busy}>Choose…</button>
          <button style={s.btn()} onClick={() => void addPath()} disabled={busy || !newPath.trim()}>Add</button>
        </div>
      </div>
      <DiscoveredModelsSection />
    </div>
  )
}
