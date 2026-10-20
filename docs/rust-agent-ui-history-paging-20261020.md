# 历史分页读取状态绑定

本轮给 RunBinding 增加每次挂载自己的分页读取回执，复用原 SessionRuntime / ChildRun 的 load_earlier、冻结 cut、页合并与有界子历史检查。不新增 Run 状态机、历史缓存、滚动算法或后台轮询。核心契约仍以已发布 v0.3.8 为基线，UI API 尚在草稿分支，TekesUI pin 保持不变。

## 公开 API

- `begin_history_page(self: &Arc<RunBinding>)`：在手势发生时捕获原 binding、scope、连接 epoch 与请求身份，立即发布 Loading。返回一次性 HistoryPageRequest；`load(self)` 消费它。
- `history_page_state()`：返回原 scope、revision、当前/请求 epoch、is_current 和 Idle / Loading / Loaded { extended } / Failed { message } / Interrupted。
- `subscribe_history_pages()`：观察读取回执。宿主仍须同时观察既有 subscribe_runtime / pointer 通知，连接或 pointer 的变化不会单独推进此回执 revision。
- 原 `load_earlier(&self)` 入口也走同一个单次读取 gate。旧调用方可以继续使用，但需要在手势时固定 epoch 的 UI 应优先捕获 request，再交给 executor。

同一 binding 的重复点击或直接调用不会排入第二个读取。新 epoch 可以创建新请求，旧请求完成或 Drop 不得清除新请求状态。Failed 只记录原错误，不自动重试。Drop / abort 发布 Interrupted，不能显示为成功；取消 future 不代表 backend read 被取消，缓存之后仍可能接收合法旧页。宿主应刷新 canonical snapshot，再决定是否仍需显式加载更早历史。

is_current 核对原挂载 pointer、连接 ready 和 epoch；它不是 Transcript 文档提交 token 或阅读锚点证明。历史页内容及 cut 仍由原 runtime / bounded-history adapter 验证，正式视图仍需用已有 publication、pane mount token 和未来 TekesUI 阅读事务校验安装。多个独立挂载各有回执，不声称对整个 endpoint 所有读取做全局去重。

是否显示/启用加载入口应同时消费 canonical TranscriptSnapshot.has_more_before / is_stale、ChildPaneBinding.supports_pagination、挂载状态和 Loading gate；不能从 extended=false 推导一套新的“历史已耗尽”缓存。自定义 ChildHistorySource 继续拥有自己的分页，不因 has_more_before 就打开 endpoint 页按钮。

## 验证与边界

新增 5 个异步测试覆盖单次读取/watch、冻结 cut 与 B 草稿隔离、失败后的显式重试、未启动与在途 abort、旧 epoch Drop 不覆盖新请求、已退休 child pointer 不发 RPC。现有有界只读子历史分页测试补充核对 Loaded 回执；第二公开 consumer 捕获/派发公共 request，不依赖 demo。

`tekes-agent-ui-demo --components --history-page-proof --offscreen /tmp/agentkit-history.png` 使用隔离 SEP fixture：A 捕获读取，切到 B 后完成 A 页，核对 A 边界扩展且 frozen cut 不变，B 的状态与草稿保留，结果不抢导航。出口 historyPagingProof 明确 nativeInteraction=false、readingAnchorAcceptance=false。这里没有实现正式 Transcript 分页控件，现有 RunSurface/ComponentDemo 仍是测试宿主。

只读对照 ClientKit ThreadTranscriptPresenter+Scroll 的请求身份、当前文档与用户阅读意图边界，以及 Tekes 的 TranscriptVisibleWindowTests 的旧文档提交拒绝。没有移植其像素阈值/滚动算法；TekesUI #22/#23 的公开消费与正式混合文档前插/高度变化/锚点事务仍待完成。

布局规则保持机器内容及 footnote 靠左、用户输入及 footnote 靠右；控件视觉继承 TekesUI 默认值和语义 tokens。本次收到的普通 ButtonContent 内部文字默认居中的说明属于未发布 WIP，未作为本分支输入或消费侧验收；未来有公开精确提交时再核对，消息外层方向不因此改变。

本地 UI workspace 178 tests（新增分页 5 个，含第二 consumer）、有界子历史 3 tests、工具 18 tests、UI strict Clippy、fmt、边界与固定输入检查通过。offscreen host 的 historyPagingProof 全部目标字段通过；本次没有重跑真实三端或根工作区全部测试，也没有原生窗口/阅读锚点验收。[本次机器收据](rust-agent-ui-history-paging-20261020.json) 与此前三端证据分别保留。
