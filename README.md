# MAF Agent

> 本地优先的 AI 桌面工作台 —— 把研究、学习与创作的过程，留在自己手里。

MAF Agent(MetaAgent Float)是一款开源的 Windows 桌面 AI 工作站，基于 **Tauri 2 + Rust + React 19** 构建。与云端 AI 助手不同，MAF 不托管任何模型与数据：你自备 API Key，数据全部存储在本机(SQLite + 纯文本文件)，只与你的模型服务商直连。

- **版本**:v1.15.0
- **许可证**:Apache-2.0(附带 [NOTICE](NOTICE) 第三方衍生声明)
- **平台**:Windows 10 / 11(64 位)

---

## 目录

- [功能特性](#功能特性)
- [技术架构](#技术架构)
- [隐私与安全](#隐私与安全)
- [系统要求](#系统要求)
- [快速开始](#快速开始)
- [配置模型服务商](#配置模型服务商)
- [项目结构](#项目结构)
- [数据存储](#数据存储)
- [快捷键](#快捷键)
- [第三方致谢](#第三方致谢)
- [许可证](#许可证)

---

## 功能特性

### AI 对话

- 流式输出，推理过程(Reasoning)折叠展示
- 多会话管理，消息级会话分支树(Branch)
- 单会话内中途切换模型
- Markdown / KaTeX 公式 / Mermaid 图表 / ECharts 图表四类渲染
- 附件本地解析：PDF(pdf-extract)、DOCX(docx-rs)、XLSX(calamine),不经过云端

### 深度研究

- 多阶段研究管线，支持 staged / concise / decide / synthesis 四种模式
- 蓝图库(Blueprint)与研究模板
- **检查点(Checkpoint)暂停 / 恢复 / 断点续研**
- 搜索引擎 + 网页抓取 + 排序全自研(`search/engine.rs`、`fetch.rs`、`rank.rs`)
- 参考文献管理、图表嵌入、研究报告导出
- 内置 23 份中文重构的科研方法论技能文档(衍生自 nature-skills)

### 学习规划

- AI 生成结构化学习计划(周维度拆解、知识点列表)
- 测验(Quiz)、间隔重复复习、掌握度(Mastery)追踪
- 学习统计与专注计时器

### 持久记忆

- 文件级记忆：每个会话对应一个 `memory.md`(YAML frontmatter + 结构化 Markdown)
- 版本历史与行级 diff,可回滚
- 文件监听(Watcher):用外部编辑器修改记忆文件，应用内实时同步
- 跨会话记忆注入与草稿交接(draft handoff)

### 知识库(笔记)

- 内置 Markdown 笔记，支持标签、预览渲染
- 保存时自动校验，Ctrl+S 快捷保存

### 翻译

- 8 种语言互译，源语言自动检测
- 输入即译(防抖自动触发)
- 术语表(Glossary)支持
- 最近 100 条历史记录，译文可一键发送到对话

### AI 绘画

- 文生图，历史缩略图管理
- 本地缩略图生成(Rust 侧 PNG 解码 + JPEG 编码)

### 全局搜索

- `Ctrl+K` 唤起，本地全文检索会话消息、记忆、知识库
- 毫秒级响应(SQLite 本地索引)

### 划词提问

- 在任意应用中选中文本后**连按两次 Ctrl+C**,自动唤起主窗口、新建会话并填入选中文字

### 用量统计

- Token 级用量热力图、健康度与成本估算

### 多模型服务商

- 内置 DeepSeek、智谱 GLM 等服务商模板，支持任意 **OpenAI 兼容** 端点自定义接入
- 多 API Key 轮换(Rotation)与故障转移(Failover)

---

## 技术架构

| 层 | 技术 |
|---|---|
| 桌面框架 | Tauri 2.12(Rust + 系统 WebView2,不捆绑 Chromium) |
| 前端 | React 19 + TypeScript 5.8 + Vite 6 + Zustand 5 + React Router 7 |
| 后端 | Rust(edition 2024,rust 1.90+),147 个 IPC 命令 |
| 本地数据库 | SQLite(rusqlite,bundled,SCHEMA_VERSION 18) |
| 网络 | reqwest(rustls-tls)+ SSE 流式解析,Rust 层直推前端 |
| 附件解析 | pdf-extract / docx-rs / calamine(全部本地原生) |
| 密钥保护 | Windows DPAPI + AES-GCM 双路径加密(`provider/keychain.rs`) |
| 渲染 | react-markdown + KaTeX + Mermaid + ECharts + highlight.js |

**架构要点**

- 复用系统 WebView2 而非捆绑 Chromium:安装包约 **24 MB**,显著小于 Electron 系同类产品
- 附件解析、搜索排序、流式转发、密钥链等重逻辑全部在 Rust 原生层完成
- 严格 CSP 白名单:`default-src 'self'; object-src 'none'; base-uri 'none'`,无远程脚本
- 前端构建产物约 8.4 MB

---

## 隐私与安全

| 维度 | 说明 |
|---|---|
| 数据存储 | 全部本地：SQLite + Markdown/JSON 文件，数据目录由用户自选、可迁移 |
| 出网范围 | 仅连接用户自选的 LLM API 端点，无任何遥测/上报 |
| API Key | Windows DPAPI 加密存储，辅以 AES-GCM 密钥链，不落明文 |
| CSP | 严格白名单，禁止远程脚本与对象嵌入 |
| 开源 | Apache-2.0,全部代码可审计 |

---

## 系统要求

- Windows 10 / 11(64 位)
- WebView2 运行时(Windows 11 自带；Windows 10 通常已预装，否则安装时自动引导)
- 一个或多个模型服务商的 API Key(如 DeepSeek、智谱 GLM,或任意 OpenAI 兼容服务)

---

## 快速开始

### 环境准备

| 依赖 | 版本 |
|---|---|
| Node.js | 18+(推荐 LTS) |
| pnpm | 9+(`corepack enable` 后自带) |
| Rust | 1.90+(rustup 安装) |
| Tauri 系统依赖 | Windows 上仅需 WebView2 + MSVC 生成工具 |

### 开发模式

```bash
# 克隆仓库后进入项目目录
cd "MetaAgent Float"

# 安装前端依赖
pnpm install

# 启动开发模式(Vite 热更新 + Tauri 窗口)
pnpm tauri dev
```

### 构建发布

```bash
# 类型检查 + 前端构建 + Rust 编译 + 打包安装程序
pnpm tauri build
```

产物位于 `src-tauri/target/release/bundle/`(NSIS 安装包与便携 exe)。

### 运行测试

```bash
cd src-tauri
cargo test        # Rust 单元测试(236 项)
cargo check       # 静态检查
```

---

## 配置模型服务商

1. 启动应用，进入 **对话页** 顶部模型菜单，或打开 **设置 → 模型服务商**
2. 选择内置模板(DeepSeek / 智谱 GLM)或 **新增 OpenAI 兼容** 服务商
3. 填入 API Key —— Key 会立即经 Windows DPAPI 加密后写入本地密钥链
4. 拉取模型列表，勾选启用，即可开始对话

支持为同一服务商配置多个 Key 做轮换与故障转移。

---

## 项目结构

```
MetaAgent Float/
├── index.html                  # Vite 入口
├── package.json                # 前端依赖与脚本
├── vite.config.ts              # Vite 配置(端口 1420)
├── tutorials/                  # 应用内教程(聊天记忆/绘图/研究/翻译)
├── src/                        # 前端源码(React 19 + TS)
│   ├── components/             # 标题栏、导航、全局搜索等通用组件
│   ├── lib/                    # IPC 封装、工具函数
│   ├── pages/                  # chat / research / study / translate / draw / settings
│   ├── stores/                 # Zustand 状态
│   └── styles/                 # 样式
└── src-tauri/                  # 后端源码(Rust)
    ├── tauri.conf.json         # 窗口、CSP、打包配置
    ├── resources/skills/       # 23 份科研方法论技能文档
    └── src/
        ├── commands/           # 147 个 Tauri IPC 命令
        │   ├── provider/       # 模型接入/密钥链/流式/用量
        │   ├── research/       # 研究管线/导出/参考文献
        │   ├── memory/         # 记忆 CRUD/版本 diff
        │   └── ...
        ├── db/                 # SQLite 会话与分支存储
        ├── provider/           # 服务商模板/Key 轮换/DPAPI 密钥链
        ├── research/           # 研究引擎(管线/蓝图/技能注入/检索排序)
        ├── study/              # 学习计划/测验/掌握度
        ├── memory/             # memory.md 读写与监听
        ├── selection.rs        # 全局划词(Win32)
        └── settings.rs         # 应用设置
```

---

## 数据存储

默认数据目录位于应用数据文件夹(可在设置中迁移),布局如下：

```
data/
├── metaagent.db                # SQLite:会话、消息、分支、用量、索引
├── settings.json               # 应用设置
├── provider_templates.json     # 服务商模板
├── secrets/                    # DPAPI/AES-GCM 加密后的 API Key
├── conversations/<id>/
│   ├── memory.md               # 会话记忆(可直接编辑)
│   └── memory.versions/        # 记忆版本快照
├── notes/                      # 知识库 Markdown 笔记
├── research/<id>/              # 研究工程与 report.md
├── translations/               # 翻译历史
├── attachments/                # 会话附件
├── images/                     # AI 绘画产物
└── study/                      # 学习计划数据
```

**重要**:记忆与笔记均为纯文本 Markdown,你可以用任何编辑器直接修改，应用会通过文件监听实时同步。删除应用前如需保留数据，备份整个数据目录即可。

---

## 快捷键

| 快捷键 | 作用 |
|---|---|
| `Ctrl + K` | 全局搜索(会话消息 / 记忆 / 知识库) |
| `Ctrl + C` ×2(任意应用中) | 划词提问：唤起主窗口并以选中文字提问 |
| `Ctrl + S`(笔记页) | 保存知识库笔记 |

---

## 第三方致谢

本项目包含以下开源作品的衍生内容，均已遵循其许可证(详见 [NOTICE](NOTICE)):

- **[nature-skills](https://github.com/Yuan1z0825/nature-skills)**(Apache-2.0)—— 研究方法论技能文档，经中文翻译与六段式模板重构，用于深度研究模块
- **[Deep-tutor](https://github.com/hershate/Deep-tutor)**(Apache-2.0)—— 学习规划管线，由 FastAPI/React 移植为 Rust/React 并重新实现
- **[Cherry Studio](https://github.com/CherryHQ/cherry-studio)**(AGPL-3.0)—— 仅参考其服务商管理与桌面助手架构理念，未复制任何代码或资源

---

## 许可证

Copyright 2026 梧桐吾桐及其团队

本项目基于 [Apache License 2.0](LICENSE) 开源，衍生与引用关系见 [NOTICE](NOTICE)。
