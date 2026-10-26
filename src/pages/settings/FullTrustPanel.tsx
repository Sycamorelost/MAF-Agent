//! 授权面板(完全信任 · 高级功能) —— 临时授权(能力令牌)与 GUI 自动化合并为一张卡片。
//! 两项均仅在审批模式为「完全信任」时可用,切回其他模式即整体关闭;
//! 进程白名单、句柄 TTL、令牌时限等内容层约束仍然生效。

import { useEffect, useState } from "react";
import {
  mcpClearCapabilities,
  mcpIssueCapability,
  mcpListCapabilities,
  mcpRevokeCapability,
  scopeLabel,
  type Capability,
} from "../../lib/ipc/capability";
import {
  guiFindElements,
  guiGetPolicy,
  guiListWindows,
  guiSavePolicy,
  type ElementInfo,
  type GuiPolicy,
  type WindowInfo,
} from "../../lib/ipc/gui";

/** 一键授权时长:30 分钟(秒)。 */
const ONE_CLICK_TTL_SECS = 30 * 60;

/** 常用进程预置(exe 名 → 中文说明),供白名单下拉快选。 */
const PRESET_APPS: { exe: string; label: string }[] = [
  { exe: "notepad.exe", label: "记事本" },
  { exe: "mspaint.exe", label: "画图" },
  { exe: "calc.exe", label: "计算器" },
  { exe: "explorer.exe", label: "文件资源管理器" },
  { exe: "WINWORD.EXE", label: "Word" },
  { exe: "EXCEL.EXE", label: "Excel" },
  { exe: "POWERPNT.EXE", label: "PowerPoint" },
  { exe: "wps.exe", label: "WPS" },
  { exe: "chrome.exe", label: "Chrome" },
  { exe: "msedge.exe", label: "Edge" },
  { exe: "Code.exe", label: "VS Code" },
];

