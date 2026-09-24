# Compose 部署目录

本目录将保存本机 Docker Compose 部署文件、非敏感参数模板及多机按角色的覆盖文件。当前尚未冻结服务镜像与可运行配置，因此没有 `compose.yaml`；本目录目前不可启动。

PostgreSQL、Kafka、S3 兼容对象存储、迁移任务和 Rust 应用以容器运行；宿主机不原生安装这些服务。Rust 日常构建和测试先直接使用本机 `cargo`/`rustc`；找不到时再检查 Mise 安装与激活。角色、数据卷、密钥、健康检查与多机限制见 [Docker Compose 部署约定](../../docs/compose-deployment.md)。
