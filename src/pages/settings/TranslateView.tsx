//! 设置 · 翻译视图(从 SettingsPage 搬出,纯搬家:视觉与行为不变)。
//! 术语表状态与 IPC 处理仍在 SettingsPage。

import type { RefObject } from 'react';
import * as ipc from '../../lib/ipc';
import { IconCheck, IconPlus, IconTrash, IconUpload } from '../../components/icons';

export interface TranslateViewProps {
  glossary: ipc.GlossaryEntry[] | null;
  glossaryDirty: boolean;
  glossaryMsg: string | null;
  savingGlossary: boolean;
  importRef: RefObject<HTMLInputElement | null>;
  onMutateGlossary: (next: ipc.GlossaryEntry[]) => void;
  onSaveGlossary: (next?: ipc.GlossaryEntry[]) => void;
  onImportGlossary: (file: File) => void;
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
}: TranslateViewProps) {
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
          {glossary?.map((entry, idx) => (
            <div className="set-row" key={idx} style={{ marginBottom: 'var(--space-2)' }}>
              <input
                className="pg-field"
                style={{ flex: 1 }}
                placeholder="原文词"
                value={entry.src}
                onChange={(e) =>
                  onMutateGlossary(
                    glossary.map((x, i) => (i === idx ? { ...x, src: e.target.value } : x)),
                  )
                }
              />
              <input
                className="pg-field"
                style={{ flex: 1 }}
                placeholder="指定译法"
                value={entry.dst}
                onChange={(e) =>
                  onMutateGlossary(
                    glossary.map((x, i) => (i === idx ? { ...x, dst: e.target.value } : x)),
                  )
                }
              />
              <input
                className="pg-field"
                style={{ flex: 1 }}
                placeholder="备注(可选)"
                value={entry.note ?? ''}
                onChange={(e) =>
                  onMutateGlossary(
                    glossary.map((x, i) => (i === idx ? { ...x, note: e.target.value } : x)),
                  )
                }
              />
              <button
                className="btn-ghost"
                title="删除词条"
                onClick={() => onMutateGlossary(glossary.filter((_, i) => i !== idx))}
              >
                <IconTrash size={14} />
              </button>
            </div>
          ))}
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
