# #19 指针元数据与共享选择修复（2026-10-18）

状态：部分完成，未关闭 Issue。基于全部 WIP，未提交/推送/tag；性能未测。没有新增组件、token、UI 框架或页面样式。

## 根因、共享位置与适用范围

1. 平台 MouseEvent 已有 button/modifiers/click_count，ContentEvent 只保留位置。现于 composition/pointer.rs 增加完整 PointerEvent 数据入口及 Component::pointer_event / overlay_pointer_event，ContentHost 路由保留原数据；只用位置副本判定命中与捕获。主按钮捕获不被次按钮 release 清除，Cancel 仍使用既有统一取消链。
2. TextFieldContent 原先固定 shift=false/count=1。现由共享 input_event 处理完整信息，沿用 TextSurface 的命中、选区、reveal、滚动和取消；未增加编辑器或滚动器。
3. 原生验收发现原 TextSurface 在词边界先 move_word_left，会把 emoji 左半边双击归给前字。现用同一 Paragraph.selection_rects 判断实际命中的字素，再复用 TextInput 的原 Unicode 分词边界；键盘 word movement 使用同一边界函数，语义不变。
4. 三击原先全选全文，现选逻辑段落并包含段末硬换行，软折行不算新段落。CRLF 作为一个字素；CR/LF/U+2028/U+2029 使用同一规则，末尾空段落保持空选区。CoreText shaper 原先只识别 LF 的显式换行/末尾空行，已在共享布局层统一，而非在单个组件补滚动偏移。

适用于所有使用 TextSurface 的 TextField/TextArea/SearchField/只读历史与独立消费者；组合输入统一由 TextFieldContent 接入。普通 Button/Switch/Slider/Menu 等通过协议默认实现保留既有主按钮交互，次按钮不隐式激活；本轮没有扩大注册组件或做全量迁移。

合理例外：装饰文本不因新数据类型变成交互控件；只读文本仍可选，disabled 仍不可选。上下文动作需组件显式实现 pointer_event，不把次按钮当普通点击。包装组件需要转交该入口，AgentKit AgentText 已迁移。旧 PointerDown/Move/Up(Point) 继续表示主按钮、无修饰、单击；不能凭简化事件推断原生元数据。原生 Drag 没有 click_count，明确传 0，其余字段逐项保留。直接组件调用可用 pointer_event；宿主通过 ContentHost::event(ContentEvent::Pointer(...)) 进入完整路由。

## 实际接入与回归

- 原生适配器：独立 composition consumer、Gallery 阅读预览、AgentKit demo 均已迁移；实际源文件转译测试覆盖所有六阶段、主/次/中/Other 按钮、全部修饰键和 down/up 点击次数。
- 公共 API：2 项协议测试；共享 controls316 + text43 共359项通过。覆盖旧事件兼容、完整字段、次按钮不释放主捕获及普通 Button 不误激活。
- 独立真实 consumer：57项通过，其中6项指针/真实CoreText测试及1项实际两宿主转译测试；验证Shift、emoji左右半边、组合字符、词首字素、三击段落、CRLF/Unicode硬分隔及末尾空段落，长只读文档拖出视口沿用reveal且产生合法滚动，Cancel后停止拖动。
- AgentKit：最新显式快照 dfdbcf7ede5549d25f44092c73d0e9c129b50c3a7479c82027463de084b7a4b8；UI13项、strict Clippy、Rust边界脚本和默认headless76项通过。真实 RunSurface 包装器Shift选区、次按钮不发送/不改选区及新指针入口的preedit屏障通过。未启动AgentKit原生窗口。
- 四主题×1x/2x×前后两阶段=16张实际GPU图；选区与原画笔接通，Light1x/DarkIC2x代表图已查看。另重跑既有四主题前插GPU回归，未改变样式。8组实际token查找4500条全部重新执行并绑定最终共享源码，complete=false。门禁在旧证据失效时拒绝，更新证据后恢复通过；未降低覆盖范围。
- public API分类新增4个composition数据类型，组件仍45，公共状态131/fixture gaps125/required269。源码、设计引用、Gold7机械门禁及严格Clippy通过；机械门禁不替代体验验收。

## Gallery 原生证据与 Gold7

仅操作已授权Gallery。原生同一位置双击emoji：修复前选17..18（前一个汉字），修复后选18..20（完整emoji），保留两份签名二进制身份、各自源码哈希、截图与事件JSON。复测Shift扩选3..14；次按钮点击输入及分页按钮均保持选区/焦点8604/pages0；三击单段0..20且click_count=3；拖出字段仍收到Primary/Drag并选3..20。所有审计应用已退出。

每一步的单独事件快照是验收依据。退出后查询AX曾重新打开应用、重写该次运行的reading-events.json，因此不以最后被重写的空会话文件替代已保存步骤快照。测试后唯一源码差异是审计日志if的Clippy格式整理；native-fixed/tested-source保留并哈希核对了实际测试宿主源码，其余共享选择/转译源与最终源码完全一致。

Gold7：G1–G3沿用原JSON DS字体/颜色/布局；G4未新增动效；G5/G6用真实字段、选区、输入来源、取消和原生前后对照核验；G7分别交付公开协议、两个独立消费边界、GPU与可运行Gallery。没有用单一组件截图推断全系统完成。

## 尚未完成

多段/多行字段的原生三击与长文自动滚动、窗口失焦发生在拖动期间的现场组合仍待验收；这些情形已具备自动化公共API证据。AgentKit原生最终体验仍等待超出Gallery的明确授权；正式公开不可变pin继续受禁止提交/推送/tag约束。物理中文IME、VoiceOver未测；性能暂不测。#17文本服务/#18受控编辑/#20多行slot/#21类型化动作继续按依赖推进。

证据：TekesUI design/reviews/pointer-metadata-20261018/receipt.json。下一可执行项为#17公共文本服务契约，同时保留#15/#16原生与pin阻塞，不将其标为关闭。
