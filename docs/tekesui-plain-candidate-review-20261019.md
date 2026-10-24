# TekesUI #23 候选的独立技术评估

本记录只评估未发布归档，不是 AgentKit 依赖升级或正式 UI 验收。

- source identity：`2f0902d9f90fb069696c52edc38d9b7e7f8f0d0c4a3f0141e21fe1ac3f3183d0`
- archive：`tekesui-plain-2f0902d9f90f.tar.gz`
- archive SHA256：`1bfe988a4ab96f8a212e9ea76b5f757682b9275aa5f2f470ef90f900e81dae90`
- 独立提取至 `/tmp/agentkit-plain-review-2f0902d9/source`，使用独立 target，未改产品 pin。

消费侧验证归档整体 hash，逐项检查所有 991 个源码文件与 manifest 一致，拒绝绝对
路径、父目录跳转和链接；归档另含一份与外部一致的 manifest。没有读取活跃工作树
源码代替归档输入，也没有将发布方收据当作消费侧测试结果。

以 Rust 1.95.0、`--locked --offline` 复验：

```sh
cargo test -p tekes-ui-controls --test plain_text_content
cargo test -p tekes-ui-controls --lib
cargo test --manifest-path examples/composition-consumer/Cargo.toml --tests
```

分别通过公开 API 3、控件核心 238、独立 consumer 139 tests。源码核查确认
`TextFieldContent::read_only_content(name, value, TextRole)` 是显式 opt-in；沿用
TextSurface 的选择、复制和受控更新，取消表单装饰及内部滚动，由父 ScrollContent
负责滚动，普通 TextField 默认保持不变。API 测试实际覆盖无装饰 geometry、父裁剪
不重新塑形、滚轮/精确滚动交给父层，以及 UTF-16 选区在受控追加后保留和旧版本拒绝。

就 #23 的只读正文入口而言，该 API 能承接 AgentKit 的文本内容，由 AgentKit 组合
气泡、正文列、推理/工具和 footnote。它不提供完整 Markdown 或混合文档阅读事务，
因此不能独自完成 Transcript。#22 的稳定子项锚点、前插/高度变化事务仍需公开交付。

本次没有消费侧 GPU/原生窗口、物理 IME、VoiceOver 或性能测试；发布方的这些局部
收据保持为发布方证据。#22、#23 公开 Issue 核查仍 OPEN。候选没有公开 commit/release
pin，产品继续使用 `431093786de47b48cd566d0d9c0eafa088b10d68`。正式升级须获得可固定
的公开交付，并复验实际 Cargo 来源、AgentKit 组件、第二 consumer 与宿主交互。
