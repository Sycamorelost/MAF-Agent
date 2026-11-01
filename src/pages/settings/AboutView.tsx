//! 设置 · 关于视图(从 SettingsPage 搬出,纯搬家:视觉与行为不变)。

import { useEffect, useState } from 'react';

import logoUrl from '../../assets/logo.png';
import { getFailureCounts } from '../../lib/ipc/diagnostics';
import { CN_NUM, TERMS } from './constants';

export interface AboutViewProps {
  appVersion: string;
}

export function AboutView({ appVersion }: AboutViewProps) {
  // P2:旁路操作失败计数(非 0 才提示,不给小白新增任何开关)
  const [failures, setFailures] = useState(0);
  useEffect(() => {
    getFailureCounts()
      .then((c) => setFailures(c.emit + c.event_log + c.audit))
      .catch(() => setFailures(0));
  }, []);

  return (
    <>
      <h2>关于</h2>

      {/* 产品标识区:logo 原比例 + 名称 + 版本(读 tauri.conf.json)+ 定位语 */}
      <div className="set-card set-product">
        <img className="set-product-logo" src={logoUrl} alt="myAgent" />
        <div className="set-product-info">
          <div className="set-product-name">myAgent</div>
          <div className="set-product-ver">
            版本 {appVersion ? `${appVersion} Beta` : '2.2.1 Beta'} · 桌面端 · Windows
          </div>
          <div className="set-product-tagline">
            MAF 团队旗下的 Agent 产品,一款专为学习、调研而创造的智能体框架软件。
            完全免费且无广告,开发者:梧桐吾桐及其团队。
          </div>
          <div className="set-product-ver">
            凭据存储:API Key 与 MCP 请求头以 DPAPI(Windows)/ AES-GCM 密文保存在本地数据目录的
            secrets 下,目录权限已收紧为仅当前系统用户可读。
          </div>
          {failures > 0 && (
            <div className="set-product-ver">
              运行自检:累计 {failures} 次旁路写入或事件发送失败(已记入本机日志,不影响正常使用)。
            </div>
          )}
        </div>
      </div>

      {/* 版本更新摘要:自 v1.17.1 至今的用户可感知改动,按类归组 */}
      <div className="set-card">
        <div className="set-card-h">更新内容 · v1.17.1 → v2.2.1 Beta</div>
        <div className="set-card-b">
          <p>
            全新：专业工作台形态——会话内嵌检查器（产物 / 变更与回滚 / 上下文与记忆 / 工具流水
            四页签，宽度可拖拽）、目标级任务计划面板（步骤进度实时同步，模型每完成一步即回写）、
            会话可按项目文件夹分组管理、四档运行时预设（经济 / 标准 / 深度 / 编程）随会话切换。
          </p>
          <p>
            界面：消息布局焕新——用户消息保持气泡、助手回复无头像通栏平铺；连续工具调用
            （2 个及以上）自动折叠为一行汇总，失败调用默认收起；输入框与正文同轴对齐并左右外扩，
            冗余的分隔线、提示栏、占位文案与状态徽标全面精简；侧边栏会话按项目卡片化分组，
            箭头方向符合折叠语义；全局背景改为纯白，区域靠边框分隔。
          </p>
          <p>
            布局：会话列表栏与检查器均支持隐藏与拖拽调宽，开合按钮统一收在会话标题栏，
            状态随重启保持；窄窗自动转为抽屉/浮层，不挤压正文。
          </p>
          <p>
            体验：首次安装启动新增「正在初始化组件…」进度提示，组件下载期间不再表现为卡死；
            渲染异常时由错误边界兜底显示提示而非整页白屏。
          </p>
          <p>
            修复：设置页「联网搜索」「运行时预设」空白与会话任务弹窗报错（14 条接口
            在重构中漏注册，已全部补齐并验证）；新建会话偶发白屏；工作区路径前缀污染
            导致的工具写文件失败；搜索引擎在部分配置下失效。
          </p>
          <p>
            学习中心（v2.1.0）：全面改版并迁移为纯 JSON 文件存储——数据统一收在
            learningcenter/ 目录，计划、题库、试卷、模板各占子文件夹，可直接查看与迁移；
            旧版 data/study 目录与旧数据库三表在升级后自动并入，迁移幂等、失败自动重试。
            试题库支持手动加题、编辑、筛选与搜索；勾选题目即可组卷整卷自测，交卷统一批改，
            答错可补作重批；界面按全局设计约束整体重构。
          </p>
          <p>
            图表与组件（v2.1.0）：Mermaid 图双向自适应窗口——小图等比放大充满
            （矢量不失真），超宽图保持原生尺寸、横向滚动不再压扁；
            分段切换按钮内容默认居中。
          </p>
          <p>
            稳定性与修复（v2.1.1）：发送消息后输入框文字与附件不再滞留——点击发送即清空，
            失败时自动回填，不再出现「已显示正在思考但文字还卡在输入框」；修复工具轮次结束后
            会话静默卡死——MCP 工具层原先在持锁状态下等待 server 应答且没有超时，任一 server
            无响应就会拖死整条工具链，现改为只持锁取句柄、工具列表 10s / 工具调用 120s 超时、
            启动过程不持锁、按工具名路由并行查询；上下文压缩子调用收紧到 60s 并超时回落硬裁剪；
            后台流任务异常中断时补发终态，界面不再无限等待。
          </p>
          <p>
            界面与交互（v2.1.1）：联网搜索来源默认折叠、字号下调，作为辅助信息不再占据主体版面；
            工作台项目文件夹删除改为应用内确认框，确认后才真正执行（修复原生确认框在弹窗前
            即已删除的问题）。
          </p>
          <p>
            安全加固（v2.2.0）：高危命令（格式化磁盘、删除根目录、改写注册表等）一律弹出人工确认，
            任何审批档位下都不再静默执行；外部 MCP 工具的风险分级收紧——仅凭工具名像「只读」
            不再自动放行，统一按中等风险走确认流程，你仍可用「始终允许」对可信工具免打扰。
          </p>
          <p>
            数据与凭据（v2.2.0）：API Key 存储目录权限收紧为仅当前系统用户可读；数据库版本升级前
            自动留存一份快照（保留最近 2 份），升级失败可手工回滚；工具调用记录按会话滚动保留
            最近 500 条，长期使用不再无限增长。
          </p>
          <p>
            稳健性（v2.2.0）：导出为 PDF 时，正文中的链接增加协议白名单与转义校验，恶意链接会
            被降级为纯文本；旁路写入或事件发送失败不再完全静默——记入本机日志并累计计数，
            本页在计数非 0 时显示一行提示；工程层面对网页抓取、内置工具与 HTML 处理做了拆分与
            合并去重，功能行为不变。
          </p>
          <p>
            修复（v2.2.1）：会话头部切换运行时档位的下拉菜单在侧栏折叠时会从窗口左缘溢出、
            内容被裁掉——菜单现在按实际位置自动选择对齐方向，靠近左缘时改为向右展开，
            任何窗口宽度与侧栏状态下都能完整显示。
          </p>
        </div>
      </div>

      {/*
        开源引用与许可声明。
        来源项目与许可(全文见仓库根目录 LICENSE / NOTICE):
        - myAgent 本体 · Apache-2.0 · https://github.com/Sycamorelost/MAF-Agent
        - Deep-tutor · Apache-2.0 · 学习规划流水线与交互重写
        - Cherry Studio · AGPL-3.0 · 仅架构/交互理念参考,未复制源码
      */}
      <div className="set-card">
        <div className="set-card-h">开源引用与许可</div>
        <div className="set-card-b set-terms">
          <section className="set-term">
            <h3 className="set-term-h">myAgent 本项目 · Apache License 2.0</h3>
            <div className="set-term-body">
              <p>
                源码获取:
                <span className="mono">https://github.com/Sycamorelost/MAF-Agent</span>
              </p>
              <p>版权所有:Copyright (c) 2026 Sycamorelost / myAgent contributors。</p>
              <p>
                许可说明:本应用以 Apache License 2.0 开源发布,允许使用、修改与再分发(含商业用途);
                分发时须保留仓库根目录 LICENSE 与 NOTICE 中的版权与许可声明,并标明修改内容。
              </p>
            </div>
          </section>

          <section className="set-term">
            <h3 className="set-term-h">Deep-tutor · Apache License 2.0</h3>
            <div className="set-term-body">
              <p>
                项目地址:<span className="mono">https://github.com/hershate/Deep-tutor</span>
              </p>
              <p>版权所有:Copyright (c) hershate and contributors。</p>
              <p>
                引用范围:学习规划板块的生成流水线、提示词契约、自学材料/测验/掌握度/复习逻辑
                (<span className="mono">src-tauri/src/study/</span>)与页面交互
                (<span className="mono">src/pages/study/</span>)。
              </p>
              <p>
                修改说明:由 FastAPI/React 重写为 Rust/React,排期与结构校验改为确定性实现,
                数据模型与界面组件体系均按本应用重构;未复制上游视觉资源或样式。
              </p>
            </div>
          </section>

          <section className="set-term">
            <h3 className="set-term-h">Cherry Studio · AGPL-3.0(架构参考)</h3>
            <div className="set-term-body">
              <p>
                项目地址:<span className="mono">https://github.com/CherryHQ/cherry-studio</span>
              </p>
              <p>版权所有:Copyright (c) CherryHQ and contributors。</p>
              <p>
                引用范围:仅架构与交互理念参考(多服务商抽象、API Key 管理与桌面助手布局);
                未复制源码、样式或资源,也未打包其依赖。
              </p>
              <p>
                许可说明:该项目以 GNU Affero General Public License v3.0 发布;
                本应用与其不存在源码级衍生关系。如后续引入其源代码,合并作品须按 AGPL-3.0 分发。
              </p>
            </div>
          </section>

          <p className="set-term-foot">
            本应用自身以 Apache License 2.0 开源发布,源码见
            <span className="mono">https://github.com/Sycamorelost/MAF-Agent</span>,
            许可证全文与第三方组件声明见仓库根目录 LICENSE / NOTICE 文件。
            以上声明仅用于致谢与合规披露,不构成对原作者的任何担保。
          </p>
        </div>
      </div>

      {/* 条款文档区:十四条,全部展开 */}
      <div className="set-card">
        <div className="set-card-h">
          myAgent 使用条款
          <span className="st date">最近更新日期:2026 年 10 月 31 日</span>
        </div>
        <div className="set-card-b set-terms">
          {TERMS.map((t) => (
            <section key={t.no} className="set-term">
              <h3 className="set-term-h">第{CN_NUM[t.no]}条 · {t.name}</h3>
              {t.items.map((it, i) => {
                // 「1.1」这类编号独立成列,正文统一悬挂缩进,便于逐条对照
                const m = /^(\d+\.\d+)\s+([\s\S]*)$/.exec(it.text);
                return (
                  <div key={i} className="set-term-item">
                    <span className="set-term-no">{m?.[1] ?? ''}</span>
                    <div className="set-term-body">
                      <p>{m?.[2] ?? it.text}</p>
                      {it.bullets && (
                        <ul>
                          {it.bullets.map((b, j) => (
                            <li key={j}>{b}</li>
                          ))}
                        </ul>
                      )}
                    </div>
                  </div>
                );
              })}
            </section>
          ))}
          <p className="set-term-foot">
            本条款为软件使用协议文本。开发者并非专业法律服务机构人员,如您对条款有疑问,建议在使用本软件前自行咨询专业人士。
          </p>
        </div>
      </div>
    </>
  );
}
