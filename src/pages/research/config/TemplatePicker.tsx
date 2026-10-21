import * as ipc from '../../../lib/ipc';
import { IconCheck, IconDoc, IconEdit, IconPlus } from '../../../components/icons';
import { useState } from 'react';

/** 提示词模板 + 研究方案(与方案库联动)。 */
export function TemplatePicker({
  mode,
  freeStructure,
  templates,
  blueprints,
  blueprintName,
  naming,
  bpError,
  params,
  whitelistDomains,
  saveBlueprint,
  onPatch,
  onOpenTpl,
  onNamingChange,
  onErrorChange,
  onApplyBlueprint,
  onBlueprintSaved,
  onOpenLibrary,
}: {
  mode: string;
  freeStructure: boolean;
  templates: ipc.ResearchTemplateDto[];
  blueprints: ipc.Blueprint[];
  blueprintName: string | null;
  naming: boolean;
  bpError: string | null;
  params: ipc.ResearchParams;
  whitelistDomains: string[];
  saveBlueprint: (name: string, desc: string, params: ipc.ResearchParams) => Promise<void>;
  onPatch: (p: Partial<ipc.ResearchParams>) => void;
  onOpenTpl: (key: string) => void;
  onNamingChange: (v: boolean) => void;
  onErrorChange: (v: string | null) => void;
  onApplyBlueprint: (bp: ipc.Blueprint) => void;
  onBlueprintSaved: (name: string) => void;
  onOpenLibrary: () => void;
}) {
  const [bpName, setBpName] = useState('');

  const onSaveBlueprint = async () => {
    const name = bpName.trim();
    if (!name) return;
    onErrorChange(null);
    try {
      await saveBlueprint(name, '', {
        ...params,
        whitelist_domains: whitelistDomains,
      });
      onBlueprintSaved(name);
      setBpName('');
      onNamingChange(false);
    } catch (e) {
      onErrorChange(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <>
      {(mode === 'legacy' || mode === 'concise') && (
        <div className="pg-sec">
          <div className="pg-lab">提示词模板</div>
          <div className="pg-chips">
            <button
              className={`pg-chip${templates.some((t) => t.key === 'decide' && !t.builtin) ? ' on' : ''}`}
              onClick={() => onOpenTpl('decide')}
              title="全局决策提示:ReAct 循环中模型决策所用的系统提示"
            >
              <IconEdit size={13} />
              全局决策提示
            </button>
            <button
              className={`pg-chip${templates.some((t) => t.key === 'report' && !t.builtin) ? ' on' : ''}`}
              onClick={() => onOpenTpl('report')}
              title="终稿报告模板:研究报告的章节结构与撰写规则"
            >
              <IconDoc size={13} />
              终稿模板
            </button>
            <button
              className={`pg-chip${freeStructure ? ' on' : ''}`}
              onClick={() => onPatch({ free_structure: !freeStructure })}
              title="不使用报告模板,AI 根据主题与资料自行决定报告章节结构"
            >
              <IconEdit size={13} />
              自由结构
            </button>
          </div>
          <div className="pg-hint">
            {freeStructure
              ? '已启用自由结构:忽略终稿模板,由 AI 自行决定报告章节结构。'
              : '高亮表示已自定义,对全部研究生效;点「恢复默认」回到内置模板。'}
          </div>
        </div>
      )}

      <div className="pg-sec">
        <div className="pg-lab">
          研究方案
          <span className="sp" />
          <span className="n">{blueprintName ?? '自定义配置'}</span>
        </div>
        <div className="pg-chips">
          {blueprints.slice(0, 5).map((bp) => (
            <span
              key={bp.name}
              className={`pg-chip${blueprintName === bp.name ? ' on' : ''}`}
              onClick={() => {
                onApplyBlueprint(bp);
                onErrorChange(null);
                onNamingChange(false);
              }}
              title={bp.desc || bp.name}
            >
              {bp.name}
            </span>
          ))}
          {!naming && (
            <button
              className="pg-chip"
              onClick={() => onNamingChange(true)}
              title="将当前配置保存为方案"
            >
              <IconPlus size={13} />
              存为方案
            </button>
          )}
          <button className="pg-chip" onClick={onOpenLibrary} title="打开方案库:全部方案 / 编辑 / 删除">
            <IconDoc size={13} />
            方案库
          </button>
        </div>
        {naming && (
          <div className="pg-pair" style={{ marginTop: 'var(--space-2)' }}>
            <input
              className="pg-field"
              style={{ flex: 1 }}
              placeholder="方案名称,如:竞品周报"
              value={bpName}
              autoFocus
              onChange={(e) => setBpName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') void onSaveBlueprint();
                if (e.key === 'Escape') onNamingChange(false);
              }}
            />
            <button className="btn-ghost" onClick={() => void onSaveBlueprint()} disabled={!bpName.trim()}>
              <IconCheck size={14} />
            </button>
          </div>
        )}
        {bpError && <div className="pg-hint" style={{ color: 'var(--err)' }}>{bpError}</div>}
        <div className="pg-hint">方案 = 参数 + 阶段编排 + 提示词 + 约束,一体保存复用;点方案名一键应用。</div>
      </div>
    </>
  );
}
