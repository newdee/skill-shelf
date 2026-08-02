import type { Entry } from "./dicts";

// Config center page + Import .env dialog. UI chrome only — namespace names,
// keys, values, tokens, versions, authors, and parser warning reasons are user
// data and are never translated.
export const configCenter: Record<string, Entry> = {
  // Page chrome
  "config.title": { zh: "配置中心", en: "Config center" },
  "config.subtitle": {
    zh: "其他服务从这里获取配置，而不是读取环境变量。",
    en: "Other services fetch their config from here instead of reading env vars.",
  },
  "config.adminOnly": { zh: "仅限管理员", en: "Admin only" },
  "config.adminOnlyDesc": {
    zh: "配置中心仅对管理员开放。请从右上角菜单登录。",
    en: "The config center is available to administrators. Sign in from the top-right menu.",
  },
  "config.loadError": {
    zh: "无法加载配置。请确认已以管理员身份登录且后端可访问。",
    en: "Couldn't load config. Check you're signed in as an admin and the backend is reachable.",
  },

  // Namespaces card
  "config.namespaces": { zh: "命名空间", en: "Namespaces" },
  "config.nsDescPre": { zh: "每个服务/环境是一个命名空间。", en: "Each service/environment is a namespace. " },
  "config.nsDescPost": {
    zh: " 会合并进每次拉取——请只在其中保留共享的非机密默认值。",
    en: " is merged into every fetch — keep only shared, non-secret defaults there.",
  },
  "config.shared": { zh: "共享", en: "shared" },
  "config.unpublished": { zh: "未发布的更改", en: "unpublished changes" },
  "config.unpublishedTitle": { zh: "未发布的更改", en: "Unpublished changes" },
  "config.newNamespace": { zh: "新建命名空间", en: "New namespace" },

  // Publish bar
  "config.publishedV": { zh: "已发布 v{v}", en: "published v{v}" },
  "config.neverPublished": { zh: "从未发布", en: "never published" },
  "config.history": { zh: "历史", en: "History" },
  "config.importEnv": { zh: "导入 .env", en: "Import .env" },
  "config.publish": { zh: "发布", en: "Publish" },

  // Draft-vs-published diff panel
  "config.diffVsPublished": { zh: "相对已发布 v{v} 的未发布更改", en: "Unpublished changes vs published v{v}" },
  "config.draftNotPublished": { zh: "未发布的草稿（尚未发布）", en: "Unpublished draft (not yet published)" },
  "config.status.added": { zh: "新增", en: "added" },
  "config.status.modified": { zh: "修改", en: "modified" },
  "config.status.removed": { zh: "移除", en: "removed" },

  // KV editor
  "config.draftKeysPre": { zh: "命名空间 ", en: "Draft keys in " },
  "config.draftKeysPost": {
    zh: " 中的草稿键——此处的修改在发布前不会影响消费方",
    en: " — edits here don't affect consumers until you Publish",
  },
  "config.noKeys": { zh: "还没有键。", en: "No keys yet." },
  "config.deleteKeyAria": { zh: "删除键 {key}", en: "Delete key {key}" },
  "config.badge.json": { zh: "json", en: "json" },
  "config.badge.secret": { zh: "机密", en: "secret" },
  "config.key": { zh: "键", en: "Key" },
  "config.valueLabel": { zh: "值（字符串或 JSON）", en: "Value (string or JSON)" },
  "config.valuePlaceholder": { zh: 'postgres://…  或  {"max":5}', en: 'postgres://…  or  {"max":5}' },
  "config.keyExists": { zh: "键已存在", en: "Key already exists" },
  "config.consumeHint": { zh: "使用服务令牌读取：", en: "Consume with a service token:" },

  // Namespace validation (same rule as the backend)
  "config.nsErr.required": { zh: "命名空间不能为空", en: "Namespace is required" },
  "config.nsErr.maxLen": { zh: "最多 128 个字符", en: "Max 128 characters" },
  "config.nsErr.dots": { zh: "不能包含 '..'", en: "Must not contain '..'" },
  "config.nsErr.charset": { zh: "仅限字母、数字和 - _ / .", en: "Letters, digits, and - _ / . only" },

  // Service tokens card
  "config.serviceTokens": { zh: "服务令牌", en: "Service tokens" },
  "config.tokensDesc1": { zh: "每个客户端获得一个令牌（通过 ", en: "Each client gets a token (sent as " },
  "config.tokensDesc2": { zh: " 发送），可读取其被授予的命名空间。", en: ") that may read its granted namespaces. " },
  "config.tokensDesc3": { zh: " 始终包含在内。", en: " is always included." },
  "config.onlyGlobal": { zh: "（仅 _global）", en: "(only _global)" },
  "config.noClients": { zh: "还没有客户端。", en: "No clients yet." },
  "config.revokeTokenAria": { zh: "撤销令牌 {name}", en: "Revoke token {name}" },
  "config.newClient": { zh: "新建客户端", en: "New client" },

  // New namespace dialog
  "config.newNamespaceDesc": { zh: "创建一个命名空间及其第一个键。", en: "Create a namespace and its first key." },
  "config.name": { zh: "名称", en: "Name" },
  "config.firstKey": { zh: "第一个键", en: "First key" },
  "config.value": { zh: "值", en: "Value" },
  "config.valueOrJson": { zh: "值或 JSON", en: "value or JSON" },

  // New service token dialog
  "config.newToken": { zh: "新建服务令牌", en: "New service token" },
  "config.newTokenDesc": { zh: "授予该服务可读取的命名空间。", en: "Grant the namespaces this service may read." },
  "config.noServiceNsPre": {
    zh: "尚无服务命名空间——此令牌将只能读取 ",
    en: "No service namespaces yet — this token will read only ",
  },
  "config.noServiceNsPost": { zh: "。", en: "." },

  // One-time token reveal
  "config.tokenFor": { zh: "{name} 的令牌", en: "Token for {name}" },
  "config.tokenOnce": {
    zh: "请立即复制——令牌只显示一次，无法找回。",
    en: "Copy it now — it is shown only once and cannot be recovered.",
  },
  "config.copyTokenAria": { zh: "复制令牌", en: "Copy token" },

  // Publish dialog
  "config.publishNs": { zh: "发布 {ns}", en: "Publish {ns}" },
  "config.publishDesc": {
    zh: "将当前草稿快照为 v{v}。解析此命名空间的消费方将立即收到新值。",
    en: "Snapshots the current draft as v{v}. Consumers resolving this namespace will immediately receive the new values.",
  },
  "config.noteOptional": { zh: "备注（可选）", en: "Note (optional)" },
  "config.notePlaceholder": { zh: "改了什么、为什么改", en: "what changed and why" },

  // Version history dialog
  "config.historyTitle": { zh: "历史 — {ns}", en: "History — {ns}" },
  "config.historyDesc": {
    zh: "已发布的版本，最新在前。回滚会将该版本载入草稿。",
    en: "Published versions, newest first. Roll back loads a version into the draft.",
  },
  "config.noNote": { zh: "（无备注）", en: "(no note)" },
  "config.hide": { zh: "隐藏", en: "Hide" },
  "config.view": { zh: "查看", en: "View" },
  "config.rollback": { zh: "回滚", en: "Roll back" },
  "config.keysCount": { zh: "{n} 个键", en: "{n} keys" },
  "config.empty": { zh: "（空）", en: "(empty)" },
  "config.noVersions": { zh: "还没有已发布的版本。", en: "No published versions yet." },

  // Confirm dialogs
  "config.deleteKeyTitle": { zh: "删除键 {key}？", en: "Delete key {key}?" },
  "config.deleteKeyDescPre": { zh: "正在解析 ", en: "Services resolving " },
  "config.deleteKeyDescPost": {
    zh: " 的服务将不再收到此键。此操作无法撤销。",
    en: " will stop receiving this key. This cannot be undone.",
  },
  "config.revokeTitle": { zh: "撤销此令牌？", en: "Revoke this token?" },
  "config.revokeDesc": {
    zh: "使用该令牌的服务将立即失去访问权限。此操作无法撤销。",
    en: "The service using it will immediately lose access. This cannot be undone.",
  },
  "config.revoke": { zh: "撤销", en: "Revoke" },

  // Toasts
  "config.toast.saveFailed": { zh: "保存失败：{msg}", en: "Save failed: {msg}" },
  "config.toast.published": {
    zh: "已发布——消费方现在解析到新版本",
    en: "Published — consumers now resolve the new version",
  },
  "config.toast.publishFailed": { zh: "发布失败：{msg}", en: "Publish failed: {msg}" },
  "config.toast.rolledBack": {
    zh: "已载入草稿——请检查后点击发布使其生效",
    en: "Loaded into draft — review, then Publish to go live",
  },
  "config.toast.rollbackFailed": { zh: "回滚失败：{msg}", en: "Rollback failed: {msg}" },
  "config.toast.createFailed": { zh: "创建失败：{msg}", en: "Create failed: {msg}" },
  "config.toast.revoked": { zh: "客户端已撤销", en: "Client revoked" },
  "config.toast.revokeFailed": { zh: "撤销失败：{msg}", en: "Revoke failed: {msg}" },
  "config.toast.copyFailed": {
    zh: "复制失败——请手动选中令牌并复制",
    en: "Couldn't copy — select the token and copy it manually",
  },

  // Import .env dialog
  "import.title": { zh: "导入 .env → {ns}", en: "Import .env → {ns}" },
  "import.description": {
    zh: "值将作为字符串导入并合并到草稿中。发布之前消费方不受影响。下方未出现的既有键保持不变。",
    en: "Values are imported as strings and merged into the draft. Consumers are unaffected until you Publish. Existing keys not present below are left untouched.",
  },
  "import.placeholder": { zh: "KEY=value\n# 注释会被忽略", en: "KEY=value\n# comments are ignored" },
  "import.contentAria": { zh: ".env 内容", en: ".env content" },
  "import.chooseFile": { zh: "选择文件", en: "Choose file" },
  "import.added": { zh: "{n} 个新增", en: "{n} added" },
  "import.overwritten": { zh: "{n} 个覆盖", en: "{n} overwritten" },
  "import.skipped": { zh: "{n} 个跳过（值相同）", en: "{n} skipped (same value)" },
  "import.invalid": { zh: "{n} 行无效", en: "{n} invalid" },
  "import.status.added": { zh: "新增", en: "added" },
  "import.status.overwritten": { zh: "覆盖", en: "overwritten" },
  "import.status.skipped": { zh: "跳过", en: "skipped" },
  "import.invalidTitle": { zh: "部分行无法导入", en: "Some lines can't be imported" },
  "import.line": { zh: "第 {n} 行:", en: "line {n}:" },
  "import.pasteHint": {
    zh: "在上方粘贴 .env 内容或选择文件以预览。",
    en: "Paste .env content above or choose a file to see a preview.",
  },
  "import.failed": { zh: "导入失败", en: "Import failed" },
  "import.importing": { zh: "导入中…", en: "Importing…" },
  "import.submit": { zh: "导入 {n} 个键到草稿", en: "Import {n} keys to draft" },
  "import.merged": {
    zh: "已将 {n} 个键合并到 {ns} 草稿——发布后生效",
    en: "Merged {n} keys into the {ns} draft — Publish to go live",
  },
  "import.readFailed": { zh: "无法读取该文件", en: "Couldn't read the file" },
};
