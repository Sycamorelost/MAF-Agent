import { useEffect, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import * as ipc from '../lib/ipc';
import type { TutorialDoc } from '../lib/ipc';
import { CodeBlock } from './MessageBubble';
import { IconChevronDown } from './icons';

/** 单张教程卡:默认折叠,展开后渲染 markdown,代码块带复制按钮。 */
export function TutorialCard({ doc, index }: { doc: TutorialDoc; index: number }) {
  const [open, setOpen] = useState(false);
  return (
    <div className={`tutorial${open ? ' open' : ''}`}>
      <div className="tut-head" onClick={() => setOpen(!open)}>
        <span className="ti">{index + 1}</span>
        <span className="tt">{doc.title}</span>
        <span className="ts">{doc.summary}</span>
        <span className="arr">
          <IconChevronDown size={14} />
        </span>
      </div>
      <div className="tut-body">
        <div className="md">
          <ReactMarkdown
            remarkPlugins={[remarkGfm]}
            components={{
              pre: CodeBlock,
              table: ({ children }) => (
                <div style={{ overflowX: 'auto', maxWidth: '100%' }}>
                  <table>{children}</table>
                </div>
              ),
            }}
          >
            {doc.content}
          </ReactMarkdown>
        </div>
      </div>
    </div>
  );
}

/** 按页面加载 tutorials/*.md 并渲染整组教程卡;无教程时不占位。 */
export function TutorialList({ page }: { page: string }) {
  const [docs, setDocs] = useState<TutorialDoc[]>([]);
  useEffect(() => {
    let alive = true;
    ipc
      .listTutorials(page)
      .then((d) => {
        if (alive) setDocs(d);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [page]);
  if (!docs.length) return null;
  return (
    <div className="tutorial-list">
      {docs.map((d, i) => (
        <TutorialCard key={d.id} doc={d} index={i} />
      ))}
    </div>
  );
}
