/** 工作台模式(D16):全局默认 + 单会话覆盖的解析与订阅。 */
import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import * as ipc from './ipc';

export type WorkbenchMode = 'chat' | 'work';

/** 解析生效模式:会话覆盖(chat / work)优先,inherit 跟随全局默认。 */
export function resolveMode(sessionMode: string | undefined, def: WorkbenchMode): WorkbenchMode {
  return sessionMode === 'chat' || sessionMode === 'work' ? sessionMode : def;
}

/** 读取全局默认模式并随 settings_changed 实时更新;未加载完成返回 null。 */
export function useDefaultWorkbench(): WorkbenchMode | null {
  const [mode, setMode] = useState<WorkbenchMode | null>(null);
  useEffect(() => {
    let alive = true;
    const apply = (s: ipc.AppSettings) => {
      if (alive) setMode(s.default_workbench === 'work' ? 'work' : 'chat');
    };
    // 设置读取失败时回落到 chat:否则 mode 永远为 null,路由门会一直渲染空白页
    ipc
      .getSettings()
      .then(apply)
      .catch(() => {
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
