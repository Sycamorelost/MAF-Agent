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
            版本 {appVersion ? `${appVersion}` : '2.4.1'} · 桌面端 · Windows
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

      {/* 版本更新摘要:按主题归并,精简表述 */}
      <div className="set-card">
        <div className="set-card-h">更新内容 · v1.17.1 → v2.4.1</div>
        <div className="set-card-b">
          <p>
            <b>工作台与界面：</b>全新专业工作台——会话内嵌检查器(产物 / 变更回滚 / 上下文记忆 /
            工具流水四页签,宽度可拖拽)、目标级任务计划、会话按项目分组、四档运行时预设;
            消息布局焕新(用户气泡 + 助手通栏、连续工具调用折叠)、窄窗抽屉化、首启进度提示、
            渲染异常兜底。
          </p>
          <p>
            <b>学习中心：</b>迁移为纯 JSON 文件存储(可查看/迁移,旧数据自动并入、幂等可重试);
            试题库支持手动加题、勾选组卷、整卷批改、答错补作重批。
          </p>
          <p>
            <b>安全加固：</b>高危命令(格式化/删根/改注册表等)一律人工确认,任何审批档位不静默执行;
            外部 MCP 工具统一按中等风险走确认;API Key 目录权限收紧为仅当前用户可读;
            数据库升级前自动留存快照(保留最近 2 份),失败可手工回滚。
          </p>
          <p>
            <b>知识库与翻译：</b>内置 11 条预置笔记(四类中英对照术语表 + 七篇速查),
            「导入预置」一键找回且合并不覆盖;翻译内置 8 类专业术语表(350 词条),
            一键导入合并不覆盖、被删词条自动补回。
          </p>
          <p>
            <b>稳定性与体验：</b>修复设置页空白、输入框滞留、MCP 工具链死锁、新建会话白屏、
            工作区路径污染写文件失败等;全应用删除操作统一走应用内确认框(确认前不执行),
            旁路写入/事件失败记入本机日志并计数提示。
          </p>
          <p>
            <b>智能体系统重构(v2.4.0)：</b>内置智能体提示词可编辑(自定义覆盖,删除即恢复内置);
            设置页新增「角色设置」编辑自定义 base prompt;智能体支持解耦存储
            (目录多文件 config.json / prompt.md / tools.json,可关联外部 prompt/tools 文件);
            智能体导入导出走原生「另存为」对话框,可选保存路径;
            聊天头预设选择器移除,全局默认预设集中到设置页管理。
          </p>
          <p>
            <b>交互与样式统一(v2.4.1)：</b>会话任务清单全新悬浮抽屉(紧凑条 4/5 输入框宽居中,
            展开向上浮出带双层阴影,支持实时刷新 + 手动刷新按钮 + 悬停详情 tooltip,
            收起反向动画严格对称);知识库侧栏接入标题栏「面板」按钮开合(与设置/绘图/翻译页一致),
            侧栏列表改为行式 `.set-nav` 同款风格(左侧 2px 品牌色选中指示线);
            全局三点导航下拉修复硬编码 `#000000` 颜色(深色主题可见);
            统一侧栏 `bg-side` 背景 + `border` 分隔线 + 选中态 `bg-active` 指示线规范。
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
