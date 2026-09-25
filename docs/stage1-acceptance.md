# 阶段 1 验收记录

状态：**A 尚未通过**。日期：2026-09-25 UTC。分支：`feat/stage1-contracts-catalog`。锁文件 SHA-256：`b24583a9a01e6ffd6f9eabaf45fe139e85318e726250f3840d68b20a6f6855ef`。本机工具链：`rustc 1.98.1`。测试样例为 `config/example.toml` 的 BTC-USDT SPOT、BTC-USDT-SWAP、同一 SPOT 两实例及跨产品输入的一实例；样例账户 `demo-main` 是本地 ID，不是已验证的远端账户。

## 已通过

| 场景 | 环境与命令 | 结果 |
| --- | --- | --- |
| 强类型身份、精确小数、schema 1 严格事实、零/无信号/未知状态 | 本机 `cargo test --offline -p model` | 13 个单元测试通过；旧未知 schema 明确拒绝。 |
| 多产品配置、同产品多实例、跨产品输入、目标模式和默认关闭的交易门禁 | 本机 `cargo test --offline -p config` | 7 个单元测试通过；实际 net 模式与双侧目标的冲突拒绝。 |
| 映射、规格、账户许可、费率组与来源/接收时效 | 本机 `cargo test --offline -p instrument`、`cargo test --offline -p integration-tests --test stage1_catalog` | 纯规则测试及离线跨包样例通过。 |
| OKX GET 签名、模拟盘头、SPOT/SWAP 单位、业务码、超大响应、费率组及目录交接 | 本机假 HTTP 服务，`cargo test --offline -p okx-client` | 7 个协议测试通过；测试使用虚构凭据，无真实 OKX 请求。 |
| 十进制数据库往返与溢出 | 新建 `okx-stage1-test-codex` 隔离 PostgreSQL 17.6 Compose 容器，`OKX_TEST_COMPOSE_PROJECT=okx-stage1-test-codex cargo test --offline -p integration-tests --test stage1_numeric_pg -- --ignored` | `NUMERIC(38,18)` 的零、最小小数、正负极值往返及超界拒绝通过；容器、网络与测试卷已清理。 |
| 工作区单元门禁 | `cargo nextest run --offline --workspace -E 'not test(isolated_services_accept_roundtrips)'` | 34 项通过；基础设施连通性、真实模拟盘与独立 PG 测试按各自环境单独运行。 |
| 依赖审计 | `cargo deny check` | advisories、bans、licenses、sources 均通过；将 `time` 锁定到 0.3.47，仍有非阻断的传递依赖重复告警。 |

## 未通过门禁

- `authorized_demo_catalog_acceptance` 默认 `ignored`，尚未提供显式授权的 OKX 模拟盘只读密钥文件及账户地区。本轮没有访问远端账户，也没有核对实际 `acctLv`、`posMode`、账户产品许可、费率组和真实规格。故测试矩阵 A 不能标记为通过。
- 实际生产/模拟盘产品规格的极值与 Rust `NUMERIC(38,18)` 上限尚未对照；若超出，需调整契约、迁移和测试后再考虑后续阶段。
- 此阶段没有应用 runner、PostgreSQL 业务迁移、Kafka 行情或任何下单/撤单路径。自动交易与实盘均不可用。没有未清订单，也没有本轮生成的市场数据缺口记录。

## 后续现场验证

仅在明确授权的 OKX 模拟盘 Read 权限凭据下，按 [`integration-tests/README.md`](../integration-tests/README.md)设置密钥文件引用与地区，运行被忽略的只读测试。应保存脱敏后的运行时间、实际账户模式、产品/策略集合、通过/失败原因，并复核官方 API 当前变更日志。验收失败时维持只读范围；不会自动切换账户模式或开启交易。
