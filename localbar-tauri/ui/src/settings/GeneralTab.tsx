import { useEffect, useState } from 'react'
import { ipc } from '../ipc'
import { type AppSettings } from '../types'
import { s } from './styles'

export function GeneralTab() {
  const [settings, setSettings] = useState<AppSettings | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [saveError, setSaveError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const load = () => {
    setLoadError(null)
    ipc.getAppSettings().then(setSettings).catch(e => setLoadError(String(e)))
  }

  useEffect(() => { load() }, [])

  const persist = async (updated: AppSettings) => {
    setBusy(true)
    setSaveError(null)
    try { await ipc.setAppSettings(updated); setSettings(updated) }
    catch (e) { setSaveError(String(e)) }
    finally { setBusy(false) }
  }

  if (loadError) return (
    <div style={{ padding: '14px 18px', display: 'flex', flexDirection: 'column' as const, gap: 8 }}>
      <span style={{ fontSize: 12, color: '#ef4444' }}>Failed to load settings: {loadError}</span>
      <button style={s.btn()} onClick={load}>Retry</button>
    </div>
  )

  if (!settings) return <p style={s.placeholder}>Loading…</p>

  return (
    <div style={{ padding: '14px 18px', display: 'flex', flexDirection: 'column' as const, gap: 20 }}>
      {saveError && <span style={{ fontSize: 11, color: '#ef4444' }}>{saveError}</span>}
      <div style={s.field}>
        <span style={s.fieldLabel}>On quit</span>
        <label style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 12 }}>
          <input
            type="checkbox"
            checked={settings.keep_servers_running_on_quit}
            disabled={busy}
            onChange={e => void persist({ ...settings, keep_servers_running_on_quit: e.target.checked })}
          />
          Keep servers running when LocalBar quits
        </label>
        <span style={{ fontSize: 11, color: '#aaa' }}>
          When off, servers LocalBar started are stopped on quit. Adopted and external servers are always left running.
        </span>
      </div>
    </div>
  )
}
