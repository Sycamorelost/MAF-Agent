/**
 * Mermaid v12 图表渲染器(DOM-first):
 *   1. 把 code 放进 <pre class="mermaid"> → 插入 host div
 *   2. mermaid.run({ nodes }) 把 pre 替换成 <svg>
 * 动态 import + code-split + securityLevel:strict。
 *
 * 宽度适配:不靠 ResizeObserver 重跑(清空重绘会闪烁/抖动),
 * 而是移除 svg 固定 width/height 保留 viewBox,靠 CSS max-width:100%;height:auto
 * 让浏览器原生缩放 —— 窗口 resize 零重绘。
 */
import { useEffect, useRef, useState } from 'react';

type MermaidMod = {
  initialize: (config: Record<string, unknown>) => void;
  run: (options?: { nodes?: Element[] }) => Promise<void>;
};

let mermaidPromise: Promise<MermaidMod> | null = null;

function getMermaid(): Promise<MermaidMod> {
  if (!mermaidPromise) {
    mermaidPromise = import('mermaid').then((raw) => {
      // mermaid v12 是 export default,动态 import 返回 ESM namespace,需 .default 取实例
      const mod = (raw as any).default ?? raw;
      mod.initialize({
        securityLevel: 'strict',
        startOnLoad: false,
        theme: 'default',
        fontFamily: 'system-ui, -apple-system, sans-serif',
      } as Record<string, unknown>);
      return mod as MermaidMod;
    });
  }
  return mermaidPromise;
}

export default function MermaidBlock({ code }: { code: string }) {
  const hostRef = useRef<HTMLDivElement>(null);
  const [err, setErr] = useState<string | null>(null);
  const [done, setDone] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setErr(null);
    setDone(false);

    void (async () => {
      const host = hostRef.current;
      if (!host) return;
      try {
        const mod = await getMermaid();
        if (cancelled || !hostRef.current) return;

        hostRef.current.innerHTML = '';
        const pre = document.createElement('pre');
        pre.className = 'mermaid';
        pre.textContent = code.trim();
        hostRef.current.appendChild(pre);

        await mod.run({ nodes: [pre] });
        if (cancelled || !hostRef.current) return;

        const svg = hostRef.current.querySelector('svg');
        if (!svg) {
          setErr('Mermaid 未生成 SVG');
          return;
        }
        // 移除固定尺寸,保留 viewBox,交给 CSS 原生缩放(防 resize 重绘闪烁)
        svg.removeAttribute('width');
        svg.removeAttribute('height');
        svg.setAttribute('preserveAspectRatio', 'xMidYMid meet');
        setDone(true);
      } catch (e: unknown) {
        if (cancelled) return;
        const msg = e instanceof Error ? e.message : JSON.stringify(e);
        setErr(`Mermaid 渲染失败: ${msg}`);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [code]);

  return (
    <div className="mermaid-block">
      <div ref={hostRef} className="mermaid-svg" />
      {!done && !err && <div className="mermaid-loading">正在渲染图表…</div>}
      {err && <div className="mermaid-error">{err}</div>}
    </div>
  );
}
