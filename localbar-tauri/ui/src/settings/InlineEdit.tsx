import { useEffect, useState } from 'react'
import { s } from './styles'

export function InlineEdit({ value, onSave, style: extraStyle, inputStyle, placeholder }: {
  value: string
  onSave: (v: string) => Promise<void>
  style?: React.CSSProperties
  inputStyle?: React.CSSProperties
  placeholder?: string
}) {
  const [draft, setDraft] = useState(value)
  const [saving, setSaving] = useState(false)

  useEffect(() => { setDraft(value) }, [value])

  const commit = async () => {
    if (draft.trim() === value) return
    setSaving(true)
    try { await onSave(draft.trim()) } finally { setSaving(false) }
  }

  return (
    <input
      style={{ ...s.input, ...extraStyle, ...inputStyle, opacity: saving ? 0.5 : 1 }}
      value={draft}
      placeholder={placeholder}
      onChange={e => setDraft(e.target.value)}
      onBlur={() => void commit()}
      onKeyDown={e => { if (e.key === 'Enter') { e.currentTarget.blur() } }}
    />
  )
}
