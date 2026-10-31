// 学习规划板块(P0)IPC 层 —— 契约与 src-tauri/src/study/mod.rs 严格对齐。

import { invoke, Channel } from '@tauri-apps/api/core';

// ---------- 类型 ----------

export interface StudyIntent {
  topic: string;
  level: string; // zero/beginner/intermediate/advanced/''(未指定)
  goal: string;
  daily_minutes: number;
  deadline: string | null;
  lang: string;
  /** 学科(自由文本,AI 从目标描述识别;'' = 未识别走通用模板) */
  subject: string;
  /** 学段:primary/junior/senior/university/adult/'' */
  grade_band: string;
  /** 考试目标(中考/高考/四六级/考研/兴趣等;null = 无) */
  exam_target: string | null;
  missing: string[];
  questions: string[];
}

export interface KnowledgeNodeSpec {
  name: string;
  difficulty: number; // 1-5
  why: string;
  must_learn: boolean;
  prereqs: string[];
}

export interface StageSpec {
  index: number;
  name: string;
  goal: string;
  difficulty: number; // 1-5
  estimated_weeks: number;
  topics: string[];
  milestone: string;
}

export interface ScheduleWeek {
  week: number;
  stage: number;
  focus: string;
  minutes_per_day: number;
}

export interface StageResource {
  stage: number;
  items: string[];
}

export interface PlanPayload {
  topic: string;
  level: string;
  goal: string;
  daily_minutes: number;
  deadline: string | null;
  lang: string;
  /** 学科(自由文本原文;'' = 通用模板) */
  subject: string;
  /** 学段(primary/junior/senior/university/adult/'') */
  grade_band: string;
  /** 考试目标(null = 无) */
  exam_target: string | null;
  /** 用户明确的内容/格式要求('' = 按模板默认) */
  preferences: string;
  nodes: KnowledgeNodeSpec[];
  stages: StageSpec[];
  schedule: ScheduleWeek[];
  resources: StageResource[];
  repaired: boolean;
}

export type StudyEvent =
  | { type: 'step_start'; step: string; label: string }
  | { type: 'step_note'; step: string; content: string }
  | { type: 'retry'; step: string; attempt: number; reason: string }
  | { type: 'paused' }
  | { type: 'resumed' }
  | { type: 'done'; plan: PlanPayload | null }
  | { type: 'cancelled' }
  | { type: 'error'; message: string };

export type PlanStatus =
  | 'generating'
  | 'active'
  | 'done'
  | 'archived'
  | 'error'
  | 'cancelled';

export interface PlanRow {
  id: string;
  topic: string;
  status: PlanStatus;
  level: string;
  goal: string;
  daily_minutes: number;
  deadline: string | null;
  lang: string;
  model_id: string | null;
  error: string | null;
  repaired: boolean;
  /** 学科附录 key(math/english/physics/chinese/programming/other;null = 未识别/旧计划) */
  subject: string | null;
  /** 学段(primary/junior/senior/university/adult;null = 未指定) */
  grade_band: string | null;
  /** 考试目标(自由文本;null = 无) */
  exam_target: string | null;
  created: string;
  updated: string;
}

export interface StudyDetail {
  plan: PlanRow;
  payload: PlanPayload | null;
  trail: StudyEvent[];
}

export interface StudyStartInput {
  topic: string;
  intent_json: string | null;
  daily_minutes: number | null;
  deadline: string | null;
  lang: string;
  provider_id: string;
  model: string;
  custom_base_url: string | null;
  custom_display_name: string | null;
  /** 是否生成每日任务(null = 默认开) */
  daily_tasks: boolean | null;
  /** 是否预生成每个知识点的完整教学内容(null = 默认关) */
  full_content: boolean | null;
}

export interface StudyPreflightInput {
  topic: string;
  lang: string;
  provider_id: string;
  model: string;
  custom_base_url: string | null;
  custom_display_name: string | null;
  /** 面板已确认的字段:已填就不追问 */
  level: string | null;
  daily_minutes: number | null;
  deadline: string | null;
}

// ---------- 命令 ----------

export function studyPreflight(input: StudyPreflightInput): Promise<StudyIntent> {
  return invoke('study_preflight', { input });
}

export function studyStart(
  input: StudyStartInput,
  onEvent: (ev: StudyEvent) => void,
): Promise<string> {
  const ch = new Channel<StudyEvent>();
  ch.onmessage = onEvent;
  return invoke('study_start', { input, channel: ch });
}

export function studyCancel(id: string): Promise<void> {
  return invoke('study_cancel', { id });
}

export function studyPause(id: string): Promise<void> {
  return invoke('study_pause', { id });
}

export function studyResume(id: string): Promise<void> {
  return invoke('study_resume', { id });
}

export function studyList(): Promise<PlanRow[]> {
  return invoke('study_list');
}

export function studyGet(id: string): Promise<StudyDetail> {
  return invoke('study_get', { id });
}

export function studyDelete(id: string): Promise<void> {
  return invoke('study_delete', { id });
}

