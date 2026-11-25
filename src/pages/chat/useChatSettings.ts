import { useEffect, useState } from 'react';
import * as ipc from '../../lib/ipc';

/** 当前模型与发送流程相关的全局设置探测:思考能力。 */
export function useChatSettings(current: { providerId: string; model: string } | null) {
  // 当前模型的思考能力(手动修正 > 内置规则表推断;查询失败按不支持处理,控件整体隐藏)
  const [thinkCap, setThinkCap] = useState<ipc.ReasoningCapability | null>(null);
  useEffect(() => {
    let alive = true;
    if (!current) {
      setThinkCap(null);
      return;
    }
    ipc.getReasoningCapability(current.providerId, current.model)
      .then((cap) => alive && setThinkCap(cap))
      .catch(() => alive && setThinkCap(null));
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [current?.providerId, current?.model]);
  return { thinkCap };
}
