import { useMemo } from 'react';
import type { MemoryEntityInfo, MemoryRelationInfo } from '../../lib/ipc/mcp';
import {
  buildRelationsOf,
  matchesEntity,
  normalizeQuery,
  typeColor,
} from '../../lib/longTermMemory';

interface Props {
  entities: MemoryEntityInfo[];
  relations: MemoryRelationInfo[];
  query: string;
  onQueryChange: (value: string) => void;
}

/** 实体明细卡:搜索 + 可展开观测与关系。 */
export function EntityDetailsCard({ entities, relations, query, onQueryChange }: Props) {
  const q = normalizeQuery(query);

  const filteredEntities = useMemo(
    () => (q ? entities.filter((e) => matchesEntity(e, q)) : entities),
    [entities, q],
  );

  const relationsOf = useMemo(() => buildRelationsOf(relations), [relations]);

  return (
    <div className="set-card">
      <div className="set-card-h">
        实体明细
        <span className="st ltm-st">
          {q ? `${filteredEntities.length} / ${entities.length}` : `${entities.length} 条`}
        </span>
      </div>
      <div className="set-card-b">
        <input
          className="pg-field ltm-search"
          type="search"
          placeholder="搜索实体名称、类型或观测内容…"
          value={query}
          onChange={(e) => onQueryChange(e.target.value)}
        />
        <div className="ltm-entities">
          {filteredEntities.length === 0 && <div className="set-hint">没有匹配的实体</div>}
          {filteredEntities.map((e) => {
            const rels = relationsOf.get(e.name) ?? [];
            return (
              <details key={e.name} className="ltm-entity">
                <summary>
                  <span className="ltm-dot" style={{ background: typeColor(e.entity_type) }} />
                  <span className="ltm-entity-name">{e.name}</span>
                  <span className="ltm-entity-type">{e.entity_type}</span>
                  <span className="ltm-entity-count">
                    {e.observations.length} 条观测 · {rels.length} 条关系
                  </span>
                </summary>
                <div className="ltm-entity-body">
                  {e.observations.length > 0 && (
                    <ul className="ltm-observations">
                      {e.observations.map((o, i) => (
                        <li key={`${o}-${i}`}>{o}</li>
                      ))}
                    </ul>
                  )}
                  {rels.length > 0 && (
                    <div className="ltm-relations">
                      {rels.map((r, i) => (
                        <span key={`${r}-${i}`} className="ltm-relation-chip">
                          {r}
                        </span>
                      ))}
                    </div>
                  )}
                  {e.observations.length === 0 && rels.length === 0 && (
                    <span className="set-hint">该实体暂无观测与关系</span>
                  )}
                </div>
              </details>
            );
          })}
        </div>
      </div>
    </div>
  );
}
