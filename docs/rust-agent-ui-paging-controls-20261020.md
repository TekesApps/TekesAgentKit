# 可复用历史分页入口

`TranscriptPaging` 是正式组件的历史读取入口，可独立挂载；它还不是正式 Transcript 或 RunView。组合公开 TekesUI `StackContent`、`ButtonContent`、`TextContent`，使用库默认控件视觉、主题与 Spacing，不自定义绘制和样式表。

组件绑定固定 RunScope，拒绝换会话输入；宿主换会话时保留原组件或创建新挂载，不复用旧 scope。`HistoryPageSnapshot::controls` 将既有读取回执和同源 canonical TranscriptSnapshot 转为展示事实：has-more、stale、当前连接/挂载、能力和读取阶段。Loaded 的 extended 字段不充当本地“耗尽”缓存。自定义子历史源仍由其源管理分页，须传 false。

`source_ready` 与回执 `is_current` 分开：重连后的旧回执失效，但新连接 ready 且 canonical 历史不再 stale 时，用户仍可发起新的读取。控件不再显示上一 epoch 的失败/Loading，也不会因旧回执失效永久禁用分页。新手势仍由 begin_history_page 重新验证；不会自动重试旧请求。

动作回调只返回原 scope，宿主立即在原 binding 捕获一次性 HistoryPageRequest，并同步 Loading 控件后才派发异步任务。组件不自动读页或重试；失败文字来自原错误，Failed/Interrupted 只有显式重试。断线、stale、旧 epoch、加载中与无分页能力均不能激活；没有更早历史时移除加载按钮。

ComponentDemo 已在主会话和可写任务、独立子代理、SideChat 的运行控件中挂载此组件。页完成只唤醒宿主并读取原 binding 回执，不切换选择，不将 A 的错误显示为 B 的全局错误；自定义 child source 禁用 endpoint 页入口。有界只读 pane 目前仍使用已有诊断/分页路径，其正式内容待 Transcript 接入。

第二公开 consumer 提供独立构造函数与挂载测试，不依赖 RunSurface 或 demo 私有类型。控件测试覆盖实际公开 AX Activate 路径、默认按钮、原 scope、Loading/断线/stale/不支持/耗尽禁用、失败/中断显式重试、换 scope 拒绝以及 extended=false 的 canonical gate。宿主集成测试从分页按钮捕获 A 请求，拒绝重复点击，切 B 后完成 A 页，确认 B 未接收 A 的回执且未抢导航。

原生回归另捕获了既有问答卡片在重连中的竞态：宿主分别读取旧 Run publication 和实时连接 epoch，同一 publication 被错误配上新 epoch，触发冲突检查。`RunSnapshot.connection_epoch` 现在随同一 authority read 发布；宿主从快照读取卡片 epoch，不放宽卡片对冲突/旧 publication 的检查。旧序列化输入缺字段时默认为 0，不能用实时 epoch 填充旧快照；Rust struct-literal 调用方须增加字段。模型回归验证旧快照保留 epoch、新快照推进 revision/epoch 和旧数据反序列化。实际响应请求仍会在 capture/dispatch 核对当前 authority，旧卡片不能借此通过新连接的回应检查。

这是组件与绑定证据；不证明物理 IME、原生分页位置或滚动锚点。机器内容与 footnote 靠左、用户气泡及外部 footnote 靠右的既有 Transcript 布局契约保持不变。TekesUI #22/#23 的公开消费门槛仍保留。

最终验证：根工作区 128、UI 工作区 184 tests（含第二公开 consumer），root/UI strict Clippy、fmt、依赖边界与实际 Cargo 固定输入检查通过。最终固定二进制通过五项原生 fixture 回归，以及真实 Native/Pi/DSH 长会话上的公开分页按钮 → 原请求 → B 隔离 → 新 epoch 历史保留检查。此前捕获的原生冲突与修复后的收据分开保留；[机器收据](rust-agent-ui-paging-controls-20261020.json) 记录源码、二进制和日志哈希。三端使用隔离状态与本地合成模型，原生 fixture 使用合成文本服务；没有物理 IME 或正式 Transcript 阅读锚点证据。
