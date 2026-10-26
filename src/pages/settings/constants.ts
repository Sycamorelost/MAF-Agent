import { type ComponentType } from 'react';
import * as ipc from '../../lib/ipc';
import { IconBot, IconBrain, IconChat, IconGlobe, IconArchive, IconLogo, IconTranslate, IconQuote, IconChart, IconPlus, IconFileText } from '../../components/icons';

export const ENGINES = [
  { id: 'bocha', name: '博查 Bocha' },
  { id: 'zhipu', name: '智谱 BigModel' },
  { id: 'deepseek', name: 'DeepSeek 搜索' },
  { id: 'searxng', name: 'SearXNG' },
  { id: 'tavily', name: 'Tavily' },
];

type NavItem = { id: string; label: string; icon: ComponentType<{ size?: number }> };
type NavGroup = { grp: string; items: NavItem[] };

export const NAV: NavGroup[] = [
  {
    grp: '模型',
    items: [
      { id: 'providers', label: '模型服务商', icon: IconBot },
      { id: 'memory', label: '记忆与提炼', icon: IconBrain },
    ],
  },
  {
    grp: '能力',
    items: [
      { id: 'search', label: '联网搜索', icon: IconGlobe },
      { id: 'translate', label: '翻译', icon: IconTranslate },
      { id: 'selection', label: '划词提问', icon: IconQuote },
    ],
  },
  {
    grp: '界面',
    items: [
      { id: 'appearance', label: '字体大小', icon: IconFileText },
      { id: 'navmenu', label: '导航菜单', icon: IconChat },
    ],
  },
  {
    grp: '扩展',
    items: [{ id: 'mcp', label: 'MCP 工具', icon: IconPlus }],
  },
  {
    grp: '数据',
    items: [
      { id: 'usage', label: '用量统计', icon: IconChart },
      { id: 'storage', label: '存储位置', icon: IconArchive },
      { id: 'about', label: '关于', icon: IconLogo },
    ],
  },
];

/** 导航菜单可见性开关项(对话/设置固定保留,不在此列)。 */
export const NAV_VISIBILITY: { key: string; label: string }[] = [
  { key: 'research', label: '研究' },
  { key: 'plan', label: '学习规划' },
  { key: 'draw', label: '绘图' },
  { key: 'translate', label: '翻译' },
  { key: 'memory', label: '记忆' },
  { key: 'notes', label: '知识库' },
  { key: 'attachments', label: '附件' },
];

/** 记忆注入策略选项(与 MemoryStrategy 对齐)。 */
export const MEMORY_STRATEGY_OPTIONS: { value: ipc.MemoryStrategy; label: string }[] = [
  { value: 'full', label: '全文' },
  { value: 'summary', label: '仅摘要' },
  { value: 'hybrid', label: '摘要 + 事实 + 待办' },
  { value: 'none', label: '不注入' },
];

/** 思考能力手动修正选项(v1.9.0 工作块 I;value → ReasoningCapability,auto = 清除修正回内置推断)。 */
export const THINK_CAP_OPTIONS: { value: string; label: string; cap: ipc.ReasoningCapability | null }[] = [
  { value: 'auto', label: '自动(按规则表推断)', cap: null },
  { value: 'full', label: '支持 · 可关 · 有档位', cap: { enabled: true, can_disable: true, levels: true } },
  { value: 'toggle', label: '支持 · 可关 · 仅开关', cap: { enabled: true, can_disable: true, levels: false } },
  { value: 'locked_levels', label: '支持 · 不可关 · 有档位', cap: { enabled: true, can_disable: false, levels: true } },
  { value: 'locked_toggle', label: '支持 · 不可关 · 仅开关', cap: { enabled: true, can_disable: false, levels: false } },
  { value: 'off', label: '不支持思考', cap: { enabled: false, can_disable: false, levels: false } },
];

/** ReasoningCapability → 选项 value(设置页回显用)。 */
export function capToValue(cap: ipc.ReasoningCapability | null): string {
  if (!cap) return 'auto';
  if (!cap.enabled) return 'off';
  if (cap.can_disable) return cap.levels ? 'full' : 'toggle';
  return cap.levels ? 'locked_levels' : 'locked_toggle';
}

/** 数字输入 → number | null(空串/非法值视为不覆盖)。 */
export function numOrNull(v: string): number | null {
  const t = v.trim();
  if (!t) return null;
  const n = Number(t);
  return Number.isFinite(n) ? n : null;
}

