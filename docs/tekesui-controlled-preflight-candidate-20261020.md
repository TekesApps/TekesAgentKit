# TekesUI #26 候选的接收与独立回放

新包身份 `2b16eaa6200fecdde9a0c615166064effb9446249cde05087d5d7c288d81d4ec`，archive SHA-256 `f9b2be424a66ce25c9d725340e46406490bd1dd64e69fd6bfe2320d9618e44d3`。已独立核验463文件，相对旧 bd3 为17路径增量、无删除、无锁升级；两个生产实现变化为 `text_input/external.rs` 和 `text_surface.rs`，其余为消费示例、测试/设计数据与 GPU 宿主。完整增量和日志/metadata 哈希见[JSON](tekesui-controlled-preflight-candidate-20261020.json)。输入保存在忽略目录 `.runtime-fixtures/tekesui-preflight-2b16eaa6200f/`，旧 bd3 和默认公开 pin 均未替换。

初始读取器按旧包的平铺路径拒绝了新的 `source/` 目录头，没有执行输入或写入源文件。检查实际布局后，仅允许这个明确前缀、目录和 manifest 内常规文件，逐项核验；不允许链接、重复、越界或未列明文件。

## 接收方独立验证

在本地独立解包的 source 中按交付 REPLAY 使用 Rust1.95.0、`--locked --offline`、关闭 debug/incremental 的 receiver cache，执行：

- 全 controls 测试388项。
- 第二公开 composition consumer 测试168项。
- controls（含 tests）和 consumer（all-targets）严格 Clippy。
- 新受控预检 GPU 测试1项，48场景/192帧。

三套实际 Cargo metadata 的本地依赖均限定在独立源包内；结束后463个输入哈希及 lock 均未改变。两份 design/gallery JSON 明确是 `contract-binding-fixture/1`，只用于既有绑定测试的 required state/token 检查；不是当前完整 runtime capability inventory，也不是控件成熟度或原生验收证据。

## 能力与接入边界

`TextInput::check_external_value` / `TextSurface::check_external_value` 与 apply 共用 private prepare。check 不改变实例、正文、选区、undo、preedit 或派生状态；同 UI turn 内 apply 仍重检。它不是跨事件 reservation，迟到结果仍须使用原 baseline 并处理实际冲突。重复 ack 可为 Unchanged 且保留 preedit。

库的公开 consumer 已示范编辑器只读预检 → 应用结构预检 → 原 reading token 校验 → 同 turn 更新原 editor 和 Transcript。接收方测试通过只是**库及其 consumer**的证据；AgentKit 完整 RunView、Composer 业务冲突/clear receipt、上下文容量、旧 token、原生焦点/选区/组字和真实三端阅读仍要独立适配。旧 bd3 原生结构阅读报告不认证本包。

该包不是公开 Git pin，默认依赖仍为 `8cf0840790334ba224afc0595897c74231c712e6`；未提升冻结、关闭 #26、合并或打 tag。下一步在明确的独立候选工作区对正式 AgentKit RunView 完成合法/拒绝更新检查，不能把发送方示例或本次库级绿色结果当作完成。
