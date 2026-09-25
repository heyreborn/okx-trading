# GitHub CLI 工作流

本仓库的 `origin` 为 `heyreborn/okx-trading`，默认分支 `main` 受保护。日常变更从功能分支发起 PR；`Rust quality` 工作流的 `quality` 检查通过且分支与 `main` 保持同步后才能合并。仓库只允许 squash merge；提交粒度、信息格式与 SHA 变化见 [Git 与提交规范](git-workflow.md#合并策略)。

## 开始前

在仓库根目录确认身份、远端和工作树：

```sh
gh auth status
git remote -v
git status -sb
git fetch origin
```

`gh` 用登录凭据访问 GitHub API；当前 `origin` 使用 SSH 进行 Git 传输，两者需要分别可用。不要把令牌、私钥或 `gh auth token` 的输出写入仓库或诊断日志。认证或网络失败时先分别检查 `gh auth status` 和 `git ls-remote origin`。

## 提交与 PR

从最新的 `main` 创建与任务对应的分支。下例中的分支名、提交标题和 PR 正文应替换为实际内容：

```sh
git switch main
git pull --ff-only origin main
git switch -c docs/example-change
# 完成修改及对应验证
git status --short
git diff --check
git add -- path/to/changed-file
git diff --cached
git commit -m 'docs(repo): describe example change'
git push -u origin HEAD
gh pr create --base main --title 'docs(repo): describe example change' --body '变更、验证和已知限制'
```

PR 正文记录变更目的、实际运行的验证命令及未验证项；不要把本机通过当作远端 CI 通过。`quality` 在面向 `main` 的 PR 和推送到 `main` 时运行；单独推送功能分支不会启动检查，需要创建或更新 PR。需要补充修复时，在同一分支提交并推送，PR 会自动更新。不要直接推送 `main`，也不要用管理员绕过合并检查。

## 检查与合并

```sh
gh pr checks --watch
gh pr view --json mergeStateStatus,statusCheckRollup,url
```

必需检查名是 `quality`，来源为 GitHub Actions。检查失败时使用 `gh run list --branch "$(git branch --show-current)"` 找到运行 ID，再用 `gh run view <运行 ID> --log-failed` 查看失败步骤；修复后重新推送并等待新检查。若 PR 落后于 `main`，先更新分支并等待重新运行的检查。仅在必需检查通过、PR 可合并且确认差异后执行：

```sh
gh pr diff
gh pr merge --squash --delete-branch
git switch main
git pull --ff-only origin main
```

合并后核对 `git status -sb` 和合并提交；把 PR、CI 链接及尚未验证的场景写入交付说明。需要撤销共享变更时创建修复或 revert，不重写 `main` 历史。

## 仓库门禁

查看最近的远端运行及 `main` 保护规则：

```sh
gh run list --workflow 'Rust quality' --limit 5
gh api repos/heyreborn/okx-trading/branches/main/protection --jq '{checks:.required_status_checks.checks,strict:.required_status_checks.strict,enforce_admins:.enforce_admins.enabled,requires_pr:(.required_pull_request_reviews != null)}'
gh api repos/heyreborn/okx-trading --jq '{allow_merge_commit,allow_rebase_merge,allow_squash_merge}'
```

当前规则要求 PR、最新分支上的 `quality` 检查，并对管理员生效；禁止强推和删除。不要求额外审阅者。仓库的三个合并选项中仅 `allow_squash_merge` 为 `true`。调整门禁后读回规则，并用检查失败的测试 PR 验证合并确实被阻止，再记录验收证据。`gh api` 修改的是远端仓库配置，应先确认目标仓库和变更范围。

命令参数以 [GitHub CLI 手册](https://cli.github.com/manual/)和 [GitHub 分支保护文档](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches)为准。
