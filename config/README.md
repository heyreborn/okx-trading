# 配置目录

这里保存可提交的非敏感配置模板。`env.example` 列出部署接线和密钥文件引用；按进程角色只设置所需字段。`crates/config::deployment` 已实现该环境接口。`example.toml` 是 `crates/config::business` 可解析的 schema 1 多产品、多实例样例，交易门禁关闭；样例账户和产品并非当前 OKX 远端事实。

业务配置放在版本化 TOML 中，包含产品、策略实例、风险预算及默认关闭的交易门禁。部署环境只提供节点身份、基础设施地址、日志过滤和密钥引用，不覆盖交易政策。真实密钥、本地 `.env`、生成日志及运行数据不提交仓库。完整规则见 [配置、环境与观测规范](../docs/configuration-and-observability.md)。
