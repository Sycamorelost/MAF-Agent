//! 首次安装向导。
//!
//! 首次启动时(设置 first_run_completed=false)由 App 挂载,用户确认后完成。

import { useState } from 'react';
import * as ipc from '../lib/ipc';

interface OnboardingDialogProps {
  onDone: () => void;
}

export default function OnboardingDialog({ onDone }: OnboardingDialogProps) {
  const [saving, setSaving] = useState(false);

  const onFinish = async () => {
    setSaving(true);
    try {
      const settings = await ipc.getSettings();
      await ipc.saveSettings({ ...settings, first_run_completed: true });
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
      <div className="dialog" style={{ maxWidth: 'calc(640px * var(--ui-scale))' }}>
        <div className="dialog-head">欢迎使用 myAgent</div>
        <div className="dialog-body">
          <p className="set-hint" style={{ lineHeight: 1.7 }}>
            myAgent 是一款本地优先的智能体框架软件。你的数据只保存在本机,不上传任何服务器。
          </p>
          <p className="set-hint" style={{ marginTop: 'var(--space-2)', lineHeight: 1.7 }}>
            系统内置了一套「底层物理契约」提示词,用于约束模型的工具调用行为,开箱即用。
          </p>
        </div>
        <div className="dialog-foot">
          <button className="btn-primary" onClick={onFinish} disabled={saving}>
            {saving ? '正在完成…' : '开始使用'}
          </button>
        </div>
      </div>
    </div>
  );
}
