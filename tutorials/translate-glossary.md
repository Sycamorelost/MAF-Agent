---
id: translate-glossary
page: translate
title: 术语表与格式保留
summary: 让专有名词译法统一，Markdown 结构不丢失
order: 1
---

术语表用于锁定专有名词译法，配置后全局自动生效。

1. 在翻译页添加「源词 → 目标词」条目，例如 `pull request → 拉取请求`。
2. 翻译时系统会把术语注入提示词，模型优先采用指定译法。
3. 翻译历史自动落盘 `data/translations/`，可随时复制或删除。

术语表文件（可直接编辑）：

```
项目根目录/data/glossary.json
```

Markdown 标记与代码块会在翻译中原样保留，只翻译正文文本。
