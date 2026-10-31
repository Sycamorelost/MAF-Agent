/**
 * 试题库:筛选、手动加题、编辑/删除、勾选组卷。
 *
 * 所有题目来自学习中心:测验生成时自动入库(同计划同题干去重),也可手动添加。
 * 渲染拆分:加题/编辑表单 BankQuestionForm、列表行 BankQuestionItem;
 * 草稿与选项解析在 bankDraft.ts;题型元数据在 qtype.ts。
 */
import { useCallback, useEffect, useState } from 'react';
import { ConfirmDialog } from '../../components/ConfirmDialog';
import { useStudyStore } from '../../stores/study';
import type { BankQuestion, BankListResult } from '../../lib/ipc/study';
import * as studyIpc from '../../lib/ipc/study';
import { QTYPE_OPTIONS, TYPE_LABEL } from './qtype';
import { EMPTY_DRAFT, normalizeOptions, parseOptions, toDraft, type Draft } from './bankDraft';
import { AddQuestionCard, EditQuestionDialog } from './BankQuestionForm';
import { BankQuestionItem } from './BankQuestionItem';

const PAGE = 50;

export function BankView({ onPaperCreated }: { onPaperCreated: (id: string) => void }) {
  const plans = useStudyStore((s) => s.list);
  const [planId, setPlanId] = useState('');
  const [qtype, setQtype] = useState('');
  /** 输入框值(回车或点搜索才生效,避免逐字请求) */
  const [kw, setKw] = useState('');
  const [keyword, setKeyword] = useState('');
  const [page, setPage] = useState(0);
  const [data, setData] = useState<BankListResult>({ items: [], total: 0 });
  const [loading, setLoading] = useState(true);
  const [err, setErr] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [busy, setBusy] = useState(false);

  const [showAdd, setShowAdd] = useState(false);
  const [draft, setDraft] = useState<Draft>(EMPTY_DRAFT);
  const [editing, setEditing] = useState<BankQuestion | null>(null);
  const [editDraft, setEditDraft] = useState<Draft>(EMPTY_DRAFT);
  const [pendingDelete, setPendingDelete] = useState<BankQuestion | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setErr(null);
    try {
      const r = await studyIpc.studyBankList({
        plan_id: planId || null,
        qtype: qtype || null,
        keyword: keyword.trim() || null,
        limit: PAGE,
        offset: page * PAGE,
      });
      setData(r);
      // 已不存在的选中项要清理,避免组卷提交旧 id
      setSelected((s) => new Set([...s].filter((id) => r.items.some((q) => q.id === id))));
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [planId, qtype, keyword, page]);

  useEffect(() => {
    void load();
  }, [load]);

  const changeFilter = (fn: () => void) => {
    setPage(0);
    setSelected(new Set());
    fn();
  };

  const toggle = (id: number) => {
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  };

  const createPaper = async () => {
    if (selected.size === 0 || busy) return;
    setBusy(true);
    setErr(null);
    try {
      const d = await studyIpc.studyPaperCreate({
        plan_id: planId || null,
        title: null,
        kind: 'exam',
        question_ids: [...selected],
      });
      setSelected(new Set());
      onPaperCreated(d.paper.id);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const addQuestion = async () => {
    if (busy) return;
    setBusy(true);
    setErr(null);
    try {
      const options = draft.qtype === 'choice' ? normalizeOptions(parseOptions(draft.options)) : [];
      await studyIpc.studyBankAdd({
        plan_id: planId || null,
        qtype: draft.qtype,
        stem: draft.stem,
        options,
        answer: draft.answer,
        explanation: draft.explanation,
        node_name: draft.node_name,
        difficulty: draft.difficulty,
      });
      setDraft(EMPTY_DRAFT);
      setShowAdd(false);
      await load();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const saveEdit = async () => {
    if (!editing || busy) return;
    setBusy(true);
    setErr(null);
    try {
      const options =
        editing.qtype === 'choice' ? normalizeOptions(parseOptions(editDraft.options)) : [];
      await studyIpc.studyBankUpdate(editing.id, {
        stem: editDraft.stem,
        options,
        answer: editDraft.answer,
        explanation: editDraft.explanation,
        difficulty: editDraft.difficulty,
      });
      setEditing(null);
      await load();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const doDelete = async () => {
    if (!pendingDelete || busy) return;
    setBusy(true);
    try {
      await studyIpc.studyBankDelete(pendingDelete.id);
      setPendingDelete(null);
      await load();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const totalPages = Math.max(1, Math.ceil(data.total / PAGE));

  return (
    <div className="rp-hist">
      <div className="pg-main-head">
        <span className="topic">试题库</span>
        <span className="pg-hint">小测生成的题目会自动入库;可勾选多题组成整卷自测</span>
        <span className="sp" />
        <button className="btn-ghost" onClick={() => setShowAdd((v) => !v)}>
          {showAdd ? '收起加题' : '手动加题'}
        </button>
        <button className="btn-primary" disabled={busy || selected.size === 0} onClick={() => void createPaper()}>
          {busy ? '处理中…' : `组卷自测(${selected.size})`}
        </button>
      </div>

      <div className="rp-hist-list">
        <div className="pg-sec st-flex">
          <select
            className="pg-field"
            value={planId}
            onChange={(e) => changeFilter(() => setPlanId(e.target.value))}
          >
            <option value="">全部学习计划</option>
            {plans.map((p) => (
              <option key={p.id} value={p.id}>
                {p.topic}
              </option>
            ))}
          </select>
          <select
            className="pg-field"
            value={qtype}
            onChange={(e) => changeFilter(() => setQtype(e.target.value))}
          >
            <option value="">全部题型</option>
            {QTYPE_OPTIONS.map((t) => (
              <option key={t} value={t}>
                {TYPE_LABEL[t]}
              </option>
            ))}
          </select>
          <input
            className="pg-field grow"
            placeholder="搜索题干或知识点…"
            value={kw}
            onChange={(e) => setKw(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') changeFilter(() => setKeyword(kw));
            }}
          />
          <button className="pg-chip" onClick={() => changeFilter(() => setKeyword(kw))}>
            搜索
          </button>
          <span className="pg-hint">共 {data.total} 道</span>
        </div>

        {err && <div className="pg-warn">{err}</div>}

        {showAdd && (
          <AddQuestionCard
            draft={draft}
            setDraft={setDraft}
            busy={busy}
            onSave={() => void addQuestion()}
            onClear={() => setDraft(EMPTY_DRAFT)}
          />
        )}

        {loading && <div className="pg-hint">加载题库…</div>}
        {!loading && data.items.length === 0 && (
          <div className="rp-empty">题库还没有题目:先在某个学习计划里做一次小测,或手动加题</div>
        )}

        {data.items.map((q) => (
          <BankQuestionItem
            key={q.id}
            q={q}
            selected={selected.has(q.id)}
            onToggle={() => toggle(q.id)}
            onEdit={() => {
              setEditing(q);
              setEditDraft(toDraft(q));
            }}
            onDelete={() => setPendingDelete(q)}
          />
        ))}

        {data.total > PAGE && (
          <div className="pg-sec st-flex st-center">
            <button className="pg-chip" disabled={page === 0 || loading} onClick={() => setPage((p) => Math.max(0, p - 1))}>
              上一页
            </button>
            <span className="pg-hint">
              {page + 1} / {totalPages}
            </span>
            <button className="pg-chip" disabled={page + 1 >= totalPages || loading} onClick={() => setPage((p) => p + 1)}>
              下一页
            </button>
          </div>
        )}
      </div>

      {editing && (
        <EditQuestionDialog
          draft={editDraft}
          setDraft={setEditDraft}
          busy={busy}
          onSave={() => void saveEdit()}
          onClose={() => setEditing(null)}
        />
      )}

      {pendingDelete && (
        <ConfirmDialog
          title="删除题目"
          message={`确定从试题库删除这道题?已生成的试卷保存了题目快照,不受影响。\n\n${pendingDelete.stem}`}
          confirmText="删除"
          danger
          busy={busy}
          onCancel={() => setPendingDelete(null)}
          onConfirm={() => void doDelete()}
        />
      )}
    </div>
  );
}
