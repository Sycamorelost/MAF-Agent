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
import {
  IconEdit,
  IconHistory,
  IconLayers,
  IconLanguages,
  IconResearch,
  agentIconComponent,
} from '../components/icons';
import { useSessionsStore } from '../stores/sessions';
import { useProvidersStore } from '../stores/providers';
import { useAgentsStore } from '../stores/agents';
import { DEFAULT_THINKING_LEVEL, useThinkingStore } from '../stores/thinking';
import type { ThinkingSelection } from '../stores/thinking';
import * as ipc from '../lib/ipc';
import { sessionPath } from '../lib/workbench';
import type { AttachmentMeta } from '../lib/ipc';

/** 首页默认提示语(用户可在设置 · 主题与字体大小 · 首页中自定义)。 */
export const DEFAULT_HOME_PROMPT = 'myAgent | 向更美好的生活出发！';

/** 快捷开始:点击只填入输入框,不直接发送,避免误触发模型计费。 */
const QUICK_PROMPTS = [
  {
    key: 'write',
    title: '写作润色',
    desc: '起草 · 改写 · 总结',
    prompt: '帮我润色下面这段文字,保持原意,让表达更自然:',
    Icon: IconEdit,
  },
  {
    key: 'research',
    title: '深度研究',
    desc: '联网检索 · 多步分析',
    prompt: '请联网检索,围绕这个主题给出一份带引用来源的分析:',
    Icon: IconResearch,
  },
  {
    key: 'translate',
    title: '翻译对比',
    desc: '中英互译 · 术语统一',
    prompt: '翻译成英文,并给出两种更地道的表达:',
    Icon: IconLanguages,
  },
  {
    key: 'code',
    title: '代码助手',
    desc: '解释 · 调试 · 重构',
    prompt: '解释这段代码的作用,并指出潜在问题:',
    Icon: IconLayers,
  },
];

/** 最近对话的相对时间(失败兜底为空串,不阻塞渲染)。 */
function relTime(iso: string): string {
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return '';
  const diff = Date.now() - t;
  const min = Math.floor(diff / 60000);
  if (min < 1) return '刚刚';
  if (min < 60) return `${min} 分钟前`;
  const hours = Math.floor(min / 60);
  if (hours < 24) return `${hours} 小时前`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days} 天前`;
  const d = new Date(t);
  return `${d.getMonth() + 1}月${d.getDate()}日`;
}

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
  const sessions = useSessionsStore((s) => s.sessions);
  const refreshSessions = useSessionsStore((s) => s.refresh);
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
  /** 最近对话加载失败(非阻塞,仅小节内提示) */
  const [recentError, setRecentError] = useState(false);

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

  // 最近对话:进入首页时刷新一次,失败只在小节内提示,不阻塞主流程
  useEffect(() => {
    let alive = true;
    refreshSessions()
      .then(() => {
        if (alive) setRecentError(false);
      })
      .catch(() => {
        if (alive) setRecentError(true);
      });
    return () => {
      alive = false;
    };
  }, [refreshSessions]);

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
  /** 最近 3 个会话(按更新时间倒序;空标题兜底为「新对话」) */
  const recents = useMemo(
    () =>
      [...sessions]
        .sort((a, b) => new Date(b.updated).getTime() - new Date(a.updated).getTime())
        .slice(0, 3),
    [sessions],
  );

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
      nav(sessionPath(id));
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

      {/* 快捷开始:只填词不发送,零额外 token */}
      <section className="home-quick" aria-label="快捷开始">
        <div className="home-sec-head">快捷开始</div>
        <div className="home-quick-grid">
          {QUICK_PROMPTS.map(({ key, title, desc, prompt, Icon }) => (
            <button
              key={key}
              type="button"
              className="home-quick-card"
              title="点击填入输入框"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => setText(prompt)}
            >
              <span className="qi">
                <Icon size={14} />
              </span>
              <span className="qt">{title}</span>
              <span className="qd">{desc}</span>
            </button>
          ))}
        </div>
      </section>

      {(recents.length > 0 || recentError) && (
        <section className="home-recent" aria-label="最近对话">
          <div className="home-sec-head">
            <IconHistory size={13} />
            <span>最近对话</span>
          </div>
          {recentError ? (
            <div className="home-recent-err">最近对话加载失败,可重新进入首页重试</div>
          ) : (
            recents.map((s) => (
              <button
                key={s.id}
                type="button"
                className="home-recent-item"
                title={s.title || '新对话'}
                onClick={() => nav(sessionPath(s.id))}
              >
                <span className="t">{s.title || '新对话'}</span>
                <span className="m">{relTime(s.updated)}</span>
              </button>
            ))
          )}
        </section>
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
