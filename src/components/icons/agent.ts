import { IconSparkles, IconLanguages, IconClipboardList, IconFileText, IconCompare, IconPullRequest, IconBug, IconWand, IconShield, IconArchive, IconNote, IconBot, IconImage, IconTranslate, IconAttach } from './glyphs';
import { IconRefresh, IconGlobe, IconCheck, IconEdit, IconDoc } from './actions';
import { IconChat, IconBrain, IconSearch, IconSettings, IconResearch } from './nav';

/** Agent 模板 icon key → 组件(未知 key 回退 IconBot)。 */
export function agentIconComponent(key: string) {
  switch (key) {
    case 'sparkles':
      return IconSparkles;
    case 'languages':
      return IconLanguages;
    case 'clipboard-list':
      return IconClipboardList;
    case 'file-text':
      return IconFileText;
    case 'git-compare':
      return IconCompare;
    case 'git-pull-request':
      return IconPullRequest;
    case 'refresh-cw':
      return IconRefresh;
    case 'bug':
      return IconBug;
    case 'search':
      return IconSearch;
    case 'wand-sparkles':
      return IconWand;
    case 'shield-check':
      return IconShield;
    case 'archive':
      return IconArchive;
    case 'chat':
      return IconChat;
    case 'note':
      return IconNote;
    case 'brain':
      return IconBrain;
    case 'globe':
      return IconGlobe;
    case 'settings':
      return IconSettings;
    case 'image':
      return IconImage;
    case 'translate':
      return IconTranslate;
    case 'research':
      return IconResearch;
    case 'attach':
      return IconAttach;
    case 'doc':
      return IconDoc;
    case 'check':
      return IconCheck;
    case 'edit':
      return IconEdit;
    default:
      return IconBot;
  }
}
