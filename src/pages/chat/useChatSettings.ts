import { useEffect, useState } from 'react';
import * as ipc from '../../lib/ipc';
import { studyMemoryDigest } from '../../lib/ipc/study';

/** 当前模型与发送流程相关的全局设置探测:上下文窗口、全裸模式、物理契约、思考能力、学习进度摘要。 */
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
  // 学习进度摘要(P2 记忆注入):进页取一次,失败静默(学习板块为空时后端返回 null)
  const [studyDigest, setStudyDigest] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    studyMemoryDigest().then((d) => alive && setStudyDigest(d)).catch(() => {});
    return () => {
      alive = false;
    };
  }, []);
  /** 全局上下文窗口(条数):会话自身未设置(max_context_messages=0)时作为默认值。
   *  默认 24 条 ≈ 12 轮,避免长会话把全部历史原样重发导致 token 线性膨胀。 */
  const [ctxWindow, setCtxWindow] = useState(24);
  /** 全裸模式(R3 A1):true = system 不追加身份锚,仅保留物理契约(人格全交智能体/中转模型) */
  const [bareMode, setBareMode] = useState(false);
  /** 用户自定义底层物理契约(空 = 内置默认) */
  const [customContract, setCustomContract] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    ipc
      .getSettings()
      .then((s) => {
        if (!alive) return;
        setCtxWindow(s.context_window_messages ?? 24);
        setBareMode(!!s.bare_mode);
        setCustomContract(s.custom_base_prompt ?? null);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);
  return { ctxWindow, bareMode, customContract, thinkCap, studyDigest };
}
