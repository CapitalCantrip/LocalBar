import { invoke } from '@tauri-apps/api/core'
import type { InstancePhase, ServerInstanceConfig } from './types'

export const ipc = {
  listInstances: (): Promise<ServerInstanceConfig[]> =>
    invoke('list_instances'),

  listInstancePhases: (): Promise<Record<string, InstancePhase>> =>
    invoke('list_instance_phases'),

  addInstance: (
    name: string,
    serverType: string,
    port: number,
    executablePath: string,
  ): Promise<string> =>
    invoke('add_instance', { name, serverType, port, executablePath }),

  removeInstance: (id: string): Promise<void> =>
    invoke('remove_instance', { id }),

  getStartWarning: (id: string): Promise<string | null> =>
    invoke('get_start_warning', { id }),

  checkMemoryWarning: (id: string): Promise<string | null> =>
    invoke('check_memory_warning', { id }),

  startInstance: (id: string): Promise<void> =>
    invoke('start_instance', { id }),

  stopInstance: (id: string): Promise<void> =>
    invoke('stop_instance', { id }),

  setStartOnLaunch: (id: string, value: boolean): Promise<void> =>
    invoke('set_start_on_launch', { id, value }),

  setSelectedModel: (id: string, modelKey: string | null): Promise<void> =>
    invoke('set_selected_model', { id, modelKey }),

  openSettings: (): Promise<void> =>
    invoke('open_settings'),

  quitApp: (): Promise<void> =>
    invoke('quit_app'),
}
