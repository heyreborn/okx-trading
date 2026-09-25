# OKX 功能与官方 API 依据

资料检索日期：2026-09-24。范围为 OKX API v5 的模拟盘、SPOT 与 USDT 本位线性 SWAP。本文是实现前的官方文档入口和核对清单，不是固定的 API 版本快照。本次已核对官方变更日志及可检索的 API 章节；主 API 页体积过大，未在本次工具会话中逐个加载全部章节正文，因此表中的字段和限速仍需在对应功能编码前逐项复核。每次实现或修改 OKX 适配器时，必须重新打开对应的 [API v5 官方文档](https://www.okx.com/docs-v5/en/)与[官方变更日志](https://www.okx.com/docs-v5/log_en/)，记录当日适用的账户模式、地区、权限、字段单位、限速、错误码和模拟盘差异。官方章节变更或链接失效时，先更新本索引和测试依据，再编码。

表中的“功能参考”指本项目的职责文档：[架构功能矩阵](architecture.md#功能归属与验收矩阵)、[产品与策略身份及绑定](product-strategy-model.md)、[契约与不变量](contracts-and-invariants.md)、[故障恢复](failure-and-recovery.md)、[测试策略](testing-strategy.md)。策略信号、组合、风险政策、Kafka、PostgreSQL 和 Parquet 属于本系统，没有对应的 OKX 业务 API；它们只能使用表中列出的 OKX 事实作为输入。

### DEV-011 只读目录核对（2026-09-25）

已重新核对[公共产品目录](https://www.okx.com/docs-v5/en/#public-data-rest-api-get-instruments)、[账户产品目录](https://www.okx.com/docs-v5/en/#trading-account-rest-api-get-instruments)与[变更日志](https://www.okx.com/docs-v5/log_en/)。公共规格中的 `tickSz` 是价格步长；`lotSz` 与 `minSz` 在 SPOT 为基础币数量、在衍生品为合约张数；SWAP 还要核对 `ctType=linear`、`ctVal`、`ctValCcy` 与结算币种。`state=post_only` 不能当作普通 `live`；变更日志新增的价格带字段不能代替实时限价接口。账户目录需要 Read 权限，文档给出的限速为每用户和产品类别 2 秒 20 次。`crates/instrument` 只校验已解析事实；地区域名、HTTP 错误码、分页及模拟盘响应样本在 DEV-012 适配器和 DEV-013 现场验收中继续核对。当前没有账户凭据，因此尚未确认实际账户模式或账户产品许可。

## 接入与连接

| 系统功能 | 官方 API / 章节 | 功能参考与实现前核对 |
| --- | --- | --- |
| 模拟盘 REST/WS 端点与隔离 | [Demo Trading Services](https://www.okx.com/docs-v5/en/#overview-demo-trading-services)、[WebSocket 总览](https://www.okx.com/docs-v5/en/#overview-websocket) | `okx-client`：模拟盘密钥、WS 地址和 REST `x-simulated-trading: 1`；禁止回退到实盘。按账户地区确认 REST/WS 域名。 |
| REST 签名、时间与服务端时钟 | [REST Authentication](https://www.okx.com/docs-v5/en/#overview-rest-authentication)、[Get system time](https://www.okx.com/docs-v5/en/#public-data-rest-api-get-system-time) `GET /api/v5/public/time` | `okx-client`：签名原文、时间戳格式、时钟偏差与日志脱敏；假服务验证签名。 |
| 私有 WS 登录、订阅和连接管理 | [WebSocket Login](https://www.okx.com/docs-v5/en/#overview-websocket-login)、[WebSocket Overview](https://www.okx.com/docs-v5/en/#overview-websocket-overview) | `okx-client`：登录确认、ping/pong、订阅确认、重连及失联门禁。 |
| REST/WS 请求限速与交易账户限额 | [Rate Limit](https://www.okx.com/docs-v5/en/#overview-rate-limit)、[Get account rate limit](https://www.okx.com/docs-v5/en/#order-book-trading-trade-get-account-rate-limit) `GET /api/v5/trade/account-rate-limit` | `okx-client`：逐接口及账户限速，区分读取与下单预算；确认错误码和重试等待。 |
| 交易所维护状态 | [System Status](https://www.okx.com/docs-v5/en/#status-get-status) `GET /api/v5/system/status` | 各用例 crate 的 health module：维护信息是健康输入，不能代替实际行情、私有流和账户事实检查。 |

## 产品目录与行情

| 系统功能 | 官方 API / 频道 | 功能参考与实现前核对 |
| --- | --- | --- |
| 账户可交易产品与公共规格 | [Account instruments](https://www.okx.com/docs-v5/en/#trading-account-rest-api-get-instruments) `GET /api/v5/account/instruments`、[Public instruments](https://www.okx.com/docs-v5/en/#public-data-rest-api-get-instruments) `GET /api/v5/public/instruments` | `instrument`：核对 `instId`/`instType` 与内部 `product_id` 唯一映射；白名单叠加账户实际可交易范围；核对 `ctVal`/币种、`lotSz`、`minSz`、`tickSz`、状态和 `groupId`。发现不等于启用；策略类型/实例和绑定属于内部配置。 |
| 产品状态和规格变化 | [Instruments channel](https://www.okx.com/docs-v5/en/#public-data-websocket-instruments-channel) | `instrument`：规格变化使旧订单计划失效；重新加载并核对开放订单。 |
| 价格限制 | [Get price limit](https://www.okx.com/docs-v5/en/#public-data-rest-api-get-price-limit) `GET /api/v5/public/price-limit`、[Price limit channel](https://www.okx.com/docs-v5/en/#public-data-websocket-price-limit-channel) | `planning`：限价输入要有来源时间；不能用本地固定偏离率代替交易所当前限制。 |
| K 线订阅、启动预热与历史补查 | [Candlesticks channel](https://www.okx.com/docs-v5/en/#order-book-trading-market-data-ws-candlesticks-channel)、[Get candlesticks](https://www.okx.com/docs-v5/en/#order-book-trading-market-data-get-candlesticks) `GET /api/v5/market/candles`、[Get candlesticks history](https://www.okx.com/docs-v5/en/#order-book-trading-market-data-get-candlesticks-history) `GET /api/v5/market/history-candles` | `market`/`strategy`：确认周期、`confirm`、分页和历史窗口；未收盘 K 线不能当作最终值。 |
| 严格逐笔与聚合成交 | [All trades channel](https://www.okx.com/docs-v5/en/#order-book-trading-market-data-ws-all-trades-channel) `trades-all`、[Trades channel](https://www.okx.com/docs-v5/en/#order-book-trading-market-data-ws-trades-channel) `trades`、[Trades history](https://www.okx.com/docs-v5/en/#order-book-trading-market-data-get-trades-history) `GET /api/v5/market/history-trades` | `market`：确认业务/公共 WS 路径、聚合语义、trade ID 与 `seqId`；不能把聚合成交当严格逐笔。 |
| 最优报价与五档盘口 | [Order book channel](https://www.okx.com/docs-v5/en/#order-book-trading-market-data-ws-order-book-channel) `bbo-tbt`、`books5`；[REST order book](https://www.okx.com/docs-v5/en/#order-book-trading-market-data-get-order-book) `GET /api/v5/market/books` | `market`/`planning`：核对快照频率、价格/数量单位和新鲜度；REST 快照不能补齐缺失的推送事件。 |
| 增量盘口与断线重建（启用时） | [Order book channel](https://www.okx.com/docs-v5/en/#order-book-trading-market-data-ws-order-book-channel) `books`、[Order book guide](https://www.okx.com/docs-v5/trick_en/) | `market`：快照与 `seqId`/`prevSeqId` 连续性；缺口重新建簿。`checksum` 已废弃，不能用固定 0 做完整性判断。 |
| 标记价与指数价格 | [Mark price channel](https://www.okx.com/docs-v5/en/#public-data-websocket-mark-price-channel)、[Index tickers channel](https://www.okx.com/docs-v5/en/#public-data-websocket-index-tickers-channel) | `planning::swap`：核对来源时间和币种；标记价、指数价和成交价不混用。 |
| 预测与历史资金费率 | [Funding rate channel](https://www.okx.com/docs-v5/en/#public-data-websocket-funding-rate-channel)、[Get funding rate](https://www.okx.com/docs-v5/en/#public-data-rest-api-get-funding-rate) `GET /api/v5/public/funding-rate`、[Get funding rate history](https://www.okx.com/docs-v5/en/#public-data-rest-api-get-funding-rate-history) | `planning::swap`：核对费率机制、预计结算时刻和字段单位；预测值不是实际扣费。 |

上述频道的 `event_id` 构造、WS/REST 是否指向同一源事件及修订语义，逐项按 [行情事件身份与采集接管](market-event-identity.md#两种身份与消息信封)记录证据。表内的 `tradeId`、`seqId`、K 线周期等仅是待核对的身份候选；不能因字段同名就推断跨产品、频道或重连后唯一。

## 账户、费用与交易条件

| 系统功能 | 官方 API / 频道 | 功能参考与实现前核对 |
| --- | --- | --- |
| 账户等级、持仓模式及交易设置 | [Get account configuration](https://www.okx.com/docs-v5/en/#trading-account-rest-api-get-account-configuration) `GET /api/v5/account/config` | `instrument`：读取实际 `acctLv`、`posMode`、手续费扣费方式、自成交保护及借币相关字段；不自动调用[设置持仓模式](https://www.okx.com/docs-v5/en/#trading-account-rest-api-set-position-mode)。 |
| 余额、权益与可用额 | [Get balance](https://www.okx.com/docs-v5/en/#trading-account-rest-api-get-balance) `GET /api/v5/account/balance`、[Account channel](https://www.okx.com/docs-v5/en/#trading-account-websocket-account-channel) | `risk`：保留币种和估值来源；推送加快更新，启动和断线后仍以 REST 核对。 |
| 净仓、双向仓及实时变化 | [Get positions](https://www.okx.com/docs-v5/en/#trading-account-rest-api-get-positions) `GET /api/v5/account/positions`、[Positions channel](https://www.okx.com/docs-v5/en/#trading-account-websocket-positions-channel)、[Balance and position channel](https://www.okx.com/docs-v5/en/#trading-account-websocket-balance-and-position-channel) | `accounting`/`risk`：按 `posSide`、`mgnMode`、`posId` 核对；外部仓位计入风险，不自动归属策略。 |
| 实际适用费率 | [Get fee rates](https://www.okx.com/docs-v5/en/#trading-account-rest-api-get-fee-rates) `GET /api/v5/account/trade-fee`、[官方费率页面](https://www.okx.com/fees) | `planning`：按 `instType`、产品或 `instFamily`、`groupId` 对应费率组；核对可能另行公布的零费率。预计费用不得覆盖实扣。 |
| 已设置杠杆与仓位档位 | [Get leverage](https://www.okx.com/docs-v5/en/#rest-api-account-get-leverage) `GET /api/v5/account/leverage-info`、[Get position tiers](https://www.okx.com/docs-v5/en/#rest-api-public-data-get-position-tiers) `GET /api/v5/public/position-tiers` | `planning::swap`：按逐仓模式、产品与侧别核验实际杠杆和档位；程序不自动设置杠杆。 |
| 最大下单量与最大可用资金 | [Get maximum buy/sell amount or open amount](https://www.okx.com/docs-v5/en/#trading-account-rest-api-get-maximum-buy-sell-amount-or-open-amount) `GET /api/v5/account/max-size`、[Get maximum available tradable amount](https://www.okx.com/docs-v5/en/#trading-account-rest-api-get-maximum-available-tradable-amount) `GET /api/v5/account/max-avail-size` | `planning`/`risk`：叠加本地在途预留；接口数值不是账户并发批准额度。 |
| 实际成交与手续费 | [Transaction details (last 3 days)](https://www.okx.com/docs-v5/en/#rest-api-trade-get-transaction-details-last-3-days) `GET /api/v5/trade/fills` | `accounting`：逐笔成交 ID、价格、费用额及费用币种幂等入账；核对分页和有限查询窗口。 |
| 实际资金费、余额变动与账单 | [Bills details (last 7 days)](https://www.okx.com/docs-v5/en/#trading-account-rest-api-get-bills-details-last-7-days) `GET /api/v5/account/bills` | `accounting`：按账单类别、产品、侧别与币种核对实际费用；历史窗口外须依赖持续归档的本地审计。 |
| 较长停机后的成交/账单补查 | [Transaction details (last 3 months)](https://www.okx.com/docs-v5/en/#rest-api-trade-get-transaction-details-last-3-months) `GET /api/v5/trade/fills-history`、[Bills details (last 3 months)](https://www.okx.com/docs-v5/en/#trading-account-rest-api-get-bills-details-last-3-months) `GET /api/v5/account/bills-archive` | `trader_usecases`/`accounting`：核对查询窗口、分页和模拟盘支持；超过官方窗口需要本地持续审计，不能推断无成交或无费用。 |

## 订单、回报与恢复

| 系统功能 | 官方 API / 频道 | 功能参考与实现前核对 |
| --- | --- | --- |
| 现货、净仓与双向永续下单 | [Place order](https://www.okx.com/docs-v5/en/#order-book-trading-trade-post-place-order) `POST /api/v5/trade/order` | `planning`/`okx-client`：按产品核对 `tdMode`、`side`、`posSide`、`reduceOnly`、`ordType`、`sz`、`px`；限价、IOC、post-only 各有不同失败语义。检查顶层 `code` 与逐订单 `sCode`。 |
| 自成交保护及账户默认值 | [Place order](https://www.okx.com/docs-v5/en/#order-book-trading-trade-post-place-order)、[Account configuration](https://www.okx.com/docs-v5/en/#trading-account-rest-api-get-account-configuration) | `planning`：读取账户默认保护及订单允许覆盖范围；自成交取消属于订单状态，不是正常成交。 |
| 撤销未完成订单 | [Cancel order](https://www.okx.com/docs-v5/en/#order-book-trading-trade-post-cancel-order) `POST /api/v5/trade/cancel-order` | `trader_usecases`：`sCode=0` 只表示撤单请求被接受；随后查询或等待 `orders` 终态，不提前释放预留。 |
| 实时订单及部分成交回报 | [Order channel](https://www.okx.com/docs-v5/en/#order-book-trading-trade-ws-order-channel) `orders` | `order::state`：首次订阅无订单快照；允许重复、跳过中间状态和先终态后补查。`fills` 私有频道有等级限制，首版不依赖它。 |
| 超时/重启后的订单核对 | [Order details](https://www.okx.com/docs-v5/en/#order-book-trading-trade-get-order-details) `GET /api/v5/trade/order`、[Pending orders](https://www.okx.com/docs-v5/en/#order-book-trading-trade-get-order-list) `GET /api/v5/trade/orders-pending`、[Order history (7 days)](https://www.okx.com/docs-v5/en/#order-book-trading-trade-get-order-history-last-7-days) `GET /api/v5/trade/orders-history`、[Order history (3 months)](https://www.okx.com/docs-v5/en/#order-book-trading-trade-get-order-history-last-3-months) `GET /api/v5/trade/orders-history-archive` | `trader_usecases`：按 `clOrdId` 和已知 `ordId` 联合核对，考虑历史查询窗口。`clOrdId` 仅要求在当前未完成订单中唯一，不能假定交易所永久去重；本地永不复用并保存远端 `ordId`。未知 POST 不盲目重发。 |
| 合约冷静期拒单 | [Place order](https://www.okx.com/docs-v5/en/#order-book-trading-trade-post-place-order)、[2026-07-07 变更日志](https://www.okx.com/docs-v5/log_en/) | `trader_usecases`：适用合约开仓可能以 HTTP 200 / 业务码 `54094` 拒绝；暂停相关产品并核对，不按网络成功判定下单成功。 |

## 当前已核对的变更

- [2026-05-20 REST 域名变更](https://www.okx.com/docs-v5/log_en/)：OKX Global 推荐 `https://openapi.okx.com`；地区域名与 WS 域名分别处理，不把 Global 地址写死给所有账户。
- [2025-11-25 手续费分组变更](https://www.okx.com/docs-v5/log_en/)：产品 `groupId` 与 `trade-fee.feeGroup` 关联；旧顶层 `maker`/`taker` 等字段列为将废弃，规划器须按实际返回的产品费率组核对。
- [2025-07-08 及 2026-08-20 订单频道变更](https://www.okx.com/docs-v5/log_en/)：订单推送可重复且 `uTime` 可能不同；`post_only` 失败时可能仅推送 `canceled`，没有先前的 `live`。
- [2026-06-23 盘口变更](https://www.okx.com/docs-v5/log_en/)：增量盘口 `checksum` 固定为 0，不再用于完整性验证；使用序列连续性。`bbo-tbt`/`books5` 不受该字段影响。
- [2026-07-07 合约冷静期变更](https://www.okx.com/docs-v5/log_en/)：适用产品的非 reduce-only 订单可被业务码 `54094` 拒绝，即使 HTTP 状态为 200。
- [2025-09-17 现货费用币种变更](https://www.okx.com/docs-v5/log_en/)：账户 `feeType` 可改变扣费币种；实际入账以成交/账单的费用币种为准。

## 每项功能编码前的核对记录

在对应变更或设计记录中填入：功能与表中官方章节链接、查询日期、适用地区及模拟盘账户模式、请求/响应字段与单位、认证权限、当前限速、分页/历史窗口、WS 首次推送与重连语义、业务错误码、相关变更日志、脱敏测试样本和负责人。缺少其中影响正确性的事实时，该功能保持只读或拒绝交易。链接本身不是验收证据；必须对照官方当日内容和模拟盘实际响应完成测试。
