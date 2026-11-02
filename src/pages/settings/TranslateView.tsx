//! 设置 · 翻译视图(从 SettingsPage 搬出,纯搬家:视觉与行为不变)。
//! 术语表状态与 IPC 处理仍在 SettingsPage。

import { useMemo, useState, type RefObject } from 'react';
import * as ipc from '../../lib/ipc';
import { IconCheck, IconPlus, IconRefresh, IconTrash, IconUpload } from '../../components/icons';

/** 单次最多渲染的词条行数(导入预置后可能上千条,全量渲染会拖慢输入)。 */
const ROW_LIMIT = 60;

export interface TranslateViewProps {
  glossary: ipc.GlossaryEntry[] | null;
  glossaryDirty: boolean;
  glossaryMsg: string | null;
  savingGlossary: boolean;
  importRef: RefObject<HTMLInputElement | null>;
  onMutateGlossary: (next: ipc.GlossaryEntry[]) => void;
  onSaveGlossary: (next?: ipc.GlossaryEntry[]) => void;
  onImportGlossary: (file: File) => void;
  onImportPresetGlossary: () => void;
}

export function TranslateView({
  glossary,
  glossaryDirty,
  glossaryMsg,
  savingGlossary,
  importRef,
  onMutateGlossary,
  onSaveGlossary,
  onImportGlossary,
  onImportPresetGlossary,
}: TranslateViewProps) {
  const [q, setQ] = useState('');
  const [showAll, setShowAll] = useState(false);

  /** 过滤后的词条(带原始下标,编辑/删除仍作用于完整列表)。 */
  const rows = useMemo(() => {
    const all = (glossary ?? []).map((e, idx) => ({ e, idx }));
    const kw = q.trim().toLowerCase();
    if (!kw) return all;
    return all.filter(({ e }) =>
      `${e.src}\n${e.dst}\n${e.note ?? ''}`.toLowerCase().includes(kw),
    );
  }, [glossary, q]);

  const visible = showAll ? rows : rows.slice(0, ROW_LIMIT);

  return (
    <>
      <h2>翻译</h2>
      <p className="set-desc">
        术语表在翻译时自动注入,保证专有名词译法一致;落盘 data/glossary.json。
      </p>

      <div className="set-card">
        <div className="set-card-h">
          术语表
          <span className="st">
            <IconCheck size={13} />
            {glossary?.length ?? 0} 条
          </span>
        </div>
        <div className="set-card-b">
          {glossary === null && <div className="set-hint">加载中…</div>}
          {glossary !== null && glossary.length === 0 && (
            <div className="set-hint" style={{ marginBottom: 'var(--space-3)' }}>
              暂无术语。添加后,翻译原文命中「原文词」时将优先采用指定译法。
            </div>
          )}
          {glossary !== null && glossary.length > 0 && (
            <div className="set-row" style={{ marginBottom: 'var(--space-2)' }}>
              <input
                className="pg-field"
                style={{ flex: 1 }}
                placeholder="搜索原文词 / 译法 / 备注"
                value={q}
                onChange={(e) => setQ(e.target.value)}
              />
              <span className="set-hint" style={{ whiteSpace: 'nowrap' }}>
                {q.trim() ? `匹配 ${rows.length} 条` : `共 ${glossary.length} 条`}
              </span>
            </div>
          )}
          {visible.map(({ e: entry, idx }) => (
            <div className="set-row" key={idx} style={{ marginBottom: 'var(--space-2)' }}>
              <input
                className="pg-field"
                style={{ flex: 1 }}
                placeholder="原文词"
                value={entry.src}
                onChange={(ev) =>
                  onMutateGlossary(
                    (glossary ?? []).map((x, i) => (i === idx ? { ...x, src: ev.target.value } : x)),
                  )
                }
              />
              <input
                className="pg-field"
                style={{ flex: 1 }}
                placeholder="指定译法"
                value={entry.dst}
                onChange={(ev) =>
                  onMutateGlossary(
                    (glossary ?? []).map((x, i) => (i === idx ? { ...x, dst: ev.target.value } : x)),
                  )
                }
              />
              <input
                className="pg-field"
                style={{ flex: 1 }}
                placeholder="备注(可选)"
                value={entry.note ?? ''}
                onChange={(ev) =>
                  onMutateGlossary(
                    (glossary ?? []).map((x, i) => (i === idx ? { ...x, note: ev.target.value } : x)),
                  )
                }
              />
              <button
                className="btn-ghost"
                title="删除词条"
                onClick={() => onMutateGlossary((glossary ?? []).filter((_, i) => i !== idx))}
              >
                <IconTrash size={14} />
              </button>
            </div>
          ))}
          {!showAll && rows.length > ROW_LIMIT && (
            <button className="btn-ghost" onClick={() => setShowAll(true)}>
              显示全部 {rows.length} 条(再渲染全部行会变慢)
            </button>
          )}
          <div className="set-actions" style={{ marginTop: 'var(--space-3)' }}>
            <button
              className="btn-ghost"
              onClick={() =>
                onMutateGlossary([...(glossary ?? []), { src: '', dst: '', note: '' }])
              }
            >
              <IconPlus size={14} />
              添加词条
            </button>
            <button className="btn-ghost" onClick={() => importRef.current?.click()}>
              <IconUpload size={14} />
              导入 JSON
            </button>
            <button
              className="btn-ghost"
              title="从内置 8 类专业术语表导入中英双向词条(合并不覆盖,可反复点击)"
              onClick={onImportPresetGlossary}
              disabled={savingGlossary}
            >
              <IconRefresh size={14} />
              一键导入预置术语
            </button>
            <span className="sp" />
            <button
              className="btn-primary"
              onClick={() => void onSaveGlossary()}
              disabled={savingGlossary || !glossaryDirty}
            >
              {savingGlossary ? '保存中…' : glossaryDirty ? '保存术语表' : '已保存'}
            </button>
          </div>
          {glossaryMsg && (
            <div className="set-hint" style={{ marginTop: 'var(--space-2)' }}>
              {glossaryMsg}
            </div>
          )}
          <input
            ref={importRef}
            type="file"
            accept=".json,application/json"
            style={{ display: 'none' }}
            onChange={(e) => {
              const f = e.target.files?.[0];
              if (f) void onImportGlossary(f);
              e.target.value = '';
            }}
          />
        </div>
      </div>

      <div className="set-card">
        <div className="set-card-h">DeepL 专用引擎</div>
        <div className="set-card-b">
          <div className="set-hint">
            设置页「模型服务商」中配置 DeepL API Key 后,翻译页即可选用。DeepL 为专用翻译引擎,
            不参与对话与模型列表拉取;术语表对 DeepL 同样生效(译文通常已较稳定,术语注入以
            LLM 类引擎为主)。
          </div>
        </div>
      </div>
    </>
  );
}
