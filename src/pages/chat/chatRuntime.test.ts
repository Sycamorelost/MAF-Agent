import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useMessagesStore } from '../../stores/messages';
import { isChatRunBusy, useChatRunsStore } from '../../stores/chatRuns';

/** 事件回调 / 调用记录 / 可替换实现(极端场景注入用) */
const h = vi.hoisted(() => ({
  chunkCb: null as null | ((p: { run_id: string; seq: number; chunk: any }) => void),
  endCb: null as null | ((p: { run_id: string; status: string }) => void),
  startCalls: [] as any[],
  appendCalls: [] as any[],
  nextId: 1,
  startImpl: null as null | ((input: any) => Promise<string>),
  stateImpl: null as null | ((runId: string, afterSeq: number) => Promise<any>),
  toolsImpl: null as null | (() => Promise<any>),
}));

vi.mock('../../lib/ipc', () => ({
  startChatRun: (input: any) => {
    h.startCalls.push(input);
    return (h.startImpl ?? (async () => 'run-x'))(input);
  },
  cancelStream: async () => {},
  chatRunState: (runId: string, afterSeq: number) =>
    (h.stateImpl ?? (async () => ({ run_id: runId, status: 'running', chunks: [], truncated: false })))(
      runId,
      afterSeq,
    ),
  onChatRun: async (cb: any) => {
    h.chunkCb = cb;
    return () => {};
  },
  onChatRunEnd: async (cb: any) => {
    h.endCb = cb;
    return () => {};
  },
  appendMessage: async (input: any) => {
    h.appendCalls.push(input);
    return {
      id: h.nextId++, session_id: input.session_id, role: input.role, content: input.content,
      model_id: input.model_id ?? null, created: new Date().toISOString(), tokens: input.tokens ?? 0,
      attachments: input.attachments ?? null, sources: input.sources ?? null,
      parent_id: null, branch_id: 'main', reasoning: input.reasoning ?? '',
    };
  },
  listMessagesPage: async () => ({ messages: [], has_more: false }),
  deleteMessage: async () => {},
  listDeliverables: async () => [],
}));

vi.mock('./chatTools', () => ({
  collectSessionTools: () => (h.toolsImpl ?? (async () => []))(),
  resolveReasoningEffort: () => 'medium',
}));

vi.mock('./memoryContext', () => ({ maybeAutoSummarize: () => {} }));

import { cancelChatRun, resumeChatRun, runChatStream } from './chatRuntime';

const storage = new Map<string, string>();
const params = (sessionId: string) => ({
  sessionId,
  providerId: 'p',
  model: 'm',
  history: [{ role: 'user', content: 'hi' }],
  sources: null,
  agent: null,
  thinkCap: null,
  thinkingSel: undefined,
});

const assistantCalls = () => h.appendCalls.filter((c) => c.role === 'assistant');

function pending<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

beforeEach(() => {
  storage.clear();
  h.startCalls.length = 0;
  h.appendCalls.length = 0;
  h.nextId = 1;
  h.startImpl = null;
  h.stateImpl = null;
  h.toolsImpl = null;
  let n = 0;
  vi.stubGlobal('crypto', { randomUUID: () => `run-${++n}` });
  vi.stubGlobal('sessionStorage', {
    getItem: (k: string) => storage.get(k) ?? null,
    setItem: (k: string, v: string) => storage.set(k, v),
    removeItem: (k: string) => storage.delete(k),
  });
  useMessagesStore.setState({
    bySession: {}, streaming: {}, streamingToken: {}, streamingReasoning: {},
    streamingReasoningInfo: {}, streamingPhase: {}, streamStartedAt: {},
  });
  useChatRunsStore.setState({ bySession: {} });
});

async function ready() {
  await vi.waitFor(() => expect(h.chunkCb).not.toBeNull());
  await vi.waitFor(() => expect(h.endCb).not.toBeNull());
}

