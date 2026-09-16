import { useCallback, useEffect, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { ipc } from './ipc'
import { type InstancePhase, type ServerInstanceConfig } from './types'

export function useInstances(onRemoved?: () => void): {
  instances: ServerInstanceConfig[]
  phases: Record<string, InstancePhase>
  refresh: () => Promise<void>
} {
  const [instances, setInstances] = useState<ServerInstanceConfig[]>([])
  const [phases, setPhases] = useState<Record<string, InstancePhase>>({})
  const onRemovedRef = useCallback(() => onRemoved?.(), [onRemoved])

  const refresh = useCallback(async () => {
    const [insts, ph] = await Promise.all([ipc.listInstances(), ipc.listInstancePhases()])
    setInstances(insts)
    setPhases(ph)
  }, [])

  useEffect(() => {
    refresh()
    const id = setInterval(refresh, 1500)
    const unlisten = listen('phase-changed', refresh)
    const unlistenRemoved = listen('instance-removed', async () => {
      await refresh()
      onRemovedRef()
    })
    return () => {
      clearInterval(id)
      unlisten.then(f => f())
      unlistenRemoved.then(f => f())
    }
  }, [refresh, onRemovedRef])

  return { instances, phases, refresh }
}
