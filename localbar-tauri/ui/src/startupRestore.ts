import { useCallback, useEffect, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { ipc } from './ipc'
import { startWithWarnings } from './startWithWarnings'

export function usePendingStartup(): string[] {
  const [pending, setPending] = useState<string[]>([])

  const refresh = useCallback(() => { void ipc.pendingStartupInstances().then(setPending) }, [])

  useEffect(() => {
    refresh()
    window.addEventListener('focus', refresh)
    const unlisten = listen<string[]>('startup-restore', e => setPending(e.payload))
    return () => {
      window.removeEventListener('focus', refresh)
      unlisten.then(f => f())
    }
  }, [refresh])

  return pending
}

export async function startSelected(
  selected: string[],
  declined: string[],
  onWarning: (id: string, message: string) => Promise<boolean>,
): Promise<void> {
  if (declined.length > 0) await ipc.skipStartupInstances(declined)
  for (const id of selected) {
    const started = await startWithWarnings(id, msg => onWarning(id, msg))
    if (!started) await ipc.skipStartupInstances([id])
  }
}
