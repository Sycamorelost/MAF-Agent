import { create } from 'zustand';
import * as ipc from '../lib/ipc';

/** AI 指令库(B3 / F1):内置 + 自定义指令,对话面板 / 设置页共用。 */
interface TextCommandsState {
  commands: ipc.TextCommand[];
  loaded: boolean;
  loading: boolean;
  load: () => Promise<void>;
  save: (cmd: ipc.TextCommand) => Promise<void>;
  remove: (id: string) => Promise<void>;
}

export const useTextCommandsStore = create<TextCommandsState>((set, get) => ({
  commands: [],
  loaded: false,
  loading: false,

  load: async () => {
    if (get().loading) return;
    set({ loading: true });
    try {
      const list = await ipc.listTextCommands();
      set({ commands: list, loaded: true });
    } finally {
      set({ loading: false });
    }
  },

  save: async (cmd) => {
    await ipc.saveTextCommand(cmd);
    await get().load();
  },

  remove: async (id) => {
    await ipc.deleteTextCommand(id);
    await get().load();
  },
}));

/** 按用途过滤:对话面板取 chat/both(已按 sort 排序;'selection' 为历史遗留值)。 */
export function commandsFor(commands: ipc.TextCommand[], scope: 'selection' | 'chat') {
  return commands.filter((c) => c.scope === scope || c.scope === 'both');
}
