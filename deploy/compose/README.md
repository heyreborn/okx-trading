# Compose 部署目录

`compose.yaml` 使用固定版本和摘要运行 PostgreSQL 17.6、Kafka 4.0.2、SeaweedFS 4.47。服务端口仅绑定宿主机回环地址，三份命名卷在 `stop`/`start` 后保留；`down -v` 会删除卷。此拓扑仅用于本机隔离验证，不代表多机高可用。

启动前，在调用进程的环境中提供 `DEV_PG_PASSWORD`、`DEV_S3_ACCESS_KEY`、`DEV_S3_SECRET_KEY`，不要写入跟踪文件。根目录执行 `docker compose -f deploy/compose/compose.yaml up -d --wait`，随后运行 `docker compose -f deploy/compose/compose.yaml --profile init run --rm kafka-init`。对象存储首次启动时创建 `okx-archive-dev` bucket。测试应使用独立 Compose 项目名和资源，不能连接交易环境。

Kafka 未开启鉴权，宿主端口仅在回环地址监听。`deploy/docker/Dockerfile` 是未来 runner 的锁定 Cargo 构建模板；当前尚无可执行应用，因此未产出应用镜像。更多边界见 [部署约定](../../docs/compose-deployment.md)。
