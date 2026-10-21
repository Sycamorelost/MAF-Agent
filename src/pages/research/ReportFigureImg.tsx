import { useEffect, useState } from 'react';
import * as ipc from '../../lib/ipc';

export function ReportFigureImg({ id, src, alt }: { id: string; src: string; alt: string }) {
  const m = /^figures\/([\w.-]+)$/.exec(src);
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    if (!m) return;
    let alive = true;
    ipc
      .researchFigureData(id, m[1])
      .then((u) => {
        if (alive) setUrl(u);
      })
      .catch(() => {
        if (alive) setFailed(true);
      });
    return () => {
      alive = false;
    };
  }, [id, m?.[1]]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!m) {
    // 非 figures 引用:仅放行 http(s),其余一律占位(安全边界)
    const safe = /^https:\/\//.test(src) ? src : null;
    return safe ? (
      <img src={safe} alt={alt} loading="lazy" />
    ) : (
      <span className="fig-missing">[图片不可用]</span>
    );
  }
  if (failed) return <span className="fig-missing">[图示加载失败]</span>;
  return url ? <img src={url} alt={alt} loading="lazy" /> : <span className="fig-missing">[图示加载中]</span>;
}
