# 用户消息复制：公开组件到宿主剪贴板

`InputMessage` 现在在气泡外显示靠右的复制按钮，保留附件 → 正文气泡 → 外置信息/操作的层级，沿用 TekesUI 默认按钮与主题。正文为空时不提供文本复制，也不把附件信息伪装成文件复制。只读或离线不妨碍复制已经显示的文本。

只读对照 ClientKit `c289bc9d33edcd83a782ac17927a8c65a027eccd` 的 `TranscriptTurnFootnote.rawInputNodes`：复制使用输入字符串，和发送时间等信息同属 input aside。Rust 入口使用规范化单条用户消息的原始 `text`，不从渲染结果、工具数据或当前选中会话反查。机器输出复制、评价、目标与 memory footnote 仍需继续补齐明确语义；本轮不声称完成全部 footnote。

## 公共接入

```rust,ignore
// 每次事件后只处理一次原 ContentHost.drain_actions() 的批次。
for copy in input_message.take_copy_intents(&actions) {
    if copy.claim() {
        host_clipboard.write(copy.text());
    }
}
```

`TranscriptCopyIntent` 携带私有、不可变的 scope、message identity 和文本，提供只读 getters；克隆共享一次 clipboard write admission。新消息文本、身份迁移或 epoch 变化会退役旧按钮 ID，旧回调不能复制后来显示的另一个值；同一内容的状态更新保留按钮实例。已经捕获的手势始终保留原文本，宿主不通过当前选择重新读取。每个新用户手势可以重新复制，不发 endpoint RPC、不自动重试。

`TranscriptContent::take_copy_intents` 汇总这些回调；RunView 把它们作为 `RunViewIntent::CopyTranscript` 返回，并遵守已有 Composer 组字屏障。ReadOnlyTranscript 的 copy 方法和 Refresh/Earlier 方法分别消费同一个 action batch，复制不会发起历史读取或解除 mutation 禁用。宿主仍需在 pointer/AX 入口执行窗口级组字屏障。

`InputMessage::set_copy_label` 或 `TranscriptContentLabels.input_copy` 由宿主提供文案。后者是未发布 UI Rust struct 新字段，使用 struct literal 的消费方需补字段或 `..Default::default()`；RunViewIntent 新增分支也需处理。没有改 SEP wire、headless 发布 API 或依赖版本。

示例宿主把 CopyTranscript 保留到主线程 draw 的顺序队列，再调用公开 `Window::set_clipboard_text`；不把本地复制放进网络任务池，避免并行任务改变两次显式复制的先后顺序。系统剪贴板适配属于宿主，共享组件没有平台或窗口依赖。

## 验证边界

相邻 [JSON](rust-agent-ui-input-copy-20261020.json) 保存结果。242 项 UI workspace 测试（包括独立 consumer 的公开复制回调）、严格 Clippy/fmt、固定八包199输入和依赖边界检查通过。新增回归覆盖原消息文本/空白、克隆 admission、旧 ID 失效、纯附件不可假复制、RunView 组字屏障、离线只读 lane 不触发读写，以及默认样式下的右侧布局。原生六项组件回归与 GPU input 行已检查；本轮没有改写系统剪贴板来做自动验收，原生剪贴板实际读回未验证。

软件拼音回归先出现失败：日志证明真实 marked text 已进入，但随后收到 Focus(false)，宿主 Cancel 并清空焦点。失焦的具体来源未确定，不能算复制回归通过，也不回写先前成功记录。随后保持窗口专注，冻结旧版对照和本轮新版均通过原系统拼音断言，且未放宽断言；最终结果、失败摘录分开记录。后台窗口切换、实体键盘和候选窗仍未验收。

TekesUI #22 的混合阅读锚点、#25 的可选中富内容、可靠流式身份关联及其余 footnote 仍是完整 Transcript 交付的剩余项。没有修改 TekesUI、Kernel 或外部 Swift 源码。
