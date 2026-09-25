# Git 与提交规范

状态：仓库工作约定。`main` 为受保护主分支；每个完成并通过相应验证的小功能都单独提交，并通过 PR 合并。GitHub CLI 的具体命令见 [GitHub CLI 工作流](gh-cli-workflow.md)。

## 提交粒度

- 一个提交对应一个可理解、可验证的逻辑变更，例如一个领域规则、一个适配器协议、一项故障修复或一组相关文档修订。不要把无关功能混在一起，也不要为同一功能把实现、必要测试和模块文档拆成互不完整的提交。
- 每项功能达到该阶段的验收条件后及时提交。尚未完成或验证失败的代码不以 `WIP` 提交到 `main`；需要分阶段落地时，每阶段须独立可构建，并在提交说明中标明能力限制。
- 提交前检查 `git status --short` 和 `git diff --cached`，仅暂存本次变更；运行相关格式化、lint、测试和文档检查。检查未运行或未通过时，在交付说明中如实记录，不把设计文档当作验收结果。
- 不提交密钥、`.env`、本机 CodeGraph 数据库、构建产物、运行数据或真实账户响应。`.codegraph/.gitignore` 和 `.codex/config.toml` 属于可提交的工具配置。首次加入外部数据或样本前先脱敏并核对授权。
- 不修改已有共享提交历史。需要撤销已共享变更时使用新的修复或 `revert` 提交；只有明确要求且确认未共享时才整理本地历史。

## 提交信息

遵循 [Conventional Commits 1.0.0](https://www.conventionalcommits.org/zh-hans/v1.0.0/)：

```text
<type>(<scope>): <简短描述>

<可选正文：原因、关键取舍、验证及限制>

<可选脚注：BREAKING CHANGE: ... 或 Refs: ...>
```

`type` 使用小写英文：`feat` 新功能、`fix` 修复、`docs` 文档、`test` 测试、`refactor` 不改变行为的重构、`perf` 性能、`build` 构建或依赖、`ci` 流水线、`chore` 仓库维护。`scope` 优先用 package 或明确领域名，如 `market`、`strategy`、`risk`、`okx-client`、`trader`、`repo`；跨模块变更可省略。标题描述实际结果，避免“更新代码”“修复问题”等笼统用语。正文解释难从差异中看出的原因、迁移或故障边界。破坏性契约变更使用 `!` 和/或 `BREAKING CHANGE:` 脚注，并同步更新协议文档与测试。

示例：

```text
feat(market): add stable identity for trade events
fix(trader): reconcile unknown order submissions by client ID
docs(architecture): define app and crate ownership
chore(repo): initialize Rust trading design repository
```

## 日常顺序

1. 选一个有明确所有者和验收条件的小功能；实施时同步更新代码、测试、package README、module rustdoc 与受影响的专题文档。
2. 运行该功能所需检查，查看工作树和暂存差异，确认没有凭据、无关文件或生成数据。
3. 在功能分支使用符合上述格式的提交信息创建提交，推送后用 `gh pr create` 向 `main` 发起 PR。
4. 用 `gh pr checks --watch` 等待必需的 `quality` 检查；失败时查看运行日志、修复并重新推送。检查通过且 PR 可合并后用 `gh pr merge --squash --delete-branch` 合并，不绕过分支保护。
5. 同步本地 `main`，在交付说明中报告 PR、合并提交、验证结果及剩余限制。完整命令和门禁核对见 [GitHub CLI 工作流](gh-cli-workflow.md)。
