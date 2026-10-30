import { describe, expect, it } from 'vitest'
import { formatModelKey, modelLabelForKey } from './modelLabel'

describe('formatModelKey', () => {
  it('keeps a single segment as is', () => {
    expect(formatModelKey('llama')).toBe('llama')
  })

  it('keeps the last two segments of a path', () => {
    expect(formatModelKey('/models/mlx-community/Qwen')).toBe('mlx-community/Qwen')
  })

  it('ignores trailing slashes', () => {
    expect(formatModelKey('a/b/')).toBe('a/b')
  })

  it('returns the trimmed key when it has no segments', () => {
    expect(formatModelKey('///')).toBe('')
  })
})

describe('modelLabelForKey', () => {
  const models = [
    { key: 'k1', display_name: 'Qwen', publisher: 'mlx-community' },
    { key: 'k2', display_name: 'Llama', publisher: null },
  ]

  it('prefixes the publisher when the model is known', () => {
    expect(modelLabelForKey('k1', models)).toBe('mlx-community/Qwen')
  })

  it('uses the display name alone without a publisher', () => {
    expect(modelLabelForKey('k2', models)).toBe('Llama')
  })

  it('falls back to the formatted key for an unknown model', () => {
    expect(modelLabelForKey('/x/y/z', models)).toBe('y/z')
    expect(modelLabelForKey('/x/y/z')).toBe('y/z')
    expect(modelLabelForKey('/x/y/z', null)).toBe('y/z')
  })
})
