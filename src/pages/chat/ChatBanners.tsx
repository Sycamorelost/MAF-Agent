import { IconX } from '../../components/icons';
import type { SwitchBanner } from './useModelHandoff';

interface ChatBannersProps {
  /** D3.2:模型切换横幅(含注入内容入口) */
  switchBanner: SwitchBanner | null;
  /** D3.5:上下文压缩提示 */
  compressNotice: string | null;
  onShowInjected: () => void;
  onCloseSwitch: () => void;
  onCloseCompress: () => void;
}

/** 会话页顶部横幅:模型切换提示 + 压缩提示。 */
export function ChatBanners({
  switchBanner,
  compressNotice,
  onShowInjected,
  onCloseSwitch,
  onCloseCompress,
}: ChatBannersProps) {
  return (
    <>
      {switchBanner && (
        <div className="switch-banner">
          <span className="switch-banner-text">
            已从 {switchBanner.from} 切换到 {switchBanner.to} · 记忆已注入约{' '}
            {switchBanner.tokens} tokens
          </span>
          <button type="button" className="switch-banner-btn" onClick={onShowInjected}>
            查看注入内容
          </button>
          <button
            type="button"
            className="switch-banner-close"
            title="关闭"
            onClick={onCloseSwitch}
          >
            <IconX size={14} />
          </button>
        </div>
      )}
      {compressNotice && (
        <div className="switch-banner compress">
          <span className="switch-banner-text">{compressNotice}</span>
          <button
            type="button"
            className="switch-banner-close"
            title="关闭"
            onClick={onCloseCompress}
          >
            <IconX size={14} />
          </button>
        </div>
      )}
    </>
  );
}
