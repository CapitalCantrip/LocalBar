import { useCallback, useEffect, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { ipc } from './ipc'
import { type ReconnectNoticeDto } from './types'

const RECONNECT_NOTICE_VISIBLE_MS = 6000

export function useReconnectNotices(): Record<string, string> {
  const [notices, setNotices] = useState<Record<string, ReconnectNoticeDto>>({})
  const seenSeqs = useRef(new Set<number>())
  const pendingDismissals = useRef<ReconnectNoticeDto[]>([])

  const dismissLater = useCallback((notice: ReconnectNoticeDto) => {
    setTimeout(() => {
      setNotices(current => {
        if (current[notice.id]?.seq !== notice.seq) return current
        const { [notice.id]: _dismissed, ...rest } = current
        return rest
      })
    }, RECONNECT_NOTICE_VISIBLE_MS)
  }, [])

  const show = useCallback((notice: ReconnectNoticeDto) => {
    if (seenSeqs.current.has(notice.seq)) return
    seenSeqs.current.add(notice.seq)
    setNotices(current => {
      const existing = current[notice.id]
      return existing && existing.seq > notice.seq ? current : { ...current, [notice.id]: notice }
    })
    if (document.hasFocus()) dismissLater(notice)
    else pendingDismissals.current.push(notice)
  }, [dismissLater])

  useEffect(() => {
    const onFocus = () => {
      pendingDismissals.current.forEach(dismissLater)
      pendingDismissals.current = []
    }
    window.addEventListener('focus', onFocus)
    ipc.listReconnectNotices().then(list => list.forEach(show))
    const unlisten = listen<ReconnectNoticeDto>('instance-reconnected', e => show(e.payload))
    return () => {
      window.removeEventListener('focus', onFocus)
      unlisten.then(f => f())
    }
  }, [show, dismissLater])

  return Object.fromEntries(Object.entries(notices).map(([id, n]) => [id, n.message]))
}
