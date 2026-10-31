/**
 * 学习中心页面骨架:板块切换(学习计划/试卷/试题库)+ 左抽屉面板 + 详情视图挂载。
 *
 * 来源声明 / Attribution:
 *   Interaction flow derived from Deep-tutor — https://github.com/hershate/Deep-tutor
 *   Copyright (c) hershate and contributors
 *   Licensed under the Apache License, Version 2.0 (see LICENSE / NOTICE).
 *   Re-implemented with myAgent's component system; no upstream CSS/assets copied.
 */
import { useEffect, useRef } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { useStudyStore } from '../../stores/study';
import { PlanList } from './PlanList';
import { PlanPanel } from './PlanPanel';
import { PlanView } from './PlanView';
import { PapersView } from './PapersView';
import { BankView } from './BankView';

/** 学习中心三板块:学习计划 / 试卷 / 试题库 */
type Tab = 'plans' | 'papers' | 'bank';

export default function StudyPage() {
  const active = useStudyStore((s) => s.active);
  const refreshList = useStudyStore((s) => s.refreshList);
  const listTimer = useRef<number | undefined>(undefined);
  const loc = useLocation();
  const nav = useNavigate();

  // 板块由地址栏 ?tab 决定:从整卷页返回时能直接回到试卷板块
  const rawTab = new URLSearchParams(loc.search).get('tab');
  const tab: Tab = rawTab === 'papers' || rawTab === 'bank' ? rawTab : 'plans';
  const setTab = (t: Tab) => nav(t === 'plans' ? '/plan' : `/plan?tab=${t}`);

  // 列表有计划在生成中时轮询刷新(打开详情走 openPlan,这里只刷列表行)
  useEffect(() => {
    void refreshList();
    if (active?.status === 'generating') {
      listTimer.current = window.setInterval(() => void refreshList(), 3000);
      return () => window.clearInterval(listTimer.current);
    }
  }, [refreshList, active?.status]);

  return (
    <div className="pg">
      {tab === 'plans' && !active && <PlanPanel />}
      <main className="pg-main">
        {/* 计划详情打开时隐藏板块切换,避免与详情页「返回」重复 */}
        {!active && (
          <div className="pg-main-head st-tabs-head">
            <div className="pg-seg st-tabs-seg">
              <button className={tab === 'plans' ? 'on' : ''} onClick={() => setTab('plans')}>
                学习计划
              </button>
              <button className={tab === 'papers' ? 'on' : ''} onClick={() => setTab('papers')}>
                试卷
              </button>
              <button className={tab === 'bank' ? 'on' : ''} onClick={() => setTab('bank')}>
                试题库
              </button>
            </div>
          </div>
        )}

        {tab === 'plans' ? (
          active ? (
            <PlanView key={active.id || 'draft'} />
          ) : (
            <PlanList />
          )
        ) : tab === 'papers' ? (
          <PapersView
            onOpenPaper={(id) => nav(`/plan/paper/${id}`)}
            onGoBank={() => setTab('bank')}
          />
        ) : (
          <BankView onPaperCreated={(id) => nav(`/plan/paper/${id}`)} />
        )}
      </main>
    </div>
  );
}
