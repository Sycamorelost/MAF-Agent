import { describe, expect, it } from 'vitest';
import type { MemoryEntityInfo, MemoryRelationInfo } from './ipc/mcp';
import {
  MAX_GRAPH_NODES,
  buildGraphLayout,
  buildRelationsOf,
  buildTypeStats,
  fmtBytes,
  matchesEntity,
  nameMatches,
  normalizeQuery,
  truncateLabel,
  typeColor,
} from './longTermMemory';

const entity = (
  name: string,
  entity_type: string,
  observations: string[] = [],
): MemoryEntityInfo => ({ name, entity_type, observations });

const relation = (from: string, to: string, relation_type = 'uses'): MemoryRelationInfo => ({
  from,
  to,
  relation_type,
});

describe('typeColor', () => {
  it('同一类型返回同一颜色', () => {
    expect(typeColor('person')).toBe(typeColor('person'));
  });

  it('不同类型通常不同颜色(哈希分布)', () => {
    expect(typeColor('person')).not.toBe(typeColor('language'));
  });
});

describe('fmtBytes / truncateLabel', () => {
  it('字节单位换算', () => {
    expect(fmtBytes(0)).toBe('0 B');
    expect(fmtBytes(512)).toBe('512 B');
    expect(fmtBytes(2048)).toBe('2.0 KB');
    expect(fmtBytes(3 * 1024 * 1024)).toBe('3.0 MB');
  });

  it('超长名称截断并加省略号', () => {
    expect(truncateLabel('short', 8)).toBe('short');
    expect(truncateLabel('abcdefghij', 8)).toBe('abcdefg…');
  });
});

describe('buildTypeStats', () => {
  it('按类型聚合数量与观测数,并按数量降序', () => {
    const stats = buildTypeStats([
      entity('A', 'person', ['x']),
      entity('B', 'person', ['y', 'z']),
      entity('C', 'language', ['fast']),
    ]);
    expect(stats).toEqual([
      { type: 'person', count: 2, observations: 3 },
      { type: 'language', count: 1, observations: 1 },
    ]);
  });

  it('空实体返回空统计', () => {
    expect(buildTypeStats([])).toEqual([]);
  });
});

describe('buildGraphLayout', () => {
  it('节点按类型聚集、观测数降序,并过滤悬空边', () => {
    const layout = buildGraphLayout(
      [entity('A', 'person', ['1']), entity('B', 'person', ['1', '2']), entity('C', 'language')],
      [relation('A', 'B'), relation('A', 'C'), relation('A', 'missing')],
    );
    expect(layout.nodes.map((n) => n.name)).toEqual(['B', 'A', 'C']);
    expect(layout.edges).toHaveLength(2);
    expect(layout.truncated).toBe(false);
    expect(layout.total).toBe(3);
  });

  it('超过上限只保留前 N 个节点', () => {
    const entities = Array.from({ length: MAX_GRAPH_NODES + 10 }, (_, i) =>
      entity(`e${i}`, 'item', [`obs${i}`]),
    );
    const layout = buildGraphLayout(entities, []);
    expect(layout.nodes).toHaveLength(MAX_GRAPH_NODES);
    expect(layout.truncated).toBe(true);
    expect(layout.total).toBe(MAX_GRAPH_NODES + 10);
  });

  it('单节点居中且半径为最小值 + 观测数', () => {
    const layout = buildGraphLayout([entity('solo', 'item', ['a', 'b'])], []);
    expect(layout.nodes[0]).toMatchObject({ x: 320, y: 190, r: 7 });
  });

  it('空图谱不产生节点与边', () => {
    const layout = buildGraphLayout([], []);
    expect(layout.nodes).toEqual([]);
    expect(layout.edges).toEqual([]);
  });
});

describe('buildRelationsOf', () => {
  it('为两端都生成方向化描述', () => {
    const map = buildRelationsOf([relation('Alice', 'Rust', 'uses')]);
    expect(map.get('Alice')).toEqual(['→ Rust(uses)']);
    expect(map.get('Rust')).toEqual(['← Alice(uses)']);
  });
});

describe('搜索匹配', () => {
  it('归一化后大小写不敏感', () => {
    expect(normalizeQuery('  RUST  ')).toBe('rust');
  });

  it('命中名称 / 类型 / 观测内容', () => {
    const e = entity('Rust', 'language', ['Memory safe']);
    expect(matchesEntity(e, 'rust')).toBe(true);
    expect(matchesEntity(e, 'lang')).toBe(true);
    expect(matchesEntity(e, 'memory')).toBe(true);
    expect(matchesEntity(e, 'python')).toBe(false);
    expect(matchesEntity(e, '')).toBe(true);
  });

  it('nameMatches 只看名称', () => {
    expect(nameMatches('Rust', 'rus')).toBe(true);
    expect(nameMatches('Rust', 'lang')).toBe(false);
    expect(nameMatches('Rust', '')).toBe(true);
  });
});
