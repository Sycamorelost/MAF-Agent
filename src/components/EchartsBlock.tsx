/**
 * ECharts 图表渲染器(DOM-first,与 MermaidBlock 同一套约定):
 *   1. 动态 import echarts/core(按需注册 bar/line/pie/scatter + SVG 渲染器)
 *   2. SSR 模式同步 setOption + renderToSVGString() 取出完整 SVG
 *   3. dispose 实例,移除 SVG 固定 width/height 保留 viewBox,交给 CSS 原生缩放
 * 宽度适配零重绘(不挂 ResizeObserver,避免清空重绘闪烁),导出 DOM 序列化即所见即所得。
 * 解析/渲染失败降级为普通代码块展示,不阻塞报告阅读。
 */
import { useEffect, useRef, useState } from 'react';

/** echarts 按需注册模块(与报告图件契约支持的类型一致) */
let echartsPromise: Promise<{
  init: typeof import('echarts/core')['init'];
  dispose: typeof import('echarts/core')['dispose'];
}> | null = null;

function getEcharts(): Promise<{
  init: typeof import('echarts/core')['init'];
  dispose: typeof import('echarts/core')['dispose'];
}> {
  if (!echartsPromise) {
    echartsPromise = Promise.all([
      import('echarts/core'),
      import('echarts/charts'),
      import('echarts/components'),
      import('echarts/renderers'),
    ]).then(([core, charts, comps, renderers]) => {
      core.use([
        charts.BarChart,
        charts.LineChart,
        charts.PieChart,
        charts.ScatterChart,
        comps.GridComponent,
        comps.TitleComponent,
        comps.TooltipComponent,
        comps.LegendComponent,
        comps.DatasetComponent,
        comps.TransformComponent,
        renderers.SVGRenderer,
      ]);
      return { init: core.init, dispose: core.dispose };
    });
  }
  return echartsPromise;
}

/** 容错解析 option:直接 JSON.parse 失败后,剥离注释/尾逗号/option= 前缀再试一次。 */
function parseOption(raw: string): Record<string, unknown> | null {
  const tryParse = (s: string): Record<string, unknown> | null => {
    try {
      const v = JSON.parse(s);
      return v && typeof v === 'object' && !Array.isArray(v) ? (v as Record<string, unknown>) : null;
    } catch {
      return null;
    }
  };
  const text = raw.trim();
  const direct = tryParse(text);
  if (direct) return direct;
  // 剥离 // 与 /* */ 注释、行尾逗号,以及「option = / Option:」前缀
  let cleaned = text.replace(/\/\/[^\n]*/g, '').replace(/\/\*[\s\S]*?\*\//g, '');
  cleaned = cleaned.replace(/,\s*([}\]])/g, '$1');
  const brace = cleaned.indexOf('{');
  if (brace > 0) cleaned = cleaned.slice(brace);
  return tryParse(cleaned);
}

export default function EchartsBlock({ code }: { code: string }) {
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
      const option = parseOption(code);
      if (!option) {
        setErr('ECharts 选项不是合法 JSON,已降级为代码展示');
        return;
      }
      try {
        const mod = await getEcharts();
        if (cancelled || !hostRef.current) return;

        hostRef.current.innerHTML = '';
        // SSR 模式同步产出完整 SVG 字符串:普通模式下 SVG 由动画帧异步绘制,
        // 同步取 DOM 只能拿到空 <svg>,dispose 还会把它清空 —— 图必然空白。
        const width = Math.max(hostRef.current.clientWidth, 240);
        const height = 360;
        const inst = (mod.init as any)(null, null, { renderer: 'svg', ssr: true, width, height });
        inst.setOption({ ...option, animation: false });
        const svgStr: string = inst.renderToSVGString();
        mod.dispose(inst);
        if (!svgStr) {
          setErr('ECharts 未生成 SVG');
          return;
        }
        hostRef.current.innerHTML = svgStr;
        // 交给 CSS 原生缩放(与 mermaid 同策略),避免固定像素宽度压破容器
        const svg = hostRef.current.querySelector('svg');
        if (svg) {
          svg.removeAttribute('width');
          svg.removeAttribute('height');
          svg.setAttribute('preserveAspectRatio', 'xMidYMid meet');
        }
        setDone(true);
      } catch (e: unknown) {
        if (cancelled) return;
        const msg = e instanceof Error ? e.message : JSON.stringify(e);
        setErr(`ECharts 渲染失败: ${msg}`);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [code]);

  return (
    <div className="echarts-block">
      <div ref={hostRef} className="echarts-svg" />
      {!done && !err && <div className="echarts-loading">正在渲染图表…</div>}
      {err && (
        <div className="echarts-error">
          <div>{err}</div>
          <pre className="echarts-raw">{code}</pre>
        </div>
      )}
    </div>
  );
}
