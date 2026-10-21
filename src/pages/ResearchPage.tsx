import { useEffect, useState } from 'react';
import { useResearchStore } from '../stores/research';
import { ConfigPanel } from './research/ConfigPanel';
import { RunView } from './research/RunView';
import { BlueprintLibrary } from './research/BlueprintLibrary';
import { HistoryView } from './research/HistoryView';

/** 学习规划「深挖研究」的草稿接手约定(见 StudyPage.deepDig) */
const RESEARCH_DRAFT_KEY = 'metaagent.research.draft';

interface ResearchDraft {
  topic?: string;
  openResearch?: string;
}

export default function ResearchPage() {
  const active = useResearchStore((s) => s.active);
  /** 主区视图:history=研究历史 / library=方案库 */
  const [mainView, setMainView] = useState<'history' | 'library'>('history');
  /** 方案库「从当前配置另存」信号:递增触发配置面板命名框 */
  const [namingSignal, setNamingSignal] = useState(0);
  /** 学习规划跳转带来的草稿(预填主题 / 直开某次研究),读取一次即清 */
  const [draft] = useState<ResearchDraft | null>(() => {
    try {
      const raw = sessionStorage.getItem(RESEARCH_DRAFT_KEY);
      if (raw) {
        sessionStorage.removeItem(RESEARCH_DRAFT_KEY);
        return JSON.parse(raw) as ResearchDraft;
      }
    } catch {
      /* 忽略坏数据 */
    }
    return null;
  });

  // 草稿带研究 id:进入页面直接打开该研究详情
  useEffect(() => {
    if (draft?.openResearch) {
      void useResearchStore.getState().openDetail(draft.openResearch);
    }
  }, [draft]);

  const openLibrary = () => setMainView('library');
  const closeLibrary = () => setMainView('history');

  return (
    <div className="pg">
      <ConfigPanel
        onOpenLibrary={openLibrary}
        namingSignal={namingSignal}
        initialTopic={draft?.topic}
      />
      <main className="pg-main">
        {active ? (
          <RunView key={active.id} active={active} />
        ) : mainView === 'library' ? (
          <BlueprintLibrary
            onBack={closeLibrary}
            onApplied={closeLibrary}
            onSaveCurrent={() => {
              setMainView('history');
              setNamingSignal((n) => n + 1);
            }}
          />
        ) : (
          <HistoryView onOpenLibrary={openLibrary} />
        )}
      </main>
    </div>
  );
}
