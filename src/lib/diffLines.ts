//! 行级 diff(迭代 D3.3):基于 LCS 的自实现,用于记忆版本并排对比。
//! 记忆文件体量小(数百行),O(n*m) DP 足够。

export interface DiffCell {
  text: string;
  type: 'same' | 'add' | 'del';
}

export interface DiffRow {
  left: DiffCell | null;
  right: DiffCell | null;
}

/** 对比两个文本,返回逐行对齐结果:left 为旧版本,right 为新版本。 */
export function diffLines(oldText: string, newText: string): DiffRow[] {
  const a = oldText.split('\n');
  const b = newText.split('\n');
  const n = a.length;
  const m = b.length;

  // LCS 长度表
  const dp: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }

  const rows: DiffRow[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      rows.push({ left: { text: a[i], type: 'same' }, right: { text: b[j], type: 'same' } });
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      rows.push({ left: { text: a[i], type: 'del' }, right: null });
      i++;
    } else {
      rows.push({ left: null, right: { text: b[j], type: 'add' } });
      j++;
    }
  }
  while (i < n) {
    rows.push({ left: { text: a[i], type: 'del' }, right: null });
    i++;
  }
  while (j < m) {
    rows.push({ left: null, right: { text: b[j], type: 'add' } });
    j++;
  }
  return rows;
}
