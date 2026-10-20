# 最终回复复制：共享语义与输出 footnote

输出复制已接入公共 TurnFootnote、TranscriptContent、RunView 和 ReadOnlyTranscript。按钮使用 TekesUI 默认样式，位于机器正文之外，与输出 footnote 一起靠左；用户 input 和对应 aside 继续靠右。没有修改 TekesUI、Kernel 或外部 Swift 源码。

只读对照 ClientKit `c289bc9d33edcd83a782ac17927a8c65a027eccd` 的 TranscriptTurnFootnote：输出 copy 使用 `output`，SnapshotBuilder 只在 `hasFinalAnswer` 时提供它。Rust 现有 presentation 已在 Native/journal 的结算路径生成 `TranscriptItem.is_final`，本轮复用该共享策略；此前“还没有可用的最终答案语义”的判断不完整。没有在控件中新增按最后一条文本、时间、工具结束或相同内容猜测答案的规则。

## 数据与回调

新增纯读取方法 `TranscriptSection::final_output()`。只在存在 completed turn、唯一标记为最终答案的 Assistant 可见且非 provisional、正文非空时返回原 TranscriptOutput；否则返回 None。缺失 turn、未知/运行中状态、隐藏消息、纯推理、非最终消息或多个最终标记都不能获得替代复制目标。工具和 reasoning 保持独立，复制内容仅是该最终消息已有的原始 `text`。

独立挂载的 footnote 使用同一帧的 section：

```rust,ignore
footer.update_with_output(scope, publication, epoch,
    section.turn.ok_or("missing turn")?, mutations_available, section.final_output())?;
for copy in footer.take_copy_intents(&actions) {
    if copy.claim() { host_clipboard.write(copy.text()); }
}
```

`update` 旧方法仍表示没有显式 output copy target，不从 turn 自造正文。`update_with_output` 预检目标角色、可见性、最终状态、turn identity 和 publication；非法目标不替换已加载数据、不推进版本。相同版本的 copy 内容冲突也失败。正文/身份/epoch 改变后退役旧按钮 ID，状态更新保留同一目标按钮；捕获的 intent 保留原 scope/message/text，克隆共享一次 clipboard admission。

TranscriptContent 自动传入 final_output，并汇总输入和输出的 copy intents；RunView 沿用 CopyTranscript，ReadOnlyTranscript 的 copy 回调独立于读取/分页回调。关闭 endpoint mutation、离线或只读不会禁用已加载正文复制，也不会因为复制发起 RPC。宿主负责系统剪贴板和窗口级组字屏障。`TurnFootnoteLabels.copy` 是未发布 UI Rust struct 的新文案字段，struct-literal 消费方须补齐或使用 Default。

## 分层验证

完整结果见[相邻 JSON](rust-agent-ui-output-copy-20261020.json)：headless 全量、UI 全量（含第二 consumer）、严格两套 Clippy、脚本测试和依赖边界分别记录。回归覆盖最终标记与后续非最终文本、无/重复目标、纯推理、临时消息、离线可复制、旧按钮失效、同版拒绝、原文本捕获和公开单独挂载的非法目标预检。

真实 Native/Pi/DSH 独立 transport 配合本地合成 provider 验证恢复、分页、重连、附件及 GPU 内容树。常规三端恢复数据各产生30个实际最终回复复制控件，Native 小型有界 child 的主回合产生1个；右侧只读 child 的输出复制也已检查截图。renderer 数量与共享 final_output 结果相等，脚本额外要求至少一个控件，不能把双方都没有识别出目标当作通过。fixture 原生六项组件及两项线程/footnote 回归、当前系统拼音软件探针分别通过。

GPU 截图和原生 pointer/状态回归不能证明系统剪贴板读回；本轮没有覆盖用户剪贴板做自动写入/恢复。它们也不证明完整 IME、候选窗或实体键盘体验。TekesUI #22 混合阅读/分页锚点、#25 可选中富内容仍为上游缺口；评价、目标/memory footnote、可靠流式阅读身份关联和完整 Run/Transcript 验收仍需继续，UI 保持 experimental。
