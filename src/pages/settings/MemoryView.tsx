//! 设置 · 记忆管理视图:提炼设置 + 原生记忆(会话 memory.md) + 长期记忆(知识图谱)。
//! 页面只做编排;三块内容各自独立在 SummarizeCard / NativeMemoryCard / LongTermMemoryPanel。

import * as ipc from '../../lib/ipc';
import { LongTermMemoryPanel } from '../../components/LongTermMemoryPanel';
import { SummarizeCard } from './SummarizeCard';
import { NativeMemoryCard } from './NativeMemoryCard';

export interface MemoryViewProps {
  settings: ipc.AppSettings | null;
  /** 已配置 Key 的服务商(仅这些可用于提炼) */
  keyedTemplates: ipc.ProviderTemplate[];
  settingsSaved: boolean;
  savingSettings: boolean;
  onPatchSettings: (patch: Partial<ipc.AppSettings>) => void;
  onSaveSettings: () => void;
}

export function MemoryView(props: MemoryViewProps) {
  return (
    <>
      <h2>记忆管理</h2>
      <p className="set-desc">
        原生记忆按会话保存为 memory.md(默认收起,展开后可编辑);长期记忆由「长期记忆」MCP
        工具跨会话共享。两处都可查看与管理。
      </p>

      <SummarizeCard
        settings={props.settings}
        keyedTemplates={props.keyedTemplates}
        settingsSaved={props.settingsSaved}
        savingSettings={props.savingSettings}
        onPatchSettings={props.onPatchSettings}
        onSaveSettings={props.onSaveSettings}
      />
      <NativeMemoryCard />
      <LongTermMemoryPanel />
    </>
  );
}
