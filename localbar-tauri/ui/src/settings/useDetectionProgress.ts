import { useEffect, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import type { MlxDetectionProgressDto } from '../types'

const DETECTION_PROGRESS_EVENT = 'mlx-detection-progress'
const TICK_MS = 1000

export function useDetectionProgress(active: boolean): string | null {
  const [message, setMessage] = useState<string | null>(null)
  const [since, setSince] = useState<number | null>(null)
  const [now, setNow] = useState(Date.now())

  useEffect(() => {
    if (!active) return
    setMessage(null)
    setSince(null)
    const timer = setInterval(() => setNow(Date.now()), TICK_MS)
    const unlisten = listen<MlxDetectionProgressDto>(DETECTION_PROGRESS_EVENT, e => {
      setMessage(e.payload.message)
      setSince(current => current ?? Date.now())
    })
    return () => {
      clearInterval(timer)
      void unlisten.then(f => f())
    }
  }, [active])

  if (!active || message === null || since === null) return null
  const elapsed = Math.max(0, Math.floor((now - since) / 1000))
  return elapsed > 0 ? `${message} ${elapsed} s` : message
}
