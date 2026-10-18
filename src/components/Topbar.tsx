import { useNavigate } from 'react-router-dom';
import { useProvidersStore } from '../stores/providers';
import { useSessionsStore } from '../stores/sessions';
import GlobalSearch from './GlobalSearch';

/** 顶栏(简约排版):左搜索,右新建对话;模型选择收口到会话页标题右侧。 */
export default function Topbar() {
  const current = useProvidersStore((s) => s.current);
  const create = useSessionsStore((s) => s.create);
  const nav = useNavigate();

  const onNew = async () => {
    const title = `新会话 ${new Date().toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}`;
    const id = await create(title, current?.model ?? 'gpt-5.2');
    nav(`/chat/${id}`);
  };

  return (
    <div className="topbar">
      <GlobalSearch />
      <div className="top-actions">
        <button className="btn-primary" onClick={onNew}>
          新建对话
        </button>
      </div>
    </div>
  );
}
