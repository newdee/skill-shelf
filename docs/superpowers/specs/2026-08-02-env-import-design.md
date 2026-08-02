# .env 导入到 Config Center Namespace — 设计文档

日期:2026-08-02
状态:已批准(brainstorming 会话确认)

## 背景与目标

Config center 的 namespace 配置目前只能在 UI 里逐条编辑。团队已有大量 `.env` 文件形式的环境配置,需要一条低摩擦的迁移路径:把 `.env` 内容一次性导入到当前 namespace 的草稿(draft),再走现有 publish 流程生效。

成功标准:用户在 ConfigCenter 页面选中一个 namespace,粘贴或选择 `.env` 文件,预览确认后,内容合并进该 namespace 的 draft;发布前对消费方(`/config/resolve`)零影响。

## 已确认的决策

1. **纯前端实现** — `.env` 解析在前端完成,复用现有 `api.putNamespace`(`PUT /config/namespace`)提交 patch。服务端零改动。
2. **值全部按字符串导入** — 不做类型推断(避免 `1.10`、`"08"` 等被误转)。保持 `.env` 语义:所见即所得。
3. **预览后覆盖合并** — 解析后先展示预览(新增/覆盖/跳过/无效),确认后合并进 draft。同名 key 覆盖;`.env` 中不存在的 key 不动(不会删除任何 key)。
4. **只进 draft** — 导入不触发 publish。发布仍走现有手动 publish 流程。

## 组件设计

### 1. 解析器 `parseDotenv`

位置:`app/src/lib/dotenv.ts`,纯函数,无依赖。

```ts
interface DotenvParseResult {
  vars: Record<string, string>;
  warnings: { line: number; text: string; reason: string }[];
}
function parseDotenv(text: string): DotenvParseResult;
```

语法支持:

- `KEY=value`,可带 `export ` 前缀
- `#` 整行注释、空行跳过
- 单引号包裹:字面值,不处理转义
- 双引号包裹:支持 `\n`、`\t`、`\"`、`\\` 转义
- 无引号值:去首尾空白,行内 ` #` 后视为注释截断
- key 规则:`[A-Za-z_][A-Za-z0-9_]*`;不合规行跳过并记入 `warnings`(行号 + 原文 + 原因)
- 同名 key 重复出现:后者覆盖前者(与 dotenv 生态一致)

### 2. 导入 Dialog(UI)

Hallmark component-scope 结论:继承现有 shadcn/Tailwind v4/OKLCH token 系统,不引入新主题;motion-cut(只用 shadcn 自带过渡);genre modern-minimal。

**入口**:ConfigCenter namespace 详情工具栏「导入 .env」outline 按钮(lucide `FileUp`),位于 publish 按钮旁。

**结构**(shadcn Dialog,`max-w-2xl`):

1. **Header** — 标题「导入 .env → \<ns\>」;description:值按字符串合并进草稿,发布前不影响消费方。
2. **输入区** — 单一 mono Textarea(placeholder `KEY=value`)+「选择文件」ghost 按钮。文件经 FileReader 读入后填充 textarea,可继续编辑。粘贴与文件同一状态源,不做 Tabs。
3. **实时预览** — textarea 内容变化即解析(无单独「解析」按钮)。
   - Badge 行:`新增 N` · `覆盖 M` · `跳过 K(值相同)` · `无效 J 行`
   - 滚动列表逐 key 展示;覆盖项显示 `旧值 → 新值`(amber 标注);跳过项 muted
   - 无效行:warning Alert 列出行号与原因
   - 空输入:Empty 组件提示粘贴或选择文件
4. **Footer** — Cancel + primary「导入 T 项到草稿」,T = 新增 + 覆盖之和(即实际写入 patch 的条数);`T = 0` 时禁用。

**分类定义**(相对当前 draft):

- 新增:key 不在 draft
- 覆盖:key 在 draft 且值不同(draft 值非字符串时,序列化后比较,仍标为覆盖)
- 跳过:key 在 draft 且字符串值完全相同 — 不包含在提交的 patch 中

**状态**:按钮 8 态齐全(default/hover/focus-visible/active/disabled/loading/error/success)。

- 成功:关闭 Dialog + sonner toast「已合并 T 项到 \<ns\> 草稿」+ react-query invalidate 刷新 namespace 视图。静默成功,无庆祝动画。
- 失败(PUT 报错):Dialog 保持打开,内嵌 destructive Alert 显示错误,输入内容保留。
- 文件读取失败:sonner error toast。

**Copy 纪律**:按钮动词化(「导入到草稿」非「确定」);预览数字全部来自真实解析结果。

### 3. 数据流

```
textarea/文件 → parseDotenv → 与 draft 对比分类 → 用户确认
→ putNamespace(ns, patch)   // patch = 新增 + 覆盖项,全为字符串
→ invalidate namespace query → toast
```

patch 不含 null(导入永不删 key),不含跳过项。

## 错误处理

| 场景 | 行为 |
| --- | --- |
| 解析结果为空(纯注释/空文件) | 确认按钮禁用,预览区显示 Empty |
| 部分行无效 | 有效行正常导入,无效行在预览警告中列出 |
| PUT 失败(网络/401/403) | Dialog 内 destructive Alert,输入保留 |
| 文件读取失败 | error toast |

## 测试

`parseDotenv` 单元测试(app 无测试基建则补 vitest 最小配置,仅覆盖此文件):

- 基本 `KEY=value` / `export ` 前缀
- 注释行、行内注释、空行
- 单引号(无转义)/ 双引号(转义)
- 无效 key、无 `=` 的行 → warnings
- 重复 key 后者胜
- 空文件 / 纯注释文件 → 空结果

分类逻辑(新增/覆盖/跳过)作为纯函数一并测试。

## 非目标

- 服务端 `.env` 解析接口(CLI/脚本导入)— 需要时再加
- 类型推断
- 导出为 .env(反向)
- 导入即发布
