/**
 * ECharts 图表渲染器(DOM-first,与 MermaidBlock 同一套约定):
 *   1. 动态 import echarts/core(按需注册 bar/line/pie/scatter + SVG 渲染器)
 *   2. 以容器实测宽度初始化,setOption 后取出最终 SVG
 *   3. dispose 实例,移除 SVG 固定 width/height 保留 viewBox,交给 CSS 原生缩放
 * 宽度适配零重绘(不挂 ResizeObserver,避免清空重绘闪烁),导出 DOM 序列化即所见即所得。
 * 解析/渲染失败降级为普通代码块展示,不阻塞报告阅读。
 */
import { useEffect, useRef, useState } from 'react';
import type { ECharts } from 'echarts/core';

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
    let chart: ECharts | null = null;
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
        // 以容器实测宽度初始化,高度固定 360,渲染完成即 dispose
        const stage = document.createElement('div');
        stage.style.width = `${Math.max(hostRef.current.clientWidth, 240)}px`;
        stage.style.height = '360px';
        hostRef.current.appendChild(stage);

        chart = mod.init(stage, undefined, { renderer: 'svg', ssr: false });
        chart.setOption(option);
        const svg = stage.querySelector('svg');
        if (!svg) {
          setErr('ECharts 未生成 SVG');
          mod.dispose(chart);
          chart = null;
          return;
        }
        const w = svg.getAttribute('width');
        const h = svg.getAttribute('height');
        if (w && h && !svg.getAttribute('viewBox')) {
          svg.setAttribute('viewBox', `0 0 ${parseFloat(w)} ${parseFloat(h)}`);
        }
        // 移除固定尺寸交给 CSS 缩放(与 mermaid 同策略)
        svg.removeAttribute('width');
        svg.removeAttribute('height');
        svg.setAttribute('preserveAspectRatio', 'xMidYMid meet');
        // SVG 就位后整体搬出 stage,丢弃实例
        hostRef.current.appendChild(svg);
        mod.dispose(chart);
        chart = null;
        stage.remove();
        setDone(true);
      } catch (e: unknown) {
        if (cancelled) return;
        const msg = e instanceof Error ? e.message : JSON.stringify(e);
        setErr(`ECharts 渲染失败: ${msg}`);
      }
    })();

    return () => {
      cancelled = true;
      if (chart) {
        // 卸载时兜底释放实例,避免测试/快速切换场景泄漏
        try {
          chart.dispose();
        } catch {
          /* 忽略释放异常 */
        }
      }
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
