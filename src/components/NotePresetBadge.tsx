/**
 * 知识库「预置」角标与标签展示辅助(预置知识库迭代)。
 * 预置笔记由后端 notes_presets 首次启动落盘,id 统一 `preset-` 前缀;
 * 用户可编辑/删除,删除后不再出现,角标随之消失。
 */

/** 预置笔记判定:id 以 preset- 开头。 */
export const isPresetNote = (id: string) => id.startsWith('preset-');

/** 列表/卡片展示用标签:预置笔记已用角标表达「预置」,标签行不再重复该词。 */
export const displayTags = (id: string, tags: string[]) =>
  isPresetNote(id) ? tags.filter((t) => t !== '预置') : tags;

/** 「预置」角标(仅预置笔记渲染,普通笔记返回 null)。 */
export default function NotePresetBadge({ id }: { id: string }) {
  if (!isPresetNote(id)) return null;
  return <span className="preset-badge">预置</span>;
}
