import type { Entry } from "./dicts";

// SkillList page. User data (skill names, descriptions, kinds as machine
// tokens) is never translated — only chrome strings live here.
export const skills: Record<string, Entry> = {
  // Header + toolbar
  "skills.title": { zh: "技能", en: "Skills" },
  "skills.new": { zh: "新建", en: "New" },
  "skills.delete": { zh: "删除", en: "Delete" },
  "skills.create": { zh: "创建", en: "Create" },
  "skills.cancel": { zh: "取消", en: "Cancel" },
  "skills.export": { zh: "导出", en: "Export" },
  "skills.pull": { zh: "拉取", en: "Pull" },
  "skills.importZip": { zh: "导入 .zip", en: "Import .zip" },
  "skills.search": { zh: "搜索技能与提示词…", en: "Search skills and prompts…" },

  // List + cards
  "skills.select": { zh: "选择 {name}", en: "Select {name}" },
  "skills.noDescription": { zh: "暂无描述。", en: "No description." },
  "skills.readonly.admin": {
    zh: "只读 — 以管理员身份登录（右上角菜单）后可创建或删除。",
    en: "Read-only — sign in as an admin (top-right menu) to create or delete.",
  },
  "skills.readonly.open": { zh: "只读 — 公开实例。", en: "Read-only — open instance." },

  // Empty states
  "skills.empty.noMatches": { zh: "没有匹配结果", en: "No matches" },
  "skills.empty.none": { zh: "还没有技能", en: "No skills yet" },
  "skills.empty.tryDifferent": { zh: "换个关键词试试。", en: "Try a different search." },
  "skills.empty.getStarted": {
    zh: "创建一个，或从 .zip / GitHub 导入。",
    en: "Create one or import from a .zip / GitHub.",
  },
  "skills.empty.nothing": { zh: "这里还什么都没有。", en: "Nothing here yet." },

  // New-skill dialog
  "skills.form.name": { zh: "名称 *", en: "Name *" },
  "skills.form.namePlaceholder": { zh: "pdf-parse", en: "pdf-parse" },
  "skills.form.kind": { zh: "类型", en: "Kind" },
  "skills.form.description": { zh: "描述 *", en: "Description *" },
  "skills.form.descriptionPlaceholder": {
    zh: "做什么 + 何时使用（路由信号）",
    en: "What it does + when to use (routing signal)",
  },
  "skills.form.github": { zh: "或从 GitHub 导入", en: "Or import from GitHub" },
  "skills.form.githubPlaceholder": { zh: "github.com/owner/repo", en: "github.com/owner/repo" },

  // Validation (mirrors backend spec rules)
  "skills.error.nameRequired": { zh: "名称不能为空", en: "Name is required" },
  "skills.error.nameTooLong": { zh: "最多 64 个字符", en: "Max 64 characters" },
  "skills.error.nameCharset": {
    zh: "仅允许小写字母、数字和连字符",
    en: "Lowercase letters, digits, and hyphens only",
  },
  "skills.error.nameHyphenEdge": { zh: "不能以连字符开头或结尾", en: "No leading or trailing hyphen" },
  "skills.error.nameHyphenDouble": { zh: "不能包含连续连字符", en: "No consecutive hyphens" },
  "skills.error.descriptionRequired": { zh: "描述不能为空", en: "Description is required" },
  "skills.error.backend": { zh: "无法连接后端：{msg}", en: "Cannot reach backend: {msg}" },

  // Toasts
  "skills.toast.created": { zh: "已创建", en: "Created" },
  "skills.toast.createFailed": { zh: "创建失败：{msg}", en: "Create failed: {msg}" },
  "skills.toast.imported": { zh: "已导入 {name}", en: "Imported {name}" },
  "skills.toast.importFailed": { zh: "导入失败：{msg}", en: "Import failed: {msg}" },
  "skills.toast.importedMany": { zh: "已导入 {n} 个：{names}", en: "Imported {n}: {names}" },
  "skills.toast.skipped": { zh: "已跳过 {n} 个", en: "Skipped {n}" },
  "skills.toast.githubFailed": { zh: "GitHub 导入失败：{msg}", en: "GitHub import failed: {msg}" },
  "skills.toast.deleted": { zh: "已删除 {n} 个", en: "Deleted {n}" },
  "skills.toast.deleteFailed": { zh: "删除失败：{msg}", en: "Delete failed: {msg}" },

  // Delete confirmation
  "skills.delete.confirm": { zh: "确定删除 {n} 个项目？", en: "Delete {n} item?" },
  "skills.delete.confirmMany": { zh: "确定删除 {n} 个项目？", en: "Delete {n} items?" },
  "skills.delete.body": {
    zh: "将永久删除该技能及其所有版本，且无法撤销。",
    en: "This permanently removes the skill and all its versions. This cannot be undone.",
  },
  "skills.delete.bodyMany": {
    zh: "将永久删除这些技能及其所有版本，且无法撤销。",
    en: "This permanently removes the skills and all their versions. This cannot be undone.",
  },
};
