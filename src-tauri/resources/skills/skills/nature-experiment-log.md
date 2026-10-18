# nature-experiment-log（本地版）

> **本地化改造说明**：本版删除飞书（lark-cli）群消息/附件拉取这一可选输入路径，不连接任何第三方软件；素材只来自当前会话上传或用户指定的本地文件/文件夹。产物只写入用户指定的**本地目录**（普通文件夹即可；若该文件夹恰好是 Obsidian vault，附带模板天然兼容，但不要求安装 Obsidian）。

## 一、定位与触发场景

**用途（本地版）**：将实验原始材料（图片、语音、文字——来自会话上传或本地路径）标准化为带 YAML frontmatter 的 Markdown 实验日志，输出到普通本地文件夹；附带的索引/异常/设备模板与 Obsidian Dataview 兼容，但 Obsidian 不是必需依赖。

**原 description 摘录**（SKILL.md frontmatter，保留以记录原貌）

> "标准化实验日志记录——直接上传或读取本地图片、语音和文字，产出带 YAML frontmatter 的 Markdown；可选集成飞书 CLI 与 Obsidian。"

本地版执行口径：**只保留前半句**——"直接上传或读取本地图片、语音和文字，产出带 YAML frontmatter 的 Markdown"；飞书 CLI 集成停用；Obsidian 降级为"输出目录若恰好是 vault 则模板可用"的兼容性说明。

`metadata.hermes.tags: [research, experiment, logging, feishu, obsidian, automation]` 为原始元数据，其中 feishu 标签在本地版不适用。

**典型触发请求（本地版）**
- "记录一个实验：316L 在 500°C 氯盐腐蚀 300 h，Ar 气氛，失重 0.0032 g。"
- "把 D:\实验\2026-10-17 这几张实验照片整理成今天的实验日志，输出到 D:\LabNotebook。"
- "根据这段语音转写补一条标准实验记录，并列出缺失信息。"

**明确不做什么（边界）**
- 不会替作者编造温度、时间、配比、设备型号或实验结果。
- 对无法从材料中确认的信息保留 `AUTHOR_INPUT_NEEDED` 或中文确认项。
- **不连接飞书或任何第三方软件**：不安装/调用 lark-cli，不读取群消息，不配置 bot 权限；素材只来自会话上传或本地路径。
- 不要求安装 Obsidian；输出目标就是普通本地文件夹。

## 二、运作模式（加载/路由机制）

### SKILL.md 作为路由

SKILL.md 本身就是完整工作流描述（155 行），非薄壳。manifest 的 `always_load` 只有 `SKILL.md` 本身；**无 axes**、无 static/ 层。所有模板与示例均按条件 `on_demand` 加载。

### manifest.yaml 结构

- `always_load`：`SKILL.md`
- **无 axes**
- `templates.on_demand`：
  - 创建或刷新实验索引仪表盘 → `templates/experiment-index.md`
  - 记录异常、失败运行、污染、意外设备行为或偏离协议 → `templates/anomaly-log.md`
  - 建立共享设备、试剂或耗材追踪 → `templates/equipment-tracking.md`
- `references.on_demand`：
  - 需要完整腐蚀/浸泡/一般材料实验日志示例 → `references/example-log.md`
  - 电化学表征（CV/SWV/EIS/电极设置/电化学窗口）示例 → `references/example-electrochemical.md`
  - 热稳定性/加热/马弗炉/管式炉/温度程序实验示例 → `references/example-thermal-stability.md`

### 对 nature-shared 的依赖

无直接引用。manifest 未声明 nature-shared。原始 `metadata.hermes.related_skills` 提及的 `feishu-cli-integration` 在本地版停用；`nature-literature-pipeline`、`obsidian` 仅作生态备注（本地版输出普通 Markdown，不依赖它们）。

## 三、核心工作流（按执行顺序）

### Step 0 接收材料（仅两种本地渠道）

用户通过以下任一方式提交：
- **直接上传** — 当前会话提交图片、音频、语音转录或文字。
- **本地材料** — 提供本地文件或文件夹路径，由 agent 读取并整理。

> 本地版**没有第三个渠道**：不调用 lark-cli、不读取任何群聊消息或云端附件；需要归档的原始材料一律先由用户保存到本机路径或直接在会话中上传。

### Step 1 提取结构化信息

通过 `vision_analyze` 和文本解析提取实验要素。对缺失或模糊字段向用户确认，不猜测实验条件或结果。

### Step 2 确认输出方式与目录

- 输出方式：**本地普通文件夹中的 Markdown**（仅此一种）。若用户指定的目录恰好是 Obsidian vault，生成文件天然可被其索引，但这只是普通文件写入，不需要 Obsidian 参与。
- 未指定目录时，先返回可保存的 Markdown，不擅自选择路径。
- 确认目标目录后，生成实验 ID 与样品批次 ID。

### Step 3 生成实验 ID 与样品批次 ID

**实验 ID 规则**：
```
{体系代码}-{设备代码}-YYMMDD-{序号}
  │        │       │       └─ 当日序号（001 起）
  │        │       └─ 日期
  │        └─ 设备代码（M=马弗炉, T=管式炉, E=电化学, G=手套箱, F=可控气氛炉, B=通用）
  └─ 体系代码（自定义，如 CL / NO / OX / HY 等）
```

**样品批次 ID 规则**：
```
{体系代码}-{候选编号}-B{序号}
  │        │         └─ 配盐批次序号
  │        └─ 候选配方编号
  └─ 体系代码
```
同一批样品跨多个实验时 `sample_batch` 保持一致，便于 dataview 追踪。