export function studyDeepDig(id: string, nodeName: string): Promise<string> {
  return invoke('study_deep_dig', { id, nodeName });
}

export function studyExportMarkdown(
  id: string,
  token: string,
  overwrite: boolean,
): Promise<string> {
  return invoke('study_export_markdown', { id, token, overwrite });
}

// ---------- P1:学习执行 ----------

export interface StudyUnit {
  id: number;
  plan_id: string;
  node_id: number;
  node_name: string;
  est_minutes: number;
  content_md: string;
  created: string;
}

export interface QuizQuestion {
  idx: number;
  /** choice/judge/short 为基础题型;calc/cloze 可判分;proof/translate/reading/recite 走自评 */
  type: 'choice' | 'judge' | 'short' | 'calc' | 'proof' | 'cloze' | 'translate' | 'reading' | 'recite';
  stem: string;
  options: string[];
  answer: string;
  explanation: string;
}

export interface StudyQuiz {
  id: number;
  plan_id: string;
  node_id: number;
  node_name: string;
  questions: QuizQuestion[];
  created: string;
}

export interface QuizAnswerInput {
  question_idx: number;
  user_answer: string;
  /** 旧版自评字段(兼容保留) */
  self_correct?: boolean | null;
  /** 主观题「批改此题」按钮的判定结果 */
  grade_correct?: boolean | null;
  /** 主观题批改得分(0-1) */
  grade_score?: number | null;
  /** 主观题批改反馈 */
  grade_feedback?: string | null;
}

export interface QuizGrade {
  question_idx: number;
  correct: boolean;
  /** 0-1(主观题可部分得分) */
  score: number;
  expected: string;
  explanation: string;
  feedback: string;
}

export interface QuizResult {
  score: number;
  mastery: number;
  results: QuizGrade[];
}

export interface NodeState {
  node_id: number;
  stage_index: number;
  name: string;
  difficulty: number;
  why: string;
  must_learn: boolean;
  mastery: number;
  done: boolean;
  has_unit: boolean;
}

export interface ReviewItem {
  id: number;
  node_id: number;
  node_name: string;
  due: string;
  interval_days: number;
  last_result: number;
  overdue: boolean;
}

export interface TodayItem {
  node_id: number;
  node_name: string;
  stage_index: number;
  difficulty: number;
  mastery: number;
}

export interface TodayPlan {
  week: number;
  total_weeks: number;
  stage_names: string[];
  nodes: TodayItem[];
  reviews: ReviewItem[];
}

/** 单元/测验生成入参(P1) */
export interface StudyGenInput {
  id: string;
  node_id: number;
  free_form?: boolean | null;
  force?: boolean | null;
  /** 错题变式再练(P3):上一轮答错题的题干与答案汇总 */
  wrong_context?: string | null;
  provider_id: string;
  model: string;
  custom_base_url: string | null;
  custom_display_name: string | null;
}

export function studyNodes(id: string): Promise<NodeState[]> {
  return invoke('study_nodes', { id });
}

export function studyUnitGenerate(input: StudyGenInput): Promise<StudyUnit> {
  return invoke('study_unit_generate', { input });
}

export function studyQuizNew(input: StudyGenInput): Promise<StudyQuiz> {
  return invoke('study_quiz_new', { input });
}

export function studyQuizSubmit(
  id: string,
  quizId: number,
  answers: QuizAnswerInput[],
): Promise<QuizResult> {
  return invoke('study_quiz_submit', { id, quizId, answers });
}

export function studyMasterySet(id: string, nodeId: number, rating: number): Promise<number> {
  return invoke('study_mastery_set', { id, nodeId, rating });
}

export function studyNodeDone(id: string, nodeId: number, done: boolean): Promise<void> {
  return invoke('study_node_done', { id, nodeId, done });
}

export function studyReviewQueue(id: string): Promise<ReviewItem[]> {
  return invoke('study_review_queue', { id });
}

export function studyReviewDone(id: string, nodeId: number, correct: boolean): Promise<ReviewItem> {
  return invoke('study_review_done', { id, nodeId, correct });
}

export function studyToday(id: string): Promise<TodayPlan> {
  return invoke('study_today', { id });
}

// ---------- P2:每日任务 ----------

export interface StudyTask {
  id: number;
  plan_id: string;
  /** YYYY-MM-DD */
  date: string;
  /** learn 学新 / quiz 检索练习 / redo 错题重做 / review 间隔复习 */
  kind: 'learn' | 'quiz' | 'redo' | 'review';
  node_id: number | null;
  node: string;
  minutes: number;
  status: 'pending' | 'done';
  /** date 早于今天且未完成 */
  overdue: boolean;
}

export function studyTasksToday(id: string): Promise<StudyTask[]> {
  return invoke('study_tasks_today', { id });
}

export function studyTaskDone(id: string, taskId: number, done: boolean): Promise<void> {
  return invoke('study_task_done', { id, taskId, done });
}

// ---------- P2:专注计时 / 统计 / 记忆注入 ----------

