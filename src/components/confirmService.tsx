/**
 * 全局应用内确认服务 —— `await uiConfirm(...)` 替代 window.confirm。
 *
 * 背景:Tauri/WebView2 下原生 confirm 的显示时机与返回值可能不同步
 * (出现「先执行后弹窗」),且样式与软件不统一。本服务挂一个全局唯一的
 * ConfirmHost(App.tsx 内),uiConfirm 返回 Promise,用户点「确认」才
 * resolve(true),从机制上保证先确认后执行。
 */
import { useEffect, useState } from 'react';
import { ConfirmDialog } from './ConfirmDialog';

export interface ConfirmOptions {
  /** 标题,默认「确认操作」 */
  title?: string;
  /** 正文,支持 \n 换行 */
  message: string;
  /** 危险操作(确认按钮变红) */
  danger?: boolean;
  confirmText?: string;
  cancelText?: string;
}

type Pending = ConfirmOptions & { resolve: (ok: boolean) => void };

/** Host 注册的请求入口(同时只弹一个;新请求到来时旧请求按「取消」结算,防止 Promise 悬挂)。 */
let push: ((p: Pending) => void) | null = null;

/** 弹应用内确认框,用户点「确认」→ true,「取消」/遮罩/Esc → false。 */
export function uiConfirm(opts: ConfirmOptions | string): Promise<boolean> {
  const o = typeof opts === 'string' ? { message: opts } : opts;
  return new Promise((resolve) => {
    if (push) {
      push({ ...o, resolve });
    } else {
      // Host 未挂载(测试环境等):视为取消,绝不静默放行危险操作
      resolve(false);
    }
  });
}

/** 全局确认弹窗宿主,挂在 App 根部一次。 */
export function ConfirmHost() {
  const [pending, setPending] = useState<Pending | null>(null);

  useEffect(() => {
    push = (p) =>
      setPending((prev) => {
        // 队列只允许一个弹窗:被顶掉的旧请求按取消结算,避免 await 永远悬挂
        prev?.resolve(false);
        return p;
      });
    return () => {
      push = null;
    };
  }, []);

  if (!pending) return null;
  return (
    <ConfirmDialog
      title={pending.title ?? '确认操作'}
      message={pending.message}
      danger={pending.danger}
      confirmText={pending.confirmText}
      cancelText={pending.cancelText}
      onConfirm={() => {
        setPending(null);
        pending.resolve(true);
      }}
      onCancel={() => {
        setPending(null);
        pending.resolve(false);
      }}
    />
  );
}
