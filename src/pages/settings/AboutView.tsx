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
            版本 {appVersion ? `${appVersion} Beta` : '2.3.1 Beta'} · 桌面端 · Windows
          </div>
          <div className="set-product-tagline">
            MAF 团队旗下的 Agent 产品,一款专为学习、调研而创造的智能体框架软件。
            完全免费且无广告,开发者:梧桐吾桐及其团队。
          </div>
          <div className="set-product-ver">
            MAF 旗下软件交流 QQ 群:1729082
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
        <div className="set-card-h">更新内容 · v1.17.1 → v2.3.1 Beta</div>
        <div className="set-card-b">
          <p>
            工作台与界面（v1.17.1 → v2.2.1）：全新专业工作台形态——会话内嵌检查器（产物 /
            变更与回滚 / 上下文与记忆 / 工具流水四页签，宽度可拖拽）、目标级任务计划面板、
            会话按项目文件夹分组管理、四档运行时预设（经济 / 标准 / 深度 / 编程）；消息布局焕新
            （用户气泡 + 助手通栏平铺、连续工具调用自动折叠）、窄窗自动抽屉化、首启初始化进度提示、
            渲染异常错误边界兜底。
          </p>
          <p>
            学习中心（v2.1.0）：全面改版并迁移为纯 JSON 文件存储（learningcenter/ 目录可直接查看
            与迁移，旧版数据自动并入、幂等可重试）；试题库支持手动加题与筛选，勾选组卷整卷自测、
            交卷统一批改、答错可补作重批。
          </p>
          <p>
            安全加固（v2.2.0）：高危命令（格式化磁盘、删除根目录、改写注册表等）一律人工确认，
            任何审批档位下不再静默执行；外部 MCP 工具仅凭工具名像「只读」不再自动放行，统一按
            中等风险走确认流程；API Key 存储目录权限收紧为仅当前系统用户可读；数据库版本升级前
            自动留存快照（保留最近 2 份），升级失败可手工回滚。
          </p>
          <p>
            稳定性修复（v1.17.1 → v2.2.1）：设置页空白与会话任务弹窗报错（14 条接口漏注册，
            已全部补齐并验证）；发送后输入框滞留、MCP 工具链持锁等待导致的会话静默卡死
            （改为不持锁 + 工具列表 10s / 调用 120s 超时）；新建会话偶发白屏、工作区路径前缀
            污染导致的写文件失败、搜索引擎在部分配置下失效；导出 PDF 链接增加协议白名单校验；
            旁路写入/事件失败记入本机日志并计数提示。
          </p>
          <p>
            知识库（v2.3.0 + v2.3.1）：内置 11 条预置笔记——IT / 学术 / 法律金融 / 办公营销
            四类中英对照术语表（参照微软语言门户、Academic Phrasebank、香港律政司法律词汇、
            UNTERM 等权威来源），及 Markdown 表格、CSV/TSV、数据清洗、Excel 公式、正则表达式、
            中文排版、图表选型七篇速查；首启自动落盘为普通笔记，可自由编辑删除；「导入预置」
            一键找回被删内容、按章节与表格首列合并且你的内容一字不动；标签独立为胶囊样式、
            摘要不再混入 Markdown 符号。
          </p>
          <p>
            翻译术语表（v2.3.1）：内置 8 类专业术语表（IT / AI / 学术 / 法律 / 金融 / 医学 /
            商务 / 通用，共 350 词条、中英双向展开 699 条），「一键导入预置术语」合并不覆盖
            你的译法、可反复点击，删掉的预置词条自动补回；命中过多时按词长优先注入，
            大词表支持搜索与分段渲染。
          </p>
          <p>
            删除确认（v2.3.1）：全应用删除操作统一走应用内确认框（20 处系统原生确认框全部替换），
            风格与软件一致，且确认前不再先执行删除；另补齐 3 处原本没有确认提示的删除入口。
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
