# Codex 工作指引

## 仓库现状

本仓库规划用 Rust 重新实现 `../okx-trading-py` 的功能。当前已有虚拟 Cargo workspace、`model`、`config`、`telemetry`、`instrument`、`okx-client` 和 `integration-tests` package，以及可运行的本机基础设施 Compose；尚无可部署的 Rust 应用或交易能力。`docs/architecture.md` 中的目录是目标结构；声称某个 package、命令、服务、测试或交易能力已存在前，先检查实际文件。

初版面向 OKX 模拟盘，支持多个 SPOT 与 USDT 本位线性 SWAP 产品、每产品多个策略实例，并按多机运行设计。自动交易须通过文档规定的准入验收后才可启用；初版不得引入实盘交易入口。

## 实现前阅读

- `docs/architecture.md`：package 归属、依赖方向、数据链路及功能矩阵。
- `docs/development-roadmap.md`：按依赖排序的 `DEV-xxx` 任务与阶段验收；实施前先核对实际完成证据。
- `docs/contracts-and-invariants.md`、`docs/implementation-protocols.md`、`docs/failure-and-recovery.md`：身份、精度、事务与恢复规则。
- `docs/technology-decisions.md`、`docs/performance-and-capacity.md`、`docs/configuration-and-observability.md`：技术栈、性能和运行约束。
- `docs/testing-strategy.md`、`docs/module-documentation.md`：验收与逐模块文档要求。
- `docs/git-workflow.md`、`docs/gh-cli-workflow.md`：小功能提交粒度、GitHub CLI、受保护分支与 PR/CI 流程。
- `docs/okx-api-reference.md`：OKX 官方 API 链接及逐功能核对清单。

## CodeGraph

仓库已通过 `codegraph init .` 初始化 `.codegraph/`，项目级 Codex MCP 配置在 `.codex/config.toml`。在可信项目中重启 Codex 后优先使用 `codegraph_explore`；没有 MCP 工具时运行 `codegraph explore "问题或符号"`。定位或理解 Rust 代码时先查询 CodeGraph，再按需读取文件或使用 `rg`。索引落后时运行 `codegraph sync .`，需要全量重建时运行 `codegraph index .`，并用 `codegraph status .` 核对文件和节点数。

当前仓库已有 Rust 源码；CodeGraph 不代替阅读 `docs/` 中的规划文档。新增 Rust package 后同步索引，再用实际符号查询验证调用关系。`.codegraph/` 的数据库是本机生成文件，不提交到版本库；仅保留其中的 `.gitignore`。

## 来源与实现规则

- 可查阅 `../okx-trading-py/docs/rust-migration-guide.md`、Python 源码和测试，以识别已实现功能及边界行为；现状须与源码、测试核对。不得直接搬运或逐行翻译 Python 代码。生产代码和测试代码都用 Rust 按本仓库架构重写。
- 实现功能前，记录 Python 行为证据、Rust 所属 package/module、不变量和 Rust 验收测试。区分已运行功能、独立技术探针、仅离线验证的逻辑与未实现规划。Python 输出不能作为唯一正确性依据。
- 实现或修改 OKX API 接入前，打开 `docs/okx-api-reference.md` 对应的官方 API 章节及变更日志，核对当前字段、单位、权限、账户模式、模拟盘差异、限速、错误码与重连语义。API 变化时先更新依据和测试；不能从 Python 代码推断交易所协议。
- 遵循规划中的虚拟 Cargo workspace：`crates/` 存放独立领域规则、跨应用契约和适配器；`apps/` 存放可部署进程及其专属用例。`apps/{ingest,observe,trader}` 各含 `usecases` 库和 `runner` 二进制。端口归所属用例库，runner 组装具体适配器；依赖不得成环，也不建立全局 ports crate。
- 领域计算保持同步、确定性；交易数值明确单位和十进制精度上限。有副作用的流程须明确资源所有权、取消、重试、幂等、顺序、背压与事务边界。OKX 下单超时表示结果未知，须用稳定 client order ID 核对，不能盲目新建订单重发。
- PostgreSQL 保存本地决策、预留、outbox 和归属等权威账本事实；Kafka 承载行情与可重放交付。不得宣称 PostgreSQL、Kafka、OKX 之间存在原子事务。市场或账户事实不可信、过期时不得批准新订单。

## 文档与验证

- 每个 Cargo package 创建或修改代码时，同次更新其 `README.md`，说明职责、实际模块、输入输出、实现流程、关键取舍、失败与恢复、资料依据、测试和已知限制。每个 Rust module 写 `//!` 文档；公开 API 的 rustdoc 写清单位、错误、副作用和不变量。细则见 `docs/module-documentation.md`。
- 始终区分规划与已实现行为。修改契约或 package 边界时，同步更新相关架构/协议文档和测试；设计文档不能充当验收通过的证据。
- 优先直接使用本机 `rustc`、`cargo`。找不到时检查 Mise 安装和 `PATH`，仍不可用才考虑固定 builder 容器。有 workspace 后先运行相关测试，再按影响范围执行 fmt、check、Clippy、Nextest、doctest、deny 和文档门禁；未运行的检查需说明。
- PostgreSQL、Kafka、对象存储及其他独立服务通过 Docker Compose 运行，不在宿主机原生安装。凭据不得进入仓库、日志、测试夹具或命令输出；测试使用隔离的基础设施，OKX 验收只使用显式授权的模拟盘凭据。

## Git 提交

遵循 `docs/git-workflow.md` 和 `docs/gh-cli-workflow.md`。每个完成并验证的小功能单独 commit，提交包含该功能必需的 Rust 代码、测试和模块文档；使用 `<type>(<scope>): <描述>` 的约定式提交信息。提交前核对暂存内容与敏感信息，通过功能分支和 PR 等待必需的 `quality` 检查，再合并到受保护的 `main`。交付时报告 PR、commit ID 和验证结果；不得把未通过验收的交易能力描述为已可用。
