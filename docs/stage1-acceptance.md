# 阶段 1 验收记录

状态：**验收 A 已通过（所列样例范围）**。现场窗口：2026-09-25 06:32-06:34 UTC。分支：`feat/stage1-contracts-catalog`，验证代码：`b730667`。锁文件 SHA-256：`434b53479e4979595777e6340de8fbfccf8ffffdd5bbc9a253244a78638b0966`。本机工具链：`rustc 1.98.1`。配置摘要：`1260403ad5bd2dc46bd497525be8836b29f5f9f43128b0a3cc40711b343c2ad1`。测试配置为 BTC-USDT SPOT、BTC-USDT-SWAP、同一 SPOT 两实例及跨产品输入的一实例；`demo-main` 是本地账户 ID，不是 OKX 返回的账户标识。

## 已通过

| 场景 | 环境与命令 | 结果 |
| --- | --- | --- |
| 强类型身份、精确小数、schema 1 严格事实、零/无信号/未知状态 | 本机 `cargo test --offline -p model` | 13 个单元测试通过；旧未知 schema 明确拒绝。 |
| 多产品配置、同产品多实例、跨产品输入、目标模式和默认关闭的交易门禁 | 本机 `cargo test --offline -p config` | 7 个单元测试通过；双侧目标拒绝实际 net 模式，net 目标接受两种模式。 |
| 映射、规格、账户许可、费率组与来源/接收时效 | 本机 `cargo test --offline -p instrument`、`cargo test --offline -p integration-tests --test stage1_catalog` | 纯规则及跨包样例通过；错误 SWAP 面值币种和过期事实被拒绝。 |
| OKX GET 签名、模拟盘头、两种模式、SPOT/SWAP 单位、业务码、超大响应、费率组、密钥文件及目录交接 | 本机假 HTTP 服务，`cargo test --offline -p okx-client` | 9 个协议测试通过，均使用虚构凭据。 |
| 十进制数据库往返与溢出 | 新建 `okx-stage1-test-codex` 隔离 PostgreSQL 17.6 Compose 容器，`OKX_TEST_COMPOSE_PROJECT=okx-stage1-test-codex cargo test --offline -p integration-tests --test stage1_numeric_pg -- --ignored` | `NUMERIC(38,18)` 的零、最小小数、正负极值往返及超界拒绝通过；容器、网络与测试卷已清理。 |
| 真实 Global 模拟盘只读目录 | `OKX_STAGE1_DEMO_READ_ONLY=1 cargo test --offline -p integration-tests --test stage1_catalog authorized_demo_catalog_acceptance -- --ignored --nocapture` | `acctLv=2`、`posMode=long_short_mode`；2 个产品、3 个策略绑定通过账户许可、公共规格、费率组和时效检查。请求仅为固定端点 GET，无下单或撤单。 |
| 工作区单元门禁 | `cargo nextest run --offline --workspace -E 'not test(isolated_services_accept_roundtrips)'` | 37 项通过；基础设施连通性、真实模拟盘与独立 PG 测试按各自环境单独运行。 |
| 依赖审计 | `cargo deny check` | advisories、bans、licenses、sources 均通过；将 `time` 锁定到 0.3.47，仍有非阻断的传递依赖重复告警。 |

## 未覆盖范围

- 真实净仓账户和 US 地区模拟盘未现场验证；两种持仓模式的解析与绑定通过假服务、离线样例验证。其他产品规格的极值尚未全部与 Rust `NUMERIC(38,18)` 上限对照。
- 此阶段没有应用 runner、PostgreSQL 业务迁移、Kafka 行情或任何下单/撤单路径。自动交易与实盘均不可用。本轮只读请求没有生成订单，也没有市场数据缺口记录。
- 本地 `.env.trader.local`、`config/demo.local.toml` 和 `secrets/okx-demo/` 均被 Git 忽略；只读验收结果与配置摘要记录于此，不保存原始账户响应或密钥。

## 复验方式

按 [`integration-tests/README.md`](../integration-tests/README.md)提供本仓库的非敏感接线与受保护的模拟盘密钥文件，再运行显式门禁的只读测试。账户模式、地区或产品集合变化时，应保存新的脱敏运行窗口、配置摘要和通过/失败原因；验收失败时维持只读范围，不自动切换账户模式或开启交易。
