import React, { useRef, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';
import rehypeHighlight from 'rehype-highlight';
import { IconCheck, IconChevronDown, IconChevronRight, IconCopy } from './icons';
import MermaidBlock from './MermaidBlock';
import EchartsBlock from './EchartsBlock';
import { ToolTrace } from './ToolTrace';

/** 超过该行的代码块默认折叠,防止长消息被大段代码刷屏;短片段保持展开。 */
const CODE_COLLAPSE_LINES = 8;

/** 代码块容器:语言标签 + 行数 + 展开/收起 + 复制。长代码默认折叠。 */
export function CodeBlock({
  children,
  streaming = false,
}: {
  children?: React.ReactNode;
  streaming?: boolean;
}) {
  const preRef = useRef<HTMLPreElement>(null);
  const [copied, setCopied] = useState(false);

  const info = extractCode(children);
  const lang = info?.lang?.toUpperCase() ?? '';
  const lines = info ? info.text.split('\n').length : 0;
  // 流式期间不折叠:用户能看到代码实时增长;流式结束后由完整渲染接管折叠
  const collapsible = !streaming && lines > CODE_COLLAPSE_LINES;
  const [collapsed, setCollapsed] = useState(collapsible);

  const onCopy = async (e: React.MouseEvent) => {
    e.stopPropagation();
    const text = preRef.current?.textContent ?? '';
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch { /* 静默 */ }
  };

  const onToggle = collapsible ? () => setCollapsed((v) => !v) : undefined;
  const onHeaderKeyDown = collapsible
    ? (e: React.KeyboardEvent) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          setCollapsed((v) => !v);
        }
      }
    : undefined;

  return (
    <div className={`codeblock ${collapsible ? 'collapsible' : ''} ${collapsed ? 'collapsed' : ''}`}>
      <div
        className="codeblock-header"
        onClick={onToggle}
        onKeyDown={onHeaderKeyDown}
        role={collapsible ? 'button' : undefined}
        tabIndex={collapsible ? 0 : undefined}
        aria-expanded={collapsible ? !collapsed : undefined}
        title={collapsible ? (collapsed ? '展开代码' : '收起代码') : undefined}
      >
        <span className="codeblock-meta">
          {collapsible && (
            <span className="codeblock-caret">
              {collapsed ? <IconChevronRight size={12} /> : <IconChevronDown size={12} />}
            </span>
          )}
          <span>{lang || 'CODE'}</span>
          {collapsible && <span className="codeblock-lines">{lines} 行</span>}
        </span>
        <button
          type="button"
          className={`codeblock-copy ${copied ? 'copied' : ''}`}
          onClick={onCopy}
          title={copied ? '已复制' : '复制代码'}
          style={{
            position: 'static',
            opacity: 1,
            background: 'transparent',
            border: 'none',
            color: 'inherit',
            padding: '0',
            fontSize: 'inherit',
            letterSpacing: 0,
            textTransform: 'none',
          }}
        >
          {copied ? <IconCheck size={13} /> : <IconCopy size={13} />}
        </button>
      </div>
      <pre ref={preRef}>{children}</pre>
    </div>
  );
}

/** 从 ReactNode 递归提取纯文本(用于 heading slugify)。 */
function extractText(children: React.ReactNode): string {
  let out = '';
  React.Children.forEach(children, (c) => {
    if (typeof c === 'string' || typeof c === 'number') out += String(c);
    else if (React.isValidElement(c)) out += extractText((c.props as { children?: React.ReactNode }).children);
    else if (Array.isArray(c)) c.forEach(extractText);
  });
  return out;
}

/** Slugify:保留中文、英文小写、数字,空格→横杠,其他→空。 */
function slugify(input: string): string {
  return input
    .trim()
    .toLowerCase()
    .replace(/[\s]+/g, '-')
    .replace(/[^\p{L}\p{N}-]+/gu, '')
    .replace(/-+/g, '-')
    .replace(/^-|-$/g, '')
    .slice(0, 80);
}

/** pre 的结构是 <pre><code className="language-x">text</code></pre>,这里从 children 提取语言与文本。 */
function extractCode(children: React.ReactNode): { lang: string; text: string } | null {
  // children 通常是单个 <code> 元素;防御数组形态取第一个
  const el = (Array.isArray(children) ? children[0] : children) as
    | React.ReactElement<{ className?: string; children?: React.ReactNode }>
    | null
    | undefined;
  if (!el || typeof el !== 'object' || !el.props) return null;
  // 语言名允许连字符(maf-tool 等自定义围栏类型)
  const lang = /language-([\w-]+)/.exec(el.props.className ?? '')?.[1] ?? '';
  const text = String(el.props.children ?? '').replace(/\n$/, '');
  return { lang, text };
}

/**
 * 把 LaTeX 原生定界符 \(...\) / \[...\] 归一化为 remark-math 支持的 $...$ / $$...$$。
 *
 * 不少模型直接输出 LaTeX 定界符,而 remark-math 只识别 $;且 CommonMark 会把
 * \( \[ 当成转义字符渲染成字面 ( [。这里在解析前先归一化。
 * 围栏代码块与行内 code 先占位保护,避免误伤其中的代码。
 */
