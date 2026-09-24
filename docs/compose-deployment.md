# Docker Compose 部署约定

状态：设计基线。本机验证环境的 PostgreSQL、Kafka、S3 兼容对象存储及其他独立服务一律运行在本机 Docker 容器中，由 Docker Compose 管理；不在宿主机原生安装数据库、Broker、对象存储或观测服务。宿主机需要 Docker Engine、Compose 插件及必要的网络/存储权限；Rust 构建和测试可优先使用本机 Mise 管理的工具链。当前仍处于文档阶段，`deploy/compose/` 尚无可启动的 Compose 文件；实现版本、配置、健康检查和恢复演练确定后再提交。

## 本机验证拓扑

| Compose 服务 | 用途 | 数据与暴露边界 |
| --- | --- | --- |
| `postgres` | 权威账本、账户/采集/观察所有权代次、检查点、outbox | 独立持久卷；仅应用内部网络可访问，调试时才临时绑定宿主机回环地址。 |
| `kafka` | 持久行情日志与独立消费组 | 独立持久卷；明确容器内及必要的宿主机监听地址，客户端使用与自身网络位置相符的地址。 |
| `object-store` | S3 兼容 Parquet 归档与清单 | 独立持久卷；归档 bucket 初始化由一次性容器任务完成。 |
| `migrate`、`bootstrap` | 数据库迁移、队列 topic 和对象存储初始化 | 一次性容器任务，幂等且有版本检查；只给任务所需权限。 |
| `ingest`、`archive-worker`、`observe`、`trader`、`notifier`、`admin` | Rust 应用进程 | 使用项目构建的镜像；按角色注入凭据；`trader` 默认只读/自动交易关闭，仅模拟盘。 |
| 可选 `metrics`/日志收集/告警组件 | 本机排障与观测验证 | 使用 Compose profile 启用；不影响 PG 审计事实与交易恢复。 |

首个本机 Compose 部署可用单节点 PG、Kafka 和对象存储验证功能、协议及重启恢复。单机容器和多副本应用不能证明宿主机故障、网络分区、PG 主库切换或 Kafka 跨故障域复制；这些仍在多机器隔离环境验收。Compose 文件由 `deploy/compose/compose.yaml` 定义服务、网络、卷、健康检查和可选 profile；基础镜像用明确版本/摘要固定，不用浮动 `latest`。发布镜像仍用可复现的多阶段容器构建；本机日常构建和测试优先直接使用本机 Rust 命令。

## Rust 构建与测试工具链

在执行 Rust 构建、格式化、lint 或测试前，先在项目根目录直接运行 `rustc --version`、`cargo --version` 并检查所需 Cargo 子命令，与计划中的 `rust-toolchain.toml`/锁定版本核对。版本兼容时直接使用本机 `cargo`、`rustc`，执行命令不加 `mise exec`。本机 Rust 构建允许使用宿主机 C 编译器/linker（`cc`，如 Ubuntu `build-essential`）；它是编译工具，不承担数据库、队列或对象存储服务。不要在探测中自动下载或升级工具链。某项工具缺失、版本不兼容或本机不可用时，再使用固定版本的 builder 容器运行该项命令；容器与本机使用同一源码、`Cargo.lock` 和 CI 门禁。

只有直接命令找不到时，才检查 `mise ls --installed rust`、`mise which rustc` 和当前 shell 的 `PATH`/激活状态。若工具链已安装但当前 shell 继承了旧环境，刷新或重新进入已激活 Mise 的 shell，再直接运行 `rustc`、`cargo`；不把“未进入 `PATH`”误判成“未安装”。同时检查 `cargo fmt`、`cargo clippy`、`cargo nextest`、`cargo deny` 等实际需要的子命令及系统 linker（如 `cc`）。探测应在目标 workspace 目录进行，因为 Mise 激活配置可能按目录变化。linker 或所需原生库缺失时可使用固定 builder 容器；PostgreSQL、Kafka、对象存储及独立观测服务仍必须由 Compose 启动。

## 配置、网络与数据

- 容器间使用 Compose 网络的服务名连接；从宿主机访问时使用明确的回环端口映射。`localhost` 在容器内指当前容器，不用它表示另一服务。生产/多机的服务地址由部署配置注入，不写死在业务代码。
- Compose `.env` 仅用于非敏感变量替换；实际 OKX、PG、Kafka 和对象存储凭据由未跟踪的密钥文件、受控注入或部署秘密源提供，并按服务授予。不得提交真实值、把它们写进镜像层或用 `docker compose config` 的输出作为公开日志。Compose secrets 是容器内文件访问机制，其来源文件/环境仍需单独保护与轮换。
- PG、Kafka 和对象存储分别使用持久卷；`docker compose down` 不应被当作备份。卷容量、数据保留、备份频率、离机备份和恢复目标在上线前冻结。初始化和迁移前备份账本；恢复后重新核对 OKX 账户、订单与归档清单。
- 健康检查区分“进程/端口可用”和“业务事实可信”。`depends_on` 的健康条件只用于启动顺序；应用仍需在运行中监测连接、时钟、Kafka lag、feed 连续性及账户事实，服务重启不能自动解除交易暂停。
- 只发布确有必要的管理端口，优先绑定本机回环地址；跨主机通信配置 TLS、认证、最小权限和防火墙。容器日志写标准输出并由 Docker/集中收集器接管；交易审计写 PG，不依赖容器日志保留。

`config/env.example` 是应用部署变量模板，不是 Compose 密钥文件。运行在 Compose 网络内时，其 PG/Kafka/对象存储地址使用服务名；本机调试容器外客户端时另提供受控的回环端口。业务 TOML 仍按 [配置与观测规范](configuration-and-observability.md)版本化，环境变量不得修改产品、风险或交易门禁。

## 多机运行

需要跨主机部署时，每台机器各自安装 Docker Engine/Compose，按主机角色运行独立的 Compose 项目或经验证的部署编排；应用镜像、配置版本与密钥引用一致。Compose 管理**该主机上的容器**，不能代替 PostgreSQL 的高可用、Kafka 副本/仲裁或对象存储跨故障域复制。基础设施成员、节点地址、复制、故障转移、备份和恢复由对应服务的集群配置与演练负责；不能把同一台宿主机上的多个容器算作多故障域。多机部署拓扑、服务发现和证书分发在首轮本机协议验证后冻结。

## 落地顺序与验收

1. 冻结服务镜像、版本/摘要、持久卷、监听地址、凭据来源与健康检查，提交 `deploy/compose/compose.yaml` 和非敏感模板；执行 `docker compose config` 验证解析。Rust 命令先直接探测本机工具链。
2. 在本机用 Compose 启动基础设施及只读应用，运行迁移/初始化容器任务；验证空卷启动、重启后数据保留、健康检查、错误凭据拒绝和日志脱敏。
3. 运行真实 PG/Kafka/对象存储集成测试，再演练容器强停、卷容量不足、备份恢复、版本升级/回退和模拟盘只读核对。
4. 多机验收另运行跨宿主机故障、复制/主从切换、网络分区与旧执行者恢复，不以本机 Compose 通过替代。

Compose 行为依据：[文件规范](https://docs.docker.com/compose/compose-file/)、[服务健康与依赖](https://docs.docker.com/reference/compose-file/services/)、[持久卷](https://docs.docker.com/reference/compose-file/volumes/)、[secrets](https://docs.docker.com/reference/compose-file/secrets/)、[生产部署说明](https://docs.docker.com/compose/how-tos/production/)。
