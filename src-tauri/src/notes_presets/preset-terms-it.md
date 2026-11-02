# IT / 编程 / 技术 常用翻译术语表

> 整理自微软语言门户（Microsoft Language Portal）、Git 官方中文本地化（git-for-windows `zh_CN.po`）、GNOME/KDE 中文本地化、MDN 中文本地化与 freeCodeCamp 中文翻译规范等权威来源，共 83 组高频中英对照。
> 用法：翻译技术文档时可对 AI 说「参照本表统一术语」；也可直接增删改，沉淀为本项目的专用术语表。

## 一、基础概念

| 英文 | 中文 | 备注 |
| --- | --- | --- |
| function | 函数 | |
| variable | 变量 | |
| constant | 常量 | |
| array | 数组 | |
| string | 字符串 | |
| integer | 整数 | |
| boolean | 布尔值 | |
| loop | 循环 | |
| recursion | 递归 | |
| parameter | 参数 | 实参=argument |
| exception | 异常 | |
| interface | 接口 | |
| library | 库 | |
| framework | 框架 | |
| dependency | 依赖 | |
| runtime | 运行时 | |
| middleware | 中间件 | |
| thread | 线程 | |
| process | 进程 | |
| asynchronous | 异步 | |
| synchronous | 同步 | |

## 二、开发协作

| 英文 | 中文 | 备注 |
| --- | --- | --- |
| compile | 编译 | |
| debug | 调试 | |
| deploy | 部署 | |
| refactor | 重构 | |
| version control | 版本控制 | |
| repository | 仓库 | |
| branch | 分支 | |
| merge | 合并 | |
| commit | 提交 | |
| pull request | 拉取请求 | GitHub 通用译法 |
| open source | 开源 | |
| license | 许可证 | 软件语境不译「执照」 |
| API | 应用程序接口 | 通常保留 API |
| SDK | 软件开发工具包 | 通常保留 SDK |

## 三、系统与网络

| 英文 | 中文 | 备注 |
| --- | --- | --- |
| frontend | 前端 | |
| backend | 后端 | |
| database | 数据库 | |
| query | 查询 | |
| index | 索引 | |
| transaction | 事务 | |
| cache | 缓存 | |
| container | 容器 | |
| virtual machine | 虚拟机 | |
| operating system | 操作系统 | |
| kernel | 内核 | |
| memory | 内存 | |
| bandwidth | 带宽 | |
| latency | 延迟 | |
| throughput | 吞吐量 | |
| scalability | 可扩展性 | |

## 四、安全与身份

| 英文 | 中文 | 备注 |
| --- | --- | --- |
| authentication | 身份验证 | 验「你是谁」 |
| authorization | 授权 | 定「你能做什么」 |
| encryption | 加密 | |
| decryption | 解密 | |
| token | 令牌 | |
| session | 会话 | |
| cookie | Cookie | 业界习惯不译 |
| credential | 凭据 | |

## 五、易译错高频词（增补）

> 以下来自 MDN 中文本地化、freeCodeCamp 中文翻译规范与微软语言门户，均为最易译错、需要统一的词。

| 英文 | 中文 | 备注 |
| --- | --- | --- |
| idempotent | 幂等 | 名词 idempotency=幂等性 |
| deprecate | 弃用 | 不译「贬低」「过时」 |
| stale | 过期的 | 数据/缓存语境 |
| side effect | 副作用 | 函数式语境 |
| lazy evaluation | 惰性求值 | 反义 eager evaluation=急切求值 |
| memoization | 记忆化 | 缓存计算结果 |
| currying | 柯里化 | |
| hydration | 水合 | SSR 语境，不译「注水」 |
| eventual consistency | 最终一致性 | |
| backpressure | 背压 | 流控语境 |
| circuit breaker | 熔断器 | 微服务语境 |
| graceful degradation | 优雅降级 | |
| fail-fast | 快速失败 | |
| snapshot | 快照 | |
| checksum | 校验和 | |
| quorum | 法定人数 | 分布式共识 |
| lease | 租约 | 分布式锁/缓存 |
| tombstone | 墓碑标记 | 逻辑删除 |
| canary release | 金丝雀发布 | |
| blue-green deployment | 蓝绿部署 | |
| sharding | 分片 | 区别于分区 partition |
| fan-out / fan-in | 扇出 / 扇入 | |
| head-of-line blocking | 队头阻塞 | HTTP/2 语境 |
| backoff | 退避 | exponential backoff=指数退避 |
| sentinel value | 哨兵值 | |
| off-by-one error | 差一错误 | 缩写 OBOE |
| reify | 具体化 | 语言理论语境 |
| mutation | 变更 | 不译「突变」 |
