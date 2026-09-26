export function isDoubleSlash(input: string): boolean {
  const pattern = /a\/\/b/
  return pattern.test(input)
}
