import type { MemoryGraphInfo } from '../../lib/ipc/mcp';
import { fmtBytes } from '../../lib/longTermMemory';

interface Props {
  data: MemoryGraphInfo | null;
  loading: boolean;
  error: string | null;
  msg: string | null;
  onRefresh: () => void;
  onClear: () => void;
}

/** 长期记忆概览卡:数据源路径 / 四项统计 / 刷新与清除。 */
export function LongTermSummaryCard({ data, loading, error, msg, onRefresh, onClear }: Props) {
  const entities = data?.entities ?? [];
  const relations = data?.relations ?? [];

  return (
    <div className="set-card">
      <div className="set-card-h">
        长期记忆(知识图谱)
        <span className="st ltm-st">{loading ? '读取中…' : `${entities.length} 个实体`}</span>
      </div>
      <div className="set-card-b">
        <p className="set-desc" style={{ marginTop: 0 }}>
          由「长期记忆」MCP server 写入 <code>mcp-memory.jsonl</code>,跨会话共享;仅禁用 server
          不会删除历史数据。文件:{data?.path ?? '-'}({fmtBytes(data?.file_bytes ?? 0)})
        </p>
        <div className="ltm-stats">
          <div className="ltm-stat">
            <b>{entities.length}</b>
            <span>实体</span>
          </div>
          <div className="ltm-stat">
            <b>{relations.length}</b>
            <span>关系</span>
          </div>
          <div className="ltm-stat">
            <b>{data?.observation_count ?? 0}</b>
            <span>观测条目</span>
          </div>
          <div className="ltm-stat">
            <b>{data?.type_count ?? 0}</b>
            <span>类型</span>
          </div>
        </div>
        <div className="set-actions" style={{ marginTop: 'var(--space-3)' }}>
          {error && <span className="set-hint" style={{ color: 'var(--err)' }}>{error}</span>}
          {msg && <span className="set-hint">{msg}</span>}
          <span className="sp" />
          <button type="button" className="btn btn-xs" onClick={onRefresh} disabled={loading}>
            刷新
          </button>
          <button
            type="button"
            className="btn btn-xs"
            onClick={onClear}
            disabled={loading || entities.length === 0}
          >
            清除长期记忆
          </button>
        </div>
      </div>
    </div>
  );
}
