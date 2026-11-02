//! 轻提示(底部居中 .toast)的通用状态:
//! - 单行提示:默认 2.2s 自动消失(可传更长时间);
//! - 多行清单(文件导入被拒):常驻,由用户滚动查看后手动关闭(不受计时器截断)。

import { useCallback, useEffect, useRef, useState } from 'react';

export function useToast() {
  const [toast, setToast] = useState<string | null>(null);
  const timer = useRef<number>(0);

  const hideToast = useCallback(() => {
    window.clearTimeout(timer.current);
    setToast(null);
  }, []);

  const showToast = useCallback((msg: string, ms = 2200) => {
    window.clearTimeout(timer.current);
    setToast(msg);
    // 多行提示留到用户手动关闭,避免没看完就消失
    if (ms > 0 && !msg.includes('\n')) {
      timer.current = window.setTimeout(() => setToast(null), ms);
    }
  }, []);

  useEffect(() => () => window.clearTimeout(timer.current), []);

  return { toast, showToast, hideToast };
}
