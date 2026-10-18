import { create } from 'zustand';
import * as ipc from '../lib/ipc';

interface AgentsState {
  templates: ipc.AgentTemplate[];
  loaded: boolean;
  loading: boolean;
  load: () => Promise<void>;
  save: (agent: ipc.AgentTemplate) => Promise<void>;
  remove: (agentId: string) => Promise<void>;
}

export const useAgentsStore = create<AgentsState>((set, get) => ({
  templates: [],
  loaded: false,
  loading: false,

  load: async () => {
    if (get().loading) return;
    set({ loading: true });
    try {
      const list = await ipc.listAgentTemplates();
      set({ templates: list, loaded: true });
    } finally {
      set({ loading: false });
    }
  },

  save: async (agent) => {
    await ipc.saveAgentTemplate(agent);
    await get().load();
  },

  remove: async (agentId) => {
    await ipc.deleteAgentTemplate(agentId);
    await get().load();
  },
}));
