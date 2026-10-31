/**
 * 题库题目表单:题干/选项/答案/解析字段(加题与编辑共用),
 * 以及手动加题卡片、编辑弹窗两个装配形态。
 */
import { QTYPE_OPTIONS, TYPE_LABEL } from './qtype';
import type { Draft } from './bankDraft';

/** 题干 + 选项(choice)+ 答案 + 解析 字段组 */
function QuestionFields({ draft, setDraft }: { draft: Draft; setDraft: (d: Draft) => void }) {
  return (
    <>
      <textarea
        className="pg-field st-area"
        placeholder="题干"
        value={draft.stem}
        onChange={(e) => setDraft({ ...draft, stem: e.target.value })}
      />
      {draft.qtype === 'choice' && (
        <textarea
          className="pg-field st-area st-mt-2"
          placeholder="选项:每行一个,如 甲 / 乙 / 丙 / 丁(会自动补 A/B/C/D)"
          value={draft.options}
          onChange={(e) => setDraft({ ...draft, options: e.target.value })}
        />
      )}
      <input
        className="pg-field st-mt-2"
        placeholder={draft.qtype === 'choice' ? '正确答案字母,如 A' : '参考答案'}
        value={draft.answer}
        onChange={(e) => setDraft({ ...draft, answer: e.target.value })}
      />
      <textarea
        className="pg-field st-area st-area-s st-mt-2"
        placeholder="解析(可选)"
        value={draft.explanation}
        onChange={(e) => setDraft({ ...draft, explanation: e.target.value })}
      />
    </>
  );
}

/** 手动加题卡片(题型/知识点/难度可填) */
export function AddQuestionCard({
  draft,
  setDraft,
  busy,
  onSave,
  onClear,
}: {
  draft: Draft;
  setDraft: (d: Draft) => void;
  busy: boolean;
  onSave: () => void;
  onClear: () => void;
}) {
  return (
    <div className="pg-sec">
      <div className="pg-lab">手动加题</div>
      <div className="st-row-wrap st-mb-2">
        <select
          className="pg-field"
          value={draft.qtype}
          onChange={(e) => setDraft({ ...draft, qtype: e.target.value })}
        >
          {QTYPE_OPTIONS.map((t) => (
            <option key={t} value={t}>
              {TYPE_LABEL[t]}
            </option>
          ))}
        </select>
        <input
          className="pg-field grow"
          placeholder="知识点(可选)"
          value={draft.node_name}
          onChange={(e) => setDraft({ ...draft, node_name: e.target.value })}
        />
        <input
          className="pg-field num"
          type="number"
          min={0}
          max={5}
          title="难度 0-5"
          value={draft.difficulty}
          onChange={(e) => setDraft({ ...draft, difficulty: Number(e.target.value) || 0 })}
        />
      </div>
      <QuestionFields draft={draft} setDraft={setDraft} />
      <div className="st-row st-mt-2">
        <button className="btn-primary" disabled={busy || !draft.stem.trim() || !draft.answer.trim()} onClick={onSave}>
          保存到题库
        </button>
        <button className="btn-ghost" onClick={onClear}>
          清空
        </button>
      </div>
    </div>
  );
}

/** 编辑题目弹窗(题型不可改,可删除后重新加题) */
export function EditQuestionDialog({
  draft,
  setDraft,
  busy,
  onSave,
  onClose,
}: {
  draft: Draft;
  setDraft: (d: Draft) => void;
  busy: boolean;
  onSave: () => void;
  onClose: () => void;
}) {
  return (
    <div className="dialog-mask" onClick={onClose}>
      <div className="dialog st-bank-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="dialog-head">编辑题目</div>
        <div className="dialog-body">
          <div className="pg-lab">题型</div>
          <div className="pg-hint">{TYPE_LABEL[draft.qtype] ?? draft.qtype}(题型不可改,可删除后重新加题)</div>
          <div className="st-mt-2">
            <QuestionFields draft={draft} setDraft={setDraft} />
          </div>
          <div className="st-row st-mt-2">
            <span className="pg-hint">难度</span>
            <input
              className="pg-field num"
              type="number"
              min={0}
              max={5}
              value={draft.difficulty}
              onChange={(e) => setDraft({ ...draft, difficulty: Number(e.target.value) || 0 })}
            />
          </div>
        </div>
        <div className="dialog-foot">
          <button className="btn-ghost" disabled={busy} onClick={onClose}>
            取消
          </button>
          <button className="btn-primary" disabled={busy || !draft.stem.trim() || !draft.answer.trim()} onClick={onSave}>
            保存
          </button>
        </div>
      </div>
    </div>
  );
}
