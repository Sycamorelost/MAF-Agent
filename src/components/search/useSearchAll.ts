import { useEffect, useRef, useState } from 'react';
import { searchAll, type SearchResults } from '../../lib/ipc';

/** 空结果常量:搜索框清空或请求失败时复用,避免状态里出现 undefined 分组 */
export const EMPTY_RESULTS: SearchResults = {
  sessions: [],
  messages: [],
  memory: [],
  notes: [],
  attachments: [],
};

interface SearchState {
  res: SearchResults;
  loading: boolean;
  error: string | null;
}

/**
 * 全局搜索数据源:250ms 去抖调用后端 search_all,空词短路不发请求。
 * 用递增序号忽略过期响应,避免快速输入时旧结果覆盖新结果。
 */
export function useSearchAll(query: string, delay = 250): SearchState {
  const [state, setState] = useState<SearchState>({
    res: EMPTY_RESULTS,
    loading: false,
    error: null,
  });
  const seq = useRef(0);

  useEffect(() => {
    const kw = query.trim();
    const id = ++seq.current;
    if (!kw) {
      setState({ res: EMPTY_RESULTS, loading: false, error: null });
      return;
    }
    setState((prev) => ({ ...prev, loading: true }));
    const timer = window.setTimeout(() => {
      searchAll(kw)
        .then((res) => {
          if (seq.current === id) setState({ res, loading: false, error: null });
        })
        .catch((e) => {
          if (seq.current !== id) return;
          setState({
            res: EMPTY_RESULTS,
            loading: false,
            error: e instanceof Error ? e.message : String(e),
          });
        });
    }, delay);
    return () => window.clearTimeout(timer);
  }, [query, delay]);

  return state;
}
