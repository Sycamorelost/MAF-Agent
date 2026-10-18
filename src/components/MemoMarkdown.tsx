import React, { useRef, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeHighlight from 'rehype-highlight';
import { IconCheck, IconCopy } from './icons';
import MermaidBlock from './MermaidBlock';
import EchartsBlock from './EchartsBlock';

/** 代码块容器:悬浮「复制」按钮,复制 pre 内全部文本。 */
export function CodeBlock({ children }: { children?: React.ReactNode }) {
  const preRef = useRef<HTMLPreElement>(null);
  const [copied, setCopied] = useState(false);

  const onCopy = async () => {
    const text = preRef.current?.textContent ?? '';
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // 剪贴板不可用时静默
    }
  };

  return (
    <div className="codeblock">
      <button
        type="button"
        className={`codeblock-copy ${copied ? 'copied' : ''}`}
        onClick={onCopy}
        title={copied ? '已复制' : '复制代码'}
      >
        {copied ? <IconCheck size={14} /> : <IconCopy size={14} />}
        <span>{copied ? '已复制' : '复制'}</span>
      </button>
      <pre ref={preRef}>{children}</pre>
    </div>
  );
}

/** pre 的结构是 <pre><code className="language-x">text</code></pre>,这里从 children 提取语言与文本。 */
function extractCode(children: React.ReactNode): { lang: string; text: string } | null {
  // children 通常是单个 <code> 元素;防御数组形态取第一个
  const el = (Array.isArray(children) ? children[0] : children) as
    | React.ReactElement<{ className?: string; children?: React.ReactNode }>
    | null
    | undefined;
  if (!el || typeof el !== 'object' || !el.props) return null;
  const lang = /language-(\w+)/.exec(el.props.className ?? '')?.[1] ?? '';
  const text = String(el.props.children ?? '').replace(/\n$/, '');
  return { lang, text };
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
    const rehype = withCodeBlock && !streaming ? [rehypeHighlight] : undefined;

    // 表格外包一层横向滚动容器,避免宽表格撑破气泡(快慢路径共用)
    const tableWrap = ({ children }: { children?: React.ReactNode }) => (
      <div className="md-table-wrap" style={{ overflowX: 'auto', maxWidth: '100%' }}>
        <table>{children}</table>
      </div>
    );

    // 快路径:无 mermaid/echarts,只需 pre → CodeBlock(或默认)
    if (!hasMermaid) {
      return (
        <ReactMarkdown
          remarkPlugins={[remarkGfm]}
          rehypePlugins={rehype}
          components={
            withCodeBlock || extraComponents
              ? { ...(withCodeBlock ? { pre: CodeBlock } : {}), table: tableWrap, ...extraComponents }
              : { table: tableWrap }
          }
        >
          {content}
        </ReactMarkdown>
      );
    }

    // 慢路径:pre 层分流 —— mermaid/echarts 直接渲染图表,其余走 CodeBlock
    const components: React.ComponentProps<typeof ReactMarkdown>['components'] = {
      // 表格外包一层横向滚动容器,避免宽表格撑破气泡
      table: tableWrap,
      pre({ children }) {
        const info = extractCode(children);
        if (info?.lang === 'mermaid') {
          return <MermaidBlock code={info.text} />;
        }
        if (info?.lang === 'echarts') {
          return <EchartsBlock code={info.text} />;
        }
        return withCodeBlock ? <CodeBlock>{children}</CodeBlock> : <pre>{children}</pre>;
      },
      ...extraComponents,
    };

    return (
      <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={rehype} components={components}>
        {content}
      </ReactMarkdown>
    );
  },
  (prev, next) =>
    prev.content === next.content &&
    prev.withCodeBlock === next.withCodeBlock &&
    prev.streaming === next.streaming &&
    prev.extraComponents === next.extraComponents,
);
