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
  title: string;
  est_minutes: number;
  content_md: string;
  created: string;
}

export interface QuizQuestion {
  idx: number;
  type: 'choice' | 'judge' | 'short';
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
  /** short 题自评:true = 记得 / false = 没记住 */
  self_correct?: boolean | null;
}

export interface QuizGrade {
  question_idx: number;
  correct: boolean;
  expected: string;
  explanation: string;
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
