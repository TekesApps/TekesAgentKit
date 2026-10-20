# 机器消息内容行

`OutputMessage` 实现公开 TekesUI Component，可独立挂载，不依赖 RunSurface。它消费原 presentation 的 `TranscriptOutput` 和原 `TranscriptSection`，不发 endpoint 请求、不维护另一套 Run 状态机。发布基线已重新确认 AgentKit v0.3.8；实验 UI 依赖保持公开完整 rev `8cf0840790334ba224afc0595897c74231c712e6`，199输入闭包未变。

## 输入与布局

- 同一 scope、单调 publication / connection epoch、相同消息 key 更新；旧来源/旧版本/冲突版本在修改前拒绝。只接受可见机器项，session notice 可以没有 section；有 section 时必须属于该 turn。
- canonical content blocks 按公开 index 顺序组合正文、推理、工具引用、附件及未知块。重复/逆序 index 拒绝；同一工具的多个公开引用仍各自保留。slot 只用于消息内部控件保留，不冒充跨分页的阅读锚点。
- 没有块间顺序的 aggregate 来源明确标示覆盖限制。工具仅从同 section 且同 turn 的既有 DTO 解析，缺失就显示未回传，不借用当前选择会话的数据。详情保留输入、结果和公开视图，工具 phase 原样呈现，不从 phase 或 idle 推断消息/turn 完成。
- 正文、推理、工具、提示与原始数据详情全部左对齐，不使用用户输入气泡。默认 Button、Content 文本、语义字号/间距与宿主 palette 来自 TekesUI；没有新增颜色、字体、圆角或通用选择/滚动实现。`with_labels` 的 text/tool 回调接受宿主 copy 与详情格式化。
- 推理、工具和 opaque 块默认局部收起。宿主转交原 action batch 后仅改变对应详情的可见性，折叠内容控件保留；展开状态通过语义 expanded 提供。流式文字、工具回传和 reconnect 更新保留已有正文节点、选区及展开状态。工具身份更换会分配新的按钮 ID，旧排队动作不能展开替代工具。

## 公共接入

```rust,ignore
let snapshot = binding.current_run().ok_or("missing run")?;
let document = TranscriptDocument::read(&snapshot.transcript)?;
let section = &document.sections[0];
let output = &section.outputs[0];
let mut row = OutputMessage::new(700, binding.scope().clone());
row.update(binding.scope(), snapshot.revision, snapshot.connection_epoch,
           output, Some(section))?;
let root = row.id();
let mut host = row.into_host()?;
// 使用宿主当前主题与公开文字 renderer 调用 host.render(...)。
// 原生事件仍交给 ContentHost，随后消费该次动作：
let actions = host.drain_actions();
host.get_mut(root).unwrap().component_mut::<OutputMessage>().unwrap()
    .handle_actions(&actions);
```

嵌入已有 ContentHost 时启用其公开 action events，转交一次 batch 后重新渲染，再投递下一次依赖布局的输入。第二 public consumer 的 `output_message_row` 直接构造相同组件，并验证实际正文挂载；没有依赖 demo 或私有实现。

## 尚未交付

Markdown 当前按完整源码文字显示；附件/未知块明确标为未支持的原始数据详情，没有图像解码、下载或富卡片预览。上游 [#25](https://github.com/TekesApps/TekesUI/issues/25) 请求可选中富内容与原生映射，已有 MarkdownView 不等于完整交互能力。此行不是完整 Transcript，turn 级信息可组合独立 [TurnFootnote](rust-agent-ui-turn-footnote-20261020.md)；消息行自身不提供独立 tool-result 事件列表或整个文档的排序/滚动；它们必须在正式组合层按已证明的事件顺序接入。匿名 preview 到 NativeEntry 的阅读别名也不能用文字或时间戳猜测。

混合阅读与历史分页保位仍待 [#22](https://github.com/TekesApps/TekesUI/issues/22)。正文控件的选区保留不等于整个混合文档的阅读锚点验收，也不等于物理中文 IME 或 VoiceOver 验收。

## 验证范围

8个新增组件测试覆盖 canonical 顺序、opaque 原始内容、来源/版本/身份隔离、聚合覆盖限制、相同工具重复引用、显式 copy 与错误/空正文提示、流式/工具回传后的选区焦点保留、收起后更新再展开、旧按钮失效，以及 macOS CoreText 宽窄窗口/浅深色的左侧排版与只读防护。

完整 UI workspace 195项、脚本工具18项、严格 Clippy、fmt 与依赖边界通过。原生 fixture 在挂载两类消息行后通过既有5个回归标记，合成输入不计作物理 IME。真实 Native/Pi/DSH 使用各自 endpoint 和隔离临时会话、本地合成 provider，分别对恢复历史的两类消息行作 GPU 离屏检查，并验证分页/重连数据保持；这不宣称三端完整消息视图或其原生交互完成。具体收据见同名 JSON，保留二进制/源码与图像散列。
