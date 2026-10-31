/**
 * 主页:品牌 logo + 预设文案 + 悬浮式输入框。
 * 直接在输入框里输入即可开启新会话(无需先点「新建对话」);
 * 会话创建后把内容写入 sessionStorage 草稿,由对话页一次性消费并直接发送。
 * 输入框工具条与对话页保持一致:附件 / 联网 / 思考 / 智能体。
 */
import { useNavigate } from 'react-router-dom';
import { useEffect, useMemo, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import Composer from '../components/Composer';
import AgentPickerDialog from '../components/AgentPickerDialog';
import { agentIconComponent } from '../components/icons';
import { useSessionsStore } from '../stores/sessions';
import { useProvidersStore } from '../stores/providers';
import { useAgentsStore } from '../stores/agents';
import { DEFAULT_THINKING_LEVEL, useThinkingStore } from '../stores/thinking';
import type { ThinkingSelection } from '../stores/thinking';
import * as ipc from '../lib/ipc';
import type { AttachmentMeta } from '../lib/ipc';

/** 首页默认提示语(用户可在设置 · 主题与字体大小 · 首页中自定义)。 */
export const DEFAULT_HOME_PROMPT = 'myAgent | 向更美好的生活出发！';

/** 首页选中的待上传附件:文件本体 + 占位元数据(真正落盘要等会话创建后) */
interface PendingFile {
  file: File;
  meta: AttachmentMeta;
}

function kindOf(name: string): AttachmentMeta['kind'] {
  const ext = name.split('.').pop()?.toLowerCase() ?? '';
  if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp'].includes(ext)) return 'image';
  if (ext === 'pdf') return 'pdf';
  if (['docx', 'doc'].includes(ext)) return 'docx';
  if (['xlsx', 'xls', 'csv', 'tsv'].includes(ext)) return 'xlsx';
  if (['txt', 'md', 'markdown', 'log', 'json', 'yaml', 'yml'].includes(ext)) return 'text';
  return 'other';
}

export default function HomePage() {
  const nav = useNavigate();
  const create = useSessionsStore((s) => s.create);
  const setWebSearch = useSessionsStore((s) => s.setWebSearch);
  const current = useProvidersStore((s) => s.current);
  const { templates, loaded, load } = useAgentsStore();
  const setThinking = useThinkingStore((s) => s.set);
  const [text, setText] = useState('');
  const [agentId, setAgentId] = useState<string | null>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [creating, setCreating] = useState(false);
  const [pending, setPending] = useState<PendingFile[]>([]);
  const [webOn, setWebOn] = useState(false);
  const [thinkSel, setThinkSel] = useState<ThinkingSelection>({
    on: true,
    level: DEFAULT_THINKING_LEVEL,
  });
  const [thinkCap, setThinkCap] = useState<ipc.ReasoningCapability | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [greeting, setGreeting] = useState(DEFAULT_HOME_PROMPT);

  // 首页提示语:读取设置里的自定义文案,并随设置保存实时更新
  useEffect(() => {
    let alive = true;
    const apply = (s: ipc.AppSettings) => {
      if (alive) setGreeting(s.home_prompt?.trim() || DEFAULT_HOME_PROMPT);
    };
    ipc
      .getSettings()
      .then(apply)
      .catch(() => {});
    const un = listen<ipc.AppSettings>('settings_changed', (e) => apply(e.payload));
    return () => {
      alive = false;
      void un.then((f) => f());
    };
  }, []);

  useEffect(() => {
    if (!loaded) void load();
  }, [loaded, load]);

  // 当前模型的思考能力(与会话页同源;查询失败按不支持处理,控件隐藏)
  useEffect(() => {
    let alive = true;
    if (!current) {
      setThinkCap(null);
      return;
    }
    ipc
      .getReasoningCapability(current.providerId, current.model)
      .then((cap) => alive && setThinkCap(cap))
      .catch(() => alive && setThinkCap(null));
    return () => {
      alive = false;
    };
  }, [current?.providerId, current?.model]);

  const agent = templates.find((t) => t.agent_id === agentId) ?? null;
  const attachments = useMemo(() => pending.map((p) => p.meta), [pending]);

  const pickFiles = (files: FileList) => {
    const arr = Array.from(files);
    const err = ipc.validateAttachmentFiles(arr, pending.length);
    if (err) {
      setError(err);
      return;
    }
    setError(null);
    const stamp = Date.now();
    setPending((prev) => [
      ...prev,
      ...arr.map((f, i) => ({
        file: f,
        meta: {
          name: f.name,
          path: `home-${stamp}-${i}`,
          kind: kindOf(f.name),
          size: f.size,
          extracted_text: null,
        },
      })),
    ]);
  };

  const removeAttachment = (index: number) => {
    setPending((prev) => prev.filter((_, i) => i !== index));
  };

  const onSend = async () => {
    const v = text.trim();
    // 空内容且无附件不建会话,避免留下空对话
    if ((!v && pending.length === 0) || creating) return;
    setCreating(true);
    setError(null);
    try {
      const title = v ? (v.length > 24 ? `${v.slice(0, 24)}…` : v) : pending[0]?.meta.name || '新对话';
      // 统一为 provider/model 格式,便于按会话回填模型
      const modelId = current ? `${current.providerId}/${current.model}` : '';
      const id = await create(title, modelId, agentId);

      // 会话级偏好:联网开关 / 思考档位(在跳转前写入,对话页直接沿用)
      if (webOn) {
        try {
          await setWebSearch(id, true);
        } catch {
          /* 联网开关失败不阻断发送 */
        }
      }
      setThinking(id, thinkSel);

      // 附件:会话已创建,落盘并交接待发送元数据
      if (pending.length > 0) {
        const metas: AttachmentMeta[] = [];
        const images: Record<string, string> = {};
        for (const { file } of pending) {
          const dataUrl = await ipc.readFileAsDataURL(file);
          const idx = dataUrl.indexOf(',');
          const b64 = idx >= 0 ? dataUrl.slice(idx + 1) : dataUrl;
          const meta = await ipc.saveAttachment(id, file.name, b64);
          metas.push(meta);
          if (meta.kind === 'image') images[meta.path] = dataUrl;
        }
        sessionStorage.setItem('metaagent.draft.att', JSON.stringify({ metas, images }));
      }

      if (v) sessionStorage.setItem('metaagent.draft', v);
      setText('');
      setPending([]);
      nav(`/chat/${id}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setCreating(false);
    }
  };

  return (
    <div className="stage home">
      <h2 className="greeting">{greeting}</h2>

      <Composer
        value={text}
        onChange={setText}
        onSend={() => void onSend()}
        sending={creating}
        attachments={attachments}
        onPickFiles={pickFiles}
        onRemoveAttachment={removeAttachment}
        webSearch={webOn}
        onToggleWebSearch={() => setWebOn((v) => !v)}
        thinking={
          thinkCap?.enabled && current
            ? {
                capability: thinkCap,
                providerId: current.providerId,
                model: current.model,
                selection: thinkSel,
                onSelect: setThinkSel,
              }
            : null
        }
      >
        <button
          type="button"
          className="tool-chip agent-chip-btn"
          title={agent ? `当前智能体:${agent.name}(点击切换)` : '点击选择智能体'}
          onClick={() => setPickerOpen(true)}
        >
          {(() => {
            const Icon = agentIconComponent(agent?.icon ?? 'bot');
            return <Icon size={13} />;
          })()}
          <span className="agent-chip-text">{agent?.name ?? '智能体'}</span>
        </button>
      </Composer>

      {error && (
        <div style={{ marginTop: 'var(--space-2)', fontSize: 'var(--font-caption)', color: 'var(--err)' }}>
          {error}
        </div>
      )}

      {pickerOpen && (
        <AgentPickerDialog
          currentAgentId={agentId}
          onSelect={(id) => {
            setAgentId(id);
            setPickerOpen(false);
          }}
          onClose={() => setPickerOpen(false)}
        />
      )}
    </div>
  );
}
