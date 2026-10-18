import { useMemo, useRef, useState } from 'react';
import { useResearchStore } from '../stores/research';
import { IconCheck, IconEdit, IconRefresh } from './icons';

/** 占位符中文说明(速查侧栏用)。 */
const PH_DESC: Record<string, string> = {
  topic: '研究主题',
  round: '当前轮次',
  max_rounds: '最大轮次',
  queries_per_round: '每轮查询数',
  references: '引用列表插入位置',
  output_lang: '输出语言',
};

/** 提取模板中的 {xxx} 占位符(去重,保持出现顺序)。 */
function extractPlaceholders(text: string): string[] {
  const out: string[] = [];
  for (const m of text.matchAll(/\{[a-zA-Z_][a-zA-Z0-9_]*\}/g)) {
    const name = m[0].slice(1, -1);
    if (!out.includes(name)) out.push(name);
  }
  return out;
}

/**
 * 研究提示词模板编辑弹窗(专项研究步骤 1)。
 * 布局对齐 plan/07-specialized-research-pages.html p6:左占位符速查 + 右编辑区 + 即时校验条。
 */
export function ResearchTemplateDialog({
  tplKey,
  onClose,
}: {
  tplKey: string;
  onClose: () => void;
}) {
  const { templates, saveTemplate, resetTemplate } = useResearchStore();
  const dto = templates.find((t) => t.key === tplKey);
  const [content, setContent] = useState(dto?.content ?? '');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const taRef = useRef<HTMLTextAreaElement>(null);

  // 即时校验:未知占位符提醒(不阻断保存);report 模板提示引用锚点
  const check = useMemo(() => {
    if (!dto) return { unknown: [] as string[], hasRefAnchor: false };
    const known = new Set(dto.placeholders);
    const unknown = extractPlaceholders(content).filter((n) => !known.has(n));
    return { unknown, hasRefAnchor: dto.key === 'report' && content.includes('{references}') };
  }, [content, dto]);

  if (!dto) return null;

  const insertPh = (name: string) => {
    const ta = taRef.current;
    const ph = `{${name}}`;
    if (!ta) {
      setContent((c) => c + ph);
      return;
    }
    const start = ta.selectionStart ?? content.length;
    const end = ta.selectionEnd ?? content.length;
    const next = content.slice(0, start) + ph + content.slice(end);
    setContent(next);
    requestAnimationFrame(() => {
      ta.focus();
      const pos = start + ph.length;
      ta.setSelectionRange(pos, pos);
    });
  };

  const onSave = async () => {
    if (saving) return;
    setSaving(true);
    setError(null);
    try {
      await saveTemplate(tplKey, content);
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  const onReset = async () => {
    if (saving) return;
    setSaving(true);
    setError(null);
    try {
      await resetTemplate(tplKey);
      const back = useResearchStore.getState().templates.find((t) => t.key === tplKey);
      setContent(back?.content ?? '');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="dialog-mask" onClick={onClose}>
      <div className="dialog tpl-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="dialog-head tpl-head">
          <IconEdit size={16} />
          编辑提示词模板 · {dto.name}
          <span className="sp" />
          <button className="btn-ghost" onClick={() => void onReset()} disabled={dto.builtin || saving}>
            <IconRefresh size={13} />
            恢复默认
          </button>
        </div>
        <div className="dialog-body tpl-body">
          <div className="tpl-side">
            <div className="grp">可用占位符(点击插入)</div>
            {dto.placeholders.map((ph) => (
              <div key={ph} className="ph-item" onClick={() => insertPh(ph)}>
                <code>{`{${ph}}`}</code>
                <span className="d">{PH_DESC[ph] ?? ph}</span>
              </div>
            ))}
            <div className="grp">说明</div>
            <div className="tpl-note">
              {dto.key === 'report'
                ? '未写 {references} 时引用列表自动追加到文末。未知占位符原样保留并在下方提示。模板保存后对全部研究生效。'
                : '轮次信息在执行时注入。未知占位符原样保留并在下方提示。模板保存后对全部研究生效。'}
            </div>
            <div className="tpl-note">
              生效优先级:阶段参数(阶段内覆盖)&gt; 研究方案 &gt; 全局自定义(本弹窗)&gt; 内置默认。
            </div>
          </div>
          <div className="tpl-main">
            <textarea
              ref={taRef}
              value={content}
              spellCheck={false}
              onChange={(e) => setContent(e.target.value)}
            />
            {check.unknown.length === 0 ? (
              <div className="tpl-warn">
                <IconCheck size={13} />
                校验通过:占位符全部有效
                {check.hasRefAnchor ? ',包含 {references} 引用锚点' : ''}
              </div>
            ) : (
              <div className="tpl-warn warn">
                未知占位符(执行时原样保留):{check.unknown.map((n) => `{${n}}`).join('、')}
              </div>
            )}
          </div>
        </div>
        <div className="tpl-foot">
          {error ? (
            <span className="tpl-foot-err">{error}</span>
          ) : (
            <span className="tpl-foot-hint">
              {dto.builtin ? '当前为内置模板,保存后生成自定义覆盖' : '当前为自定义模板,「恢复默认」回到内置模板'}
            </span>
          )}
          <span className="sp" />
          <button className="btn-ghost" onClick={onClose} disabled={saving}>
            取消
          </button>
          <button className="btn-primary" onClick={() => void onSave()} disabled={saving || !content.trim()}>
            <IconCheck size={13} />
            {saving ? '保存中…' : '保存模板'}
          </button>
        </div>
      </div>
    </div>
  );
}
