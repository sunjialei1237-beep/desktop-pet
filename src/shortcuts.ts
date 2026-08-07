// 统一管理键盘快捷键。
// - F12 切换 Debug 面板，但部分笔记本将其劫持（如睡眠键），故以 Ctrl+Shift+D 为可靠替补
// - 用 e.code（物理按键）而非 e.key 判断，避免中文输入法把组合键截获成 "Process"
// - Esc 无条件关闭 Debug 面板：单键、无修饰键冲突，是面板打开后最可靠的退出通道
export function isDebugToggle(e: KeyboardEvent): boolean {
  if (e.isComposing || e.key === "Process") return false;
  return e.key === "F12" || (e.ctrlKey && e.shiftKey && e.code === "KeyD");
}

export function isDebugClose(e: KeyboardEvent): boolean {
  if (e.isComposing || e.key === "Process") return false;
  return e.key === "Escape";
}
