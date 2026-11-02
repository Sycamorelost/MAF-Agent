import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('../lib/ipc', () => ({
  appendMessage: vi.fn(),
  listMessagesPage: vi.fn(),
  deleteMessage: vi.fn(),
}));

import * as ipc from '../lib/ipc';
import { useMessagesStore } from './messages';

const makeRow = (id: number): ipc.MessageRow => ({
  id,
  session_id: 's1',
  role: 'user',
  content: '你好',
  model_id: 'm',
  created: 't',
  tokens: 0,
  attachments: null,
  sources: null,
  parent_id: null,
  branch_id: 'main',
  reasoning: '',
});

const reset = () =>
  useMessagesStore.setState({
    bySession: {},
    hasMore: {},
    streaming: {},
    streamingReasoning: {},
    streamingReasoningInfo: {},
    streamingPhase: {},
    streamStartedAt: {},
  });

describe('messages store · P0-1 乐观渲染', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    reset();
  });

  it('appendUserOptimistic 立即插入临时行(负 id)', () => {
    const tempId = useMessagesStore.getState().appendUserOptimistic('s1', '你好', 'm', null);
    expect(tempId).toBeLessThan(0);
    const list = useMessagesStore.getState().bySession['s1'];
    expect(list).toHaveLength(1);
    expect(list[0].id).toBe(tempId);
    expect(list[0].content).toBe('你好');
  });

  it('appendUser 带 optimisticId:落库后就地替换为真实行,不追加不闪动', async () => {
    vi.mocked(ipc.appendMessage).mockResolvedValueOnce(makeRow(42));
    const tempId = useMessagesStore.getState().appendUserOptimistic('s1', '你好', 'm', null);
    const realId = await useMessagesStore
      .getState()
      .appendUser('s1', '你好', 'm', null, tempId);
    expect(realId).toBe(42);
    const list = useMessagesStore.getState().bySession['s1'];
    expect(list).toHaveLength(1);
    expect(list[0].id).toBe(42);
  });

  it('appendUser 不带 optimisticId:维持原有追加行为', async () => {
    vi.mocked(ipc.appendMessage).mockResolvedValueOnce(makeRow(7));
    // 预置缓存,避免 pushRow 走「缓存未加载 → 整页重拉」的兜底分支
    useMessagesStore.setState((s) => ({ bySession: { ...s.bySession, s1: [] } }));
    await useMessagesStore.getState().appendUser('s1', '你好');
    expect(useMessagesStore.getState().bySession['s1'].map((m) => m.id)).toEqual([7]);
  });

  it('removeLocalRow 清掉临时行(失败/取消兜底)', () => {
    const tempId = useMessagesStore.getState().appendUserOptimistic('s1', '你好', 'm', null);
    useMessagesStore.getState().removeLocalRow('s1', tempId);
    expect(useMessagesStore.getState().bySession['s1']).toHaveLength(0);
  });
});

describe('messages store · P0-2/P1-4 占位阶段与计时', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    reset();
  });

  it('startStreaming 置 preparing 阶段与开始时间戳', () => {
    useMessagesStore.getState().startStreaming('s1');
    const s = useMessagesStore.getState();
    expect(s.streamingPhase['s1']).toBe('preparing');
    expect(s.streamStartedAt['s1']).toBeGreaterThan(0);
  });

  it('setStreamPhase 推进阶段,null 恢复默认', () => {
    useMessagesStore.getState().startStreaming('s1');
    useMessagesStore.getState().setStreamPhase('s1', 'search');
    expect(useMessagesStore.getState().streamingPhase['s1']).toBe('search');
    useMessagesStore.getState().setStreamPhase('s1', null);
    expect(useMessagesStore.getState().streamingPhase['s1']).toBeUndefined();
  });

  it('endStreaming 清理阶段与时间戳', () => {
    useMessagesStore.getState().startStreaming('s1');
    useMessagesStore.getState().endStreaming('s1');
    const s = useMessagesStore.getState();
    expect(s.streamingPhase['s1']).toBeUndefined();
    expect(s.streamStartedAt['s1']).toBeUndefined();
  });
});

describe('messages store · P2-9 首包直通', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    reset();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('首个 delta 立即渲染,不等 50ms 合批;阶段随之清除', () => {
    useMessagesStore.getState().startStreaming('s1');
    useMessagesStore.getState().pushStreamDelta('s1', '首');
    // 不推进定时器也应立即可见
    expect(useMessagesStore.getState().streaming['s1']).toBe('首');
    expect(useMessagesStore.getState().streamingPhase['s1']).toBeUndefined();
  });

  it('后续 delta 仍走合批:定时器推进后才合并', () => {
    useMessagesStore.getState().startStreaming('s1');
    useMessagesStore.getState().pushStreamDelta('s1', '首');
    useMessagesStore.getState().pushStreamDelta('s1', '二');
    useMessagesStore.getState().pushStreamDelta('s1', '三');
    // 合批期内仍是首包内容
    expect(useMessagesStore.getState().streaming['s1']).toBe('首');
    vi.advanceTimersByTime(60);
    expect(useMessagesStore.getState().streaming['s1']).toBe('首二三');
  });

  it('首个思考 chunk 同样直通', () => {
    useMessagesStore.getState().startStreaming('s1');
    useMessagesStore.getState().pushStreamReasoning('s1', '想');
    expect(useMessagesStore.getState().streamingReasoning['s1']).toBe('想');
  });

  it('endStreaming 冲掉未刷出的合批缓冲,返回完整累积', () => {
    useMessagesStore.getState().startStreaming('s1');
    useMessagesStore.getState().pushStreamDelta('s1', '首');
    useMessagesStore.getState().pushStreamDelta('s1', '尾');
    const { content } = useMessagesStore.getState().endStreaming('s1');
    expect(content).toBe('首尾');
  });
});