export function FullTrustPanel({ fullTrust }: { fullTrust: boolean }) {
  // ---- 临时授权 ----
  const [caps, setCaps] = useState<Capability[]>([]);
  const [capMsg, setCapMsg] = useState("");
  const [capBusy, setCapBusy] = useState(false);

  const refreshCaps = () =>
    mcpListCapabilities()
      .then(setCaps)
      .catch((e) => setCapMsg(`读取令牌失败: ${e}`));

  useEffect(() => {
    refreshCaps();
    // 令牌会自然过期,定时刷新让剩余时间保持准确
    const timer = setInterval(refreshCaps, 30_000);
    return () => clearInterval(timer);
  }, []);

  /** 一键签发:覆盖全部作用域,30 分钟有效,不限次数。 */
  const onOneClick = async () => {
    setCapBusy(true);
    try {
      const cap = await mcpIssueCapability(
        "一键临时授权",
        ["fs_read", "fs_write", "exec", "net", "ui"],
        ONE_CLICK_TTL_SECS,
        undefined,
      );
      setCapMsg(`✅ 已授权全部能力,${fmtTime(cap.expires_at)} 前工具调用不再弹窗`);
      await refreshCaps();
    } catch (e: any) {
      setCapMsg(`❌ 授权失败: ${e}`);
    } finally {
      setCapBusy(false);
    }
  };

  const onRevoke = async (id: string) => {
    try {
      await mcpRevokeCapability(id);
      await refreshCaps();
      setCapMsg("已撤销该授权");
    } catch (e: any) {
      setCapMsg(`❌ 撤销失败: ${e}`);
    }
  };

  // ---- GUI 自动化 ----
  const [policy, setPolicy] = useState<GuiPolicy | null>(null);
  const [apps, setApps] = useState<string[]>([]);
  const [customApp, setCustomApp] = useState("");
  const [wins, setWins] = useState<WindowInfo[]>([]);
  const [preview, setPreview] = useState<{ title: string; items: ElementInfo[] } | null>(null);
  const [guiMsg, setGuiMsg] = useState("");
  const [guiBusy, setGuiBusy] = useState(false);

  useEffect(() => {
    guiGetPolicy()
      .then((p) => {
        setPolicy(p);
        setApps(p.allowed_apps);
      })
      .catch((e) => setGuiMsg(`读取策略失败: ${e}`));
  }, []);

  const savePolicy = async (next: GuiPolicy) => {
    setGuiBusy(true);
    try {
      await guiSavePolicy(next);
      setPolicy(next);
      setGuiMsg("✅ GUI 策略已保存,对下一次工具调用生效");
    } catch (e: any) {
      setGuiMsg(`❌ 保存失败: ${e}`);
    } finally {
      setGuiBusy(false);
    }
  };

  const onSaveApps = async () => {
    if (!policy) return;
    await savePolicy({ ...policy, allowed_apps: apps });
  };

  /** 下拉/自定义添加进程:去重、忽略空串;选 `*` 即不限制(独占)。 */
  const addApp = (raw: string) => {
    const exe = raw.trim();
    if (!exe) return;
    setApps((prev) => (exe === "*" ? ["*"] : [...prev.filter((a) => a !== "*"), ...prev.includes(exe) ? [] : [exe]]));
  };

  const onListWindows = async () => {
    try {
      const list = await guiListWindows();
      setWins(list);
      setGuiMsg(list.length ? `发现 ${list.length} 个可见窗口` : "没有可见窗口");
    } catch (e: any) {
      setGuiMsg(`❌ 枚举窗口失败: ${e}`);
    }
  };

  const onProbe = async (w: WindowInfo) => {
    try {
      const items = await guiFindElements(w.hwnd, { limit: 12 });
      setPreview({ title: w.title, items });
      setGuiMsg(items.length ? `「${w.title}」发现 ${items.length} 个元素` : "未找到元素");
    } catch (e: any) {
      setGuiMsg(`❌ ${e}`);
    }
  };

  if (!policy) return <div className="set-hint">加载授权策略…</div>;

  return (
    <div className="mcp-policy-card">
      <div className="mcp-time-label">授权 · 高级功能(完全信任)</div>
      <div className="mcp-policy-desc">
        临时授权与 GUI 自动化已并入「完全信任」总开关:仅当审批模式为「完全信任」时可用,
        切回手动 / 自动 / 自定义即整体关闭;进程白名单、句柄 TTL、令牌时限等内容层约束仍然生效。
      </div>
      {!fullTrust && <div className="mcp-warn">当前非完全信任模式,以下功能处于关闭状态。</div>}

      {/* 临时授权(能力令牌) */}
      <div className="mcp-time-label" style={{ marginTop: 14 }}>
        临时授权
      </div>
      <div className="mcp-policy-desc">
        把全部能力收缩为一张 30 分钟时限的凭据:期间工具调用按作用域放行并逐次落审计,
        到期或重启后自动失效。适合交给子任务 / 委派等需要凭据边界的场景。
      </div>
      <div className="mcp-server-actions">
        <button
          className="btn btn-xs btn-primary"
          disabled={capBusy || !fullTrust}
          onClick={onOneClick}
        >
          一键授权 30 分钟
        </button>
        {caps.length > 0 && (
          <button
            className="btn btn-xs"
            onClick={async () => {
              await mcpClearCapabilities();
              await refreshCaps();
              setCapMsg("已撤销全部授权");
            }}
          >
            撤销全部
          </button>
        )}
      </div>
      {capMsg && <div className={`mcp-msg${capMsg.startsWith("❌") ? " err" : ""}`}>{capMsg}</div>}
      {caps.length > 0 && (
        <div className="cap-list">
          {caps.map((c) => (
            <div key={c.id} className="cap-row">
              <div className="cap-row-main">
                <strong>{c.label}</strong>
                <span className="cap-meta">
                  {fmtTime(c.expires_at)} 前有效 · 已用 {c.uses}
                  {c.max_uses == null ? " 次" : ` / ${c.max_uses} 次`}
                </span>
              </div>
              <span className="cap-scope-tags">
                {c.scopes.map((s) => (
                  <span key={s} className="mcp-chip">
                    {scopeLabel(s)}
                  </span>
                ))}
              </span>
              <button className="btn btn-xs btn-ghost" onClick={() => onRevoke(c.id)}>
                撤销
              </button>
            </div>
          ))}
        </div>
      )}

      {/* GUI 自动化(Windows UI Automation) */}
      <div className="mcp-time-label" style={{ marginTop: 14 }}>
        GUI 自动化(Windows UI Automation)
      </div>
      <div className="mcp-policy-desc">
        模型可枚举窗口、查找元素并执行点击 / 写值 / 输入;可操作的进程由下方白名单限定,
        句柄有存活时间,输入有字数上限。
      </div>
      {fullTrust && (
        <div className="mcp-warn">
          模型将代替你操作鼠标键盘,仅在你信任的任务下使用。当前白名单:
          {policy.allowed_apps.includes("*") ? "所有窗口" : policy.allowed_apps.join("、") || "(空)"}
        </div>
      )}

      <details className="mcp-tools">
        <summary>高级:限定可操作的进程(可选)</summary>
        <div className="mcp-field">
          <span>允许操作的进程(`*` 表示不限制;点 × 移除,改完点「保存白名单」生效)</span>
          <div style={{ display: "flex", flexWrap: "wrap", gap: 6, margin: "6px 0" }}>
            {apps.length === 0 && <span className="set-hint">(空:不允许操作任何进程)</span>}
            {apps.map((a) => (
              <span key={a} className="mcp-chip" style={{ display: "inline-flex", alignItems: "center", gap: 4 }}>
                {a}
                <button
                  type="button"
                  className="btn btn-xs btn-ghost"
                  style={{ padding: "0 4px", lineHeight: 1 }}
                  onClick={() => setApps((prev) => prev.filter((x) => x !== a))}
                >
                  ×
                </button>
              </span>
            ))}
          </div>
          <div className="mcp-server-actions">
            <select
              className="mcp-allow-input"
              style={{ maxWidth: 320 }}
              value=""
              onChange={(e) => {
                addApp(e.target.value);
                e.target.value = "";
              }}
            >
              <option value="">选择常用进程…</option>
              <option value="*">* — 不限制(所有窗口)</option>
              {PRESET_APPS.filter((p) => !apps.includes(p.exe)).map((p) => (
                <option key={p.exe} value={p.exe}>
                  {p.exe} — {p.label}
                </option>
              ))}
              {wins
                .map((w) => w.process)
                .filter((v, i, arr) => v && arr.indexOf(v) === i)
                .filter((p) => !apps.includes(p) && !PRESET_APPS.some((x) => x.exe === p))
                .map((p) => (
                  <option key={p} value={p}>
                    {p} — 当前窗口
                  </option>
                ))}
            </select>
            <input
              className="mcp-allow-input"
              style={{ maxWidth: 200 }}
              placeholder="自定义进程名,如 wps.exe"
              value={customApp}
              onChange={(e) => setCustomApp(e.target.value)}
            />
            <button
              className="btn btn-xs"
              onClick={() => {
                addApp(customApp);
                setCustomApp("");
              }}
            >
              添加
            </button>
            <button className="btn btn-xs" disabled={guiBusy} onClick={onSaveApps}>
              保存白名单
            </button>
          </div>
        </div>
      </details>

      <div className="mcp-server-actions">
        <button className="btn btn-xs" disabled={!fullTrust} onClick={onListWindows}>
          枚举可见窗口
        </button>
        <span className="cap-meta">
          句柄 TTL {policy.element_ttl_secs}s · 单次最多 {policy.max_elements} 个元素 · 输入上限{" "}
          {policy.max_text_len} 字符
        </span>
      </div>

      {guiMsg && <div className={`mcp-msg${guiMsg.startsWith("❌") ? " err" : ""}`}>{guiMsg}</div>}

      {wins.length > 0 && (
        <div className="gui-win-list">
          {wins.map((w) => (
            <div key={w.hwnd} className="gui-win-row">
              <div className="cap-row-main">
                <strong className="gui-win-title">{w.title}</strong>
                <span className="cap-meta">
                  {w.process} · hwnd {w.hwnd}
                </span>
              </div>
              <button className="btn btn-xs" disabled={!fullTrust} onClick={() => onProbe(w)}>
                查看元素
              </button>
            </div>
          ))}
        </div>
      )}

      {preview && (
        <div className="gui-elem-list">
          <div className="cap-meta">「{preview.title}」的元素预览</div>
          {preview.items.length === 0 ? (
            <div className="set-hint">未找到元素</div>
          ) : (
            preview.items.map((e) => (
              <div key={e.element_id} className="gui-elem-row">
                <code>{e.element_id}</code>
                <span className="mcp-chip">{e.control_type}</span>
                <span className="gui-elem-name">{e.name || e.automation_id || "(无名称)"}</span>
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}

function fmtTime(sec: number): string {
  return new Date(sec * 1000).toLocaleTimeString("zh-CN", {
    hour12: false,
    hour: "2-digit",
    minute: "2-digit",
  });
}
