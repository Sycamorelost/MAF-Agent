import * as ipc from '../../lib/ipc';
import { TutorialList } from '../../components/TutorialCard';
import AsideDrawer from '../../components/AsideDrawer';
import { IconImage, IconSparkles } from '../../components/icons';

const SIZES = [
  { value: '1024x1024', label: '1024×1024' },
  { value: '1792x1024', label: '1792×1024' },
  { value: '1024x1792', label: '1024×1792' },
];

/** 风格 chip:标签 + 合并进 prompt 的英文关键词。 */
export const STYLES = [
  { id: 'none', label: '无', prompt: '' },
  { id: 'photo', label: '写实摄影', prompt: 'photorealistic, high detail, 8k' },
  { id: 'anime', label: '动漫', prompt: 'anime style, cel shading' },
  { id: 'watercolor', label: '水彩', prompt: 'watercolor painting, soft colors' },
  { id: 'oil', label: '油画', prompt: 'oil painting, textured brushstrokes' },
  { id: 'cyber', label: '赛博朋克', prompt: 'cyberpunk, neon lights, futuristic' },
  { id: '3d', label: '3D 渲染', prompt: '3d render, octane, studio lighting' },
  { id: 'sketch', label: '线稿', prompt: 'pencil sketch, monochrome line art' },
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
          <div className="pg-lab">Prompt</div>
          <textarea
            className="pg-area"
            rows={3}
            placeholder="描述想要的画面,例如:黄昏时分的湖边小屋,水彩风格"
            value={prompt}
            onChange={(e) => onPromptChange(e.target.value)}
          />
          <textarea
            className="pg-area"
            rows={2}
            style={{ marginTop: 'var(--space-2)' }}
            placeholder="Negative prompt(可选):不希望出现的元素"
            value={negative}
            onChange={(e) => onNegativeChange(e.target.value)}
          />
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
