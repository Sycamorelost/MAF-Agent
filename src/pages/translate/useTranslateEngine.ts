import { useEffect, useRef } from 'react';
import type { Dispatch, SetStateAction } from 'react';
import * as ipc from '../../lib/ipc';

/** 流式去抖:与消息流相同的 30ms 批量刷新策略。 */
const FLUSH_MS = 30;

interface Args {
  providerId: string;
  model: string;
  noModel: boolean;
  srcLang: string;
  dstLang: string;
  text: string;
  running: boolean;
  /** 自定义(OpenAI 兼容)服务商的 base_url / 显示名,仅 providerId='custom' 时有值 */
  customBaseUrl: string | null;
  customDisplayName: string | null;
  setResult: Dispatch<SetStateAction<string>>;
  setRunning: Dispatch<SetStateAction<boolean>>;
  setCopied: Dispatch<SetStateAction<boolean>>;
  setError: Dispatch<SetStateAction<string | null>>;
  setHistory: Dispatch<SetStateAction<ipc.TranslationRecord[] | null>>;
}

/**
 * 翻译流式引擎:请求 / 30ms 去抖累积 / 停止 / 自动翻译防抖 / 落历史。
 * 状态仍由页面持有,这里只搬运原有的请求与缓冲逻辑。
 */
export function useTranslateEngine({
  providerId,
  model,
  noModel,
  srcLang,
  dstLang,
  text,
  running,
  customBaseUrl,
  customDisplayName,
  setResult,
  setRunning,
  setCopied,
  setError,
  setHistory,
}: Args) {
  const bufRef = useRef('');
  /** 完整译文累积(去抖缓冲之外的全量,用于落历史) */
  const accRef = useRef('');
  const timerRef = useRef<number | undefined>(undefined);
  const cancelRef = useRef<(() => void) | null>(null);
  /** 跳过下一次自动翻译的签名(历史回填场景,避免覆盖已存译文) */
  const skipSig = useRef<string | null>(null);

  // 卸载时取消流并清定时器
  useEffect(
    () => () => {
      cancelRef.current?.();
      if (timerRef.current !== undefined) window.clearTimeout(timerRef.current);
    },
    [],
  );

  const scheduleFlush = () => {
    if (timerRef.current !== undefined) return;
    timerRef.current = window.setTimeout(() => {
      timerRef.current = undefined;
      const pending = bufRef.current;
      bufRef.current = '';
      if (pending) setResult((r) => r + pending);
    }, FLUSH_MS);
  };

  /** 立即冲掉缓冲里未刷出的内容(完成/出错/停止时调)。 */
  const flushNow = () => {
    if (timerRef.current !== undefined) {
      window.clearTimeout(timerRef.current);
      timerRef.current = undefined;
    }
    const pending = bufRef.current;
    bufRef.current = '';
    if (pending) setResult((r) => r + pending);
  };

  const canRun = !!providerId && (noModel || !!model) && !!text.trim() && !running;

  const stop = () => {
    cancelRef.current?.();
    cancelRef.current = null;
    flushNow();
    setRunning(false);
  };

  const run = async () => {
    // 缺 provider / 模型时给出明确原因,不再静默 return(否则表现为「点了没反应」)
    if (!providerId) {
      setError('请先在设置页配置并选中一个模型服务商');
      return;
    }
    if (!noModel && !model) {
      setError('当前服务商没有可用模型,请在设置页启用至少一个模型');
      return;
    }
    if (!text.trim()) return;
    if (running) stop(); // 重新翻译:先停旧流(自动触发 / 手动重试共用)
    setError(null);
    setResult('');
    setCopied(false);
    bufRef.current = '';
    accRef.current = '';
    const srcText = text.trim();
    setRunning(true);

    const handle = ipc.translateStream(
      {
        provider_id: providerId,
        model: noModel ? providerId : model,
        text: srcText,
        source_lang: srcLang || null,
        target_lang: dstLang,
        custom_base_url: customBaseUrl,
        custom_display_name: customDisplayName,
      },
      (chunk) => {
        if (chunk.type === 'delta') {
          accRef.current += chunk.content;
          bufRef.current += chunk.content;
          scheduleFlush();
        }
      },
    );
    cancelRef.current = handle.cancel;

    try {
      await handle.done;
      flushNow();
      // 落历史(仅完整完成;手动停止不存)
      const finalText = accRef.current.trim();
      if (finalText) {
        const rec: ipc.TranslationRecord = {
          created: new Date().toISOString(),
          provider_id: providerId,
          model: noModel ? '' : model,
          source_lang: srcLang || null,
          target_lang: dstLang,
          source_text: srcText,
          result_text: finalText,
        };
        ipc
          .saveTranslation(rec)
          .then((fileName) => {
            const saved = { ...rec, file_name: fileName };
            setHistory((h) => (h ? [saved, ...h] : h));
          })
          .catch(() => {});
      }
    } catch (e) {
      flushNow();
      setError(String(e));
    } finally {
      setRunning(false);
      cancelRef.current = null;
    }
  };

  // 自动翻译:输入原文 / 切换方向后 800ms 防抖触发,无需手动点击;
  // 清空文本立即停止并清空译文;历史回填(签名匹配)跳过一次,保留已存译文。
  useEffect(() => {
    if (skipSig.current !== null) {
      const sig = `${text}\u0000${srcLang}\u0000${dstLang}`;
      const match = skipSig.current === sig;
      skipSig.current = null;
      if (match) return;
    }
    if (!providerId || (!noModel && !model)) return;
    if (!text.trim()) {
      if (running) stop();
      setResult('');
      setError(null);
      return;
    }
    const t = window.setTimeout(() => void run(), 800);
    return () => window.clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text, srcLang, dstLang, providerId, model]);

  /** 历史回填:跳过下一次自动翻译(签名与回填文本/方向一致时)。 */
  const skipNext = (sig: string) => {
    skipSig.current = sig;
  };

  return { canRun, run, stop, skipNext };
}
