//! 设置 · 关于视图(从 SettingsPage 搬出,纯搬家:视觉与行为不变)。

import { useEffect, useState } from 'react';

import logoUrl from '../../assets/logo.png';
import { getFailureCounts } from '../../lib/ipc/diagnostics';
import { CN_NUM, TERMS } from './constants';

export interface AboutViewProps {
  appVersion: string;
}

export function AboutView({ appVersion }: AboutViewProps) {
  const [failures, setFailures] = useState(0);
  useEffect(() => {
    getFailureCounts()
      .then((c) => setFailures(c.emit + c.event_log + c.audit))
      .catch(() => setFailures(0));
  }, []);

  return (
    <>
      <h2>关于</h2>

      {/* 产品标识区 */}
      <div className="set-card set-product">
        <img className="set-product-logo" src={logoUrl} alt="myAgent" />
        <div className="set-product-info">
          <div className="set-product-name">myAgent</div>
          <div className="set-product-ver">
            版本 {appVersion ? `${appVersion}` : '2.10.1'} · 桌面端 · Windows
          </div>
          <div className="set-product-tagline">
            MAF 团队旗下的 Agent 产品,一款专为学习、调研而创造的智能体框架软件。完全免费且无广告,开发者:梧桐吾桐及其团队。
          </div>
          <div className="set-product-ver">MAF 旗下软件交流 QQ 群:1729082</div>
          <div className="set-product-ver">
            凭据存储:API Key 与 MCP 请求头以 DPAPI(Windows)/ AES-GCM 密文保存在本地数据目录的 secrets 下,目录权限已收紧为仅当前系统用户可读。
          </div>
          {failures > 0 && (
            <div className="set-product-ver">
              运行自检:累计 {failures} 次旁路写入或事件发送失败(已记入本机日志,不影响正常使用)。
            </div>
          )}
        </div>
      </div>

      {/* 更新内容:每段覆盖一个主题,避免按版本号重复相近内容 */}
      <div className="set-card">
        <div className="set-card-h">更新内容 · v1.17.1 → v2.10.1</div>
        <div className="set-card-b">
          <p>
            <b>归档、多选与托盘修正(v2.10.1)：</b>会话右键新增「多选」与「归档会话」——多选后可批量归档 / 恢复 / 移入项目 / 删除,批量卡片复用弹窗主题令牌;归档只置 <code>archived</code> 标记,消息、记忆、附件、项目归属与更新时间全部不动,导航新增「归档」页可查看、打开与恢复;<b>项目同样可归档</b>,归档时级联其会话、恢复时一并回来,不会出现「项目已隐藏但会话仍在列表」。托盘菜单宽度按最长文字自适应并紧贴图标,子菜单在窗口底部 / 右侧会自动上移或翻转到左侧,不再被裁切;附件中心右侧预览支持图片缩略图与文档正文开头(<code>1200</code> 字上限)。
          </p>
          <p>
            <b>历史压缩与项目模型(v2.10)：</b>压缩快照首次落盘(<code>session_compactions</code> 表),水位线按 <code>id &gt; watermark</code> 单调推进,删除/回滚不再跳水位;会话可绑定项目目录,继承项目 <code>model_override</code> 与项目 <code>shared_context</code> 作为项目 guard 快照,旧预设壳只留占位不再影响运行。
          </p>
          <p>
            <b>MCP 按工具加载与结果不丢(v2.10)：</b>工具名加 <code>{'mcp__{server}__{tool}'}</code> 命名空间(FNV-1a 短哈希防超长),调度器按裸名→命名空间名两段反查 server,解决同工具名串台;非文本结果(长资源、图片/音频)自动落盘到 <code>mcp-results/</code> 并回传路径,不再静默丢弃;应用启动按需拉起启用的 server,20 秒总超时。
          </p>
          <p>
            <b>托盘同主题菜单与 Token 护栏(v2.10)：</b>原生菜单替换为无边框透明 WebviewWindow,右键托盘图标弹出同主题菜单(显示主窗 / 小窗 / 退出),样式复用主窗口 CSS 变量、随主题切换;左键单击取消,双击仍只开小窗;Segmented Cache 命中率改为厂商白名单,未知模型保守 0,避免系统性低估成本。
          </p>
          <p>
            <b>文档与演示文稿模板(v2.8–2.9)：</b><code>fs_export_docx</code> 内置公文、学术论文、实验报告、工作总结等模板,公文按 GB/T 9704 自动编号;<code>slides_export</code> 新增组会汇报、答辩、数据分析等场景模板与 <code>academic</code>/<code>ocean</code>/<code>sunset</code> 主题;新增 <code>image_compose</code>(海报/截图标注)与 <code>diagram_export</code>(Excalidraw/drawio 互转),本地处理不上传。
          </p>
          <p>
            <b>Token 估算三层校准与工具按需下发(v2.6)：</b>官方 BPE 词表 + 历史用量中位数校准 + 分段缓存预估,每条请求在 <code>session_events</code> 留档含 messages / tools / cache_saved 分项拆解;工具规格改为按需下发——只读全量,高频写常驻,低频写与会话历史里调用过的外部工具才注入,GUI 工具仅在「完全信任」档解锁。
          </p>
          <p>
            <b>对话阅读体验与入口收敛(v2.6)：</b>选中消息文字弹复制/追问浮条,追问与引用统一;引用卡片复用 Markdown / KaTeX,超过 5 行折叠;<code>latex</code>/<code>tex</code>/<code>math</code> 代码围栏自动归一化为块级公式;全局搜索改为弹出式面板(Ctrl+K 唤起,Esc 关闭),侧栏「新对话」改全宽胶囊、「设置」常驻贴底;小窗固定出现在任务栏上方右下角。
          </p>
          <p>
            <b>安全与权限(v2.5)：</b>文件操作默认工作区沙箱,相对路径拒 <code>..</code> 越界,绝对路径显式放行,路径组件级边界比较不误判 <code>D:\proj</code> 与 <code>D:\proj2</code>;MCP 工具权限三档可选(逐次审批 / 工作区 / 完全信任),完全信任档放权 GUI 自动化并全量放行高危 shell(仍留审计);数据库升级前自动快照保留 2 份,异常可手工回滚。
          </p>
          <p>
            <b>工作台与界面(v2.4–2.5)：</b>会话右侧工具流水检查器(宽度可拖拽)、目标级任务清单、会话按项目分组;消息布局焕新(用户气泡 + 助手通栏、连续工具调用折叠);侧栏抽屉化、Toast 全局浮层、悬浮任务清单抽屉(实时刷新 + 悬停详情);内置智能体模板可编辑、支持解耦目录存储、可关联外部 prompt/tools 文件,设置页「角色设置」编辑全局 base prompt。
          </p>
        </div>
      </div>

      {/* 开源引用与许可声明 */}
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
              <p>版权所有:Copyright 2026 梧桐吾桐及其团队。</p>
              <p>
                许可说明:本应用以 Apache License 2.0 开源发布,允许使用、修改与再分发(含商业用途);分发时须保留仓库根目录 LICENSE 与 NOTICE 中的版权与许可声明,并标明修改内容。
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
                引用范围:仅架构与交互理念参考(多服务商抽象、API Key 管理与桌面助手布局);未复制源码、样式或资源,也未打包其依赖。
              </p>
              <p>
                许可说明:该项目以 GNU Affero General Public License v3.0 发布;本应用与其不存在源码级衍生关系。如后续引入其源代码,合并作品须按 AGPL-3.0 分发。
              </p>
            </div>
          </section>

          <section className="set-term">
            <h3 className="set-term-h">MCP 官方参考服务器 · MIT(内置服务)</h3>
            <div className="set-term-body">
              <p>
                项目地址:<span className="mono">https://github.com/modelcontextprotocol/servers</span>
              </p>
              <p>版权所有:Copyright (c) Anthropic, PBC and contributors。</p>
              <p>
                引用范围:内置「项目文件」(server-filesystem)、「长期记忆」(server-memory)、「顺序思考」(server-sequential-thinking)服务;随应用启动经 npx 拉起,不随安装包捆绑。
              </p>
            </div>
          </section>

          <section className="set-term">
            <h3 className="set-term-h">agent-search-mcp · Apache-2.0(联网搜索)</h3>
            <div className="set-term-body">
              <p>
                项目地址:<span className="mono">https://github.com/lennney/agent-search-mcp</span>
              </p>
              <p>版权所有:Copyright (c) lennney and contributors。</p>
              <p>
                引用范围:内置「联网搜索」默认服务,零 API Key 多引擎聚合检索;随应用启动经 npx 拉起,不随安装包捆绑。
              </p>
            </div>
          </section>

          <section className="set-term">
            <h3 className="set-term-h">ppt-rs · Apache-2.0(演示文稿导出)</h3>
            <div className="set-term-body">
              <p>
                项目地址:<span className="mono">https://crates.io/crates/ppt-rs</span>
              </p>
              <p>版权所有:Copyright (c) ppt-rs contributors。</p>
              <p>
                引用范围:内置 <code>slides_export</code> 工具用它生成 <code>.pptx</code> 文件;该库随安装包一同编译分发,许可文本见仓库根目录 NOTICE。
              </p>
            </div>
          </section>

          <p className="set-term-foot">
            图片文字识别(ocr_extract)调用 Windows 系统自带的 OCR 能力,不捆绑额外的识别模型;该能力的可用语言取决于系统已安装的 OCR 语言包。
            本应用自身以 Apache License 2.0 开源发布,源码见
            <span className="mono">https://github.com/Sycamorelost/MAF-Agent</span>,
            许可证全文与第三方组件声明见仓库根目录 LICENSE / NOTICE 文件。
            以上声明仅用于致谢与合规披露,不构成对原作者的任何担保。
          </p>
        </div>
      </div>

      {/* 条款文档区 */}
      <div className="set-card">
        <div className="set-card-h">
          myAgent 使用条款
          <span className="st date">最近更新日期:2026 年 11 月 25 日</span>
        </div>
        <div className="set-card-b set-terms">
          {TERMS.map((t) => (
            <section key={t.no} className="set-term">
              <h3 className="set-term-h">第{CN_NUM[t.no]}条 · {t.name}</h3>
              {t.items.map((it, i) => {
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
