# 技术决策

状态：设计基线，2026-09-24。本文说明首版多机模拟盘系统的技术选择。库版本在创建 workspace 时锁定于 `Cargo.lock`，上线前以兼容性、故障和容量测试确认；此处不把当前最新版本当成永久约束。职责划分见 [架构与功能归属](architecture.md)，性能预算与验证见 [性能与容量设计](performance-and-capacity.md)，配置及观测边界见 [配置、环境与观测规范](configuration-and-observability.md)。

## 选择原则

1. 先保证订单事实可恢复、账户额度不重复花费，再优化吞吐和部署便利。
2. 业务算法保持同步纯计算；网络、数据库和队列只出现在用例端口及适配器之后。
3. 多机系统允许至少一次行情交付和重放；绝不声称 PostgreSQL、Kafka 与 OKX 之间存在跨系统原子提交。
4. 以明确的精度、顺序、失败语义和运维能力选型；不要求基础设施服务端由 Rust 编写。

## 决策记录

| 决策 | 选择与理由 | 暂未采用的方案及取舍 | 验证点 |
| --- | --- | --- | --- |
| Rust 工程 | Stable Rust、2024 edition、Cargo 虚拟 workspace、`resolver = "3"`；共享 `Cargo.lock`、依赖及 lint。 | 单 crate 会混合业务和外部依赖；每个 module 一个 crate 会使边界过细。 | `cargo check --workspace --all-targets`、依赖图无环。 |
| 用例与端口边界 | `apps/{ingest,observe,trader}` 各有 `usecases` 库和 `runner` binary 两个 package；端口 trait 放在各自 `usecases/src/ports.rs`，由 `runner` 的本地适配器包装类型实现。`crates/` 放独立领域规则、跨应用契约与适配器；归档、通知、管理初版各一个 binary package，`admin` 作为运维 CLI。 | 单一 package 的库和入口共享依赖声明，不能由 Cargo 强制业务与适配器隔离；全局 `ports` crate 容易汇集不相关用例的接口。分成两个 package 增加 manifest 和包装代码。 | 检查 Cargo 依赖图：`usecases` 不依赖具体适配器，适配器不依赖 `usecases`；`runner` 同时依赖两者并完成组装。 |
| 交易领域边界 | `instrument`、`portfolio`、`planning`、`risk`、`order`、`accounting` 分别负责规格、目标合并、候选、额度、订单状态和成交归属；跨域稳定类型由 `model` 定义。 | 单一 `trading` crate 会扩大变更及编译边界；继续拆到每个 SPOT/SWAP module 则增加类型转换和维护成本。 | 依赖图无环；规划不写预留、风险不操作数据库、成交归属不自行提交事务。 |
| 本机安装位置 | PG、Kafka、对象存储及独立观测服务由 Docker Compose 运行；Rust 构建/测试先直接探测本机命令，找不到才检查 Mise/`PATH`，仍不可用才用固定 builder 容器。 | 数据服务原生安装易产生状态漂移；直接使用已安装 Rust 工具链可减少构建开销。单机 Compose 不提供跨宿主机高可用。 | 本机 `rustc`/`cargo` 版本探测、`docker compose config`、空卷启动、重启保留及备份恢复。 |
| 异步运行 | Tokio；每个进程有任务监督、取消令牌、有界队列和超时预算。 | 自建线程池增加生命周期和背压复杂度。 | 关闭、重连和队列满时任务可终止且状态可观测。 |
| HTTP | 复用 `reqwest::Client`，统一签名、限速、超时及错误分类；默认不自动重试下单 POST。 | 每请求新建客户端会增加连接与 TLS 开销。 | 假服务覆盖签名、限速、超时、未知结果和敏感字段脱敏。 |
| WebSocket | `tokio-tungstenite`，公共/业务流与私有流分开连接和健康状态。 | 自行实现 WS 协议没有业务收益。 | ping/pong、重订阅、序列缺口、登录和断线恢复。 |
| 行情日志 | 多节点 Kafka，Rust 客户端 `rdkafka`；同产品使用稳定 key，消费者手动提交位点。 | JetStream 的 Rust 客户端和部署体验较好，但此处按产品分区、消费组重平衡、独立归档/观察进度更适合 Kafka。没有选定特定 Kafka 兼容发行版。 | 真实集群验证重平衡、位点、突发背压、磁盘与副本故障。 |
| 权威账本 | 高可用 PostgreSQL、SQLx、SQL 迁移；账户行锁和唯一约束保护批准事务。 | SQLite 不适合多机共享写入与账户级并发批准。 | 并发预留、故障切换、恢复与备份还原。 |
| feed 单活发布权 | PostgreSQL 中独立的 feed 租约/代次记录，`ingest` 使用仅限该记录的凭据；事件携带代次。 | 固定节点分配缺少自动故障接管；租约不能撤销已在途的 Kafka 消息，消费者仍需隔离旧代次并重建可信状态。 | 双节点竞争、旧发布者迟到、PG 不可用和确认未知后崩溃。 |
| 策略实例单活与目标提交 | PostgreSQL 保存实例所有权代次；状态检查点、输入水位、目标与 outbox 同事务，Kafka 位点随后确认。 | Kafka 分区分配无法独自限定跨产品实例的唯一执行者，也无法与 PG 目标原子提交。 | 双观察者竞争、跨分区输入、PG 提交后崩溃、旧目标迟到和确定性回放。 |
| 精确小数 | `BigDecimal` 作为解析与存储计算基础，外层定义 Money、Price、BaseQuantity、Contracts、Ratio；每个字段限制精度、范围和舍入。 | `f64` 不适合订单金额；`rust_decimal` 的有效位数可能小于数据库允许范围。 | 最大值、极小值、乘除中间值、步长取整和 PG 往返测试。 |
| 历史归档 | Arrow/Parquet 不可变文件，S3 兼容对象存储，`object_store` 负责对象访问。 | 节点本地文件系统不能直接作为多机共享归档。 | 对象写入中断、重复消费、清单一致性、数据保留与读取。 |
| 离线查询 | DataFusion 查询归档；在线策略引擎不依赖查询引擎。 | 为离线研究引入独立数据库会增加事实副本和运维成本。 | 大窗口查询、缺口识别、事件去重与内存上限。 |
| 配置 | `serde` 严格解析版本化 TOML；进程环境只承载部署参数及密钥引用。 | 无约束环境覆盖会使多机节点执行不同的风险政策。 | 未知字段、版本摘要冲突、缺凭据和滚动发布测试。 |
| 观测 | `tracing` + `tracing-subscriber` 输出 JSON 标准日志；独立指标和健康接口。事件 ID 用于日志关联，PG 审计记录保存交易事实。 | 纯文本日志难以关联跨机事件；把日志当审计账本不能保证持久性和事务一致性。 | 延迟、积压、未知订单、暂停原因、标签基数、审计一致性及敏感信息泄漏检查。 |

