import { useCallback, useEffect, useRef, useState } from 'react'
import { ipc } from '../ipc'
import { type DiscoveredModel } from '../types'
import { s } from './styles'

function fmtBytes(b: number | null): string {
  if (b === null) return '—'
  if (b >= 1e9) return `${(b / 1e9).toFixed(1)} GB`
  if (b >= 1e6) return `${(b / 1e6).toFixed(0)} MB`
  if (b >= 1e3) return `${(b / 1e3).toFixed(0)} KB`
  return `${b} B`
}

function fmtDate(secs: number | null): string {
  if (secs === null) return ''
  const d = new Date(secs * 1000)
  return d.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' })
}

function hfLink(m: DiscoveredModel): { href: string; label: string } {
  if (m.server_type === 'mlx-lm') {
    const slug = m.publisher ? `${m.publisher}/${m.display_name}` : m.key.split('/').slice(-2).join('/')
    return { href: `https://huggingface.co/${slug}`, label: 'HuggingFace ↗' }
  }
  const name = m.key.split(':')[0]
  return { href: `https://huggingface.co/models?search=${encodeURIComponent(name)}`, label: 'Search HF ↗' }
}

const colGrid: React.CSSProperties = {
  display: 'grid',
  gridTemplateColumns: '90px 1fr 70px 120px 65px 90px 30px',
  alignItems: 'center',
  gap: '0 8px',
  padding: '3px 0',
}

const colHdr: React.CSSProperties = {
  fontSize: 10,
  fontWeight: 600,
  color: '#aaa',
  textTransform: 'uppercase' as const,
  letterSpacing: '0.04em',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap' as const,
  paddingBottom: 2,
}

const colCell: React.CSSProperties = {
  fontSize: 11,
  color: '#666',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap' as const,
}

export function DiscoveredModelsSection() {
  const [models, setModels] = useState<DiscoveredModel[] | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const seqRef = useRef(0)

  const scan = useCallback(async () => {
    const seq = ++seqRef.current
    setLoading(true)
    setError(null)
    try {
      const list = await ipc.listAllDiscoveredModels()
      if (seq === seqRef.current) setModels(list)
    } catch (e) {
      if (seq === seqRef.current) setError(String(e))
    } finally {
      if (seq === seqRef.current) setLoading(false)
    }
  }, [])

  useEffect(() => { void scan() }, [scan])

  const grouped = (models ?? []).reduce<Record<string, DiscoveredModel[]>>((acc, m) => {
    ;(acc[m.server_type] ??= []).push(m)
    return acc
  }, {})

  return (
    <div style={s.field}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
        <span style={s.fieldLabel}>Discovered Models</span>
        {loading && <span style={s.refreshNote}>scanning…</span>}
        {!loading && (
          <button style={{ ...s.btn(), padding: '1px 6px', fontSize: 11 }} onClick={() => void scan()}>↺</button>
        )}
      </div>
      {error && <span style={{ fontSize: 11, color: '#ef4444' }}>{error}</span>}
      {models !== null && models.length === 0 && !loading && (
        <span style={{ fontSize: 11, color: '#aaa' }}>No models found</span>
      )}
      {Object.entries(grouped).map(([stype, rows]) => (
        <div key={stype} style={{ marginTop: 8 }}>
          <span style={{ ...s.fieldLabel, marginBottom: 4, display: 'block' }}>{stype}</span>
          <div style={colGrid}>
            <span style={colHdr}>Publisher</span>
            <span style={colHdr}>Model</span>
            <span style={colHdr}>Arch</span>
            <span style={colHdr}>Params · Quant</span>
            <span style={{ ...colHdr, textAlign: 'right' as const }}>Size</span>
            <span style={colHdr}>Modified</span>
            <span style={colHdr} />
          </div>
          {rows.map(m => {
            const link = hfLink(m)
            const paramsQuant = [m.parameter_count, m.quantization].filter(Boolean).join(' · ') || '—'
            return (
              <div key={m.key} style={{ ...colGrid, borderBottom: '1px solid #f0f0f0' }}>
                <span style={colCell} title={m.publisher ?? ''}>{m.publisher ?? '—'}</span>
                <span style={{ ...colCell, color: '#222', fontWeight: 500, fontSize: 12 }} title={m.key}>{m.display_name}</span>
                <span style={colCell}>{m.architecture ?? '—'}</span>
                <span style={colCell}>{paramsQuant}</span>
                <span style={{ ...colCell, textAlign: 'right' as const }}>{fmtBytes(m.size_bytes)}</span>
                <span style={colCell}>{fmtDate(m.modified_secs) || '—'}</span>
                <button
                  style={{ fontSize: 11, color: '#1d4ed8', background: 'none', border: 'none', cursor: 'pointer', padding: 0, alignSelf: 'center', textDecoration: 'underline' }}
                  onClick={() => void ipc.openUrl(link.href)}
                >{link.label}</button>
              </div>
            )
          })}
        </div>
      ))}
    </div>
  )
}
