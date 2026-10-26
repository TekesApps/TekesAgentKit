# TekesUI #23 新公开输入的独立复验

本记录评估公开 commit
[`61ddf7c68fccd819e20a01ba3104a8a0f8d7e64f`](https://github.com/TekesApps/TekesUI/commit/61ddf7c68fccd819e20a01ba3104a8a0f8d7e64f)，
不是旧 2f090 归档的重复接收，也不是产品 pin 升级。正式输入仍为 4310937。

消费侧独立 clone 公共分支并核对 HEAD，取回公开 base 后确认仅 14 文件差异。
使用该 commit 的 `docs/agentkit/public-source-issue23.json` 校验 198 输入，SHA256 为
`31c66c8d033f75acb2c3745fd20a5b53e9bfc320e290a1959179f355b934324c`。
AgentKit 临时副本显式改用此 Git rev；实际 Cargo 解析的八个 TekesUI 包均来自该公开
SHA，逐文件核对 198 输入通过。产品 manifest/lock/tekesui-source.json 未升级。

在隔离 checkout/target，Rust 1.95.0、locked/offline 下，独立运行通过：

- plain_text_content 公开 API 3 tests；controls 核心 233 tests。
- 上游独立 composition consumer 134 tests。
- AgentKit 创建 WIP 源快照的 UI workspace 110 tests，其中包括第二公开 consumer 和
  一项新的公共 plain-content 探针。它不是当前产品 pin 的测试，也不是正式 Transcript。

[探针源码](../scripts/probes/tekesui-issue23-public.rs) 使用现有 RunController 的历史与
实际 fixture transient stream，逐项挂载 read_only_content，并确认正文在 Content 模式、
只读且未变形；实际增量受控追加后保留原 UTF-16 选区。没有另建 Run、编辑或滚动状态机。
复现时在隔离 AgentKit 副本中仅替换 UI Git rev，解析锁文件，再将该探针复制到
ui/demo/tests/public_plain_transcript_candidate.rs 并运行 UI workspace tests。
源快照、输入、日志 hash 与测试范围见 [机器收据](tekesui-issue23-public-review-20261020.json)。

但该 SHA 的远端 CI 失败。消费侧也独立复现：

```sh
cargo +1.95.0 test --locked --offline -p tekes-ui-controls \
  --test component_registry_api \
  every_classified_public_type_is_accessible_to_a_library_consumer -- --exact
```

该断言报告 public type surface changed；公共类型清单新增 TextSurfacePresentation，
生成绑定测试尚未登记。没有改生成文件、跳过断言或把局部 API/consumer 通过当成完整 CI。
建议修复并公开精确后继增量后再检查；当前不建议将此 SHA 提升为正式冻结输入。

这次消费侧未运行 GPU/原生窗口、物理 IME、VoiceOver 或性能验收。发布方的原生/GPU
收据不替代消费侧证据。#22 稳定子项锚点和混合文档阅读事务仍独立未交付；#23 的正文
入口通过也不能单独证明正式 Transcript/RunView、分页/阅读位置已经完成。
