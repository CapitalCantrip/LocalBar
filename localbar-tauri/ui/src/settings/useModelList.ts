import { useCallback, useEffect, useRef, useState } from 'react'
import { ipc } from '../ipc'
import { type ModelRef } from '../types'

export function useModelList(instanceId: string): {
  models: ModelRef[]
  loading: boolean
  error: string | null
  refresh: () => Promise<void>
} {
  const [models, setModels] = useState<ModelRef[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const fetchRef = useRef(0)

  const refresh = useCallback(async () => {
    const seq = ++fetchRef.current
    setLoading(true)
    setError(null)
    try {
      const list = await ipc.listModels(instanceId)
      if (seq === fetchRef.current) setModels(list)
    } catch (e) {
      if (seq === fetchRef.current) setError(String(e))
    } finally {
      if (seq === fetchRef.current) setLoading(false)
    }
  }, [instanceId])

  useEffect(() => { void refresh() }, [refresh])

  return { models, loading, error, refresh }
}
