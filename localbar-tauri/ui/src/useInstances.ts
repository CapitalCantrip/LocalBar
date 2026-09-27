import { useCallback, useEffect, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { ipc } from './ipc'
import { type InstancePhase, type InstancePidDto, type ServerInstanceConfig } from './types'

async function fetchPids(instances: ServerInstanceConfig[]): Promise<Record<string, InstancePidDto | null>> {
  const entries = await Promise.all(
    instances.map(async i => [i.id, await ipc.getInstancePid(i.id)] as const)
  )
  return Object.fromEntries(entries)
}

export function useInstances(onRemoved?: () => void): {
  instances: ServerInstanceConfig[]
  phases: Record<string, InstancePhase>
  pids: Record<string, InstancePidDto | null>
  refresh: () => Promise<void>
} {
  const [instances, setInstances] = useState<ServerInstanceConfig[]>([])
  const [phases, setPhases] = useState<Record<string, InstancePhase>>({})
  const [pids, setPids] = useState<Record<string, InstancePidDto | null>>({})
  const onRemovedRef = useCallback(() => onRemoved?.(), [onRemoved])

  const refresh = useCallback(async () => {
    const [insts, ph] = await Promise.all([ipc.listInstances(), ipc.listInstancePhases()])
    setInstances(insts)
    setPhases(ph)
    setPids(await fetchPids(insts))
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

  return { instances, phases, pids, refresh }
}
