//! 首次安装向导:确认底层 Prompt 设定。
//!
//! 首次启动时(设置 first_run_completed=false)由 App 挂载,用户确认或自定义底层 Prompt 后完成。

import { useState } from 'react';
import * as ipc from '../lib/ipc';
import { BASE_PHYSICAL_CONTRACT } from '../lib/systemPrompt';

interface OnboardingDialogProps {
  onDone: () => void;
}

export default function OnboardingDialog({ onDone }: OnboardingDialogProps) {
  const [step, setStep] = useState(0);
  const [customPrompt, setCustomPrompt] = useState(BASE_PHYSICAL_CONTRACT);
  const [useCustom, setUseCustom] = useState(false);
  const [saving, setSaving] = useState(false);

  const onFinish = async () => {
    setSaving(true);
    try {
      const settings = await ipc.getSettings();
      const next = {
        ...settings,
        first_run_completed: true,
        custom_base_prompt: useCustom ? customPrompt : null,
      };
      await ipc.saveSettings(next);
      onDone();
    } catch (e) {
      console.error('完成向导失败', e);
      onDone();
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="dialog-mask">
      <div className="dialog" style={{ maxWidth: 640 }}>
        {step === 0 && (
          <>
            <div className="dialog-head">欢迎使用 myAgent</div>
            <div className="dialog-body">
              <p className="set-hint" style={{ lineHeight: 1.7 }}>
                myAgent 是一款本地优先的智能体框架软件。你的数据只保存在本机,不上传任何服务器。
              </p>
              <p className="set-hint" style={{ marginTop: 'var(--space-2)', lineHeight: 1.7 }}>
                系统内置了一套「底层物理契约」提示词,用于约束模型的工具调用行为。
                你可以直接使用默认版本,也可以根据自己的需求修改。
              </p>
            </div>
            <div className="dialog-foot">
              <button
                className="btn-primary"
                onClick={() => setStep(1)}
              >
                下一步
              </button>
            </div>
          </>
        )}

        {step === 1 && (
          <>
            <div className="dialog-head">确认底层 Prompt 设定</div>
            <div className="dialog-body">
              <p className="set-hint" style={{ lineHeight: 1.7, marginBottom: 'var(--space-3)' }}>
                底层物理契约是系统注入到每次请求中的最低层约束,保证模型正确处理工具调用、记忆分工与失败场景。
              </p>

              <label className="mcp-builtin-toggle" style={{ marginBottom: 'var(--space-2)' }}>
                <input
                  type="checkbox"
                  checked={useCustom}
                  onChange={(e) => setUseCustom(e.target.checked)}
                />
                <span>自定义底层 Prompt(不勾选则使用系统内置默认)</span>
              </label>

              <div className="mcp-prompt-preview" style={{ maxHeight: 360 }}>
                <textarea
                  className="mcp-prompt-text"
                  style={{ width: '100%', minHeight: 320, resize: 'vertical', border: 'none', outline: 'none', background: 'transparent' }}
                  value={customPrompt}
                  onChange={(e) => setCustomPrompt(e.target.value)}
                  disabled={!useCustom}
                />
              </div>
            </div>
            <div className="dialog-foot">
              <button className="btn-ghost" onClick={() => setStep(0)} disabled={saving}>
                上一步
              </button>
              <span className="sp" />
              <button className="btn-ghost" onClick={() => setCustomPrompt(BASE_PHYSICAL_CONTRACT)} disabled={!useCustom || saving}>
                恢复默认
              </button>
              <button className="btn-primary" onClick={onFinish} disabled={saving}>
                {saving ? '保存中…' : '完成'}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
