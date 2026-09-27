export type ModelLabelRef = { key: string, display_name: string, publisher: string | null }

export function formatModelKey(key: string): string {
  const trimmed = key.replace(/\/+$/, '')
  const segments = trimmed.split('/').filter(Boolean)
  if (segments.length === 0) return trimmed
  if (segments.length === 1) return segments[0]
  return segments.slice(-2).join('/')
}

export function modelLabelForKey(key: string, models?: readonly ModelLabelRef[] | null): string {
  const match = models?.find(m => m.key === key)
  if (!match) return formatModelKey(key)
  return match.publisher ? `${match.publisher}/${match.display_name}` : match.display_name
}
