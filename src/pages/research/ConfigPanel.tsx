import { useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useResearchStore } from '../../stores/research';
import { useProvidersStore } from '../../stores/providers';
import { TutorialList } from '../../components/TutorialCard';
import { ResearchTemplateDialog } from '../../components/ResearchTemplateDialog';
import { StagePipelineEditor } from '../../components/StagePipelineEditor';
import AsideDrawer from '../../components/AsideDrawer';
import * as ipc from '../../lib/ipc';
import { IconAttach, IconBrain, IconCheck, IconDoc, IconEdit, IconGlobe, IconPlay, IconPlus, IconResearch } from '../../components/icons';
import { DEPTH_PRESETS, SOURCE_MODES } from './constants';
import { estimateTokens, estimateStagedTokens, fmtTok } from './helpers';

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

export function ConfigPanel({
  onOpenLibrary,
  namingSignal,
  initialTopic,
}: {
  onOpenLibrary: () => void;
  namingSignal: number;
  /** 外部预填主题(学习规划「深挖研究」跳转时带入),仅在首次挂载生效 */
  initialTopic?: string;
}) {
  const {
    searchCfg,
    loadSearchCfg,
    start,
    blueprints,
    loadBlueprints,
    saveBlueprint,
    pendingBlueprint,
    consumeBlueprint,
    templates,
    loadTemplates,
  } = useResearchStore();
  const current = useProvidersStore((s) => s.current);
  const templatesProviders = useProvidersStore((s) => s.templates);
  const hasKey = useProvidersStore((s) => s.hasKey);
  const nav = useNavigate();
  /** 参考资料文件选择框(隐藏,由「导入文件」按钮触发) */
  const refInput = useRef<HTMLInputElement>(null);

  const [topic, setTopic] = useState(initialTopic ?? '');
  const [params, setParams] = useState<ipc.ResearchParams>({ ...ipc.DEFAULT_RESEARCH_PARAMS });
  const [isCustom, setIsCustom] = useState(false);
  const [modelSel, setModelSel] = useState(''); // '' = 跟随当前
  const [whitelistText, setWhitelistText] = useState('');
  const [bpNaming, setBpNaming] = useState(false);
  const [bpName, setBpName] = useState('');
  const [bpError, setBpError] = useState<string | null>(null);
  /** 当前配置来源方案名(null = 自定义配置) */
  const [blueprintName, setBlueprintName] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [tplDlg, setTplDlg] = useState<string | null>(null);
  // 参考资料:导入中的文件名 + 各文件的格式/字符数(用于提示实际注入量)
  const [refBusy, setRefBusy] = useState<string | null>(null);
  const [refErr, setRefErr] = useState<string | null>(null);
  const [refInfo, setRefInfo] = useState<Record<string, ipc.ResearchReferenceInfo>>({});

  useEffect(() => {
    loadSearchCfg();
    loadBlueprints();
    loadTemplates();
  }, [loadSearchCfg, loadBlueprints, loadTemplates]);

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

  // 方案库「应用 / 从当前配置另存」信号:打开命名 / 应用暂存方案
  useEffect(() => {
    if (namingSignal > 0) setBpNaming(true);
  }, [namingSignal]);

  useEffect(() => {
    if (!pendingBlueprint) return;
    const bp = consumeBlueprint();
    if (bp) applyBlueprintToPanel(bp);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pendingBlueprint]);

  const patch = (p: Partial<ipc.ResearchParams>) => {
    setBlueprintName(null); // 手动调整后配置与方案脱钩,变为自定义配置
    setParams((s) => ({ ...s, ...p }));
  };

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
      patch({ reference_files: Array.from(new Set([...refFiles, ...added])) });
    }
  };

  /** 移除参考资料:先删本机文件,再从注入列表摘除。 */
  const onRemoveRef = async (name: string) => {
    try {
      await ipc.deleteResearchReference(name);
    } catch {
      // 文件可能已被外部删除:此处不阻断,继续从列表摘除
    }
    patch({ reference_files: refFiles.filter((n) => n !== name) });
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

  const engineReady =
    !!searchCfg &&
    (searchCfg.engine === 'searxng' ? !!searchCfg.base_url : searchCfg.has_api_key);

  // 科研增强:草稿生图候选(已配 Key 且提供生图模型的服务商)
  const imageProviders = useMemo(
    () => templatesProviders.filter((t) => t.image_models.length > 0 && hasKey[t.id]),
    [templatesProviders, hasKey],
  );
  const imgModels =
    imageProviders.find((t) => t.id === params.skill_image_provider)?.image_models ?? [];

  /** 开启 AI 草稿时自动补全首个可用生图服务商/模型;再点关闭。 */
  const onToggleImageDraft = () => {
    if (!params.skill_image_draft && !params.skill_image_provider) {
      const first = imageProviders[0];
      patch({
        skill_image_draft: true,
        skill_image_provider: first?.id ?? null,
        skill_image_model: first?.image_models[0] ?? null,
      });
      return;
    }
    patch({ skill_image_draft: !params.skill_image_draft });
  };

  // 执行模型下拉:跟随当前 + 全部已配 key 的 provider×model
  const modelOptions = useMemo(() => {
    const out: { value: string; label: string }[] = [];
    for (const t of templatesProviders) {
      if (!hasKey[t.id]) continue;
      for (const m of t.models) {
        out.push({ value: `${t.id}::${m}`, label: `${t.display_name} · ${m}` });
      }
    }
    return out;
  }, [templatesProviders, hasKey]);

  const modelLabel = current
    ? `${templatesProviders.find((t) => t.id === current.providerId)?.display_name ?? current.providerId} · ${current.model}`
    : '未选择';

  const applyBlueprintToPanel = (bp: ipc.Blueprint) => {
    setParams({ ...ipc.DEFAULT_RESEARCH_PARAMS, ...bp.params });
    setIsCustom(![4, 8, 15].includes(bp.params.max_rounds));
    setWhitelistText(bp.params.whitelist_domains.join(', '));
    setBlueprintName(bp.name);
    setBpNaming(false);
    setBpError(null);
  };

  const onSaveBlueprint = async () => {
    const name = bpName.trim();
    if (!name) return;
    setBpError(null);
    try {
      await saveBlueprint(name, '', {
        ...params,
        whitelist_domains: parseDomains(whitelistText),
      });
      setBlueprintName(name);
      setBpName('');
      setBpNaming(false);
    } catch (e) {
      setBpError(e instanceof Error ? e.message : String(e));
    }
  };

  const parseDomains = (text: string) =>
    text
      .split(/[,,\s]+/)
      .map((s) => s.trim())
      .filter(Boolean);

  const onStart = async () => {
    if (!topic.trim() || starting) return;
    setStarting(true);
    setError(null);
    try {
      const domains = parseDomains(whitelistText);
      const finalParams = { ...params, whitelist_domains: domains };
      let override: { providerId: string; model: string } | null = null;
      if (modelSel) {
        const [providerId, ...rest] = modelSel.split('::');
        override = { providerId, model: rest.join('::') };
      }
      await start(topic.trim(), finalParams, override, blueprintName);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setStarting(false);
    }
  };

  const est = params.mode === 'staged' ? estimateStagedTokens(params) : estimateTokens(params);

  return (
    <>
      <AsideDrawer>
      <aside className="pg-aside rp-config drawer">
      <div className="pg-aside-head">
        <h2>
          <IconResearch size={18} />
          研究配置
        </h2>
        <p>点标题栏「面板」可开合本栏;所有参数可调,可保存为预设复用。</p>
      </div>

      <div className="pg-sec">
        <div className="pg-lab">研究模式</div>
        <div className="mode-seg">
          <button
            className={params.mode === 'concise' ? 'on' : ''}
            onClick={() => patch({ mode: 'concise' })}
          >
            简洁研究
          </button>
          <button
            className={params.mode === 'legacy' ? 'on' : ''}
            onClick={() => patch({ mode: 'legacy' })}
          >
            快速研究
          </button>
          <button
            className={params.mode === 'staged' ? 'on' : ''}
            onClick={() =>
              patch(
                params.stages.length > 0
                  ? { mode: 'staged' }
                  : { mode: 'staged', stages: ipc.builtinStages() },
              )
            }
          >
            专项研究
          </button>
        </div>
        {params.mode === 'concise' && (
          <div className="pg-hint">
            简洁模式单对话多轮迭代:第 1 轮输出完整初稿,之后每轮自评上一稿不足 → 轻量检索补充 → 整稿重写,逐轮更准。
          </div>
        )}
        {params.mode === 'staged' && (
          <div className="pg-hint">
            专项模式按「阶段流水线」执行:每阶段独立参数与约束,前一阶段结论自动注入后续阶段。
          </div>
        )}
      </div>

      <div className="pg-sec">
        <div className="pg-lab">研究主题</div>
        <textarea
          className="pg-area"
          rows={3}
          placeholder="想研究什么?例如:2026 年端侧小模型推理优化方案对比"
          value={topic}
          onChange={(e) => setTopic(e.target.value)}
        />
      </div>

      <div className="pg-sec">
        <div className="pg-lab">{params.mode === 'concise' ? '迭代轮次' : '研究深度'}</div>
        {params.mode === 'concise' ? (
          <div className="pg-row">
            <span>轮数</span>
            <input
              className="pg-field num"
              type="number"
              min={1}
              max={10}
              value={params.max_rounds}
              onChange={(e) =>
                patch({ max_rounds: Math.max(1, Math.min(10, Number(e.target.value) || 1)) })
              }
            />
          </div>
        ) : (
          <>
        <div className="pg-seg joined">
          {DEPTH_PRESETS.map((p) => (
            <button
              key={p.rounds}
              className={!isCustom && params.max_rounds === p.rounds ? 'on' : ''}
              onClick={() => {
                setIsCustom(false);
                patch({ max_rounds: p.rounds });
              }}
            >
              {p.label}
            </button>
          ))}
          <button className={isCustom ? 'on' : ''} onClick={() => setIsCustom(true)}>
            自定义
          </button>
        </div>
        {isCustom && (
          <div className="pg-row">
            <span>轮次</span>
            <input
              className="pg-field num"
              type="number"
              min={1}
              max={25}
              value={params.max_rounds}
              onChange={(e) =>
                patch({ max_rounds: Math.max(1, Math.min(25, Number(e.target.value) || 1)) })
              }
            />
          </div>
        )}
          </>
        )}
      </div>

      <div className="pg-sec">
        <div className="pg-lab">信息源</div>
        <div className="pg-chips">
          {SOURCE_MODES.map((m) => (
            <button
              key={m.id}
              className={`pg-chip${params.source_mode === m.id ? ' on' : ''}`}
              onClick={() => patch({ source_mode: m.id })}
            >
              {m.label}
            </button>
          ))}
        </div>
        {params.source_mode === 'whitelist' && (
          <input
            className="pg-field"
            style={{ width: '100%', marginTop: 'var(--space-2)' }}
            placeholder="域名,逗号分隔,如 arxiv.org, github.com"
            value={whitelistText}
            onChange={(e) => setWhitelistText(e.target.value)}
          />
        )}
        <div className="pg-row">
          <span>每轮查询数</span>
          <select
            className="pg-field"
            value={params.queries_per_round}
            onChange={(e) => patch({ queries_per_round: Number(e.target.value) })}
          >
            {[2, 3, 4, 5, 6, 8].map((n) => (
              <option key={n} value={n}>
                {n} 条
              </option>
            ))}
          </select>
        </div>
        <div className="pg-row">
          <span>单源阅读长度</span>
          <select
            className="pg-field"
            value={params.read_length}
            onChange={(e) => patch({ read_length: e.target.value })}
          >
            <option value="digest">摘要(240 字)</option>
            <option value="full">扩展(≤1200 字)</option>
          </select>
        </div>
        <div className="pg-row">
          <span>反思策略</span>
          <select
            className="pg-field"
            value={params.reflect}
            onChange={(e) => patch({ reflect: e.target.value })}
          >
            <option value="every">每轮反思</option>
            <option value="every3">每 3 轮</option>
            <option value="final">仅开头规划</option>
          </select>
        </div>
      </div>

      <div className="pg-sec">
        <div className="pg-lab">执行模型</div>
        <select
          className="pg-field"
          style={{ width: '100%' }}
          value={modelSel}
          onChange={(e) => setModelSel(e.target.value)}
          title={modelSel ? undefined : modelLabel}
        >
          <option value="">跟随当前 · {modelLabel}</option>
          {modelOptions.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      </div>

      <div className="pg-sec">
        <div className="pg-lab">
          全局约束
          <span className="sp" />
          <span className="n">触发任一即收尾</span>
        </div>
        <div className="gc-grid">
          <div className="gc-item">
            <span className="l">总 Token 预算</span>
            <input
              className="pg-field num"
              type="number"
              min={0}
              step={10000}
              title="0 = 不限"
              value={params.token_budget}
              onChange={(e) => patch({ token_budget: Math.max(0, Number(e.target.value) || 0) })}
            />
          </div>
          <div className="gc-item">
            <span className="l">总检索次数 (0 不限)</span>
            <input
              className="pg-field num"
              type="number"
              min={0}
              max={200}
              value={params.max_searches}
              onChange={(e) => patch({ max_searches: Math.max(0, Number(e.target.value) || 0) })}
            />
          </div>
          {params.mode === 'staged' && (
            <>
              <div className="gc-item">
                <span className="l">总时长·分钟 (0 不限)</span>
                <input
                  className="pg-field num"
                  type="number"
                  min={0}
                  max={600}
                  value={params.max_duration_minutes}
                  onChange={(e) =>
                    patch({ max_duration_minutes: Math.max(0, Number(e.target.value) || 0) })
                  }
                />
              </div>
              <div className="gc-item">
                <span className="l">总资料上限 (0 不限)</span>
                <input
                  className="pg-field num"
                  type="number"
                  min={0}
                  max={500}
                  value={params.max_sources}
                  onChange={(e) =>
                    patch({ max_sources: Math.max(0, Number(e.target.value) || 0) })
                  }
                />
              </div>
              <div className="gc-item">
                <span className="l">查询并发 1-5</span>
                <input
                  className="pg-field num"
                  type="number"
                  min={1}
                  max={5}
                  value={params.concurrent_queries}
                  onChange={(e) =>
                    patch({
                      concurrent_queries: Math.max(1, Math.min(5, Number(e.target.value) || 1)),
                    })
                  }
                />
              </div>
              <div className="gc-item">
                <span className="l">单查询失败策略</span>
                <select
                  className="pg-field"
                  value={params.fail_policy}
                  onChange={(e) => patch({ fail_policy: e.target.value })}
                >
                  <option value="skip">跳过继续</option>
                  <option value="abort_stage">中止本阶段</option>
                  <option value="abort_research">中止研究</option>
                </select>
              </div>
              <div className="gc-item">
                <span className="l">URL 去重范围</span>
                <select
                  className="pg-field"
                  value={params.dedup_scope}
                  onChange={(e) => patch({ dedup_scope: e.target.value })}
                >
                  <option value="global">全程去重</option>
                  <option value="stage">仅阶段内</option>
                </select>
                {params.dedup_scope === 'stage' && (
                  <div className="pg-hint">分阶段去重下,资料上限仍按全局累计。</div>
                )}
              </div>
            </>
          )}
          <div className="gc-item">
            <span className="l">输出语言</span>
            <select
              className="pg-field"
              value={params.output_lang}
              onChange={(e) => patch({ output_lang: e.target.value })}
            >
              <option value="zh">中文</option>
              <option value="en">English</option>
            </select>
          </div>
        </div>
        {params.mode === 'staged' && (
          <div className="pg-row" style={{ marginTop: 'var(--space-2)' }}>
            <button
              type="button"
              className={`tool-chip${params.checkpoint ? ' on' : ''}`}
              onClick={() => patch({ checkpoint: !params.checkpoint })}
            >
              <IconCheck size={12} />
              检查点模式
            </button>
            <button
              type="button"
              className={`tool-chip${templates.some((t) => t.key === 'decide' && !t.builtin) ? ' on' : ''}`}
              title="全局决策提示:ReAct 循环中模型决策所用的系统提示"
              onClick={() => setTplDlg('decide')}
            >
              <IconEdit size={12} />
              全局决策提示
            </button>
            <button
              type="button"
              className={`tool-chip${templates.some((t) => t.key === 'report' && !t.builtin) ? ' on' : ''}`}
              title="终稿报告模板:研究报告的章节结构与撰写规则"
              onClick={() => setTplDlg('report')}
            >
              <IconDoc size={12} />
              终稿模板
            </button>
            <button
              type="button"
              className={`tool-chip${params.free_structure ? ' on' : ''}`}
              title="不使用报告模板,AI 根据主题与资料自行决定报告章节结构"
              onClick={() => patch({ free_structure: !params.free_structure })}
            >
              <IconEdit size={12} />
              自由结构
            </button>
          </div>
        )}
        <div className="pg-row" style={{ marginTop: 'var(--space-2)' }}>
          <span>搜索引擎</span>
          <span style={{ color: engineReady ? 'var(--ok)' : 'var(--warn)' }}>
            {searchCfg ? (engineReady ? '已就绪' : '未就绪') : '加载中…'}
          </span>
        </div>
        {params.mode === 'staged' && (
          <div className="pg-hint">
            检查点:每阶段结束暂停,可查看摘要、调整下阶段配置后再继续;高亮模板表示已自定义。
          </div>
        )}
      </div>

      {params.mode === 'staged' && (
        <div className="pg-sec">
          <StagePipelineEditor stages={params.stages} onChange={(stages) => patch({ stages })} />
        </div>
      )}

      {(params.mode === 'legacy' || params.mode === 'concise') && (
        <div className="pg-sec">
          <div className="pg-lab">提示词模板</div>
          <div className="pg-chips">
            <button
              className={`pg-chip${templates.some((t) => t.key === 'decide' && !t.builtin) ? ' on' : ''}`}
              onClick={() => setTplDlg('decide')}
              title="全局决策提示:ReAct 循环中模型决策所用的系统提示"
            >
              <IconEdit size={13} />
              全局决策提示
            </button>
            <button
              className={`pg-chip${templates.some((t) => t.key === 'report' && !t.builtin) ? ' on' : ''}`}
              onClick={() => setTplDlg('report')}
              title="终稿报告模板:研究报告的章节结构与撰写规则"
            >
              <IconDoc size={13} />
              终稿模板
            </button>
            <button
              className={`pg-chip${params.free_structure ? ' on' : ''}`}
              onClick={() => patch({ free_structure: !params.free_structure })}
              title="不使用报告模板,AI 根据主题与资料自行决定报告章节结构"
            >
              <IconEdit size={13} />
              自由结构
            </button>
          </div>
          <div className="pg-hint">
            {params.free_structure
              ? '已启用自由结构:忽略终稿模板,由 AI 自行决定报告章节结构。'
              : '高亮表示已自定义,对全部研究生效;点「恢复默认」回到内置模板。'}
          </div>
        </div>
      )}

      <div className="pg-sec">
        <div className="pg-lab">
          研究方案
          <span className="sp" />
          <span className="n">{blueprintName ?? '自定义配置'}</span>
        </div>
        <div className="pg-chips">
          {blueprints.slice(0, 5).map((bp) => (
            <span
              key={bp.name}
              className={`pg-chip${blueprintName === bp.name ? ' on' : ''}`}
              onClick={() => applyBlueprintToPanel(bp)}
              title={bp.desc || bp.name}
            >
              {bp.name}
            </span>
          ))}
          {!bpNaming && (
            <button className="pg-chip" onClick={() => setBpNaming(true)} title="将当前配置保存为方案">
              <IconPlus size={13} />
              存为方案
            </button>
          )}
          <button className="pg-chip" onClick={onOpenLibrary} title="打开方案库:全部方案 / 编辑 / 删除">
            <IconDoc size={13} />
            方案库
          </button>
        </div>
        {bpNaming && (
          <div className="pg-pair" style={{ marginTop: 'var(--space-2)' }}>
            <input
              className="pg-field"
              style={{ flex: 1 }}
              placeholder="方案名称,如:竞品周报"
              value={bpName}
              autoFocus
              onChange={(e) => setBpName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') void onSaveBlueprint();
                if (e.key === 'Escape') setBpNaming(false);
              }}
            />
            <button className="btn-ghost" onClick={() => void onSaveBlueprint()} disabled={!bpName.trim()}>
              <IconCheck size={14} />
            </button>
          </div>
        )}
        {bpError && <div className="pg-hint" style={{ color: 'var(--err)' }}>{bpError}</div>}
        <div className="pg-hint">方案 = 参数 + 阶段编排 + 提示词 + 约束,一体保存复用;点方案名一键应用。</div>
      </div>

      <div className="pg-sec">
        <div className="pg-lab">
          <IconBrain size={14} />
          科研增强
        </div>
        <div
          className="skill-row"
          onClick={() => patch({ skill_enhance: !params.skill_enhance })}
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
              onClick={() => patch({ skill_audit: !params.skill_audit })}
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
                    patch({
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
                  onChange={(e) => patch({ skill_image_model: e.target.value })}
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

      {!engineReady && (
        <div className="pg-warn">
          尚未配置可用的搜索引擎,请先到「设置 → 联网搜索」完成配置。
          <div className="set-actions" style={{ marginTop: 'var(--space-2)' }}>
            <button className="btn-ghost" onClick={() => nav('/settings')}>
              <IconGlobe size={14} />
              前往设置
            </button>
          </div>
        </div>
      )}
      {error && <div className="pg-warn">{error}</div>}

      <div className="pg-hint" style={{ color: 'var(--warn)' }}>
        研究任务消耗 token 较多,请留意预算设置。
      </div>

      <div className="pg-foot">
        <div className="pg-est">
          <span>方案</span>
          <b className="ellipsis" title={blueprintName ?? undefined}>
            {blueprintName ?? '自定义配置'}
          </b>
        </div>
        <div className="pg-est">
          <span>预计消耗</span>
          <b>
            {fmtTok(est)}
            {params.token_budget > 0 &&
            (params.mode === 'staged'
              ? estimateStagedTokens({ ...params, token_budget: 0 })
              : estimateTokens({ ...params, token_budget: 0 })) > params.token_budget
              ? ' · 触顶预算'
              : ''}
          </b>
        </div>
        {params.mode === 'staged' && (
          <div className="pg-est">
            <span>阶段</span>
            <b>
              {params.stages.filter((s) => s.enabled).length} / {params.stages.length} 启用
            </b>
          </div>
        )}
        <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
          <button
            className="btn-ghost"
            style={{ flex: 1, justifyContent: 'center', border: '1px solid var(--border)' }}
            onClick={() => setBpNaming(true)}
            title="将当前配置保存为方案"
          >
            <IconDoc size={13} />
            存为方案
          </button>
          <button
            className="pg-run"
            style={{ flex: 2 }}
            onClick={onStart}
            disabled={
              !topic.trim() ||
              !engineReady ||
              starting ||
              (params.source_mode === 'whitelist' && parseDomains(whitelistText).length === 0) ||
              (params.mode === 'staged' && params.stages.every((s) => !s.enabled))
            }
          >
            <IconPlay size={15} />
            {starting
              ? '启动中…'
              : params.mode === 'staged'
                ? '开始专项研究'
                : params.mode === 'concise'
                  ? '开始简洁研究'
                  : '开始研究'}
          </button>
        </div>
      </div>
      <TutorialList page="research" />
      </aside>
      </AsideDrawer>
      {tplDlg && <ResearchTemplateDialog tplKey={tplDlg} onClose={() => setTplDlg(null)} />}
    </>
  );
}

/* ---------------- 右主区:历史记录 ---------------- */




