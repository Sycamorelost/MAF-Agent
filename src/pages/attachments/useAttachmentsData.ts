//! 附件中心数据层:附件列表、会话配额、拒绝记录与图片缩略缓存。
//! 页面只负责筛选与渲染,数据加载 / 刷新集中在此,避免主组件过长。

import { useEffect, useMemo, useRef, useState } from 'react';
import {
  clearAttachmentRejects,
  getSettings,
  listAllAttachments,
  listAttachmentRejects,
  readAttachmentData,
  type AttachmentEntry,
  type AttachmentRejectList,
} from '../../lib/ipc';
import {
  DEFAULT_SESSION_MAX_FILES,
  DEFAULT_SESSION_MAX_MB,
  clampSessionMaxFiles,
  clampSessionMaxMb,
} from '../../lib/attachmentPolicy';
import { usageBySession } from '../../lib/attachmentQuota';

export interface QuotaConfig {
  maxFiles: number;
  maxMb: number;
}

export function useAttachmentsData(flash: (text: string) => void) {
  const [entries, setEntries] = useState<AttachmentEntry[] | null>(null);
  const [quota, setQuota] = useState<QuotaConfig>({
    maxFiles: DEFAULT_SESSION_MAX_FILES,
    maxMb: DEFAULT_SESSION_MAX_MB,
  });
  const [imgUrls, setImgUrls] = useState<Record<string, string>>({});
  const [rejects, setRejects] = useState<AttachmentRejectList | null>(null);
  const [rejectsLoading, setRejectsLoading] = useState(false);
  const [clearingRejects, setClearingRejects] = useState(false);
  const requestedRef = useRef<Set<string>>(new Set());
  /** flash 每次渲染都是新函数,用 ref 保持稳定引用避免重复加载 */
  const flashRef = useRef(flash);
  flashRef.current = flash;

  /** 拒绝记录(列表 + 头部角标);失败不打扰主流程。 */
  const refreshRejects = async () => {
    setRejectsLoading(true);
    try {
      setRejects(await listAttachmentRejects(200));
    } catch {
      /* 拒绝记录加载失败不阻断附件浏览 */
    } finally {
      setRejectsLoading(false);
    }
  };

  const clearRejects = async () => {
    setClearingRejects(true);
    try {
      await clearAttachmentRejects();
      setRejects({ total: 0, items: [] });
    } catch (err) {
      flashRef.current('清空拒绝记录失败: ' + String(err));
    } finally {
      setClearingRejects(false);
    }
  };

  useEffect(() => {
    // 配额与拒绝记录:各自独立,任一失败不影响另一项
    getSettings()
      .then((s) =>
        setQuota({
          maxFiles: clampSessionMaxFiles(s.attachment_session_max_files),
          maxMb: clampSessionMaxMb(s.attachment_session_max_mb),
        }),
      )
      .catch(() => {});
    void refreshRejects();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    listAllAttachments()
      .then(setEntries)
      .catch((e) => {
        setEntries([]);
        flashRef.current('加载失败: ' + String(e));
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 图片缩略:逐个读 dataURL(已请求过的跳过,失败保持图标)
  useEffect(() => {
    if (!entries) return;
    for (const e of entries) {
      if (e.meta.kind !== 'image' || requestedRef.current.has(e.meta.path)) continue;
      requestedRef.current.add(e.meta.path);
      readAttachmentData(e.meta.path)
        .then((url) => setImgUrls((prev) => ({ ...prev, [e.meta.path]: url })))
        .catch(() => {});
    }
  }, [entries]);

  /** 配额口径基于全部条目(搜索/筛选不影响用量)。 */
  const usageMap = useMemo(() => usageBySession(entries ?? []), [entries]);

  return {
    entries,
    setEntries,
    quota,
    imgUrls,
    usageMap,
    rejects,
    rejectsLoading,
    clearingRejects,
    refreshRejects,
    clearRejects,
  };
}
