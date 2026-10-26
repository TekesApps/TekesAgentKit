# 输入脚注的原控件右对齐组合（2026-10-22，实验状态）

> 后续规则更正：只读输入信息应使用复制控件前的剩余行宽，40% 只适用于备注等动作标签。
> 本文及原 JSON 保留阶段实现和程序归属；当前规则见[剩余行宽更正](rust-agent-ui-information-width-20261022.md)。

只读对照 ClientKit `TranscriptTurnFootnote.rawInputNodes` / `rawMessageNodes`、MetalTextKit
`MetalVirtualLayout.appendAside` 的 input/message lane，以及 Tekes `ClientAgentHostAdapters`。
原信息在复制入口前，同一脚注带从右向左分配，并位于用户气泡外；没有将“每个控件单独贴右边”
解释为 Source 的单行排列。来源文件哈希见机器记录，未复制 Swift 源码或调用 Swift 工具链。

## 当前改动与宿主契约

`InputMessage` 原先将信息和复制按钮作为两个纵向行。现在添加透明业务组合节点，原
`ids().footnote` TextContent 与原 SDK ButtonContent 成为其子控件；单行在列右侧排列，
信息先、复制后，垂直居中。信息最大占列40%，同时保留原复制控件所需空间；
原 SDK 单行 LabelOptions / TailEllipsis 负责实际裁剪，完整 source/语义名称保持。
没有手工截字符串、字宽估计、绘制按钮、第二字体/输入引擎或新的执行/反馈状态机。

新增公开查询：

- `aside_id()`：透明脚注组，稳定使用当前 message namespace 中已预留的 root+4；
  原 `InputMessageIds` 字段与 `ids().footnote` 保持。它不是 clipboard 或 endpoint 权限。
- `information_text()`：原 TextContent 的完整格式化信息，包含画面上被省略的文字；
  它是display query，不反向推断时间、投递或连接状态。
- 宿主通过原 `copy_button_id()` / `take_copy_intents()` 派发，原 scope/message/text 捕获和
  shared one-shot claim 不变。元信息更新/改 copy label 不重建原复制按钮，正文或来源/epoch
  修正仍按原规则退役旧按钮。复制只提供本地文字，不获得 endpoint mutation 权限。
- 原 `.children()` 的直接拓扑改为 details → bubble → aside；应按公开ID查询后代，
  不再假设直接child[1]就是时间或将旧frame关系当契约。原 input 阅读项ID保持，aside不是新阅读项。

输入行的渲染宿主现在需要实现 SDK 已有 `TextSurfaceHost::shape_label`，包括信息和复制标签。
可复用 demo `label_host` / `native::TextBackend` 的现有 SDK 服务接入。
缺失时原 SDK 返回 `CompositionError::MissingLabelServices`，不偷偷回退成多行或自制省略号。
此为尚未发布 UI0.0.0 的明确宿主迁移要求；headless presentation / SEP wire 不变。

气泡与aside之间采用 Source 的Regular语义净空，控件/字体/颜色/圆角仍采用库默认。
纯附件消息的原空气泡对象保留，但不占行与额外gap，也不暴露不存在的正文或文件复制入口。
附件详情仍在气泡/aside之前，未改变原顺序、原控件和只读文字。
历史 edit/rewind、sent-as-goal 等剩余入口没有伪造，仍需按真实契约后续接入；
当前复制仍是文字按钮，紧凑图标跟踪现有#47。

## 验证与程序归属

UI workspace520项通过，既有#49门禁1项ignored；strict all-target Clippy、fmt、demo build和
UI边界通过。修正了旧“时间与复制纵向分开/时间直接贴列右端”的几何断言，仍检查原 copy
身份/来源失效、只读正文、输入pending→recorded的明确身份迁移和坏frame拒绝。
初始fixture编译诊断和Clippy诊断保留，不删除失败。

第二公开consumer使用正式CoreText服务，4主题×3字号比例×180/320/770宽度×正文/纯附件：
信息不超过40%、由SDK单行省略、完整文字保留；复制/信息同带、复制贴右、正文外净空和
纯附件无空气泡间距。信息更新、原copy激活和改copy label时，另一原Composer的完整marked
baseline、编辑器实例和焦点保持；capture复制原scope/text且仅claim一次。缺少label服务
明确失败另有门禁。它不是VoiceOver、实体输入或全部窄宽/本地化视觉验收。

冻结程序 `.runtime-fixtures/AgentKitInputAside20261022.app`，SHA256
`bd162855b299e777283fb3bf20e509d37c5717c8835548e1ed91452abcdc6df6`。
改动源码和来源哈希逐一匹配；同程序真实Native0.9.3、Pi1.3.0、DSH0.2.1-rc.2的公开数据→原
组件树→离屏GPU全部通过，新增几何断言检查原输入脚注带和原复制/信息相对位置。
Native为明确UI-only，不声称完整lifecycle/archive/child协议通过。所有provider/session隔离且
本地受控，没有付费模型调用。

三端离屏截图已检查其中Native图：用户气泡外的时间/复制确为同一右对齐带，机器内容仍靠左。
这是本程序离屏像素证据，不是成对ClientKit截图或原生窗口/阅读锚点/IME验收。

当前仍有其他TekesUI原生验收app运行，本轮没有启动新的前台窗口；native反馈探针已加
输入aside断言并通过编译，但**尚未执行**。当前pin原生全套/系统拼音的先前未完成项仍见
[3bef intake](tekesui-3bef-public-intake-20261022.md)，旧09程序成功不转移到本程序。
物理键盘/候选窗/VoiceOver、跨进程#48、Popover#49、其余全部Views和最终review仍开放。
PR #10保持draft，不合并、不打tag。

本阶段仍固定公开TekesUI `3bef66754a256e7723887de7d33518f5122805ad`，8包/242文件
清单`e6f0dbcdfdadcc8e4a67a46cefd1a814956461d3896a78f8469091c4420a6824`未改变。
原始日志、失败、截图、frozen程序收据在 `.runtime-fixtures/input-aside-row-20261022/`；
摘要见[机器记录](rust-agent-ui-input-aside-row-20261022.json)。复用既有UI/root target，没有新建
或删除target/cache，释放空间0；保留源码/WIP、固定输入、验收程序与所有证据。
