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

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => setDraft(e.target.value)
  const handleBlur = () => void commit()
  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') e.currentTarget.blur()
  }

  return (
    <input
      style={{ ...s.input, ...extraStyle, ...inputStyle, opacity: saving ? 0.5 : 1 }}
      value={draft}
      placeholder={placeholder}
      onChange={handleChange}
      onBlur={handleBlur}
      onKeyDown={handleKeyDown}
    />
  )
}
