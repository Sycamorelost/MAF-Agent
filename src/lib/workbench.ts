/** 会话直达路径:所有入口统一用它导航到 /chat/:id。 */
export function sessionPath(id: string): string {
  return `/chat/${id}`;
}
