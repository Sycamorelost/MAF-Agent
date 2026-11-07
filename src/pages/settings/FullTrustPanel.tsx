//! GUI 自动化面板(仅「完全信任」模式可用) —— 枚举窗口、查找元素、白名单与句柄约束。

import { useEffect, useState } from "react";
import {
  guiFindElements,
  guiGetPolicy,
  guiListWindows,
  guiSavePolicy,
  type ElementInfo,
  type GuiPolicy,
  type WindowInfo,
} from "../../lib/ipc/gui";

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
      setGuiMsg("GUI 策略已保存,对下一次工具调用生效");
    } catch (e: any) {
      setGuiMsg(`保存失败: ${e}`);
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
      setGuiMsg(`枚举窗口失败: ${e}`);
    }
  };

  const onProbe = async (w: WindowInfo) => {
    try {
      const items = await guiFindElements(w.hwnd, { limit: 12 });
      setPreview({ title: w.title, items });
      setGuiMsg(items.length ? `「${w.title}」发现 ${items.length} 个元素` : "未找到元素");
    } catch (e: any) {
      setGuiMsg(`错误: ${e}`);
    }
  };

  if (!policy) return <div className="set-hint">加载授权策略…</div>;

  return (
    <div className="mcp-policy-card">
      <div className="mcp-time-label">高级功能 · GUI 自动化(仅「完全信任」可用)</div>
      <div className="mcp-policy-desc">
        仅当上面的工具权限选为「完全信任」时,下面的 GUI 自动化功能才可用;
        改成「逐次审批」或「工作区」后自动关闭。可操作的进程由白名单限定,句柄有存活时间,输入有字数上限。
      </div>
      {!fullTrust && <div className="mcp-warn">当前不是「完全信任」,以下功能处于关闭状态。</div>}

      {/* GUI 自动化(Windows UI Automation) */}
      <div className="mcp-policy-desc">
        仅支持 Windows(使用系统 UI Automation;macOS / Linux 暂不可用,面板会提示不支持)。
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

      {guiMsg && <div className={`mcp-msg${guiMsg.startsWith("错误") || guiMsg.includes("失败") ? " err" : ""}`}>{guiMsg}</div>}

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
