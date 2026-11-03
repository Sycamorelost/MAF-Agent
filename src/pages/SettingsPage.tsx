import { useEffect, useState, type ReactElement } from 'react';
import { useSearchParams } from 'react-router-dom';
import { getVersion } from '@tauri-apps/api/app';
import AsideDrawer from '../components/AsideDrawer';

import { NAV } from './settings/constants';
import { useUsageData } from './settings/useUsageData';
import { useProviderAdmin } from './settings/useProviderAdmin';
import { useGeneralSettings } from './settings/useGeneralSettings';
import { UsageView } from './settings/UsageView';
import { ProvidersView } from './settings/ProvidersView';
import { MemoryView } from './settings/MemoryView';
import { SearchView } from './settings/SearchView';
import { TranslateView } from './settings/TranslateView';
import { StorageView } from './settings/StorageView';
import { AboutView } from './settings/AboutView';
import { NavMenuView } from './settings/NavMenuView';
import { AppearanceView } from './settings/AppearanceView';
import { PresetsView } from './settings/PresetsView';
import { McpView } from './settings/McpView';
import { AgentsView } from './settings/AgentsView';

export default function SettingsPage() {
  const [searchParams] = useSearchParams();
  const initialNav = searchParams.get('tab') ?? 'providers';
  const [nav, setNav] = useState(initialNav);
  // 关于页版本号:运行期读 tauri.conf.json(单一事实源,避免多处硬编码漂移)
  const [appVersion, setAppVersion] = useState('');
  useEffect(() => {
    getVersion()
      .then(setAppVersion)
      .catch(() => setAppVersion(''));
  }, []);

  // 已在设置页时,再次点击「设置」通过 query 切分区
  useEffect(() => {
    const tab = searchParams.get('tab');
    if (tab) setNav(tab);
  }, [searchParams]);

  const {
    templates,
    storedIds,
    keyInputs,
    setKeyInputs,
    savingKey,
    rotation,
    slotKeys,
    savingTpl,
    tplMsg,
    addingTpl,
    setAddingTpl,
    newTpl,
    setNewTpl,
    modelConfig,
    cards,
    configMsg,
    fetching,
    fetchMsg,
    keyedTemplates,
    onSaveKey,
    onPatchSlot,
    onDeleteSlot,
    onPatchRotation,
    onClearKey,
    reloadCards,
    onFetchModels,
    onPatchOverride,
    onEditTemplate,
    onSetModels,
    onAddTemplate,
    onDeleteTemplate,
    onRestoreBuiltins,
  } = useProviderAdmin();

  const {
    settings,
    savingSettings,
    settingsSaved,
    dataDir,
    dirMsg,
    dirBusy,
    dirRestarting,
    searchCfg,
    searchDraft,
    savingSearch,
    searchTestStatus,
    searchTesting,
    glossary,
    glossaryDirty,
    glossaryMsg,
    savingGlossary,
    importRef,
    engineLabel,
    engineReady,
    mcpServerNames,
    hiddenNav,
    navOrder,
    onSaveSettings,
    onChangeDataDir,
    onOpenDataDir,
    onSaveSearch,
    onTestSearch,
    patchSearchDraft,
    mutateGlossary,
    onSaveGlossary,
    onImportGlossary,
    onImportPresetGlossary,
    patchSettings,
    saveSettingsPatch,
    toggleNavItem,
    reorderNavItem,
    toggleMinimizeToTray,
    setDefaultPreset,
  } = useGeneralSettings();

  const providersView = (
    <ProvidersView
      templates={templates}
      storedIds={storedIds}
      keyInputs={keyInputs}
      onKeyInput={(id, v) => setKeyInputs((m) => ({ ...m, [id]: v }))}
      savingKey={savingKey}
      modelConfig={modelConfig}
      cards={cards}
      configMsg={configMsg}
      tplMsg={tplMsg}
      fetching={fetching}
      fetchMsg={fetchMsg}
      savingTpl={savingTpl}
      rotation={rotation}
      slotKeys={slotKeys}
      addingTpl={addingTpl}
      onAddingTplChange={setAddingTpl}
      newTpl={newTpl}
      onNewTplChange={setNewTpl}
      onRestoreBuiltins={onRestoreBuiltins}
      onClearKey={onClearKey}
      onPatchOverride={onPatchOverride}
      onFetchModels={onFetchModels}
      onEditTemplate={onEditTemplate}
      onSetModels={onSetModels}
      onDeleteTemplate={onDeleteTemplate}
      onSaveKey={onSaveKey}
      onPatchSlot={onPatchSlot}
      onDeleteSlot={onDeleteSlot}
      onPatchRotation={onPatchRotation}
      reloadCards={reloadCards}
      onAddTemplate={onAddTemplate}
    />
  );

  const memoryView = (
    <MemoryView
      settings={settings}
      keyedTemplates={keyedTemplates}
      settingsSaved={settingsSaved}
      savingSettings={savingSettings}
      onPatchSettings={patchSettings}
      onSaveSettings={onSaveSettings}
    />
  );

  const searchView = (
    <SearchView
      searchCfg={searchCfg}
      searchDraft={searchDraft}
      engineLabel={engineLabel}
      engineReady={engineReady}
      savingSearch={savingSearch}
      searchTestStatus={searchTestStatus}
      searchTesting={searchTesting}
      mcpServerNames={mcpServerNames}
      onDraftChange={patchSearchDraft}
      onSaveSearch={onSaveSearch}
      onTestSearch={onTestSearch}
    />
  );

  const translateView = (
    <TranslateView
      glossary={glossary}
      glossaryDirty={glossaryDirty}
      glossaryMsg={glossaryMsg}
      savingGlossary={savingGlossary}
      importRef={importRef}
      onMutateGlossary={mutateGlossary}
      onSaveGlossary={onSaveGlossary}
      onImportGlossary={onImportGlossary}
      onImportPresetGlossary={onImportPresetGlossary}
    />
  );

  const storageView = (
    <StorageView
      dataDir={dataDir}
      dirMsg={dirMsg}
      dirBusy={dirBusy}
      dirRestarting={dirRestarting}
      onChangeDataDir={onChangeDataDir}
      onOpenDataDir={onOpenDataDir}
      settings={settings}
      onSaveAttachmentSettings={saveSettingsPatch}
    />
  );

  const aboutView = <AboutView appVersion={appVersion} />;

  const navmenuView = (
    <NavMenuView
      hiddenNav={hiddenNav}
      navOrder={navOrder}
      minimizeToTray={settings?.minimize_to_tray ?? false}
      onToggleNavItem={toggleNavItem}
      onReorderNavItem={reorderNavItem}
      onToggleMinimizeToTray={toggleMinimizeToTray}
    />
  );

  // ---- 用量统计(视图与数据源见 settings/UsageView.tsx)----

  const usageData = useUsageData(nav === 'usage');
  const usageView = <UsageView data={usageData} />;
  const viewByNav: Record<string, ReactElement> = {
    providers: providersView,
    memory: memoryView,
    search: searchView,
    translate: translateView,
    appearance: <AppearanceView />,
    navmenu: navmenuView,
    presets: (
      <PresetsView
        current={settings?.default_preset ?? null}
        onSelect={setDefaultPreset}
        settings={settings}
        onSave={saveSettingsPatch}
      />
    ),
    usage: usageView,
    storage: storageView,
    mcp: <McpView />,
    agents: <AgentsView />,
    about: aboutView,
  };

  return (
    <div className="pg pg-settings">
      {/* 左:设置分区导航(玻璃抽屉,标题栏「面板」按钮释放) */}
      <AsideDrawer>
      <nav className="set-nav drawer">
        <div className="drawer-hint">点标题栏「面板」可开合本栏</div>
        {NAV.map((g) => (
          <div key={g.grp}>
            <div className="grp">{g.grp}</div>
            {g.items.map((it) => {
              const Icon = it.icon;
              return (
                <div
                  key={it.id}
                  className={`it${nav === it.id ? ' on' : ''}`}
                  onClick={() => setNav(it.id)}
                >
                  <Icon size={15} />
                  {it.label}
                </div>
              );
            })}
          </div>
        ))}
      </nav>
      </AsideDrawer>

      <div className="set-content">{viewByNav[nav]}</div>
    </div>
  );
}
