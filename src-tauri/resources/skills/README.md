# 内置科研方法论资源（只读）

本目录为 MAF Agent 研究功能「科研增强」能力的内置方法论知识库，随安装包只读分发。

## 来源与许可（Attribution & License）

- 方法论源自开源项目 **nature-skills**：
  https://github.com/Yuan1z0825/nature-skills
- 著作权：Copyright (c) Yuan1z0825 and contributors。
- 许可：**Apache License 2.0**（上游仓库 LICENSE 文件；本应用整体亦以 Apache-2.0 发布，
  许可证全文见仓库根目录 `LICENSE`，第三方声明见根目录 `NOTICE`）。
- 本目录文档为中文本地化重构版（**非原仓库文件原样复制，属衍生作品**），著作权归原作者所有；
  修改点见根目录 `NOTICE`（翻译、六节结构重排、剔除第三方服务端点）。
- 已剔除原包中面向第三方服务的路由（OpenAlex/PubMed/CrossRef/arXiv/MCP 等端点与脚本引用）；
  实际检索一律使用用户在本应用内配置的搜索渠道，全程本地运行、零网络外联。

## 用途

- `00-overview.md`：技能集总体运作模式（五步路由协议）。
- `skills/*.md`：23 个科研技能的重构文档（六节模板：定位与触发 / 运作模式 /
  核心工作流 / 输出契约与质量检查 / 关键规则与红线 / 配套资源）。

研究管线在「查询构造 / 资料消化 / 报告撰写 / 引用规范 / 深度审计 / 图示规划」
各环节按场景注入轻量方法论片段（经适配层改写为研究报告口径），不整包灌入；
本目录文档作为知识底座随包分发，供核查与溯源，运行时不做用户编辑。

- PPTX 相关路由（nature-paper2ppt / nature-image2ppt）与图片型 PDF 产物
  （nature-paper-trans）明确不移植。
