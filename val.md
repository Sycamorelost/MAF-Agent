# MAF Agent 功能新增报告:学习规划板块(Study Planner)审阅稿

> 日期:2026-10-21。
> 调研对象:[Deep-tutor](https://github.com/hershate/Deep-tutor)(本地源码 `D:\MyProject\Deep-tutor-main`),FastAPI + React + Skills 提示词技能包 + SQLite 的 AI 学习导师平台。
> 结论先行:**强烈推荐引入学习规划板块**。Deep-tutor 的"确定性流水线 + 技能提示词包 + JSON Schema 校验重试"三件套与本项目研究引擎(分阶段/检查点/模板注入)同构,方法论可直接移植,提示词包与数据契约可近乎原样复用。

---

## §1 Deep-tutor 调研结论

### 1.1 它是什么

面向"给定一个学习主题 → 产出一份可执行学习计划 → 陪伴学习到掌握"闭环的 AI 导师:

```
对齐意图 → 知识地图 → 阶段设计 → [资料 ∥ 排期] → 校验 → 出计划
   ↓                                                        ↓
学习单元(自学材料) → 测验 → 掌握度 → SM-2-lite 复习 → 打卡/专注/统计
```

### 1.2 三个核心设计(与本项目高度同构)

| Deep-tutor 的做法 | 本项目已有对应物 | 移植成本 |
|---|---|---|
| **确定性编排流水线**:`parse → understand → map → stage → [resource ∥ schedule] → validate → export`,不用自由 Agent 乱选工具;validate 失败带反馈重跑 ≤3 次,再失败降级默认 3 阶段并标记 `repaired` | 研究引擎的 staged 模式(阶段推进/检查点/暂停终止)、concise 模式 | 低,流程骨架照搬 |
| **技能提示词包**:`skills/*/SKILL.md` 运行时注册为 system prompt;稳定方法论放前缀(可缓存),易变输入放 user message;**只允许返回约定 Schema 的 JSON**,Python 端 pydantic 校验 | `templates.json` 技能/模板注入体系(fragments/skills,`template::effective` 已支持自定义覆盖内置) | 低,新技能条目即可 |
| **JSON 契约驱动**:每个工具定义输入/输出 Schema(如 `StudyUnitSpec`:单元标题/目标/前置知识/预估时间/markdown 内容),切分算法"难点单独成单元、易点聚类、前置排序" | 研究引擎的 trail 事件/检查点 JSON、报告模板 `{references}` 锚点 | 低 |

### 1.3 值得直接复用的资产(已核对源码)

| 资产 | Deep-tutor 位置 | 复用方式 |
|---|---|---|
| 意图澄清方法论(抽取 topic/level/goal/daily_minutes/deadline,判空缺字段触发追问,禁止编造) | `skills/deep-tutor-intent/SKILL.md` | **提示词近乎原样移植** |
| 阶段设计规则(3-6 阶段、难度 1-5、相邻差 ≤2、里程碑只依赖当前及之前阶段、topics 3-6 个) | `skills/deep-tutor-stages/SKILL.md` | **原样移植**(本报告 §3.2 附契约) |
| 计划数据模型(plans/knowledge_nodes/study_units/progress/quizzes/quiz_attempts/study_materials/learning_logs) | `backend/alembic/versions/beba50db3cc1_initial_schema.py` | 字段设计照搬,SQLite 建表 |
| 自学单元切分算法(知识点按阶段+难度排序,难点独立成单元,易点聚类) | `backend/app/tools/generate_study_materials.py:L164-209` | Rust 重写,逻辑照搬 |
| SM-2-lite 复习排程 + 打卡 + 专注计时交互 | `ProgressBoard/CheckinCalendar/FocusTimer.tsx` | 交互模式参考,P1 实现 |
| 五维校验(依赖闭包/难度阶梯/认知负荷/里程碑可行性/缺口分析) | `skills/deep-tutor-validate/SKILL.md` | 提示词移植,作为流水线终检 |

---

## §2 功能定位:MAF Agent 学习规划板块

### 2.1 一句话定位

新增「**学习规划**」顶级板块(与 研究/绘图/翻译 并列):用户输入"想学什么",AI 生成一份**阶段化、带资料、带里程碑、可打卡**的学习计划;学习过程中可生成**自学材料**、做**小测验**、按遗忘曲线**复习**,并与现有 研究引擎/知识库/会话记忆 联动。

### 2.2 与现有板块的协同(不重复造轮子)

| 现有能力 | 在学习规划中的角色 |
|---|---|
| 研究引擎(staged/concise) | 计划内"某知识点想深挖"时一键发起研究,报告落回该节点 |
| `templates.json` 技能注入 | 承载 8 个新技能提示词(intent/map/stages/resources/schedule/validate/guide/quiz),支持用户自定义覆盖,沿用"自由结构/模板可选"理念 |
| 知识库(notes) | 学习笔记沉淀:单元笔记、测验错题可一键存为知识卡片 |
| 会话记忆 | 学完的单元摘要写入记忆,会话中自动注入"我在学 X,已学到 Y" |
| 附件 materials | 计划可挂附件作为学习资料源 |
| 玻璃抽屉布局体系 | 新页完全沿用 pg/pg-aside 抽屉 + 标题栏 pill 开合 + 挤出式让位 |

> **UI 沿用原则**:Deep-tutor 仅借鉴流程编排与数据契约,**视觉零移植**。`/plan` 页全部由本项目既有组件类名与 `_tokens.css` 变量构成,不引入新色彩/新控件范式(映射清单见 §3.1 与 §4.3)。

---

## §3 功能设计(用户视角)

### 3.1 页面结构 `/plan`(UI 严格沿用本项目既有组件)

布局 = 全站统一骨架:`.pg` 容器 + `.pg-aside.drawer` 左抽屉(标题栏 pill 开合、挤出式让位、右上角 aside-close ✕)+ `.pg-main`。各区域与既有实现的对应:

- **主区-列表视图**:同研究首页方案库卡片的网格与卡片样式(同一套卡片类与 hover/选中态);头部 `pg-main-head` + `pg-chip` 过滤(进行中/已完成/已归档);卡内进度条复用 `rp-progress`;`新建计划` = `pg-foot` 主按钮同款。
- **主区-计划详情视图**(核心):
  - 顶部 `rp-head` 同款行:主题 + `mode-tag` 式状态徽标 + 进度条 + `btn-ghost` 操作(继续学习/生成测验/导出,导出下拉复用 `rp-export-menu`);
  - 阶段树:复用研究页 `StageTrail` 阶段卡与 `rp-tl` 节点结构(标题行 + pill 徽标 + 内容区),知识点行 = `side-item` 式行样式(悬停/选中态一致),`must_learn` 用现有 `pill` 徽标,掌握度用 `rp-progress` 细条;
  - 底部今日待办/打卡(P2):普通 `pg-sec` 分组 + 简单网格,不引入日历组件库。
- **学习视图**(点知识点的「学习」):复用运行页"上时间线下内容"结构——单元目录为列表样式(`side-list` 同款),材料正文用 `.md report-md` 的 MemoMarkdown 渲染(与研究报告完全一致),读完勾选 = `pg-chip` 完成态。
- **左抽屉(规划面板)**:`pg-aside-head`(标题 + 一句指导文案)+ `pg-sec / pg-lab / pg-field / pg-pair` 表单体系(与研究配置面板完全同款);澄清追问 = 面板内 `pg-warn` 追加追问字段,不弹窗打断;偏好保存 = 面板底部 `pg-foot`。
- **弹层**:测验作答/删除确认等一律走本项目 dialog 体系(遮罩 + 玻璃卡片 + `panel-toggle` ✕,同 ResearchTemplateDialog/AgentPickerDialog),**不移植 Deep-tutor 的 QuizModal 视觉**。
- **文案与提示**:`pg-hint`(含「研究任务消耗 token 较多,请留意预算设置」同款预算提示)、`pg-warn`、`side-empty` 空态,口径与现有各页一致。

### 3.2 计划生成流水线(确定性编排,移植 Deep-tutor)

```
① parse      解析输入 + 已选附件
② intent     意图理解:抽 {topic, level, goal, daily_minutes, deadline, lang};
             必填缺失 → 前端弹澄清问题(最多 2 轮),用户答完续跑
③ map        知识地图:节点含 name/difficulty(1-5)/why/must_learn/前置依赖边
④ stages     阶段设计:3-6 阶段,难度递增相邻差≤2,里程碑仅依赖已学(契约见 §1.3)
⑤ resource ∥ schedule(并行)
             资料:每阶段 2-4 个可检索/可挂附件的学习资源建议
             排期:按 daily_minutes 把阶段展开成周粒度日程
⑥ validate   五维校验(依赖闭包/难度阶梯/认知负荷/里程碑可行性/缺口分析),
             问题项带反馈重跑对应步骤 ≤3 次;仍失败 → 降级 3 阶段默认结构并标 repaired
⑦ export     落库 + 返回计划 JSON
```

每步事件以 trail 事件流推给前端(与研究页时间线同款 UI),支持 暂停/终止/断点续跑(复用研究引擎检查点机制)。

### 3.3 学习执行闭环(P1)

- **自学材料**:按单元切分算法(难点独立、易点聚类)生成学习单元,`self-study-guide` 技能产出 Markdown(概念讲解/示例/小结自测),可配「自由结构」开关;
- **测验**:`study-quiz` 技能按单元出 5 题(选择/判断/简答),前端以 dialog 体系弹层作答(样式同 ResearchTemplateDialog),判分结果回写;
- **掌握度与复习**:节点 mastery = 加权(自评完成 ×0.4 + 测验正确率 ×0.6);错题与低掌握节点进 SM-2-lite 复习队列(间隔 1/3/7/14/30 天);
- **打卡与专注**:完成今日待办自动打卡;内置专注计时器(Pomodoro,可选);
- **统计**:连续天数/累计时长/掌握度趋势(ProgressBoard 同款交互)。

---

## §4 技术设计

### 4.1 数据模型(SQLite,字段设计照搬 Deep-tutor schema)

```
plans          id, title, topic, level, goal, daily_minutes, deadline,
               status(generating/active/done/archived), stages_json, created, updated
knowledge_nodes id, plan_id, stage_index, name, difficulty, why, must_learn,
               prereq_json, mastery REAL DEFAULT 0, done_at
study_units    id, plan_id, node_ids_json, title, objectives_json,
               prerequisites_json, est_minutes, content_md, created
quizzes        id, plan_id, unit_id, payload_json(题目数组), created
quiz_attempts  id, quiz_id, question_idx, user_answer, correct, answered_at
reviews        id, plan_id, node_id, due, interval_days, last_result
checkins       id, plan_id, date, minutes, note
study_logs     id, plan_id, node_id, kind(learn/quiz/review/research), minutes, at
```

### 4.2 后端(Rust,新增 `src-tauri/src/study/`)

- `study/mod.rs`:编排流水线 + 命令 `study_start / study_events / study_pause / study_resume / study_cancel / study_list / study_detail / study_clarify / study_unit_generate / study_quiz_new / study_quiz_submit / study_checkin / study_review_queue`;
- LLM 调用、事件通道、检查点、JSON 提取与校验重试(≤3 次 + 降级)**直接复用 research 引擎现有设施**;
- 技能提示词注册进 `templates.json`(8 个新 key,`study_intent` 等),支持用户自定义覆盖与「自由结构」;
- 校验规则(阶段数 3-6、难度差 ≤2、里程碑引用检查)在 Rust 端做**确定性校验**,不依赖模型自检。

### 4.3 前端(新增 `src/pages/StudyPage.tsx` + 子组件)

- 路由 `/plan`,NavMenu 增「学习规划」项(新增一枚与现有 icons 同规格的 16px 线性图标,原比例使用不缩放);Titlebar `asideAllowed` 增 `/plan`;
- **Deep-tutor 组件 → 本项目等价实现**(只取交互逻辑,视觉零移植):

  | Deep-tutor 组件 | 本项目等价实现 |
  |---|---|
  | Plans 列表卡 | 研究方案库同款卡片 + `pg-chip` 状态过滤 |
  | PlanDetail 阶段树 | `StageTrail` 阶段卡 + `rp-tl` 节点 + `side-item` 行样式 |
  | PlanOutline 大纲 | `pg-sec` 折叠分组 |
  | Learn 阅读页 | 运行页"时间线 + 内容"结构,正文 `report-md` 同款渲染 |
  | QuizModal | 项目 dialog 体系(遮罩 + 玻璃卡片 + `panel-toggle` ✕) |
  | ProgressBoard 掌握度 | `rp-progress` 进度条 + `pill` 数字徽标 |
  | CheckinCalendar / FocusTimer | `pg-sec` 简单网格 / `pg-foot` 内计时 chip(P2) |

- 样式只允许使用 `_tokens.css` 变量与既有类名;确需新增类,命名与视觉均从现有组件派生(沿用 `pg-`/`rp-`/`side-` 同一套间距、圆角、玻璃参数),不新增色彩与控件范式;
- 时间线复用研究页 `rp-tl` 样式与贴底跟随滚动逻辑;token 提示沿用 pg-hint 口径。

### 4.4 直接移植提示词契约示例(阶段设计,stages)

```json
{"stages":[{"index":0,"name":"基础","goal":"…","difficulty":1,"estimated_weeks":2,
  "topics":[{"name":"…","why":"…","must_learn":true}],"milestone":"…"}]}
```
硬规则:阶段 3-6 个;每阶段 topics 3-6 个;相邻阶段难度差 ≤2;milestone 只用当前及之前阶段知识可达成;topics 不得编造地图外知识点;只输出 JSON,不包代码围栏。

---

## §5 实施计划

| 阶段 | 内容 |
|---|---|
| **P0(本期)** | 建表 + `study` 模块流水线(intent/map/stages/resource/schedule/validate)+ `/plan` 页(列表/详情/阶段树/时间线)+ 意图澄清交互 + 深挖研究联动 + 导出 Markdown |
| **P1** | 自学材料生成 + 测验/掌握度 + SM-2-lite 复习队列 + 今日待办 |
| **P2** | 打卡日历 + 专注计时 + 统计面板 + 记忆注入(学过的内容在会话中被感知) |

## §6 验收标准

1. 输入「零基础,每天 30 分钟,8 周学会 Rust」→ 产出 3-6 阶段计划,难度递增无跳级,里程碑不引用未来知识;
2. 缺「每天可投入时长」时,流程暂停并在规划面板内联追问(`pg-warn` 追加字段),答后续跑不重来;
3. 校验失败自动带反馈重试 ≤3 次,最终降级方案带 `repaired` 标记提示;
4. 知识点「深挖研究」唤起研究引擎,完成后报告链接挂回该节点;
5. 全程时间线可暂停/终止/断点续跑;启动按钮旁有 token 预算提示;
6. UI 抽查:全部交互元素由既有组件类名构成(§4.3 映射表逐项核对),与现有页面并排无违和,无新增色彩/控件范式;
7. `cargo test` 新增流水线校验用例全绿,`tsc -b` 零错误。

## §7 风险与对策

| 风险 | 对策 |
|---|---|
| 规划全流程 token 消耗大(7 步 × 多次调用) | 沿用本项目预算提示 + 允许「精简模式」(跳过资料/排期,仅出阶段树);技能前缀稳定便于 prompt 缓存 |
| 模型输出不合 Schema | Rust 端确定性校验 + 反馈重试 ≤3 + 降级默认 3 阶段(标 repaired),永不空转 |
| 打卡/统计诱导过度设计 | P2 再做,先把"生成计划→学习→深挖"跑通 |

---

## §8 P0 实施结果(2026-10-21)

P0 已全部落地并通过验证,与 §4 设计的差异点如下:

| 项 | 结果 |
|---|---|
| 数据库 | V15 三表 `plans` / `knowledge_nodes` / `study_logs` 落地;计划完整载荷存 `plans.stages_json` 并镜像 `data/study/{id}/plan.json`,过程事件落 `trail.jsonl`;重启清理复用 L1 模式(generating → error) |
| 后端 | 新增 `src-tauri/src/study/`(mod 流水线 / template 提示词 / store 持久化)与 `commands/study.rs`,注册 10 个命令:study_preflight / study_start / study_cancel / study_pause / study_resume / study_list / study_get / study_delete / study_deep_dig / study_export_markdown |
| 流水线 | intent(合并 preflight 结果,零 LLM)→ map → stages(确定性校验 + 反馈重试 ≤3 + 降级 repaired)→ **resources ∥ schedule** → validate → export;校验规则:阶段 3-6、每阶段 topics 3-6、难度差 ≤2、知识点全覆盖不重复、必学全安排、里程碑非空 |
| **差异①** | 排期(schedule)改为 **Rust 确定性计算**(阶段周数展开、上限 78 周),不调 LLM——零幻觉零 token,优于原设计的模型排期 |
| **差异②** | 澄清由 `study_preflight` 命令完成:缺字段回面板 `pg-warn` 内联追问,带答案 `study_start`(意图 JSON 直传合并);补充回答时面板提供结构化控件(水平下拉/分钟数/日期),不是自由文本 |
| 深挖研究联动 | `study_deep_dig` 落 `study_logs(kind='research')` 并返回预填主题;经 `sessionStorage['metaagent.research.draft']` 跳 `/research`,ConfigPanel 新增 `initialTopic` prop 预填;报告链接挂回节点留 P1 |
| 前端 | `src/pages/StudyPage.tsx`(列表 / 运行时间线 / 计划总览阶段树)+ `stores/study.ts` + `lib/ipc/study.ts` + `IconGraduation`;全部复用既有类名(rp-hist/rp-tl/rp-head/mode-tag/pill/pg-* 等),零新增视觉范式;NavMenu 增「学习规划」,Titlebar asideAllowed 增 `/plan`,App 增路由 |
| 验证 | `tsc -b` 零错误;`cargo test` 217 通过 0 失败(新增:结构校验 4 例、降级覆盖、排期展开/上限、JSON 容错提取、节点/资料解析修复、导出 Markdown、模板回退、mark_interrupted) |

---

## §9 P1+P2 实施结果(2026-10-21)

P1(学习执行闭环)与 P2(打卡/专注/统计/记忆注入)已全部落地,`tsc -b` 零错误,`cargo test` 232 通过 0 失败。与 §3.3/§4.1 设计的差异点:

| 项 | 结果 |
|---|---|
| 数据库 | V15→V16:新增 `study_units` / `quizzes` / `quiz_attempts` / `reviews` 四表(均含迁移测试);`knowledge_nodes` 追加 `self_rating` / `quiz_score` 列(ensure_column,-1 = 未评哨兵) |
| 自学材料 | `study_units` 每知识点一份(UNIQUE(plan_id,node_id) 幂等缓存,force 可重建),非设计稿的聚类切分——一个知识点一份指南,更贴阅读心智;模板 `study_guide` 输出 Markdown(概念讲解/常见误区/小结自测) |
| 测验 | 模板 `study_quiz` 恰好 5 题(choice/judge/short);Rust 端 `parse_quiz` 容错提取 JSON + 归一判分(choice 取首个字母 / judge 归一对错 / short 用户自评),解析失败带反馈重试 1 次 |
| 掌握度 | `mastery = 自评×0.4 + 测验×0.6`,单项缺失取另一项,双缺为 0;`set_mastery` ≥1.0 置 done_at;自评入口在 LearnView 底部三档(没懂 0 / 大概懂 0.5 / 掌握 1),非独立弹层 |
| 复习 | SM-2-lite:间隔 [1,3,7,14,30] 天,答对晋级一档、答错回退 1 天;测验 score≥0.8 晋级(无记录 3 天起步),否则重置 1 天;due 为 YYYY-MM-DD 字符串直接比较 |
| 今日待办 | 由 `plans.created` 推当前排期周 → 该周阶段内未完成节点 + 到期复习,合入计划详情页顶部;不单独建页 |
| 打卡 | **不建 checkins 表**(与 §4.1 差异):当日存在任意 `study_logs` 行(learn/quiz/research/focus 皆可)即视为打卡,零冗余 |
| 统计面板 | 连续打卡 / 累计天数 / 专注分钟 / 完成节点 / 平均掌握 5 项 chips + 35 天活动网格(升序最旧→今天,透明度按 activity);专注计时 `focus_log` clamp 1-480 分钟,满 1 分钟才落库 |
| 记忆注入 | 后端 `study_memory_digest` 命令(最多 3 个 active/done 计划,每计划一行进度 + 最近单元名);ChatPage `buildSystemWith` 把摘要块追加到 system(与记忆正文并列,无记忆时也可独立注入) |
| 前端 | StudyPage 扩展:LearnView(材料阅读 + 重新生成 + 自评)/ QuizDialog(答题/交卷/结果)/ StatsSec + FocusTimer / PlanDetail 阶段树节点行加掌握度 pill 与 学习/测验 按钮;全部沿用 pg-* / pill / dialog 既有类名,零新增视觉范式 |
| 验证 | `tsc -b` 零错误;`cargo test` 232 通过 0 失败(P1/P2 新增 15 例:grade/next_interval/weighted_mastery/current_week/strip_outer_fence/parse_quiz/quiz_submit/review_done/set_mastery+today/nodes_state/stats/memory_digest 等);dev 运行确认无编译错误 |

### §9.1 真机 UI 完整测试(2026-10-21)

在运行中的真实应用(现有计划「入门java的后端开发」)上以桌面自动化完成端到端测试,10/10 步骤通过,无任何报错:

| 环节 | 结果 |
|---|---|
| 导航与渲染 | ⋮ 菜单 → 学习规划 → 列表 → 计划详情:计划总览 chips / 今日待办(第 1/12 周 + 3 个本周知识点)/ 阶段树(学习/测验/深挖按钮、本周 pill)/ 学习统计 5 chips + 35 格网格全部正常渲染 |
| 专注计时 | 开始 → 计时 01:00 → 结束并记录,无报错;专注 +1 分钟落库(重载确认) |
| 学习材料 | 「Java基础语法」生成约 15 秒完成,Markdown 三小节(概念讲解/常见误区/小结自测)渲染正常;自评「大概懂」记录成功 |
| 测验 | 恰好 5 题(3 单选 + 1 判断 + 1 简答);简答「显示参考答案并自评」→ 交卷判分:得分 100%、掌握度 80%(=0.5×0.4+1.0×0.6)、复习排到 2026-10-24(+3 天);期间出题解析失败触发自动重试一次,二次成功——重试机制真实生效 |
| 持久化 | 重载后掌握 pill(80%)、统计(连续打卡 1 天/平均掌握 4%)、活动网格末格变绿均正确 |

测试后修复的两个观察项:①详情页徽标把「载荷就绪」误显示为「已完成」(与列表「进行中」不一致)→ 就绪态不再显示徽标;②专注结束后统计不即时刷新 → 落库后回调整体刷新。修复后 `tsc -b` 零错误,HMR 无报错。

---

# §10 软件工程改造决策报告(2026-10-21 · 待审阅)

> 本文覆盖 val.md 的决策范围:前半部分(§1–§9.1)保留为学习规划板块的历史实施记录,本 §10 为**当前唯一生效的改造决策稿**。
> 结论先行:改造分两条线并行推进——
> **线一(正确性,P0)**:修复 token 用量统计与统计页价格计算的 14 处口径缺陷,先保证"数字可信",再谈拆分;
> **线二(可维护性,P1)**:对 12 个超标文件做**纯搬运式拆分**(零行为变更),前端页面 ≤400 行、组件 ≤300 行,后端单文件 ≤500 行、`mod.rs` ≤300 行。
> 两线互不阻塞,但线一优先——因为拆分后的统计模块会成为线一的长期载体,先定口径再搬家,避免返工。

## 10.1 现状:文件规模盘点

统计口径:`src/**/*.ts(x)` 与 `src-tauri/src/**/*.rs` 全量行数(2026-10-21 实测)。

| # | 文件 | 行数 | 超限倍数(目标 500 / 页面 400) |
|---|---|---|---|
| 1 | `src-tauri/src/research/mod.rs` | 2841 | 5.7× |
| 2 | `src-tauri/src/commands/provider.rs` | 2259 | 4.5× |
| 3 | `src/pages/SettingsPage.tsx` | 1614 | 4.0× |
| 4 | `src-tauri/src/study/mod.rs` | 1468 | 2.9× |
| 5 | `src-tauri/src/study/learn.rs` | 1362 | 2.7× |
| 6 | `src/pages/StudyPage.tsx` | 1342 | 3.4× |
| 7 | `src/pages/ChatPage.tsx` | 1328 | 3.3× |
| 8 | `src-tauri/src/commands/research.rs` | 1052 | 2.1× |
| 9 | `src/pages/research/ConfigPanel.tsx` | 981 | 2.5× |
| 10 | `src-tauri/src/provider/model_cards.rs` | 871 | 1.7× |
| 11 | `src-tauri/src/provider/openai.rs` | 796 | 1.6× |
| 12 | `src-tauri/src/research/search.rs` | 748 | 1.5× |
| 13 | `src/pages/TranslatePage.tsx` | 635 | 1.6× |
| 14 | `src-tauri/src/commands/memory.rs` | 611 | 1.2× |
| 15 | `src-tauri/src/db/schema.rs` | 568 | 1.1× |
| 16 | `src/components/StagePipelineEditor.tsx` | 564 | 1.9×(组件阈值 300) |
| 17 | `src-tauri/src/db/messages.rs` | 557 | 1.1× |
| 18 | `src-tauri/src/db/usage.rs` | 552 | 1.1× |
| 19 | `src/pages/DrawPage.tsx` | 545 | 1.4× |
| 20 | `src/components/AgentPickerDialog.tsx` | 542 | 1.8×(组件阈值 300) |
| 21 | `src-tauri/src/research/blueprint.rs` | 527 | 1.1× |
| 22 | `src/components/Composer.tsx` | 519 | 1.7×(组件阈值 300) |
| 23 | `src-tauri/src/paths.rs` | 507 | 1.0× |
| 24 | `src-tauri/src/commands/attachment.rs` | 505 | 1.0× |
| 25 | `src/components/icons.tsx` | 502 | 数据文件,豁免 |

**规模红线(建议写入工程规范)**

- 后端:普通模块 ≤500 行;`mod.rs` 只允许做「声明 + 编排 + 导出」,≤300 行。
- 前端:页面(`pages/*.tsx`)≤400 行;组件 ≤300 行;纯逻辑 hook/store ≤200 行。
- 豁免白名单:`provider/model_cards/builtin_cards.rs`、刊例目录表、`components/icons.tsx`、`pages/settings/constants.ts`——属于"数据表/图标表",按条数而非逻辑复杂度增长。

## 10.2 拆分方案(纯搬运,零行为变更)

**拆分原则(硬约束)**

1. 只移动代码,不顺手重命名、不重构逻辑、不改公共 API 签名;确需改路径的,同步 `commands/mod.rs` 的注册表。
2. 一个阶段只动一个文件;每个阶段独立提交,可单独回滚。
3. 每个阶段结束必须 `tsc -b` 零错误 + `cargo test` 全绿(当前基线 232 通过)。
4. 跨模块符号一律 `pub(crate)` 显式导出,不使用 `include!`、不复制粘贴。

### 10.2.1 后端映射

| 现文件(行数) | 拆分后 | 各文件职责与目标行数 |
|---|---|---|
| `research/mod.rs`(2841) | `research/mod.rs` + `pipeline.rs` + `prompts.rs` + `validate.rs` + `events.rs` + `export.rs` | mod ≤250(声明/编排);pipeline ≤450(阶段推进/检查点);prompts ≤350(模板拼装);validate ≤250;events ≤200;export ≤250 |
| `commands/provider.rs`(2259) | `commands/provider/{mod,chat,usage,pricing,keys,models}.rs` | mod ≤150(仅注册);chat ≤450(流式/回退/Key 轮换);usage ≤250(4 个统计命令);pricing ≤300(`PricingCtx`/`pricing_for`/`cost_micro_of`);keys ≤300;models ≤350 |
| `study/mod.rs`(1468) | `study/{mod,pipeline,validate,schedule}.rs` | mod ≤200;pipeline ≤450;validate ≤300;schedule ≤250 |
| `study/learn.rs`(1362) | `study/learn/{mod,units,quiz,mastery,review,stats}.rs` | 各 ≤300;mod 仅 re-export |
| `commands/research.rs`(1052) | `commands/research/{mod,start,events,list,export}.rs` | 各 ≤300 |
| `commands/memory.rs`(611) | `commands/memory/{mod,crud,diff}.rs` | 各 ≤300 |
| `provider/model_cards.rs`(871) | `model_cards/{mod,builtin,catalog,vendor,user}.rs` | mod ≤200;catalog/vendor 为数据表,豁免行数 |
| `provider/openai.rs`(796) | `provider/openai/{mod,chunk,body}.rs` | mod ≤250;chunk ≤250(含 usage 解析);body ≤200 |
| `research/search.rs`(748) | `research/search/{mod,engine,fetch,rank}.rs` | 各 ≤300 |
| `db/schema.rs`(568) | `db/schema/{mod,migrate_v1,v16}.rs` + `tests` | mod ≤150;迁移段按版本区间切分 |
| `db/messages.rs`(557) | `db/messages/{mod,crud,search}.rs` | 各 ≤300 |
| `db/usage.rs`(552) | `db/usage/{mod,record,query}.rs` | record ≤200(写入/掩码);query ≤350(聚合);mod 仅 re-export |

### 10.2.2 前端映射

| 现文件(行数) | 拆分后 | 职责与目标行数 |
|---|---|---|
| `SettingsPage.tsx`(1614) | `pages/settings/{SettingsPage,ProvidersView,UsageView,SearchView,GeneralView,DataDirView,NavMenuView,AboutView}.tsx` + `pages/settings/useUsageData.ts` | 壳 ≤150(导航/路由 query);各视图 ≤350;UsageView ≤400(含热力图/明细表);`useUsageData` ≤150(取数/下钻状态) |
| `StudyPage.tsx`(1342) | `pages/study/{StudyPage,PlanList,PlanDetail,LearnView,QuizDialog,StatsSec,FocusTimer}.tsx` | 壳 ≤150;Detail ≤350;LearnView ≤300;QuizDialog ≤300;Stats ≤250 |
| `ChatPage.tsx`(1328) | `pages/chat/{ChatPage,MessageList,ChatHeader}.tsx` + `pages/chat/useChatSend.ts` + `useDraftHandoff.ts` | 壳 ≤200;列表 ≤250;发送 hook ≤350(含工具条/附件/草稿);交接 hook ≤120 |
| `research/ConfigPanel.tsx`(981) | `research/config/{ConfigPanel,BasicForm,AdvancedForm,TemplatePicker}.tsx` | 壳 ≤200;表单各 ≤300 |
| `TranslatePage.tsx`(635) | `pages/translate/{TranslatePage,TranslateForm,GlossaryPanel}.tsx` | 各 ≤300 |
| `DrawPage.tsx`(545) | `pages/draw/{DrawPage,DrawForm,DrawResult}.tsx` | 各 ≤300 |
| `StagePipelineEditor.tsx`(564) | `components/stage/{StagePipelineEditor,StageCard}.tsx` + `useStageDrag.ts` | 各 ≤300 |
| `AgentPickerDialog.tsx`(542) | `components/agent/{AgentPickerDialog,AgentCard}.tsx` | 各 ≤300 |
| `Composer.tsx`(519) | `components/composer/{Composer,AttachTray,ToolBar}.tsx` | 各 ≤250 |
| `lib/ipc/research.ts`(481) | 按域拆 `research/{session,template,export}.ts` | 各 ≤250 |
| `stores/research.ts`(420) | `stores/research/{session,trail}.ts` | 各 ≤250 |

> 拆分顺序(每阶段可独立回滚):**A** `db/usage.rs` + `SettingsPage` 用量视图(与 §10.3/§10.4 修复同批) → **B** SettingsPage 其余视图 → **C** `commands/provider.rs` + `openai.rs` → **D** `research/mod.rs` + `commands/research.rs` → **E** `study/mod.rs` + `study/learn.rs` → **F** ChatPage / StudyPage / 组件层。

## 10.3 Token 用量统计:缺陷清单与修复口径

**当前链路**:`openai.rs` 解析 usage → `provider.rs` 采集 `(input, output, total, reasoning)` → `db/usage.rs::insert_on` 落行 → `totals_on/summary_on/hourly_on` 聚合。

| ID | 缺陷(证据位置) | 影响 | 修复 |
|---|---|---|---|
| U1 | `SUM(cost_micro)` 对 NULL 直接忽略,且无"未计价"计数(`db/usage.rs:193/235/368`) | 未定价请求成本被静默漏算,总成本偏低且无提示 | `UsageTotals`/`UsageGroup` 增加 `uncosted_requests` 与 `cost_coverage`;`SUM(COALESCE(cost_micro,0))` 显式化 |
| U2 | `pricing_for` 要求 input+output 均在;厂商只回 `total_tokens` 时 input/output 为 None(`provider.rs:1276`) | 该请求成本算作 0 微元,计入 `cost_source=model_card` 却为 ¥0,误导 | 只给 total 时按 1:1 无法拆分 → 记 `cost_source='unpriced'`、`cost_micro=NULL`,UI 标"计价不完整" |
| U3 | 从未解析 `prompt_tokens_details.cached_tokens` / `cache_read_input_tokens`(`openai.rs:94-104`) | 缓存命中价永远用不上,长会话成本高估(可达 5–10×) | `StreamChunk::Usage` 增 `cached_tokens`;`usage_record` 增列(V17);成本按命中/未命中分段计价 |
| U4 | `input_price = input.or(cache_miss).or(cache_hit)`(`provider.rs:1253`) | 同时有 hit/miss 时静默选后者,口径错 | `PricingCtx` 保留三档价,按 cached 数量分段;无 cached 信息时用 miss 价并标 `cache_unknown` |
| U5 | `reasoning_tokens` 单列,但同时计入 `completion_tokens`(OpenAI 语义) | 用户易理解为"额外 token",产生重复阅读 | 文案改为"其中思考";并加完整性校验 `output ≥ reasoning`(违反的厂商按 reasoning 兜底) |
| U6 | `usage_entries(limit)` 不带 `days`/维度过滤(`provider.rs:577`,`db/usage.rs:312`) | 图表是"近 7 天",明细是"全历史最近 100 条";下钻只过滤这 100 条 → 明细与图表对不上 | 签名改 `usage_entries(limit, days, dimension, key)`,过滤下沉 SQL;前端下钻传参 |
| U7 | 分组 `LIMIT 50` 且无"其他"桶(`db/usage.rs:237`) | 分组成本之和 < 总览,用户认为算错 | 前 50 组 + `其他` 聚合行(requests/tokens/cost 累加),保证 Σ分组 = 总览 |
| U8 | 仅 `(provider_id,model_id,created_at)`、`(slot_id,created_at)` 复合索引(`schema.rs:120-121`) | 纯时间范围查询无法走索引,90 天扫描全表 | 新增 `CREATE INDEX idx_usage_time ON usage_record(created_at)` |
| U9 | `cost_source` 恒为 `"model_card"`(`provider.rs:1271`) | 用户手改价与内置刊例不可区分,无法解释成本变化 | 按卡片 id 是否属于内置集合判定 `model_card` / `custom`,快照内记录 `card_id` 与 `manual` 标记 |
| U10 | `pricing_snapshot` 已落库但 `UsageEntry` 不返回、无重算入口 | 历史成本不可审计、无法复算 | 明细增加"计价来源/单价"抽屉;新增 `usage_recalc(days)` 干跑对比(只报告差异,不写库) |
| U11 | OpenRouter USD→CNY 固定 7.15 且未写入快照(`model_cards.rs:272`) | 汇率变动后历史成本无法解释;快照只有 CNY 值 | 快照增 `fx_rate`/`source_currency`/`note`;汇率改为可配置常量,新记录生效,旧记录不改 |
| U12 | `per_image`/`per_char` 存进快照却不参与计算(`provider.rs:1260-1261`) | 生图、翻译类请求成本恒为 ¥0 | 计价按模型能力分派:tokens / image / char 三种口径;绘图页落 `images` 计数 |
| U13 | 定价匹配用 `official_model_id` 全等(`provider.rs:1251`) | 中转站带日期后缀/前缀的模型名匹配不到 → 未计价 | 增加规范化回退(去日期后缀、大小写、`provider/model` 前缀);仍失败标 unpriced |
| U14 | 单条成本 `(i+o).round() as i64` 四舍五入到微元(`provider.rs:1279`) | 低价模型小额请求被舍成 0,累计系统性偏低 | 存 `cost_micro` 改为 REAL(不 round)或新增 `cost_nano`;聚合在 SQL 内浮点求和,仅展示时截断 |

**统一口径(写入代码注释与设置页说明)**

```
total_tokens  = input_tokens + output_tokens(reasoning 已含在 output 内,单独展示不重复相加)
cost          = (input - cached) × miss_price + cached × hit_price
                + output × output_price
                + images × per_image + chars × per_char
currency      = CNY;USD 来源按快照内 fx_rate 换算并标注
coverage      = 有定价的成功请求 / 成功请求
```

**数据模型变更(建议 V17)**

```sql
ALTER TABLE usage_record ADD COLUMN cached_tokens INTEGER;   -- NULL = 厂商未返回
ALTER TABLE usage_record ADD COLUMN cost_micro REAL;         -- 复用列改为 REAL(或新列 cost_nano)
CREATE INDEX IF NOT EXISTS idx_usage_time ON usage_record(created_at);
```

> 迁移策略:新列可空、不回填旧值;旧记录成本一律保持原值,避免历史数字跳变。`cost_micro` 由 `INTEGER` 改 `REAL` 时 SQLite 需重建表——若担心风险,采用**新增 `cost_nano INTEGER`** 的 A 方案见 §10.5 D1。

## 10.4 统计页价格计算:缺陷清单与修复

| ID | 现状(证据位置) | 影响 | 修复 |
|---|---|---|---|
| S1 | `fmtCost` 按量级选 6/4/2 位小数,`yuan===0 → ¥0`(`SettingsPage.tsx:1244`) | 小于 0.5 微元的请求显示 ¥0,用户以为免费 | 保留位数改为按"至少 1 位有效数字";0 与"未计价"分开显示(¥0 vs —) |
| S2 | 分组成本 `g.cost_micro > 0` 才渲染(`SettingsPage.tsx:1444`) | 未计价分组不显示任何成本线索 | 分组行末尾:有价 → 金额;无价 → `未计价 n 次` pill |
| S3 | 明细固定 `usageEntries(100)`,不随 `usageDays` 变(`SettingsPage.tsx:203-207`) | 时间范围与明细不一致 | 见 U6,签名带 `days` |
| S4 | 下钻在客户端对 100 条过滤(`SettingsPage.tsx:1304`) | 下钻结果与分类统计数量不符 | 过滤下沉 SQL(U6),前端只传 key |
| S5 | 总览"总成本(按定价表估算)"无覆盖率(`SettingsPage.tsx:1359`) | 用户无法判断 ¥ 是否可信 | 加副标题:`已计价 92% · 8 次未计价` |
| S6 | 成本来源列只有 `cost_source` 字符串,无单价展开 | 无法解释单条金额 | 明细行可展开显示快照单价与 token 构成(含 cached) |
| S7 | 汇率固定 7.15 且 UI 未披露(`model_cards.rs:272`) | 汇率变动导致金额不可复现 | 价格说明处标注"USD 按快照汇率换算",并展示快照汇率 |
| S8 | 无重算/对账入口 | 调价后无法评估历史偏差 | 新增"按当前定价试算"按钮:只读对比,输出差异清单(不改库) |

**前端结构随之调整**:`UsageView` 拆出后,内部再分 `UsageOverview` / `UsageHeatmap` / `UsageGroups` / `UsageEntries` / `UsageHealth` 五个片段(随 §10.2.2 A 阶段一并落地),避免拆分后再次膨胀。

## 10.5 待决策项(需审阅确认)

| 编号 | 决策 | 选项 | 推荐 |
|---|---|---|---|
| D1 | 成本精度 | A. 新增 `cost_nano INTEGER`(不改旧列,零迁移风险);B. `cost_micro` 改 REAL(需重建表);C. 维持微元四舍五入 | **A**:新列按纳元写入,聚合用新列,展示回退到元;旧记录 NULL 时用 `cost_micro×1000` 兜底 |
| D2 | 缓存计价 | A. 解析 `cached_tokens` 并分段计价,缺失时按 miss 价;b. 不解析 | **A**(U3 是当前最大金额偏差源) |
| D3 | 拆分范围 | A. 按 §10.2 全量六阶段;B. 仅拆 Top 8(≤900 行文件);C. 仅拆统计相关文件 | **A**,但按阶段合并验收,不做"大爆炸"式重构 |
| D4 | 是否开工 | A. 报告批准后立即执行线一(计量修复)+ 阶段 A;B. 仅执行计量修复;C. 暂不执行 | 待您确认 |

## 10.6 验收标准

1. **计量回归**:固定 4 组样本(普通对话 / 缓存命中 / 生图 / 未定价)比对:单条成本误差 ≤1 纳元,30 天聚合与手工复算一致;分组之和 = 总览(含"其他"桶)。
2. **口径一致性**:总览、分组、明细三处的时间范围与筛选条件完全一致;未计价请求在总览、分组、明细均有可辨识标记。
3. **零行为变更拆分**:每阶段结束后 `git diff` 仅包含文件移动与 `mod` 声明;`cargo test` 全绿(≥232),`tsc -b` 零错误;UI 冒烟覆盖设置/学习/会话/研究四页。
4. **规模门禁**:拆分后无普通源文件 >500 行、页面 >400 行、组件 >300 行;`mod.rs` ≤300 行。
5. **文档同步**:本 §10 勾选完成状态;统计口径写入设置页说明文案。

## 10.7 风险与对策

| 风险 | 对策 |
|---|---|
| 拆分期间与并行开发冲突 | 每阶段单文件、独立提交、当天合入;先拆低频改动文件(settings/usage) |
| 改 `cost_micro` 类型触发 SQLite 重建表 | 采用 D1-A,新增列而非改类型;迁移仅 `ALTER ADD` + 索引 |
| 新口径导致历史数字"看起来变了" | 旧记录不改写;只在明细标注"旧口径";提供"按当前定价试算"对比视图 |
| 厂商不返回 cached_tokens | 记 NULL、按 miss 价计,快照标 `cache_unknown`,不猜测 |
| 拆分后循环依赖 | 依赖方向固定:`db → provider → commands → pages`;同层拆分只允许单向 re-export |

---

**审阅请求**:请就 §10.5 的 D1–D4 给出选择(或直接批准推荐项)。批准后按"线一(计量修复)→ 阶段 A → 阶段 B…"推进,每阶段交付后附 `cargo test` 与 `tsc -b` 结果。
