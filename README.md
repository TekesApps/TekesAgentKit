# TekesAgentKit

与 TekesClientKit 语义对齐的 Rust Agent 客户端套件。

**状态：架构评估与仓库骨架。尚未实现连接、Mirror、展示状态或 UI；当前编译成功不代表功能可用。**

- SessionEndpoint 为统一语义边界，Kernel / Pi / DSH 为独立适配 crate。
- 本地默认内存投影，远端默认内存投影加持久缓存；缓存策略与部署位置解耦。
- Presentation 拥有 Agent 展示状态，Components 未来以 TekesUI 组成业务界面。
- Host 拥有窗口、导航、产品主题/文案、场景参数、安装、凭证与系统权限适配。

参见 [架构评估](docs/assessment.md)、[开发阶段](docs/roadmap.md)、[核对来源](docs/source-baseline.json)。

```sh
cargo fmt --all --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

本轮无运行行为，因此未添加功能测试。所有 crate 暂不发布；尚未选定开源许可证。
开发使用 TekesUI 时必须显式指定路径覆盖；发布前必须改为已发布精确版本或不可变 Git revision。