资料入口：[Cargo workspace](https://doc.rust-lang.org/cargo/reference/workspaces.html)、[Tokio](https://tokio.rs/tokio/tutorial)、[reqwest](https://docs.rs/reqwest/latest/reqwest/struct.Client.html)、[tokio-tungstenite](https://docs.rs/tokio-tungstenite/latest/tokio_tungstenite/)、[rdkafka](https://docs.rs/rdkafka/latest/rdkafka/)、[SQLx](https://docs.rs/sqlx/latest/sqlx/struct.Transaction.html)、[tracing-subscriber](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/)、[Arrow Parquet](https://arrow.apache.org/rust/parquet/index.html)、[object_store](https://docs.rs/object_store/latest/object_store/)、[DataFusion](https://datafusion.apache.org/)。

## 与实施协议的对应选择

选库不等于确认行为。以下选择以 [实施协议](implementation-protocols.md) 的具体失败边界为依据；创建 workspace 时再锁定相互兼容的版本及 feature，先做最小集成原型。

| 关注点 | Rust 组件与用法 | 选择原因及限制 | 确认方法 |
| --- | --- | --- | --- |
| 异步生命周期 | Tokio 有界 `mpsc`、`JoinSet`；`tokio-util` 的 `CancellationToken`/`TaskTracker` | 可以限制积压并等待任务退出；取消只停止本地 Future，不撤销已发出的 OKX 请求。Tokio `select!` 的每个分支仍须逐一检查取消安全性。 | 注入队列满、任务退出、取消、重启和关闭超时，检查事件归属与健康状态。 |
| HTTP 与 TLS | 复用 `reqwest::Client`，明确启用 rustls 后端；客户端构造时限定代理、重定向、主机和超时策略 | 连接池适合频繁只读请求；默认请求重试不适用于未知结果的交易 POST。TLS 证书校验不能被测试配置静默关闭。 | 假 OKX 校验签名和超时；错误域名、证书及代理配置必须启动失败。 |
| WS 与 TLS | `tokio-tungstenite` 的 rustls feature，公共/业务/私有连接各自维护状态 | 与 Tokio 统一；连接状态、订阅状态和数据可信状态必须分别跟踪。 | 断线、重订阅、ping/pong、序列缺口及私有 REST 恢复测试。 |
| 金额与数量 | `BigDecimal` + `model` 强类型包装 + 显式 `Context`/舍入规则 | 支持超过固定小数常见精度的输入；任意精度本身不会限制内存或自动给出正确舍入，不能依赖库的默认精度。 | 极值、非法精度、除法、步长取整、分摊余数、SQL NUMERIC 往返。 |
| 本地并发与接管 | SQLx 事务 + PostgreSQL 行锁/唯一约束；代次和 outbox 保存在 PG | 数据库是账户批准的共享协调点；进程锁和 Kafka 分区所有权不能保护账户额度。代次不能防止旧节点已在途的 OKX POST。 | 真实 PG 双执行者竞争、锁等待、主库切换、旧节点恢复和远端订单核对。 |
| 行情分发 | `rdkafka` 的异步生产者/消费者 + Kafka 集群，按产品 key 分区 | Kafka 提供持久日志、分区顺序与独立消费组；`rdkafka` 基于 librdkafka，需验证目标部署的原生依赖和构建方式。跨分区仍无全局顺序。 | Broker 确认、重平衡、重复消费、位点回退、突发背压及节点故障。 |
| 归档与研究 | Arrow/Parquet + `object_store`；研究使用 DataFusion | 对象存储便于多节点读取，Parquet 保留可移植列式数据；对象写入和 Kafka 位点无共同事务。 | 对象写入中断、清单/事件 ID 去重、查询一致性与保留窗口演练。 |
| 日志与告警 | `tracing` 结构化 span/event；具体指标导出后端由部署确定 | 以账户、决策、订单和事件 ID 串起跨进程证据，避免将可观测性后端耦合到业务 crate。 | 故障测试中核对暂停原因、指标及密钥/账户数据脱敏。 |
| 测试环境 | `proptest`、Nextest；PG/Kafka/对象存储集成测试由 Compose 启动；Rust 命令直接用本机工具链，找不到时检查 Mise 激活，仍不可用时使用固定 builder 容器；另设真实多节点故障环境。 | 性质测试验证不变量，Compose 验证实际服务协议；单机容器不能证明跨宿主机网络分区和主从切换。 | CI 分层运行，进程接管与灾难恢复必须在多节点环境单独通过。 |

参考：[Tokio 有界队列](https://docs.rs/tokio/latest/tokio/sync/mpsc/)、[Tokio 任务跟踪](https://docs.rs/tokio-util/latest/tokio_util/task/struct.TaskTracker.html)、[BigDecimal Context](https://docs.rs/bigdecimal/latest/bigdecimal/struct.Context.html)、[reqwest TLS 配置](https://docs.rs/reqwest/latest/reqwest/struct.ClientBuilder.html)、[Docker Compose 规范](https://docs.docker.com/compose/compose-file/)。

若最小原型发现某组件不能满足确认方法，先记录可复现失败、影响的协议边界和替代组件，再更新本决策与测试。不能只因基准吞吐更高就替换账本、订单身份或恢复机制。

## 多机部署决定

- `ingest`、`archive-worker`、`observe` 可多副本运行，按产品和消费分区分担工作。公共行情有来源会话标识；副本切换后不能假定行情连续。
- `trader` 按账户分片；同一账户只有一个有权批准和发送的执行者，其他副本待命。实际下单前需通过账户所有权、远端事实和私有流门禁；接管期间先暂停新单并对账。
- Kafka 用于短期持久行情日志，Parquet 对象存储用于长期归档；二者保留期独立。Kafka 不承载订单权威事实。
- 本机验证全部独立服务由 `deploy/compose/` 的 Compose 文件启动，不在宿主机原生安装；PG/Kafka/对象存储可各先运行单节点。多机阶段每台主机用 Compose 管理本机容器，并另行配置跨故障域复制、备份及故障转移。副本数、保留期、分区数、限流和队列容量在负载与故障试验后确定。详见 [部署约定](compose-deployment.md)。
- 初版只用模拟盘。生产拓扑、网络代理、密钥管理、告警渠道及对象存储供应方式在部署决策中确定，不写入业务 crate。

## 版本与替换规则

依赖版本由根 workspace 统一声明并提交 `Cargo.lock`。升级先检查 Rust 最低版本、协议兼容与安全公告，再运行完整 CI、真实基础设施集成测试和相关故障测试。更换 Kafka 实现、PostgreSQL 驱动或对象存储后端时，先满足端口契约与恢复测试，不能只验证 API 编译成功。记录影响数据格式或消息身份的变更，并提供显式迁移或拒绝旧版本的路径。
