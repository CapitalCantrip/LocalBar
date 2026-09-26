import { ipc } from './ipc'

export async function startWithWarnings(
  id: string,
  onWarning: (message: string) => Promise<boolean>,
): Promise<boolean> {
  const warning = await ipc.getStartWarning(id)
  if (warning && !(await onWarning(warning))) return false

  const memWarning = await ipc.checkMemoryWarning(id)
  if (memWarning && !(await onWarning(memWarning))) return false

  await ipc.startInstance(id)
  return true
}