function normalizeMathDelimiters(src: string): string {
  if (!src.includes('\\(') && !src.includes('\\[')) return src;
  const stashed: string[] = [];
  const hide = (m: string) => {
    stashed.push(m);
    return `\u0000${stashed.length - 1}\u0000`;
  };
  let s = src
    .replace(/```[\s\S]*?```/g, hide)
    .replace(/~~~[\s\S]*?~~~/g, hide)
    .replace(/`[^`\n]*`/g, hide);
  // 独占整行的 \[...\](LaTeX 行间公式)转块级 $$,其余转行内 $;
  // 行内公式不可跨行,故多行内容一律走块级。
  const toBlock = (body: string) => `\n\n$$\n${body.trim()}\n$$\n\n`;
  const toInline = (body: string) =>
    body.includes('\n') ? toBlock(body) : `$${body.trim()}$`;
  s = s
    .replace(/(^[ \t]*)\\\[([\s\S]*?)\\\]([ \t]*$)/gm, (_m, _lead, body: string) => toBlock(body))
    .replace(/\\\[([\s\S]*?)\\\]/g, (_m, body: string) => toInline(body))
    .replace(/\\\(([\s\S]*?)\\\)/g, (_m, body: string) => toInline(body));
  return s.replace(/\u0000(\d+)\u0000/g, (_m, i: string) => stashed[Number(i)] ?? '');
}

/**
 * 缓存 Markdown 解析结果:content 不变时不重新解析。
 * 用于消息气泡 / 记忆面板等高频渲染场景,避免流式刷新导致历史内容重复解析。
 *
 * mermaid 拦截必须在 pre 层:ReactMarkdown 结构是 pre > code,
 * 若在 code 层拦截,mermaid 图仍会被外层 pre(CodeBlock 深色底+复制按钮)包裹。
 */
export const MemoMarkdown = React.memo(
  ({
    content,
    withCodeBlock = false,
    streaming = false,
    extraComponents,
  }: {
    content: string;
    withCodeBlock?: boolean;
    /** 流式降级:跳过 mermaid 检测与语法高亮(highlight.js 全量重扫开销大);
     *  流式结束后由历史消息的完整渲染接管,高亮/图表自动恢复。 */
    streaming?: boolean;
    /** 追加元素映射(如研究报告的 img 解析);引用需稳定,变更会触发重渲染。 */
    extraComponents?: React.ComponentProps<typeof ReactMarkdown>['components'];
  }) => {
    const hasMermaid = streaming
      ? false
      : /```(?:mermaid|echarts)[\s\S]*?```/.test(content);
    // 公式:流式期间跳过解析(与 mermaid 一致),流式结束后由完整渲染接管
    const body = streaming ? content : normalizeMathDelimiters(content);
    const remark: React.ComponentProps<typeof ReactMarkdown>['remarkPlugins'] = streaming
      ? [remarkGfm]
      : [remarkGfm, remarkMath];
    // KaTeX 排在 highlight 之前;throwOnError:false 让个别非法公式只标红而不中断整篇渲染
    const rehype: React.ComponentProps<typeof ReactMarkdown>['rehypePlugins'] = streaming
      ? undefined
      : withCodeBlock
        ? [[rehypeKatex, { throwOnError: false, errorColor: '#cc0000' }], rehypeHighlight]
        : [[rehypeKatex, { throwOnError: false, errorColor: '#cc0000' }]];

    // 表格外包一层横向滚动容器,避免宽表格撑破气泡(快慢路径共用)
    const tableWrap = ({ children }: { children?: React.ReactNode }) => (
      <div className="md-table-wrap" style={{ overflowX: 'auto', maxWidth: '100%' }}>
        <table>{children}</table>
      </div>
    );

    // heading 注入 id:slugify 文本 → 纲要跳转的锚点
    const headingComponents: Record<string, React.ComponentType<any>> = {};
    for (const tag of ['h1', 'h2', 'h3', 'h4', 'h5', 'h6']) {
      headingComponents[tag] = ({ children, ...rest }: any) => {
        const id = slugify(extractText(children));
        return React.createElement(tag, { ...rest, id }, children);
      };
    }

    const baseComponents: React.ComponentProps<typeof ReactMarkdown>['components'] = {
      ...headingComponents,
      table: tableWrap,
    };

    // 快路径:无 mermaid/echarts,只需 pre → maf-tool 痕迹 / CodeBlock(或默认)
    if (!hasMermaid) {
      const merged = {
        ...baseComponents,
        pre: (p: { children?: React.ReactNode }) => {
          const info = extractCode(p.children);
          if (info?.lang === 'maf-tool') return <ToolTrace raw={info.text} />;
          return withCodeBlock ? <CodeBlock streaming={streaming} {...p} /> : <pre>{p.children}</pre>;
        },
        ...extraComponents,
      };
      return (
        <ReactMarkdown
          remarkPlugins={remark}
          rehypePlugins={rehype}
          components={merged}
        >
          {body}
        </ReactMarkdown>
      );
    }

    // 慢路径:pre 层分流 —— mermaid/echarts 直接渲染图表,maf-tool 走工具痕迹,其余走 CodeBlock
    const components: React.ComponentProps<typeof ReactMarkdown>['components'] = {
      ...baseComponents,
      pre({ children }) {
        const info = extractCode(children);
        if (info?.lang === 'maf-tool') {
          return <ToolTrace raw={info.text} />;
        }
        if (info?.lang === 'mermaid') {
          return <MermaidBlock code={info.text} />;
        }
        if (info?.lang === 'echarts') {
          return <EchartsBlock code={info.text} />;
        }
        return withCodeBlock ? <CodeBlock streaming={streaming}>{children}</CodeBlock> : <pre>{children}</pre>;
      },
      ...extraComponents,
    };

    return (
      <ReactMarkdown remarkPlugins={remark} rehypePlugins={rehype} components={components}>
        {body}
      </ReactMarkdown>
    );
  },
  (prev, next) =>
    prev.content === next.content &&
    prev.withCodeBlock === next.withCodeBlock &&
    prev.streaming === next.streaming &&
    prev.extraComponents === next.extraComponents,
);
