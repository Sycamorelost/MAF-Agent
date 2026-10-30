//! 上下文与记忆页签(P6 / 设计稿):大%用量卡(图例+统计)+ 一键压缩 + memory.md 摘要卡
//! + 最近写入 + 说明 + 底部状态行。

import { useEffect, useState } from 'react';
import {
  getSettings,
  listMemoryVersions,
  rollbackMemory,
  summarizeMemory,
  writeMemory,
} from '../../../../lib/ipc';
import { renderFull } from '../../../../components/MemPanel';
import { IconCheck, IconInfo, IconRefresh, IconSparkles } from '../../../../components/icons';
import { useProvidersStore } from '../../../../stores/providers';
import type { WorkbenchData } from '../../useWorkbenchData';

export function ContextTab({ sessionId, data }: { sessionId: string; data: WorkbenchData }) {
  const { budget, memory, memoryError, refreshMemory } = data;
  const current = useProvidersStore((s) => s.current);
  const [draft, setDraft] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [versions, setVersions] = useState<number>(0);

  // 外部刷新(压缩/其它页写入)时同步草稿;用户编辑中不覆盖
  useEffect(() => {
    setDraft((d) => (d === null ? (memory?.body ?? null) : d));
  }, [memory]);

  useEffect(() => {
    listMemoryVersions(sessionId)
      .then((v) => setVersions(v.length))
      .catch(() => {});
  }, [sessionId, memory]);

  const pct = budget && budget.limit > 0 ? Math.min(100, (budget.tokens / budget.limit) * 100) : 0;
  const usedK = budget ? (budget.tokens / 1000).toFixed(1) : '0';
  const limitK = budget ? Math.round(budget.limit / 1000) : 0;
  const remain = Math.max(0, 100 - pct);
  const bodyChars = memory?.body?.length ?? 0;

  const compress = async () => {
    if (!current) {
      setError('未选择模型,无法压缩');
      return;
    }
    if (
      !window.confirm(
        '压缩将把当前对话提炼为记忆摘要。\n保留:任务计划、产物索引与最近 10 条对话;其余细节合并为摘要。\n继续?',
      )
    ) {
      return;
    }
    setBusy('compress');
    setError(null);
    try {
      const settings = await getSettings().catch(() => null);
      const content = await summarizeMemory({
        session_id: sessionId,
        provider_id: settings?.summarize_provider_id || current.providerId,
        model: settings?.summarize_model || current.model,
        custom_base_url: null,
        custom_display_name: null,
      });
      await writeMemory(sessionId, content, 'manual_compress');
      setDraft(null);
      await refreshMemory();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  const undo = async () => {
    setBusy('undo');
    setError(null);
    try {
      const vs = await listMemoryVersions(sessionId);
      if (vs.length < 2) {
        setError('没有可撤销的历史版本');
        return;
      }
      await rollbackMemory(sessionId, vs[1].id);
      setDraft(null);
      await refreshMemory();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  const save = async () => {
    if (!memory || draft === null) return;
    setBusy('save');
    setError(null);
    try {
      await writeMemory(sessionId, renderFull(memory.frontmatter, draft), 'inspector_edit');
      await refreshMemory();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  const dirty = draft !== null && memory !== null && draft !== memory.body;

  return (
    <div className="insp-list">
      {/* 用量卡 */}
      <div className="insp-ctx">
        <div className="insp-ctx-big">
          <span className={`insp-ctx-pct${budget?.over ? ' over' : ''}`}>{Math.round(pct)}%</span>
          <span className="insp-ctx-used">
            已用 {usedK}k / {limitK}k tokens
          </span>
        </div>
        <div className={`insp-ctx-bar${budget?.over ? ' over' : ''}`}>
          <i style={{ width: `${pct}%` }} />
        </div>
        <div className="insp-ctx-legend">
          <span>
            <i className="lg lg-chat" /> 对话 {Math.round(pct)}%
          </span>
          <span>
            <i className="lg lg-tool" /> 工具结果
          </span>
          <span>
            <i className="lg lg-mem" /> 记忆
          </span>
          <span>
            <i className="lg lg-left" /> 剩余 {remain}%
          </span>
        </div>
        <div className="insp-ctx-stats">
          消息 {data.messages.length} 条 · 压缩记录 {versions} 次
        </div>
        <div className="insp-ctx-actions">
          <button
            type="button"
            className="btn-brand"
            disabled={busy !== null}
            title="调 LLM 提炼记忆,压缩注入体积(结果可继续编辑)"
            onClick={() => void compress()}
          >
            <IconSparkles size={13} />
            {busy === 'compress' ? '压缩中…' : '一键压缩'}
          </button>
          <button
            type="button"
            className="btn-ghost"
            disabled={busy !== null || versions < 2}
            title="回退到上一次压缩前"
            onClick={() => void undo()}
          >
            <IconRefresh size={13} />
            撤销上次
          </button>
        </div>
        {budget?.over && <div className="insp-ctx-warn">已超预算,发送时会自动压缩</div>}
        <div className="insp-ctx-note">
          压缩保留:任务计划、产物索引、最近 10 条对话。原始消息仍可在会话存储中查看。
        </div>
      </div>

      {/* memory.md 摘要卡 */}
      <div className="ins-label">
        memory.md
        <span className="sp" />
        <span className="n">项目记忆 · 自动注入</span>
      </div>
      <div className="insp-mem">
        <div className="insp-ctx-head">
          <IconInfo size={13} />
          <span>本次会话摘要</span>
          <span className="sp" />
          <span className="insp-ctx-num">
            {bodyChars} / 800
            {dirty && <span className="insp-ctx-dirty">未保存</span>}
          </span>
        </div>
        {memoryError && <div className="insp-err">{memoryError}</div>}
        {error && <div className="insp-err">{error}</div>}
        {memory && draft !== null ? (
          <>
            <textarea
              className="insp-mem-editor"
              value={draft}
              spellCheck={false}
              onChange={(e) => setDraft(e.target.value)}
            />
            <div className="insp-ctx-actions spread">
              <button
                type="button"
                className="btn-ghost btn-xs"
                disabled={!dirty || busy !== null}
                onClick={() => setDraft(memory.body)}
              >
                重置
              </button>
              <span className="sp" />
              <button
                type="button"
                className="btn-brand btn-xs"
                disabled={!dirty || busy !== null}
                onClick={() => void save()}
              >
                <IconCheck size={12} />
                {busy === 'save' ? '保存中…' : '保存记忆'}
              </button>
            </div>
          </>
        ) : (
          !memoryError && <div className="insp-empty">加载中…</div>
        )}
      </div>

      <div className="insp-note">
        <IconInfo size={12} />
        memory.md 仅在本项目生效;写入前会展示摘要,取消保存不会影响已有记忆。
      </div>
      <div className="insp-statusbar">
        记忆 {bodyChars} / 800 字
        <span className="sp" />
        自动压缩 40 条
      </div>
    </div>
  );
}