describe('chatRuntime 极端场景', () => {
  it('正常流:乱序/重复 seq 不重复、结束后只落库一次', async () => {
    useMessagesStore.setState({ bySession: { s1: [] } });
    await runChatStream(params('s1'));
    await ready();
    h.chunkCb!({ run_id: 'run-1', seq: 2, chunk: { type: 'delta', content: '好' } });
    h.chunkCb!({ run_id: 'run-1', seq: 1, chunk: { type: 'delta', content: '你' } }); // 乱序丢弃
    h.chunkCb!({ run_id: 'run-1', seq: 2, chunk: { type: 'delta', content: '好' } }); // 重复丢弃
    h.endCb!({ run_id: 'run-1', status: 'done' });
    h.endCb!({ run_id: 'run-1', status: 'done' }); // 终态幂等
    await vi.waitFor(() => expect(assistantCalls()).toHaveLength(1));
    expect(assistantCalls()[0].content).toBe('好');
    expect(assistantCalls()[0].partial).toBe(false);
    await vi.waitFor(() => expect(isChatRunBusy('s1')).toBe(false));
    expect(useMessagesStore.getState().streaming['s1']).toBeUndefined();
    expect(storage.has('maf-run:s1')).toBe(false);
  });

  it('工具收集期间停止:不发请求、不留占位、可再次发送', async () => {
    useMessagesStore.setState({ bySession: { s2: [] } });
    const d = pending<any>();
    h.toolsImpl = () => d.promise;
    const p = runChatStream(params('s2'));
    expect(isChatRunBusy('s2')).toBe(true);
    cancelChatRun('s2');
    d.resolve([]);
    await p;
    expect(h.startCalls).toHaveLength(0);
    expect(isChatRunBusy('s2')).toBe(false);
    expect(useMessagesStore.getState().streaming['s2']).toBeUndefined();
    // 残留取消标记不能误伤下一次发送
    h.toolsImpl = null;
    await runChatStream(params('s2'));
    expect(h.startCalls).toHaveLength(1);
    h.endCb!({ run_id: h.startCalls[0].run_id, status: 'done' });
  });

  it('后端启动失败:抛出给调用方、清空忙碌与占位', async () => {
    useMessagesStore.setState({ bySession: { s3: [] } });
    h.startImpl = async () => {
      throw new Error('provider p 未配置 API Key');
    };
    await expect(runChatStream(params('s3'))).rejects.toThrow('未配置 API Key');
    expect(isChatRunBusy('s3')).toBe(false);
    expect(useMessagesStore.getState().streaming['s3']).toBeUndefined();
    expect(storage.has('maf-run:s3')).toBe(false);
  });

  it('同会话并发发送被拒绝;会话隔离互不影响', async () => {
    useMessagesStore.setState({ bySession: { s4: [], s5: [] } });
    await runChatStream(params('s4'));
    const runId = h.startCalls[0].run_id;
    await expect(runChatStream(params('s4'))).rejects.toThrow('正在生成回复');
    // 另一个会话可以照常开始
    await runChatStream(params('s5'));
    expect(isChatRunBusy('s5')).toBe(true);
    await ready();
    h.chunkCb!({ run_id: runId, seq: 1, chunk: { type: 'delta', content: 'A' } });
    h.chunkCb!({ run_id: h.startCalls[1].run_id, seq: 1, chunk: { type: 'delta', content: 'B' } });
    h.endCb!({ run_id: runId, status: 'done' });
    h.endCb!({ run_id: h.startCalls[1].run_id, status: 'done' });
    await vi.waitFor(() => expect(assistantCalls()).toHaveLength(2));
    const bySession = Object.fromEntries(assistantCalls().map((c) => [c.session_id, c.content]));
    expect(bySession).toEqual({ s4: 'A', s5: 'B' });
  });

  it('报错中断:已生成内容以 partial 落库并带错误提示', async () => {
    useMessagesStore.setState({ bySession: { s6: [] } });
    await runChatStream(params('s6'));
    await ready();
    const runId = h.startCalls[0].run_id;
    h.chunkCb!({ run_id: runId, seq: 1, chunk: { type: 'delta', content: '半段' } });
    h.chunkCb!({ run_id: runId, seq: 2, chunk: { type: 'error', message: '连接被重置' } });
    h.endCb!({ run_id: runId, status: 'error' });
    await vi.waitFor(() => expect(assistantCalls()).toHaveLength(1));
    expect(assistantCalls()[0]).toMatchObject({ content: '半段', partial: true });
    await vi.waitFor(() =>
      expect(useChatRunsStore.getState().bySession['s6']?.error).toBe('连接被重置'),
    );
  });

  it('用户取消:partial 落库、终态 cancelled', async () => {
    useMessagesStore.setState({ bySession: { s7: [] } });
    await runChatStream(params('s7'));
    await ready();
    const runId = h.startCalls[0].run_id;
    h.chunkCb!({ run_id: runId, seq: 1, chunk: { type: 'delta', content: '写到一半' } });
    cancelChatRun('s7');
    h.endCb!({ run_id: runId, status: 'cancelled' });
    await vi.waitFor(() => expect(assistantCalls()).toHaveLength(1));
    expect(assistantCalls()[0]).toMatchObject({ content: '写到一半', partial: true });
    await vi.waitFor(() =>
      expect(useChatRunsStore.getState().bySession['s7']?.runId).toBeNull(),
    );
  });

  it('刷新恢复:重放快照 + 实时事件交织,内容完整且只落库一次', async () => {
    useMessagesStore.setState({ bySession: { s8: [] } });
    storage.set('maf-run:s8', JSON.stringify({ runId: 'run-8', model: 'm', providerId: 'p', sourcesJson: null }));
    const d = pending<any>();
    h.stateImpl = () => d.promise;
    const p = resumeChatRun('s8');
    await ready();
    // 快照还没回来,实时事件先到(含终态)
    h.chunkCb!({ run_id: 'run-8', seq: 3, chunk: { type: 'delta', content: '后' } });
    h.endCb!({ run_id: 'run-8', status: 'done' });
    d.resolve({
      run_id: 'run-8', status: 'running', truncated: false,
      chunks: [
        { seq: 1, chunk: { type: 'delta', content: '旧' } },
        { seq: 2, chunk: { type: 'delta', content: '文' } },
      ],
    });
    await p;
    await vi.waitFor(() => expect(assistantCalls()).toHaveLength(1));
    expect(assistantCalls()[0].content).toBe('旧文后');
    expect(isChatRunBusy('s8')).toBe(false);
    expect(storage.has('maf-run:s8')).toBe(false);
  });

  it('刷新恢复但 run 已过期:清掉占位与元信息,不报错', async () => {
    useMessagesStore.setState({ bySession: { s9: [] } });
    storage.set('maf-run:s9', JSON.stringify({ runId: 'run-9', model: 'm', providerId: 'p', sourcesJson: null }));
    h.stateImpl = async () => {
      throw new Error('run 不存在或已过期');
    };
    await resumeChatRun('s9');
    expect(useMessagesStore.getState().streaming['s9']).toBeUndefined();
    expect(isChatRunBusy('s9')).toBe(false);
    expect(storage.has('maf-run:s9')).toBe(false);
  });
});