**设备代码速查**：
| 代码 | 设备 | 场景 |
|------|------|------|
| M | 马弗炉 | 热处理、浸泡腐蚀 |
| T | 管式炉 | 气氛控制、脱水、热稳定性 |
| E | 电化学工作站 | CV/SWV/EIS |
| G | 手套箱 | 配盐、称量、取样 |
| F | 可控气氛炉 | 精密气氛控制 |
| B | 通用 | 干燥、清洗、制样 |

### Step 4 写出标准化日志

文件路径：`{OUTPUT_ROOT}/实验日志/{体系}/{类型}/{exp_id}.md`

日志包含完整 YAML frontmatter + Markdown 正文。示例 frontmatter 字段（来自 `references/example-log.md`、`example-electrochemical.md`、`example-thermal-stability.md`）：
- `exp_id`、`date`、`salt_system`、`salt_batch`、`salt_composition`
- `exp_type`、`furnace`、`crucible`、`total_salt_mass_g`
- `temperature_profile`、`atmosphere`、`material`、`material_category`
- `sample_id`、`sample_dimensions_mm`、`pre_mass_g`、`post_mass_g`、`mass_loss_g`
- `corrosion_rate_um_year`、`anomaly`、`anomaly_ref`、`tags`
- 电化学特有：`test_method`、`working_electrode`、`counter_electrode`、`reference_electrode`、`potential_window_V`、`scan_rate_mV_s`、`evaluation_target`
- 热稳定性特有：`condensate`、`absorption_solution`

Markdown 正文标准章节：
1. `# 实验目的`
2. `# 实验步骤`
3. `# 观察`
4. `# 结果`
5. `# 异常`
6. `# 下一步`
7. `# 原始材料`（归档路径与附件索引）

### Step 5 归档原始附件

原始附件存入：
```
{OUTPUT_ROOT}/raw/experiments/YYYY.MM.DD_描述_EXPID/
├── 笔记.md
├── 图片/
└── 语音/
```
并在日志 `# 原始材料` 节建立引用链接。

### Step 6 更新索引与异常记录（可选，纯本地文件）

在输出目录中维护以下本地 Markdown 索引（模板为 Dataview 查询语法，安装 Obsidian 时可直接用；不安装时就是普通 Markdown 文件，人工阅读同样有效）：
- `实验日志/实验索引.md` — 索引/仪表盘模板
- `实验日志/异常记录.md` — 异常记录
- `实验日志/公共/设备与试剂追踪.md` — 设备与试剂追踪

发现异常时追加异常记录。模板字段：`date`、`related_exp`、`anomaly_type`（设备故障/操作失误/结果异常/安全事件）、`severity`（低/中/高）、`status`（待处理/已处理/需复查）。

### Step 7 告知用户

告知生成文件及原始材料的具体位置。

## 四、输出契约与质量检查

- **必产出**：一条带 YAML frontmatter 的实验日志、原始附件归档目录、缺失字段清单、后续实验动作建议。
- **命名规范**：日志文件名 `{exp_id}.md`；原始附件目录 `YYYY.MM.DD_描述_EXPID/`。
- **目录结构**（{OUTPUT_ROOT} 为用户指定的本地文件夹）：
  ```
  {OUTPUT_ROOT}/
  ├── raw/experiments/                       ← 原始层
  │   └── YYYY.MM.DD_描述_EXPID/
  │       ├── 笔记.md
  │       ├── 图片/
  │       └── 语音/
  └── 实验日志/                               ← 标准层
      ├── 实验索引.md
      ├── 异常记录.md
      ├── {体系A}/
      │   ├── 实验类型1/
      │   ├── 实验类型2/
      │   └── ...
      └── 公共/
          └── 设备与试剂追踪.md
  ```
- **模糊信息处理**：温度记不清、样品编号不明等主动询问，不猜测写入。
- **字段完整性**：模板是建议结构，可增删字段；但 `exp_id`、`date` 是核心必填。

## 五、关键规则与红线

- **绝不编造**实验条件、结果、温度、时间、配比、设备型号。
- **不猜测**写入：无法确认的信息保留 `AUTHOR_INPUT_NEEDED` 或中文确认项。
- **零外部连接**：不安装/调用 lark-cli 或任何第三方软件 CLI；不读取群消息/云附件；素材仅来自会话上传与本地路径。
- 未指定目录时，只返回可保存的 Markdown，不擅自选择路径。
- 不要求安装 Obsidian；输出即普通本地 Markdown 文件，Dataview 模板只是附带的兼容福利。

## 六、配套资源

**references/**（一句话主题）：
- `example-log.md` — 材料腐蚀浸泡实验完整日志示例（含 YAML frontmatter 与全部标准章节）
- `example-electrochemical.md` — 电化学表征（CV 窗口测试）完整日志示例
- `example-thermal-stability.md` — 热稳定性实验完整日志示例

**templates/**（一句话主题）：
- `experiment-index.md` — Dataview 查询仪表盘模板（`TABLE date, salt_system, exp_type, material, anomaly FROM "实验日志" WHERE exp_id SORT date DESC`）
- `anomaly-log.md` — 异常记录模板（YAML 字段：date / related_exp / anomaly_type / severity / status + 描述/影响/处理/备注四节）
- `equipment-tracking.md` — 设备与试剂追踪模板（设备/坩埚/试剂三张表 + 使用/校准/库存更新指引）

**scripts/**：无。

**agents/openai.yaml**：
```yaml
interface:
  display_name: "Nature Experiment Log"
  short_description: "Turn experiment notes and media into structured logs"
  default_prompt: "Use $nature-experiment-log to turn these notes and media into a structured experiment log."
```

**evals**：无显式 evals 文件；README.md 提供 3 条典型请求作为代表性 prompt 主题。
