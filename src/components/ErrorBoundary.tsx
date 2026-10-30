import { Component } from 'react';
import type { ErrorInfo, ReactNode } from 'react';

interface Props {
  children: ReactNode;
}
interface State {
  error: Error | null;
}

/**
 * 全局渲染兜底:任何子树渲染异常(如 store selector 返回新引用导致的渲染崩溃)
 * 不再整页白屏,而是显示错误摘要与「重新加载」入口,便于定位与自救。
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // 保留组件栈到控制台,便于 dev/CDP 排查
    console.error('[ErrorBoundary]', error, info.componentStack);
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <div
        style={{
          height: '100vh',
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          gap: 'var(--space-4)',
          background: 'var(--bg-main)',
          color: 'var(--text-1)',
          padding: 'var(--space-8)',
        }}
      >
        <div
          style={{
            maxWidth: 560,
            width: '100%',
            background: 'var(--err-soft)',
            borderRadius: 'var(--radius-m)',
            padding: 'var(--space-4)',
            fontSize: 13,
            lineHeight: 1.6,
            color: 'var(--text-2)',
            wordBreak: 'break-all',
          }}
        >
          <div style={{ color: 'var(--err)', fontWeight: 600, marginBottom: 'var(--space-2)' }}>
            页面渲染出现异常
          </div>
          {error.message}
        </div>
        <button
          type="button"
          className="btn-primary"
          onClick={() => window.location.reload()}
        >
          重新加载
        </button>
      </div>
    );
  }
}
