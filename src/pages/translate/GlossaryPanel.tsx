import { useMemo } from 'react';
import * as ipc from '../../lib/ipc';
import { TutorialList } from '../../components/TutorialCard';
import AsideDrawer from '../../components/AsideDrawer';
import { IconTranslate, IconStop } from '../../components/icons';

interface Props {
  readyTemplates: ipc.ProviderTemplate[];
  providerId: string;
  model: string;
  noModel: boolean;
  activeTemplate?: ipc.ProviderTemplate;
  onPickProvider: (id: string) => void;
  onModelChange: (model: string) => void;
  running: boolean;
  canRun: boolean;
  onRun: () => void;
  onStop: () => void;
  srcChars: number;
  text: string;
  glossary: ipc.GlossaryEntry[];
}

/** token 粗估:与后端一致的 chars/3 口径。 */
const estTokens = (chars: number) => Math.ceil(chars / 3);

const fmtTok = (n: number) => (n >= 1000 ? `≈ ${(n / 1000).toFixed(1)}k tok` : `≈ ${n} tok`);

/** 左栏「翻译设置」:引擎 / 术语表 / 运行按钮(术语表命中提示在此展示)。 */
export default function GlossaryPanel({
  readyTemplates,
  providerId,
  model,
  noModel,
  activeTemplate,
  onPickProvider,
  onModelChange,
  running,
  canRun,
  onRun,
  onStop,
  srcChars,
  text,
  glossary,
}: Props) {
  /** 术语表命中统计(与后端匹配口径一致:子串 + 忽略大小写)。 */
  const glossaryHits = useMemo(() => {
    if (!text.trim() || glossary.length === 0) return [];
    const lower = text.toLowerCase();
    return glossary.filter((e) => {
      const s = e.src.trim();
      return s && (text.includes(s) || lower.includes(s.toLowerCase()));
    });
  }, [text, glossary]);

  return (
    <AsideDrawer>
      <aside className="pg-aside drawer">
        <div className="pg-aside-head">
          <h2>
            <IconTranslate size={18} />
            翻译设置
          </h2>
          <p>点标题栏「面板」可开合本栏;仅输出译文,保留原文格式与术语一致性。</p>
        </div>

        <div className="pg-sec">
          <div className="pg-lab">翻译引擎</div>
          <select
            className="pg-field"
            value={providerId}
            onChange={(e) => onPickProvider(e.target.value)}
            style={{ marginBottom: noModel ? 0 : 'var(--space-2)' }}
          >
            {readyTemplates.length === 0 && <option value="">未配置</option>}
            {readyTemplates.map((t) => (
              <option key={t.id} value={t.id}>
                {t.display_name}
              </option>
            ))}
          </select>
          {!noModel && (
            <select className="pg-field" value={model} onChange={(e) => onModelChange(e.target.value)}>
              {(activeTemplate?.models ?? []).map((m) => (
                <option key={m} value={m}>
                  {m}
                </option>
              ))}
              {activeTemplate && !activeTemplate.models.includes(model) && model && (
                <option value={model}>{model}</option>
              )}
            </select>
          )}
          {noModel && <div className="pg-hint">专用翻译引擎,无需选择模型。</div>}
        </div>

        <div className="pg-sec">
          <div className="pg-lab">
            术语表
            <span className="sp" />
            <span className="n">{glossary.length} 条</span>
          </div>
          <div className="pg-hint">
            {glossary.length === 0
              ? '未配置术语表,可在 设置 → 翻译 中添加。'
              : glossaryHits.length > 0
                ? `原文命中 ${glossaryHits.length} 条,将按术语表翻译。`
                : '原文未命中任何词条。'}
          </div>
        </div>

        <div className="pg-foot">
          <div className="pg-est">
            <span>原文</span>
            <b>{srcChars} 字</b>
            <span className="pg-est-tok">{fmtTok(estTokens(srcChars))}</span>
          </div>
          {running ? (
            <button className="pg-run stop" onClick={onStop}>
              <IconStop size={15} />
              停止
            </button>
          ) : (
            <button className="pg-run" disabled={!canRun} onClick={onRun}>
              <IconTranslate size={15} />
              开始翻译
            </button>
          )}
        </div>
        <TutorialList page="translate" />
      </aside>
    </AsideDrawer>
  );
}
