import { useEffect, useState } from 'react';
import * as ipc from '../../lib/ipc';
import { parseHumanVoiceLevel, type HumanVoiceLevel } from '../../lib/humanVoice';

/** 当前模型与发送流程相关的全局设置探测:全裸模式、物理契约、文字质感、思考能力。 */
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
  /** 全裸模式(R3 A1):true = system 不追加身份锚,仅保留物理契约(人格全交智能体/中转模型) */
  const [bareMode, setBareMode] = useState(false);
  /** 用户自定义底层物理契约(空 = 内置默认) */
  const [customContract, setCustomContract] = useState<string | null>(null);
  /** 文字质感(去 AI 味)全局档位:off / light / standard(缺省 standard) */
  const [humanVoiceLevel, setHumanVoiceLevel] = useState<HumanVoiceLevel>('standard');
  useEffect(() => {
    let alive = true;
    ipc
      .getSettings()
      .then((s) => {
        if (!alive) return;
        setBareMode(!!s.bare_mode);
        setCustomContract(s.custom_base_prompt ?? null);
        setHumanVoiceLevel(parseHumanVoiceLevel(s.human_voice_level));
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);
  return { bareMode, customContract, humanVoiceLevel, thinkCap };
}
