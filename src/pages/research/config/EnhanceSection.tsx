import { useEffect, useMemo, useRef, useState } from 'react';
import * as ipc from '../../../lib/ipc';
import { IconAttach, IconBrain, IconDoc } from '../../../components/icons';

/* 参考资料可接受的扩展名(与后端 classify 的文本类分支一致;图片需 OCR 不支持)。 */
const REF_ACCEPT =
  '.docx,.xlsx,.xlsm,.xls,.ods,.pdf,.txt,.md,.markdown,.csv,.tsv,.json,.yaml,.yml,.toml,.log';
/** 单文件上限,与后端 REF_MAX_BYTES 保持一致。 */
const REF_MAX_BYTES = 32 * 1024 * 1024;
/** 与后端 REF_TOTAL_BUDGET_CHARS 保持一致,用于提示实际注入量。 */
const REF_TOTAL_BUDGET_CHARS = 30_000;

/** 读本地文件为 base64(不含 data: 前缀)。 */
function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const fr = new FileReader();
    fr.onerror = () => reject(new Error(`读取文件失败:${file.name}`));
    fr.onload = () => {
      const s = String(fr.result ?? '');
      resolve(s.slice(s.indexOf(',') + 1));
    };
    fr.readAsDataURL(file);
  });
}

/** 科研增强(方法论/审计/生图)+ 参考资料导入。 */
export function EnhanceSection({
  params,
  imageProviders,
  onPatch,
}: {
  params: ipc.ResearchParams;
  imageProviders: ipc.ProviderTemplate[];
  onPatch: (p: Partial<ipc.ResearchParams>) => void;
}) {
  /** 参考资料文件选择框(隐藏,由「导入文件」按钮触发) */
  const refInput = useRef<HTMLInputElement>(null);
  const imgModels =
    imageProviders.find((t) => t.id === params.skill_image_provider)?.image_models ?? [];

  // 参考资料:导入中的文件名 + 各文件的格式/字符数(用于提示实际注入量)
  const [refBusy, setRefBusy] = useState<string | null>(null);
  const [refErr, setRefErr] = useState<string | null>(null);
  const [refInfo, setRefInfo] = useState<Record<string, ipc.ResearchReferenceInfo>>({});

  // 参考资料:载入本机已导入文件的元数据(字符数用于提示实际注入量)
  useEffect(() => {
    ipc
      .listResearchReferences()
      .then((list) => {
        const m: Record<string, ipc.ResearchReferenceInfo> = {};
        for (const r of list) m[r.name] = r;
        setRefInfo(m);
      })
      .catch(() => {
        /* 目录不存在等:忽略,首次导入时自动创建 */
      });
  }, []);

  // ---------- 参考资料:导入 / 移除 ----------
  const refFiles = params.reference_files;

  /** 导入本地文件:前端读为 base64 → 后端白名单落盘 + 提取文本。 */
  const onPickRefs = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files ?? []);
    e.target.value = ''; // 允许重复选择同一文件
    if (files.length === 0 || refBusy) return;
    setRefErr(null);
    const added: string[] = [];
    const infos: Record<string, ipc.ResearchReferenceInfo> = {};
    try {
      for (const f of files) {
        if (f.size > REF_MAX_BYTES) {
          setRefErr(`${f.name} 超出 32MB 上限,已跳过`);
          continue;
        }
        setRefBusy(f.name);
        const info = await ipc.importResearchReference(f.name, await fileToBase64(f));
        infos[info.name] = info;
        added.push(info.name);
      }
    } catch (err) {
      setRefErr(err instanceof Error ? err.message : String(err));
    } finally {
      setRefBusy(null);
    }
    if (added.length > 0) {
      setRefInfo((s) => ({ ...s, ...infos }));
      onPatch({ reference_files: Array.from(new Set([...refFiles, ...added])) });
    }
  };

  /** 移除参考资料:先删本机文件,再从注入列表摘除。 */
  const onRemoveRef = async (name: string) => {
    try {
      await ipc.deleteResearchReference(name);
    } catch {
      // 文件可能已被外部删除:此处不阻断,继续从列表摘除
    }
    onPatch({ reference_files: refFiles.filter((n) => n !== name) });
  };

  /** 当前已选资料的实际注入量(受后端 3 万字符总量上限约束)。 */
  const refInjected = useMemo(() => {
    let used = 0;
    for (const n of refFiles) {
      const c = refInfo[n]?.chars ?? 0;
      used += Math.min(Math.max(c, 0), Math.max(REF_TOTAL_BUDGET_CHARS - used, 0));
    }
    return used;
  }, [refFiles, refInfo]);

  /** 开启 AI 草稿时自动补全首个可用生图服务商/模型;再点关闭。 */
  const onToggleImageDraft = () => {
    if (!params.skill_image_draft && !params.skill_image_provider) {
      const first = imageProviders[0];
      onPatch({
        skill_image_draft: true,
        skill_image_provider: first?.id ?? null,
        skill_image_model: first?.image_models[0] ?? null,
      });
      return;
    }
    onPatch({ skill_image_draft: !params.skill_image_draft });
  };

  return (
    <>
      <div className="pg-sec">
        <div className="pg-lab">
          <IconBrain size={14} />
          科研增强
        </div>
        <div
          className="skill-row"
          onClick={() => onPatch({ skill_enhance: !params.skill_enhance })}
          role="presentation"
        >
          <div className="skill-row-main">
            <b>方法论注入</b>
            <span>内置研究方法论技能:检索构造 / 资料消化 / 写作纪律 / 图件契约,全部本地运行</span>
          </div>
          <div
            className={`set-switch${params.skill_enhance ? ' on' : ''}`}
            role="switch"
            aria-checked={params.skill_enhance}
          />
        </div>
        {params.skill_enhance && (
          <>
            <div
              className="skill-row"
              onClick={() => onPatch({ skill_audit: !params.skill_audit })}
              role="presentation"
            >
              <div className="skill-row-main">
                <b>深度审计</b>
                <span>终稿追加引用与统计审计附录,增加一次模型调用</span>
              </div>
              <div
                className={`set-switch${params.skill_audit ? ' on' : ''}`}
                role="switch"
                aria-checked={params.skill_audit}
              />
            </div>
            <div
              className="skill-row"
              onClick={onToggleImageDraft}
              role="presentation"
            >
              <div className="skill-row-main">
                <b>AI 概念示意草稿</b>
                <span>终稿追加一张概念草图并标注「AI 生成」,不构成证据</span>
              </div>
              <div
                className={`set-switch${params.skill_image_draft ? ' on' : ''}`}
                role="switch"
                aria-checked={params.skill_image_draft}
              />
            </div>
            {params.skill_image_draft && (
              <div className="pg-pair">
                <select
                  className="pg-field"
                  value={params.skill_image_provider ?? ''}
                  onChange={(e) => {
                    const id = e.target.value;
                    const models =
                      imageProviders.find((t) => t.id === id)?.image_models ?? [];
                    onPatch({
                      skill_image_provider: id,
                      skill_image_model: models[0] ?? null,
                    });
                  }}
                  aria-label="草稿生图服务商"
                >
                  {imageProviders.map((t) => (
                    <option key={t.id} value={t.id}>
                      {t.display_name}
                    </option>
                  ))}
                </select>
                <select
                  className="pg-field"
                  value={params.skill_image_model ?? ''}
                  onChange={(e) => onPatch({ skill_image_model: e.target.value })}
                  aria-label="草稿生图模型"
                >
                  {imgModels.map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </select>
              </div>
            )}
            {params.skill_image_draft && imageProviders.length === 0 && (
              <div className="pg-hint" style={{ color: 'var(--err)' }}>
                未找到已配置密钥且提供生图模型的服务商,请先在设置页完成生图配置。
              </div>
            )}
          </>
        )}
        <div className="pg-hint">方法论注入默认开启;关闭后回退原始研究管线。</div>
      </div>

      {/* 参考资料:本地文件解析并注入研究提示词 */}
      <div className="pg-sec">
        <div className="pg-lab">
          <IconDoc size={14} />
          参考资料
        </div>
        <div className="ref-list">
          {refFiles.map((n) => {
            const info = refInfo[n];
            return (
              <div className="ref-item" key={n}>
                <IconDoc size={14} />
                <span className="ref-name" title={n}>
                  {n}
                </span>
                <span className="ref-meta">
                  {info ? `${info.kind} · ${info.chars.toLocaleString()} 字` : '已导入'}
                </span>
                <button
                  type="button"
                  className="ref-del"
                  title="移除该资料"
                  onClick={() => void onRemoveRef(n)}
                >
                  ×
                </button>
              </div>
            );
          })}
          {refFiles.length === 0 && (
            <div className="pg-hint">
              未导入参考资料。支持 docx / xlsx / xls / pdf 与文本类文件。
            </div>
          )}
        </div>
        <div className="set-actions" style={{ marginTop: 'var(--space-2)' }}>
          <button
            className="btn-ghost"
            disabled={!!refBusy}
            onClick={() => refInput.current?.click()}
          >
            <IconAttach size={14} />
            {refBusy ? `解析中:${refBusy}` : '导入文件'}
          </button>
        </div>
        <input
          ref={refInput}
          type="file"
          multiple
          accept={REF_ACCEPT}
          style={{ display: 'none' }}
          onChange={(e) => void onPickRefs(e)}
        />
        {refErr && (
          <div className="pg-hint" style={{ color: 'var(--err)' }}>
            {refErr}
          </div>
        )}
        <div className="pg-hint">
          文件仅在本机解析,正文提取后注入研究提示词(总量上限 3 万字符,超出按导入顺序截断)。
          图片与扫描件需 OCR,当前版本不支持。
          {refFiles.length > 0 && (
            <>
              {' '}
              当前实际注入约 {refInjected.toLocaleString()} 字符 ≈{' '}
              {Math.round(refInjected / 1.5).toLocaleString()} token
            </>
          )}
        </div>
      </div>
    </>
  );
}
