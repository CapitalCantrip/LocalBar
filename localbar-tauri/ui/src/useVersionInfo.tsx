import { useEffect, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { ipc } from './ipc'
import type { UpdateCheckOutcome } from './types'

export function useVersionInfo() {
  const [version, setVersion] = useState<string | null>(null)
  const [update, setUpdate] = useState<UpdateCheckOutcome | null>(null)

  useEffect(() => {
    ipc.getAppVersion().then(setVersion).catch(() => {})
    ipc.getUpdateStatus().then(setUpdate).catch(() => {})
    const unlisten = listen<UpdateCheckOutcome>('update-status-changed', e => setUpdate(e.payload))
    return () => { void unlisten.then(f => f()) }
  }, [])

  return { version, update, setUpdate }
}

export function UpdateAvailableLink({ update }: { update: UpdateCheckOutcome | null }) {
  if (update?.type !== 'available') return null
  return (
    <a
      href={update.url}
      style={{ color: '#2563eb', cursor: 'pointer' }}
      onClick={e => { e.preventDefault(); void ipc.openUrl(update.url) }}
    >
      v{update.version} is available
    </a>
  )
}
