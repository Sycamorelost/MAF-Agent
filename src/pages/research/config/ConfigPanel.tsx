import { useEffect, useMemo, useState } from 'react';
import { useResearchStore } from '../../../stores/research';
import { useProvidersStore } from '../../../stores/providers';
import { TutorialList } from '../../../components/TutorialCard';
import { ResearchTemplateDialog } from '../../../components/ResearchTemplateDialog';
import AsideDrawer from '../../../components/AsideDrawer';
import * as ipc from '../../../lib/ipc';
import { IconResearch } from '../../../components/icons';
import { BasicForm } from './BasicForm';
import { AdvancedForm } from './AdvancedForm';
import { TemplatePicker } from './TemplatePicker';
import { EnhanceSection } from './EnhanceSection';
import { ConfigFooter } from './ConfigFooter';

export function ConfigPanel({
  onOpenLibrary,
  namingSignal,
  initialTopic,
}: {
  onOpenLibrary: () => void;
  namingSignal: number;
  /** 外部预填主题(学习规划「深挖研究」跳转时带入),仅在首次挂载生效 */
  initialTopic?: string;
}) {
  const {
    searchCfg, loadSearchCfg, start, blueprints, loadBlueprints,
    saveBlueprint, pendingBlueprint, consumeBlueprint, templates, loadTemplates,
  } = useResearchStore();
  const current = useProvidersStore((s) => s.current);
  const templatesProviders = useProvidersStore((s) => s.templates);
  const hasKey = useProvidersStore((s) => s.hasKey);

  const [topic, setTopic] = useState(initialTopic ?? '');
  const [params, setParams] = useState<ipc.ResearchParams>({ ...ipc.DEFAULT_RESEARCH_PARAMS });
  const [isCustom, setIsCustom] = useState(false);
  const [modelSel, setModelSel] = useState(''); // '' = 跟随当前
  const [whitelistText, setWhitelistText] = useState('');
  const [bpNaming, setBpNaming] = useState(false);
  const [bpError, setBpError] = useState<string | null>(null);
  /** 当前配置来源方案名(null = 自定义配置) */
  const [blueprintName, setBlueprintName] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [tplDlg, setTplDlg] = useState<string | null>(null);

  useEffect(() => {
    loadSearchCfg();
    loadBlueprints();
    loadTemplates();
  }, [loadSearchCfg, loadBlueprints, loadTemplates]);

  // 方案库「应用 / 从当前配置另存」信号:打开命名 / 应用暂存方案
  useEffect(() => {
    if (namingSignal > 0) setBpNaming(true);
  }, [namingSignal]);

  useEffect(() => {
    if (!pendingBlueprint) return;
    const bp = consumeBlueprint();
    if (bp) applyBlueprintToPanel(bp);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pendingBlueprint]);

  const patch = (p: Partial<ipc.ResearchParams>) => {
    setBlueprintName(null); // 手动调整后配置与方案脱钩,变为自定义配置
    setParams((s) => ({ ...s, ...p }));
  };

  const engineReady = !!searchCfg &&
    (searchCfg.engine === 'searxng' ? !!searchCfg.base_url : searchCfg.has_api_key);

  // 科研增强:草稿生图候选(已配 Key 且提供生图模型的服务商)
  const imageProviders = useMemo(
    () => templatesProviders.filter((t) => t.image_models.length > 0 && hasKey[t.id]),
    [templatesProviders, hasKey],
  );

  const applyBlueprintToPanel = (bp: ipc.Blueprint) => {
    setParams({ ...ipc.DEFAULT_RESEARCH_PARAMS, ...bp.params });
    setIsCustom(![4, 8, 15].includes(bp.params.max_rounds));
    setWhitelistText(bp.params.whitelist_domains.join(', '));
    setBlueprintName(bp.name);
    setBpNaming(false);
    setBpError(null);
  };

  const parseDomains = (text: string) =>
    text.split(/[,,\s]+/).map((s) => s.trim()).filter(Boolean);

  const whitelistDomains = parseDomains(whitelistText);

  const onStart = async () => {
    if (!topic.trim() || starting) return;
    setStarting(true);
    setError(null);
    try {
      const finalParams = { ...params, whitelist_domains: whitelistDomains };
      let override: { providerId: string; model: string } | null = null;
      if (modelSel) {
        const [providerId, ...rest] = modelSel.split('::');
        override = { providerId, model: rest.join('::') };
      }
      await start(topic.trim(), finalParams, override, blueprintName);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setStarting(false);
    }
  };

  return (
    <>
      <AsideDrawer>
        <aside className="pg-aside rp-config drawer">
          <div className="pg-aside-head">
            <h2>
              <IconResearch size={18} />
              研究配置
            </h2>
            <p>点标题栏「面板」可开合本栏;所有参数可调,可保存为预设复用。</p>
          </div>

          <BasicForm
            params={params}
            topic={topic}
            isCustom={isCustom}
            whitelistText={whitelistText}
            modelSel={modelSel}
            templatesProviders={templatesProviders}
            hasKey={hasKey}
            current={current}
            onPatch={patch}
            onIsCustomChange={setIsCustom}
            onTopicChange={setTopic}
            onWhitelistTextChange={setWhitelistText}
            onModelSelChange={setModelSel}
          />

          <AdvancedForm
            params={params}
            templates={templates}
            searchCfg={searchCfg}
            engineReady={engineReady}
            onPatch={patch}
            onOpenTpl={setTplDlg}
          />

          <TemplatePicker
            mode={params.mode}
            freeStructure={params.free_structure}
            templates={templates}
            blueprints={blueprints}
            blueprintName={blueprintName}
            naming={bpNaming}
            params={params}
            whitelistDomains={whitelistDomains}
            saveBlueprint={saveBlueprint}
            onPatch={patch}
            onOpenTpl={setTplDlg}
            onNamingChange={setBpNaming}
            bpError={bpError}
            onErrorChange={setBpError}
            onApplyBlueprint={applyBlueprintToPanel}
            onBlueprintSaved={setBlueprintName}
            onOpenLibrary={onOpenLibrary}
          />

          <EnhanceSection
            params={params}
            imageProviders={imageProviders}
            onPatch={patch}
          />

          <ConfigFooter
            topic={topic}
            params={params}
            blueprintName={blueprintName}
            starting={starting}
            error={error}
            engineReady={engineReady}
            whitelistDomains={whitelistDomains}
            onNaming={() => setBpNaming(true)}
            onStart={onStart}
          />

          <TutorialList page="research" />
        </aside>
      </AsideDrawer>
      {tplDlg && <ResearchTemplateDialog tplKey={tplDlg} onClose={() => setTplDlg(null)} />}
    </>
  );
}
