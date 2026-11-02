import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { WorkbenchChrome } from '../../lib/workbenchChrome';

/** 检查器展开态持久化键(P3-1 起两态通用;无记录时默认 work 开 / chat 收)。 */
const CHAT_INSPECTOR_KEY = 'chat-inspector-open';
/** 旧 work 外壳的持久化键:仅作一次性迁移读取。 */
const WB_INSPECTOR_KEY_LEGACY = 'wb-inspector-open';

/**
 * 检查器开关状态(与记忆面板互斥,右栏同时只开一个)。
 * 含窄窗自动收起/恢复:窄窗(≤760,检查器转浮层)进入时收起避免遮挡中栏;
 * 回到宽窗时仅恢复「被自动收起」的展开态,用户手动收起的不打扰。
 */
export function useChatInspector(memCollapsed: boolean, memToggle: () => void) {
  /** 用户显式开关偏好;null = 未设置,工作台默认开 */
  const [inspectorPref, setInspectorPref] = useState<boolean | null>(() => {
    const v = localStorage.getItem(CHAT_INSPECTOR_KEY);
    if (v !== null) return v === '1';
    // 迁移:沿用旧 work 外壳的检查器开关态(WorkbenchPage 已删)
    const legacy = localStorage.getItem(WB_INSPECTOR_KEY_LEGACY);
    return legacy === null ? true : legacy === '1';
  });
  const chatInspectorOpen = inspectorPref ?? true;

  /** 开关检查器:与记忆面板互斥(右栏同时只开一个) */
  const toggleChatInspector = useCallback(() => {
    const next = !chatInspectorOpen;
    localStorage.setItem(CHAT_INSPECTOR_KEY, next ? '1' : '0');
    setInspectorPref(next);
    if (next && !memCollapsed) memToggle();
  }, [chatInspectorOpen, memCollapsed, memToggle]);

  /** 展开记忆面板时收起检查器(互斥) */
  const onToggleMemPanel = useCallback(() => {
    if (memCollapsed && chatInspectorOpen) {
      localStorage.setItem(CHAT_INSPECTOR_KEY, '0');
      setInspectorPref(false);
    }
    memToggle();
  }, [memCollapsed, memToggle, chatInspectorOpen]);

  const inspectorAutoClosed = useRef(false);
  const inspectorOpenRef = useRef(chatInspectorOpen);
  inspectorOpenRef.current = chatInspectorOpen;
  useEffect(() => {
    const mq = window.matchMedia('(max-width: 760px)');
    const apply = (narrow: boolean) => {
      if (narrow) {
        if (!inspectorOpenRef.current) return;
        inspectorAutoClosed.current = true;
        localStorage.setItem(CHAT_INSPECTOR_KEY, '0');
        setInspectorPref(false);
      } else if (inspectorAutoClosed.current) {
        inspectorAutoClosed.current = false;
        localStorage.setItem(CHAT_INSPECTOR_KEY, '1');
        setInspectorPref(true);
      }
    };
    apply(mq.matches);
    const onChange = (e: MediaQueryListEvent) => apply(e.matches);
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  }, []);

  /** 头部 chrome:由本地检查器状态提供 */
  const chromeValue = useMemo<WorkbenchChrome>(
    () => ({
      inspectorOpen: chatInspectorOpen,
      onToggleInspector: toggleChatInspector,
    }),
    [chatInspectorOpen, toggleChatInspector],
  );

  return { chatInspectorOpen, toggleChatInspector, onToggleMemPanel, chromeValue };
}