export interface DayStat {
  date: string;
  minutes: number;
  activity: number;
}

export interface StudyStats {
  streak_days: number;
  total_days: number;
  total_minutes: number;
  done_nodes: number;
  total_nodes: number;
  mastery_avg: number;
  /** 最近 35 天(含今天,升序) */
  days: DayStat[];
}

export function studyFocusLog(id: string, nodeId: number | null, minutes: number): Promise<void> {
  return invoke('study_focus_log', { id, nodeId, minutes });
}

export function studyStats(id: string): Promise<StudyStats> {
  return invoke('study_stats', { id });
}

export function studyMemoryDigest(): Promise<string | null> {
  return invoke('study_memory_digest');
}

// ---------- P4:学习中心(试题库 / 试卷 / 批改) ----------

export interface BankQuestion {
  id: number;
  plan_id: string | null;
  node_id: number | null;
  node_name: string;
  subject: string;
  qtype: string;
  stem: string;
  options: string[];
  answer: string;
  explanation: string;
  difficulty: number;
  /** quiz(小测生成) / manual(手动加题) */
  source: string;
  created: string;
  updated: string;
}

export interface BankListResult {
  items: BankQuestion[];
  total: number;
}

export interface BankListInput {
  plan_id?: string | null;
  qtype?: string | null;
  keyword?: string | null;
  limit?: number;
  offset?: number;
}

export interface BankAddInput {
  plan_id?: string | null;
  node_id?: number | null;
  node_name?: string;
  subject?: string;
  qtype?: string;
  stem: string;
  options?: string[];
  answer: string;
  explanation?: string;
  difficulty?: number;
}

export interface BankUpdateInput {
  stem: string;
  options?: string[];
  answer: string;
  explanation?: string;
  difficulty?: number;
}

export interface GradeOutcome {
  score: number;
  correct: boolean;
  feedback: string;
  expected: string;
  explanation: string;
}

export interface PaperRow {
  id: string;
  plan_id: string | null;
  title: string;
  /** quiz(小测) / exam(整卷) */
  kind: string;
  /** draft(未提交) / graded(已批改) */
  status: string;
  score: number | null;
  total: number;
  created: string;
  submitted_at: string | null;
  graded_at: string | null;
  answered: number;
}

export interface PaperItem {
  id: number;
  question_id: number;
  idx: number;
  qtype: string;
  stem: string;
  options: string[];
  answer: string;
  explanation: string;
  node_name: string;
  user_answer: string | null;
  correct: boolean | null;
  score: number | null;
  feedback: string | null;
  graded: boolean;
}

export interface PaperDetail {
  paper: PaperRow;
  items: PaperItem[];
}

export interface PaperAnswerInput {
  idx: number;
  user_answer: string;
}

export interface PaperItemResult {
  question_idx: number;
  correct: boolean;
  score: number;
  expected: string;
  explanation: string;
  feedback: string;
  user_answer: string;
}

export interface PaperResult {
  score: number;
  total: number;
  correct_count: number;
  results: PaperItemResult[];
}

export interface ProviderCreds {
  provider_id: string;
  model: string;
  custom_base_url: string | null;
  custom_display_name: string | null;
}

export function studyBankList(input: BankListInput): Promise<BankListResult> {
  return invoke('study_bank_list', { input });
}

export function studyBankAdd(input: BankAddInput): Promise<BankQuestion> {
  return invoke('study_bank_add', { input });
}

export function studyBankUpdate(id: number, input: BankUpdateInput): Promise<BankQuestion> {
  return invoke('study_bank_update', { id, input });
}

export function studyBankDelete(id: number): Promise<void> {
  return invoke('study_bank_delete', { id });
}

export function studyPaperList(planId: string | null): Promise<PaperRow[]> {
  return invoke('study_paper_list', { planId });
}

export function studyPaperGet(id: string): Promise<PaperDetail> {
  return invoke('study_paper_get', { id });
}

export function studyPaperCreate(input: {
  plan_id?: string | null;
  title?: string | null;
  kind?: string | null;
  question_ids: number[];
}): Promise<PaperDetail> {
  return invoke('study_paper_create', { input });
}

export function studyPaperAuto(input: {
  plan_id?: string | null;
  node_ids?: number[];
  count: number;
  title?: string | null;
  kind?: string | null;
}): Promise<PaperDetail> {
  return invoke('study_paper_auto', { input });
}

export function studyPaperSubmit(
  input: {
    paper_id: string;
    answers: PaperAnswerInput[];
  } & ProviderCreds,
): Promise<PaperResult> {
  return invoke('study_paper_submit', { input });
}

export function studyPaperDelete(id: string): Promise<void> {
  return invoke('study_paper_delete', { id });
}

export function studyQuestionGrade(
  input: {
    question_id?: number | null;
    qtype?: string | null;
    stem?: string | null;
    answer?: string | null;
    explanation?: string | null;
    user_answer: string;
  } & ProviderCreds,
): Promise<GradeOutcome> {
  return invoke('study_question_grade', { input });
}
