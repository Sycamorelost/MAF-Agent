/** 工作台模式(D16):全局默认 + 单会话覆盖的解析与订阅。 */
import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import * as ipc from './ipc';
import { useSessionsStore } from '../stores/sessions';

export type WorkbenchMode = 'chat' | 'work';

/** 解析生效模式:会话覆盖(chat / work)优先,inherit 跟随全局默认。 */
export function resolveMode(sessionMode: string | undefined, def: WorkbenchMode): WorkbenchMode {
  return sessionMode === 'chat' || sessionMode === 'work' ? sessionMode : def;
}

/** 模块级缓存:供导航帮助函数等非 hook 场景即时解析全局默认模式(由 useDefaultWorkbench 同步)。 */
let cachedDefault: WorkbenchMode = 'chat';

/**
 * 目标会话的直达路径:按生效模式直接返回 /work/:id 或 /chat/:id。
 * 各入口(会话列表 / 全局搜索 / 新建会话等)统一用它导航,
 * 专业模式下不会再先落到 /chat/:id 再被路由门二次重定向(消除中间跳转)。
 */
export function sessionPath(id: string): string {
  const s = useSessionsStore.getState().sessions.find((x) => x.id === id);
  return resolveMode(s?.workbench_mode, cachedDefault) === 'work' ? `/work/${id}` : `/chat/${id}`;
}

/** 读取全局默认模式并随 settings_changed 实时更新;未加载完成返回 null。 */
export function useDefaultWorkbench(): WorkbenchMode | null {
  const [mode, setMode] = useState<WorkbenchMode | null>(null);
  useEffect(() => {
    let alive = true;
    const apply = (s: ipc.AppSettings) => {
      const m: WorkbenchMode = s.default_workbench === 'work' ? 'work' : 'chat';
      cachedDefault = m;
      if (alive) setMode(m);
    };
    // 设置读取失败时回落到 chat:否则 mode 永远为 null,路由门会一直渲染空白页
    ipc
      .getSettings()
      .then(apply)
      .catch(() => {
        cachedDefault = 'chat';
        if (alive) setMode('chat');
      });
    const un = listen<ipc.AppSettings>('settings_changed', (e) => apply(e.payload));
    return () => {
      alive = false;
      void un.then((f) => f());
    };
  }, []);
  return mode;
}
