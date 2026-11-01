import * as ipc from '../../lib/ipc';
import { TutorialList } from '../../components/TutorialCard';
import AsideDrawer from '../../components/AsideDrawer';
import { IconImage, IconSparkles } from '../../components/icons';

const SIZES = [
  { value: '1024x1024', label: '1024×1024' },
  { value: '1792x1024', label: '1792×1024' },
  { value: '1024x1792', label: '1024×1792' },
];

/** 风格 chip:标签 + 合并进 prompt 的英文关键词。
 * 关键词只描述「视觉风格」,不堆 8k / masterpiece / octane 这类无增益的质量词:
 * 后者对 FLUX / DALL·E 会被后端剥离,对 SD 也只是白占 token。 */
export const STYLES = [
  { id: 'none', label: '无', prompt: '' },
  { id: 'photo', label: '写实摄影', prompt: 'photorealistic photo, natural lighting, shallow depth of field' },
  { id: 'anime', label: '动漫', prompt: 'anime style, cel shading, clean line art' },
  { id: 'watercolor', label: '水彩', prompt: 'watercolor painting, soft colors, paper texture' },
  { id: 'oil', label: '油画', prompt: 'oil painting, visible brushstrokes, rich color' },
  { id: 'cyber', label: '赛博朋克', prompt: 'cyberpunk, neon lights, futuristic city' },
  { id: '3d', label: '3D 渲染', prompt: '3d render, soft studio lighting, clean surfaces' },
  { id: 'sketch', label: '线稿', prompt: 'pencil sketch, monochrome line art, hatching' },
];

interface Props {
  readyTemplates: ipc.ProviderTemplate[];
  providerId: string;
  model: string;
  activeTemplate?: ipc.ProviderTemplate;
  onPickProvider: (id: string) => void;
  onModelChange: (model: string) => void;
  prompt: string;
  onPromptChange: (v: string) => void;
  negative: string;
  onNegativeChange: (v: string) => void;
  /** 实时组装结果(字数 / token / 截断 / 告警);无有效输入时为 null */
  composed: ipc.ComposedPrompt | null;
  composedError: string | null;
  styleId: string;
  onStyleChange: (id: string) => void;
  size: string;
  onSizeChange: (v: string) => void;
  n: number;
  onNChange: (n: number) => void;
  error: string | null;
  running: boolean;
  canRun: boolean;
  onRun: () => void;
}