/** 成本档位选项。 */
export const COST_TIER_OPTIONS = Object.entries(ipc.COST_TIER_LABEL).map(([value, label]) => ({
  value,
  label,
}));

export const CN_NUM = ['', '一', '二', '三', '四', '五', '六', '七', '八', '九', '十', '十一', '十二', '十三', '十四', '十五'];

export type TermItem = { text: string; bullets?: string[] };
export const TERMS: { no: number; name: string; items: TermItem[] }[] = [
  {
    no: 1,
    name: '条款的接受与修订',
    items: [
      {
        text: '1.1 本使用条款(以下简称"本条款")是您(以下简称"用户"或"您")与 MAF Agent 开发者梧桐吾桐及其团队(以下简称"开发者")之间,就 MAF Agent 软件(以下简称"本软件")的安装、运行与使用所订立的协议。',
      },
      {
        text: '1.2 您在安装、运行或以任何方式使用本软件前,应完整阅读本条款。您安装、运行或继续使用本软件,即视为您已阅读、理解并同意受本条款约束。若您不同意本条款的任何内容,请勿安装或使用本软件,并应将其卸载。',
      },
      {
        text: '1.3 开发者保留不时修订本条款的权利。修订时,开发者将更新本页顶部的"最近更新日期"。修订后的条款自标注日期起随新版本生效;您在修订版本发布后继续使用本软件,即视为您同意修订后的条款。',
      },
    ],
  },
  {
    no: 2,
    name: '定义',
    items: [
      { text: '2.1 "本软件"指 MAF Agent 桌面应用程序,及其附带的界面、文档与本地功能模块。' },
      {
        text: '2.2 "模型服务商"指由用户自行配置、凭 API Key 接入的第三方大模型服务提供方,包括用户自行添加的兼容接口。',
      },
      {
        text: '2.3 "用户数据"指用户使用本软件过程中在本机产生或保存的数据,包括会话记录、消息、记忆文件、知识库笔记、附件、生成图片及配置信息。',
      },
      { text: '2.4 "API Key"指用户向模型服务商申请、用于调用其模型服务的身份凭证。' },
    ],
  },
  {
    no: 3,
    name: '软件描述与授权范围',
    items: [
      {
        text: '3.1 本软件是一款专为学习、调研而创造的智能体框架软件,提供本地会话、文件即记忆、专项研究、知识库、翻译、绘画、划词提问等框架功能。本软件自身不生产、不持有大模型内容,模型能力由用户自行配置的模型服务商提供。',
      },
      {
        text: '3.2 本软件以 Apache License 2.0 开源发布，源码见 https://github.com/Sycamorelost/MAF-Agent 。您对本软件的安装、运行、修改、分发以及商业使用，均以仓库根目录 LICENSE 授予的权利为准；本条款仅作补充说明，与 LICENSE 冲突时以 LICENSE 为准。(免费与无广告承诺见第十五条。)',
      },
      {
        text: '3.3 依据开源许可证，您可将本软件用于商业用途(含商业转售、有偿托管、捆绑销售等)，但须遵守 LICENSE 与 NOTICE 中关于署名、许可与免责声明的要求，不得以开发者名义对修改后的版本作出担保或背书。许可证未授予的商标等权利仍由开发者保留。',
      },
    ],
  },
  {
    no: 4,
    name: '第三方技术依赖与开源组件',
    items: [
      {
        text: '4.1 本软件基于开源技术构建,下列组件的著作权归各自权利人所有,其使用受各自随附许可证的约束:',
        bullets: [
          '应用与界面框架:Tauri、React、React DOM、React Router、Zustand、TypeScript、Vite;',
          '本地数据:SQLite(经 rusqlite 以 bundled 方式编入,您无需另行安装数据库);',
          '网络与本地调度:reqwest(采用 rustls)、futures、notify、tokio(Tauri 运行时);',
          '加密与编码:操作系统提供的 Windows DPAPI,以及 aes-gcm、sha2、base64;',
          '文档解析与图像:docx-rs、calamine、pdf-extract、image;',
          '内容渲染:react-markdown、remark-gfm、remark-math、rehype-katex、katex、rehype-highlight、highlight.js、mermaid、echarts。',
        ],
      },
      {
        text: '4.2 本软件在 Windows 上运行时使用系统提供的 WebView 运行时。该运行时由其权利人提供并适用其相应条款,不属于本软件的组成部分。',
      },
      {
        text: '4.3 上述组件及全部依赖的具体名称与版本,以本软件源码工程内的清单文件(Cargo.toml、package.json)及各组件自带的许可证文本为准。如任一组件条款与您的使用场景存在冲突,应以该组件条款为准并停止相关使用。',
      },
      {
        text: '4.4 开发者尊重知识产权。如您认为本软件包含的内容侵犯您的合法权益,可通过第十二条载明的联系方式提交权属证明与事实说明,开发者将在核实后依法处理。',
      },
    ],
  },
  {
    no: 5,
    name: '第三方模型服务与 API Key',
    items: [
      {
        text: '5.1 本软件不提供任何大模型服务。您须自行向模型服务商注册、取得 API Key,并自行承担该服务商按其规则收取的费用。模型服务是否可用、输出内容质量、计费方式及对您数据的处理,均受您与该服务商之间协议及其服务条款、隐私政策的约束。',
      },
      {
        text: '5.2 开发者不控制、不背书任何模型服务商及其输出内容,不对第三方服务的可用性、准确性、及时性或合法性作出任何陈述或保证。',
      },
      {
        text: '5.3 您应妥善保管 API Key,并对在您的 Key 下发生的全部调用及由此产生的费用承担全部责任。因您保管不善、Key 泄露或自行向他人披露所造成的损失,开发者不承担责任。',
      },
      {
        text: '5.4 模型请求由您的设备直接发送至您所配置的模型服务商;开发者不经手、不中转、不存储上述请求内容。',
      },
    ],
  },
  {
    no: 6,
    name: '数据存储与隐私',
    items: [
      {
        text: '6.1 本软件为本地优先应用。用户数据保存于您的本机,保存位置由您在首次启动引导或设置中自行选定,您可在设置中查看与更换数据目录。',
      },
      {
        text: '6.2 开发者不收集、不上传、不托管用户数据;本软件不包含由开发者运营的云端存储或账号体系。用户数据的保管、备份与保密由您自行负责。',
      },
      {
        text: '6.3 API Key 在 Windows 上通过操作系统提供的 DPAPI 机制加密后保存于本机,本软件任何界面均不回显 Key 的明文;旧版本以其他方式保存的密钥文件按兼容方式读取。',
      },
      {
        text: '6.4 与模型服务商之间的网络通信发生在您的设备与该服务商之间,通信内容可能被其按自身隐私政策处理,开发者对此不承担责任。',
      },
      { text: '6.5 卸载本软件或删除其数据目录可能导致用户数据不可恢复,请您事先自行备份。' },
    ],
  },
  {
    no: 7,
    name: '用户行为规范',
    items: [
      {
        text: '7.1 作为您使用本软件的条件,您承诺遵守所适用的法律法规,不得利用本软件从事任何违法或侵权行为,包括但不限于:',
        bullets: [
          '(1) 生成、复制、传播含有危害国家安全、破坏社会稳定、淫秽色情、暴力恐怖或其他法律法规禁止内容的信息;',
          '(2) 侵害他人名誉权、隐私权、知识产权或其他合法权益;',
          '(3) 对模型服务商或任何第三方系统实施未授权访问、干扰、攻击,或试图绕过其访问控制与计费规则;',
          '(4) 对本软件实施反向工程后用于侵权分发,或移除、篡改软件中的署名与权利声明;',
          '(5) 违反本软件开源许可证(Apache License 2.0)的使用行为，例如移除、篡改 LICENSE/NOTICE 中的署名与权利声明后分发。',
        ],
      },
      {
        text: '7.2 大模型输出可能包含错误、过时或带有偏见的内容。您在将其用于学习、研究或其他决策前,应自行核实其准确性与适用性。',
      },
    ],
  },
  {
    no: 8,
    name: '知识产权与源码获取',
    items: [
      {
        text: '8.1 本软件以 Apache License 2.0 开源发布，源码自行获取：https://github.com/Sycamorelost/MAF-Agent 。著作权及其他知识产权归开发者所有，授权范围与义务以仓库根目录 LICENSE 为准。',
      },
      {
        text: '8.2 您转载、分发本软件或其源码(含修改版本)时，应保留 LICENSE、NOTICE 中的版权与许可声明，标明修改内容，不得隐去或篡改软件来源，不得暗示修改版本由开发者背书。',
      },
      {
        text: '8.3 本软件界面使用的图标、文案等素材的权利归开发者所有;第三方开源组件的权利归属适用第四条。',
      },
    ],
  },
  {
    no: 9,
    name: '生成内容',
    items: [
      {
        text: '9.1 您对输入本软件的全部内容享有相应权利并承担责任,并保证其使用不侵犯任何第三方权益。',
      },
      {
        text: '9.2 模型生成内容的权属与使用限制,适用您与模型服务商之间的约定及相关法律规定。开发者不对生成内容主张所有权,亦不对其合法性、准确性或可商用性作出保证。',
      },
      { text: '9.3 您因使用、复制、传播生成内容而产生的纠纷与责任,由您自行承担。' },
    ],
  },
  {
    no: 10,
    name: '免责声明',
    items: [
      {
        text: '10.1 本软件按"现状"和"现有"基础免费提供。在适用法律允许的最大范围内,开发者不就本软件作出任何明示或默示的保证,包括但不限于对适销性、特定用途适用性、不侵权、运行不中断或不存在错误的保证。',
      },
      {
        text: '10.2 开发者不保证本软件与所有模型服务商接口持续兼容,亦不保证模型服务始终可用。因第三方服务变更、中断、限流、接口调整或计费变化导致的功能异常,不属于开发者的责任范围。',
      },
      {
        text: '10.3 因您自行更改数据目录、删除数据文件、使用非开发者提供的版本,或未按软件说明操作而导致的数据丢失或软件异常,开发者不承担责任。',
      },
      {
        text: '10.4 「完全信任」授权模式(含其解锁的临时授权与 GUI 自动化)为您在设置页明示选择开启的高权限模式。开启后,模型输出可直接触发本机命令执行、文件写入与鼠标键盘操作,不再逐次询问;您应仅在同时信任所用模型与全部工具输出内容时开启。因开启该模式而产生的任何操作后果,由您自行承担。',
      },
    ],
  },
  {
    no: 11,
    name: '责任限制',
    items: [
      {
        text: '11.1 在适用法律允许的最大范围内,对于因使用或无法使用本软件、依赖模型输出内容、第三方服务故障,或用户数据丢失、泄露所引起的任何直接、间接、附带或后果性损失(包括但不限于数据损失、利润损失、业务中断),开发者在任何情况下均不承担赔偿责任。',
      },
      {
        text: '11.2 本软件为免费软件,您理解并同意,前述责任限制是在免费提供软件的基础上作出的合理安排。',
      },
    ],
  },
  {
    no: 12,
    name: '其他',
    items: [
      {
        text: '12.1 本条款任何条款被有权机关认定为无效或不可执行的,不影响其余条款的效力。',
      },
      { text: '12.2 因本条款或使用本软件所产生的争议,双方应首先友好协商解决。' },
      { text: '12.3 联系方式:QQ 1425408561。' },
    ],
  },
  {
    no: 13,
    name: '科研增强功能与技能来源',
    items: [
      {
        text: '13.1 本软件研究功能中的「科研增强」能力,其方法论基于开源项目 nature-skills(作者 Yuan1z0825,项目地址:https://github.com/Yuan1z0825/nature-skills ,本功能对应其「其他 agent 场景」章节:https://github.com/Yuan1z0825/nature-skills#55-%E5%85%B6%E4%BB%96-agent-%E5%9C%BA%E6%99%AF )。上述技能方法论文档经本地化重构后随本软件内置,仅供本软件本地运行使用;其著作权归原作者所有。',
      },
      {
        text: '13.2 科研增强功能的检索、生成、审计与导出全部在本机完成;本软件未移植、未启用该项目文档中面向任何第三方服务的路由,亦不为其建立任何网络连接。',
      },
    ],
  },
  {
    no: 14,
    name: '输出格式边界',
    items: [
      {
        text: '14.1 研究报告导出仅提供 Markdown 与 HTML 两种格式,全部在本机生成。本软件不提供 PPTX、DOCX、PDF 等其他格式的导出,以保障输出的稳定性与质量。',
      },
      {
        text: '14.2 本软件图示能力包括本地矢量图渲染与用户自行配置的模型服务图像生成;AI 生成图像仅作为概念草稿,不构成实验证据。',
      },
    ],
  },
  {
    no: 15,
    name: '无广告与去商业化声明',
    items: [
      {
        text: '15.1 本软件完全免费,不提供任何第三方广告服务,不包含任何原生广告、推广位或赞助内容,亦无内购项目。',
      },
      {
        text: '15.2 本软件现在不会、将来也不会参与任何商业化内容或商业化运作,将持续坚持开源免费的发展方向。',
      },
      {
        text: '15.3 本软件源码公开于 https://github.com/Sycamorelost/MAF-Agent ，无需申请即可自行获取；获取、修改与分发须遵循仓库根目录的 LICENSE 与 NOTICE。',
      },
    ],
  },
];

