import { invoke } from '@tauri-apps/api/core'
import type { InstancePhase, ServerInstanceConfig } from './types'

export const ipc = {
  listInstances: (): Promise<ServerInstanceConfig[]> =>
    invoke('list_instances'),

  listInstancePhases: (): Promise<Record<string, InstancePhase>> =>
    invoke('list_instance_phases'),

  startInstance: (id: string): Promise<void> =>
    invoke('start_instance', { id }),

  stopInstance: (id: string): Promise<void> =>
    invoke('stop_instance', { id }),

  openSettings: (): Promise<void> =>
    invoke('open_settings'),

  quitApp: (): Promise<void> =>
    invoke('quit_app'),
}