/** 左栏「生成参数」:模型 / Prompt / 风格 / 输出 / 运行按钮。 */
export default function DrawForm({
  readyTemplates,
  providerId,
  model,
  activeTemplate,
  onPickProvider,
  onModelChange,
  prompt,
  onPromptChange,
  negative,
  onNegativeChange,
  composed,
  composedError,
  styleId,
  onStyleChange,
  size,
  onSizeChange,
  n,
  onNChange,
  error,
  running,
  canRun,
  onRun,
}: Props) {
  return (
    <AsideDrawer>
      <aside className="pg-aside dw-params drawer">
        <div className="pg-aside-head">
          <h2>
            <IconImage size={18} />
            生成参数
          </h2>
          <p>点标题栏「面板」可开合本栏;图片自动保存到 data/images/,参数留档同名 meta.json。</p>
        </div>

        <div className="pg-sec">
          <div className="pg-lab">生成模型</div>
          <select
            className="pg-field"
            value={providerId}
            onChange={(e) => onPickProvider(e.target.value)}
            style={{ marginBottom: 'var(--space-2)' }}
          >
            {readyTemplates.length === 0 && <option value="">未配置</option>}
            {readyTemplates.map((t) => (
              <option key={t.id} value={t.id}>
                {t.display_name}
              </option>
            ))}
          </select>
          <input
            className="pg-field"
            list="draw-image-models"
            value={model}
            onChange={(e) => onModelChange(e.target.value)}
            placeholder="如 dall-e-3 / cogview-3-plus"
          />
          <datalist id="draw-image-models">
            {(activeTemplate?.image_models ?? []).map((m) => (
              <option key={m} value={m} />
            ))}
          </datalist>
        </div>

        <div className="pg-sec">
          <div className="pg-lab">
            <span>Prompt</span>
            <span className="sp" />
            {composed && (
              <span className={`dw-count${composed.truncated ? ' over' : ''}`}>
                {composed.chars}/{composed.max_chars} 字符 · ≈{composed.est_tokens} token
              </span>
            )}
          </div>
          <textarea
            className="pg-area"
            rows={4}
            placeholder="主体 → 动作 → 环境 → 风格;例如:黄昏时分的湖边小屋,暖色灯光,水彩风格"
            value={prompt}
            onChange={(e) => onPromptChange(e.target.value)}
          />

          {composed && (
            <div className="dw-prompt-meta">
              <span className="dw-fam">{composed.family_label}</span>
              {composed.truncated && <span className="dw-cut">已按上限截断</span>}
              <span className="sp" />
              <span className="dw-hint">规则组装 · 无额外模型调用</span>
            </div>
          )}
          {composed?.warnings.map((w) => (
            <div key={w} className="dw-prompt-warn">{w}</div>
          ))}
          {composedError && <div className="dw-prompt-warn">{composedError}</div>}

          <div className="pg-lab dw-lab-sub">Negative prompt</div>
          <textarea
            className="pg-area"
            rows={2}
            placeholder="可选:不希望出现的元素;该通道无独立 negative 参数,会以 Avoid: 文本追加"
            value={negative}
            onChange={(e) => onNegativeChange(e.target.value)}
          />

          {composed && composed.prompt && (
            <details className="dw-final">
              <summary>最终 Prompt 预览(实际发给模型)</summary>
              <pre>{composed.prompt}</pre>
            </details>
          )}
        </div>

        <div className="pg-sec">
          <div className="pg-lab">风格</div>
          <div className="pg-chips">
            {STYLES.map((s) => (
              <button
                key={s.id}
                className={`pg-chip${styleId === s.id ? ' on' : ''}`}
                onClick={() => onStyleChange(s.id)}
              >
                {s.label}
              </button>
            ))}
          </div>
        </div>

        <div className="pg-sec">
          <div className="pg-lab">输出</div>
          <div className="pg-chips">
            {SIZES.map((s) => (
              <button
                key={s.value}
                className={`pg-chip${size === s.value ? ' on' : ''}`}
                onClick={() => onSizeChange(s.value)}
              >
                {s.label}
              </button>
            ))}
          </div>
          <div className="pg-row">
            <span>生成数量</span>
            <select className="pg-field num" value={n} onChange={(e) => onNChange(Number(e.target.value))}>
              {[1, 2, 3, 4].map((x) => (
                <option key={x} value={x}>
                  {x} 张
                </option>
              ))}
            </select>
          </div>
          <div className="pg-row">
            <span title="当前内置 Provider 不支持,已预留">种子 / Guidance</span>
            <span style={{ color: 'var(--text-4)', fontSize: 'var(--font-caption)' }}>
              预留(随 Provider 开放)
            </span>
          </div>
        </div>

        {activeTemplate && activeTemplate.image_models.length === 0 && (
          <div className="pg-warn">
            {activeTemplate.display_name} 未内置图像模型;若其支持 /images/generations
            可手填模型名,否则请换 OpenAI / 智谱。
          </div>
        )}
        {readyTemplates.length === 0 && (
          <div className="pg-warn">尚未配置任何 Provider 的 API Key,请先在对话页顶部模型菜单中配置。</div>
        )}
        {error && <div className="pg-warn">{error}</div>}

        <div className="pg-foot">
          <div className="pg-est">
            <span>出图规模</span>
            <b>
              {n} 张 · {SIZES.find((s) => s.value === size)?.label}
            </b>
          </div>
          <button className="pg-run" disabled={!canRun} onClick={onRun}>
            <IconSparkles size={15} />
            {running ? '生成中…' : '开始生成'}
          </button>
        </div>
        <TutorialList page="draw" />
      </aside>
    </AsideDrawer>
  );
}
