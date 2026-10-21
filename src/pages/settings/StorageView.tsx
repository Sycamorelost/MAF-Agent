//! 设置 · 存储位置视图(从 SettingsPage 搬出,纯搬家:视觉与行为不变)。

import * as ipc from '../../lib/ipc';

export interface StorageViewProps {
  dataDir: ipc.DataDirInfo | null;
  dirMsg: string | null;
  dirBusy: boolean;
  dirRestarting: boolean;
  onChangeDataDir: () => void;
  onOpenDataDir: () => void;
}

export function StorageView({
  dataDir,
  dirMsg,
  dirBusy,
  dirRestarting,
  onChangeDataDir,
  onOpenDataDir,
}: StorageViewProps) {
  return (
    <>
      {dirRestarting && (
        <div className="dialog-mask">
          <div className="dialog">
            <div className="dialog-head">正在重启应用</div>
            <div className="dialog-body">
              <p className="set-hint">数据正在迁移到你选择的新目录,应用即将自动重新启动…</p>
            </div>
          </div>
        </div>
      )}
      <h2>存储位置</h2>
      <p className="set-desc">
        无账号体系,数据只存本机,统一落在下方所示的本地数据目录下(可自定义位置)。
      </p>

      <div className="set-card">
        <div className="set-card-h">当前数据目录</div>
        <div className="set-card-b">
          <div className="set-row">
            <label className="lbl">目录</label>
            <span
              style={{
                flex: 1,
                fontFamily: 'var(--font-mono)',
                color: 'var(--text-1)',
                wordBreak: 'break-all',
              }}
            >
              {dataDir?.path ?? '读取中…'}
            </span>
          </div>
          <div className="set-row">
            <label className="lbl">引导配置</label>
            <span
              className="set-hint"
              style={{ fontFamily: 'var(--font-mono)', wordBreak: 'break-all' }}
            >
              {dataDir?.config_path ?? '—'}
            </span>
          </div>
          <div className="set-actions" style={{ marginTop: 'var(--space-2)' }}>
            <button className="btn-primary" onClick={onChangeDataDir} disabled={dirBusy}>
              更改目录
            </button>
            <button className="btn-ghost" onClick={onOpenDataDir}>
              在资源管理器中打开
            </button>
            <span className="sp" />
            {dataDir?.legacy_adopted && (
              <span className="set-hint">检测到历史数据目录,建议更改到自定义位置</span>
            )}
          </div>
          {dataDir?.migration_error && (
            <div className="set-hint" style={{ color: 'var(--err)' }}>
              上次迁移失败:{dataDir.migration_error}
            </div>
          )}
          {dirMsg && (
            <div
              className="set-hint"
              style={{ color: dirMsg.includes('失败') ? 'var(--err)' : 'var(--text-2)' }}
            >
              {dirMsg}
            </div>
          )}
        </div>
      </div>

      <div className="set-card">
        <div className="set-card-h">内部结构</div>
        <div className="set-card-b" style={{ padding: 0 }}>
          <table className="set-table">
            <thead>
              <tr>
                <th>内容</th>
                <th>路径</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>会话与消息</td>
                <td className="mn">data/metaagent.db</td>
              </tr>
              <tr>
                <td>会话记忆</td>
                <td className="mn">data/conversations/&#123;id&#125;/memory.md</td>
              </tr>
              <tr>
                <td>记忆快照</td>
                <td className="mn">data/conversations/&#123;id&#125;/memory.versions/</td>
              </tr>
              <tr>
                <td>附件</td>
                <td className="mn">data/attachments/&#123;session&#125;/</td>
              </tr>
              <tr>
                <td>生成图片</td>
                <td className="mn">data/images/</td>
              </tr>
              <tr>
                <td>翻译历史</td>
                <td className="mn">data/translations/</td>
              </tr>
              <tr>
                <td>术语表</td>
                <td className="mn">data/glossary.json</td>
              </tr>
              <tr>
                <td>模型卡覆盖</td>
                <td className="mn">data/model_cards.json</td>
              </tr>
              <tr>
                <td>单模型配置</td>
                <td className="mn">data/provider_model_config.json</td>
              </tr>
              <tr>
                <td>服务商模板</td>
                <td className="mn">data/provider_templates.json</td>
              </tr>
              <tr>
                <td>厂家模型缓存</td>
                <td className="mn">data/provider_models.json</td>
              </tr>
              <tr>
                <td>深度研究</td>
                <td className="mn">data/research/&#123;id&#125;/report.md</td>
              </tr>
              <tr>
                <td>密钥(加密)</td>
                <td className="mn">data/secrets/&#123;provider&#125;.bin</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </>
  );
}
