//! 设置 · 关于视图(从 SettingsPage 搬出,纯搬家:视觉与行为不变)。

import logoUrl from '../../assets/logo.png';
import { CN_NUM, TERMS } from './constants';

export interface AboutViewProps {
  appVersion: string;
}

export function AboutView({ appVersion }: AboutViewProps) {
  return (
    <>
      <h2>关于</h2>

      {/* 产品标识区:logo 原比例 + 名称 + 版本(读 tauri.conf.json)+ 定位语 */}
      <div className="set-card set-product">
        <img className="set-product-logo" src={logoUrl} alt="myAgent" />
        <div className="set-product-info">
          <div className="set-product-name">myAgent</div>
          <div className="set-product-ver">
            版本 {appVersion || '2.0.1'} · 桌面端 · Windows
          </div>
          <div className="set-product-tagline">
            MAF 团队旗下的 Agent 产品,一款专为学习、调研而创造的智能体框架软件。
            完全免费且无广告,开发者:梧桐吾桐及其团队。
          </div>
        </div>
      </div>

      {/* 版本更新摘要:只列用户可感知的改动 */}
      <div className="set-card">
        <div className="set-card-h">本次更新 · v2.0.1</div>
        <div className="set-card-b">
          <p>
            修复：修复设置页「联网搜索」「运行时预设」内容空白、会话任务面板加载失败、
            搜索引擎调用失效等因模块重构导致的命令注册缺失问题，相关功能已全部恢复。
          </p>
          <p>
            修复：修复新建会话后页面白屏、局部数据目录路径异常等问题，
            会话创建与文件读写链路稳定性提升。
          </p>
          <p>
            体验：新增全局渲染异常兜底，界面异常时显示错误摘要与一键重载，不再整页白屏。
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
