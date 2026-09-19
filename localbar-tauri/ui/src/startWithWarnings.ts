import { ipc } from './ipc'

/**
 * Runs the start-warning → memory-warning → startInstance sequence.
 * `onWarning` receives the warning message and must return true to proceed.
 * Returns true if the instance was started, false if the user cancelled.
 */
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
