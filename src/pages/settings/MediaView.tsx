//! 设置 · 能力 · 媒体:图片生成服务商与模型。
//! OCR 使用 Windows 系统识别组件,无需配置,这里只给出说明。

import * as ipc from '../../lib/ipc';
import { IconCheck } from '../../components/icons';

interface Props {
  settings: ipc.AppSettings | null;
  /** 已配置 Key 的服务商(仅这些可用于图片生成) */
  keyedTemplates: ipc.ProviderTemplate[];
  settingsSaved: boolean;
  savingSettings: boolean;
  onPatchSettings: (patch: Partial<ipc.AppSettings>) => void;
  onSaveSettings: () => void;
}

export function MediaView({
  settings,
  keyedTemplates,
  settingsSaved,
  savingSettings,
  onPatchSettings,
  onSaveSettings,
}: Props) {
  const pid = settings?.image_provider_id ?? '';
  const tpl = keyedTemplates.find((t) => t.id === pid);
  const models = tpl?.models ?? [];
  const cur = settings?.image_model ?? '';
  // 只把明显是图像模型的条目作为候选:文本模型列表混进来容易误选(服务商会直接报错)
  const imageLike = models.filter((m) =>
    /image|dall-e|cogview|flux|sdxl|seedream|janus/i.test(m),
  );
  const opts = Array.from(new Set([...(cur ? [cur] : []), ...imageLike]));

  return (
    <>
      <h2>媒体能力</h2>
      <p className="set-desc">
        对话中生成图片时,请求从这里配置的服务商发出;产物保存在工作区 outputs/images/ 并登记为交付物。
      </p>

      <div className="set-card">
        <div className="set-card-h">
          图片生成
          {settingsSaved && (
            <span className="st">
              <IconCheck size={13} />已保存
            </span>
          )}
        </div>
        <div className="set-card-b">
          <div className="set-row">
            <label className="lbl">服务商</label>
            <select
              className="pg-field"
              value={pid}
              onChange={(e) =>
                onPatchSettings({ image_provider_id: e.target.value || null })
              }
            >
              <option value="">未启用(模型无法调用 image_generate)</option>
              {keyedTemplates.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.display_name}
                </option>
              ))}
            </select>
          </div>
          <div className="set-row">
            <label className="lbl">图像模型</label>
            <input
              className="pg-field"
              list="image-model-options"
              value={cur}
              disabled={!pid}
              placeholder={pid ? '如 gpt-image-1 / cogview-3-flash' : '跟随服务商'}
              onChange={(e) => onPatchSettings({ image_model: e.target.value || null })}
            />
            <datalist id="image-model-options">
              {opts.map((m) => (
                <option key={m} value={m} />
              ))}
            </datalist>
          </div>
          <div className="set-hint" style={{ color: 'var(--text-dim)', fontSize: 12 }}>
            必须填服务商的图像生成模型:OpenAI 用 gpt-image-1 / dall-e-3,智谱用
            cogview-3-flash(免费)或 cogview-4。填文本模型(如 glm-5.3)会被服务商拒绝。
            改完点下方「保存媒体设置」生效。
          </div>
          {pid && imageLike.length > 0 && (
            <div
              className="set-hint"
              style={{ display: 'flex', gap: 6, flexWrap: 'wrap', alignItems: 'center' }}
            >
              <span style={{ color: 'var(--text-dim)', fontSize: 12 }}>该服务商可用的图像模型:</span>
              {imageLike.map((m) => (
                <button
                  key={m}
                  type="button"
                  className="btn-ghost"
                  style={{ fontSize: 12, padding: '2px 8px' }}
                  disabled={m === cur}
                  onClick={() => onPatchSettings({ image_model: m })}
                >
                  {m}
                </button>
              ))}
            </div>
          )}
          <div className="set-actions" style={{ marginTop: 'var(--space-3)' }}>
            <span className="sp" />
            <button className="btn-primary" onClick={onSaveSettings} disabled={savingSettings}>
              保存媒体设置
            </button>
          </div>
        </div>
      </div>

      <div className="set-card">
        <div className="set-card-h">图片文字识别(OCR)</div>
        <div className="set-card-b">
          <div className="set-hint" style={{ color: 'var(--text-dim)', fontSize: 12 }}>
            使用 Windows 内置 OCR 组件,无需配置或下载模型。识别语言取决于系统已安装的语言包
            (设置 → 时间和语言 → 语言 → 可选功能中的「光学字符识别」)。
          </div>
        </div>
      </div>
    </>
  );
}
