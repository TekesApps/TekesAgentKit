# 输出只读脚注与当前原生验收（2026-10-22，实验状态）

后续三端原生窗口补验见[独立结果](rust-agent-ui-native-resume-20261022.md)：
三端反馈、流式发送和取消/恢复通过；流式阅读组合仍因失焦未完成，不能据此宣称全部通过。

默认公开 TekesUI 精确固定 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`。
`TurnFootnote` 的原时间、运行信息、操作禁用说明和回执提示现在使用 SDK 单行
`LabelOptions::single_line(TailEllipsis)`；正常输出带按前面原控件之后的剩余宽度分配。
只读信息不使用备注等动作标签的 40% 上限。完整原文/AX name、原信息节点与所有权保持。
对照 ClientKit `TranscriptTurnFootnote` 与 `MetalVirtualLayout.appendAside` output lane；
Tekes host adapter 保持只读。业务组件仅提供顺序/宽度约束，位置由原 SDK layout helper 计算。
没有手动截字符串、绘制控件、焦点/输入/滚动引擎或另一套 presentation。

宿主需要 `TextSurfaceHost::shape_label`；独立 TurnFootnote 缺少服务会由 SDK 明确报
`MissingLabelServices`。没有新增公共状态或改变 copy/feedback/action authority。
原 footer/信息/copy/feedback 身份保留，feedback 表单展开仍沿用既有原控件路径。

## 组件与公开消费

UI workspace 522 passed、1 既有 #49 ignored。随后仅给 opt-in 原生测试宿主增加外部按键
节奏选项；最终严格 all-target Clippy、demo build、fmt、UI边界通过。
第二公开 consumer 用真实 CoreText，4主题×3字号×3列宽×短/长备注及180窄宽往返：
信息按前序控件后的剩余宽度单行省略，完整 source/AX 保持，宽列不误套40%；原信息、
原反馈按钮与另一原 Composer 的实例、焦点和完整 opaque marked baseline 保持。
单独缺 label 服务明确失败；原反馈及 caption transition 的回归继续通过。

## 精确程序和分层原生证据

最终 `AgentKitExternalIME20261022.app` SHA256
`2696969bb4d0a27804acf9fbe48d4cb06b974ff67f571b030b9098936fae497e`。
同程序完成：

- owned 主阅读 fixture：20→44，原分页/Start/Latest、前插/刷新锚点、A/B阅读与草稿隔离、
  重连阅读恢复、原 editor/focus/合成marked baseline 保持，无提交。
- owned 只读子历史 fixture：38→75，原分页/刷新、跨pane复制、主草稿及原editor/marked保持。
  它是公开有界 SEP fixture，不是三端真实 child 完整验收。
- 正常应用系统简体拼音一次通过：CUA绑定应用依次按 n/i/h/a/o，真实marked文本为 ni hao；
  组字内原发送与导航点击被挡，空格确认你好后恰好提交一次。确认前核对实际前台PID、
  bundle、可执行文件和输入源，前后输入源均为 SCIM.ITABC。原生事件、AX观察和前台轨迹保留。
  此次路由是 bound-app keys，不称为旧脚本的逐键PID定向证明；不声明实体键盘、候选窗或VoiceOver。
- Native0.9.3 UI-only、Pi1.3.0、DSH0.2.1-rc.2：真实公开协议数据→原控件→离屏GPU通过；
  包括中文备注保存/重开、删除后重新读取、RunPort反馈回调与草稿保持。全部为隔离本地受控来源。

前一程序 `7f7ef8a9…` 的主/只读原生及三端离屏独立保留；不将其收据转移给最终程序。
最初CUA AX读取cannotComplete单列；最终正常app已能观察原输入区，但完整AX/VoiceOver仍未验收。
首次外部IME尝试因未及时投递按键而在原marked断言失败，原日志不覆盖；后续同程序批量按键通过。
中断时的未结束完整suite保留，后续全量522项为新的完整日志，不从截断输出推断通过。

## 外部按键探针

`--system-ime-selftest --system-ime-external-input` 仅改变测试宿主的外部驱动节奏，
保留原字面 marked/committed 检查、发送/导航屏障点击和结束时恰好一次持久输入的断言。
15秒检查marked，17秒打印COMMIT_READY，30秒检查你好，35秒结束。
原快速 PID 驱动模式的节奏不变；新结果明确标为 bound-app-key-events。
CUA只发送原生键，没有paste、setValue、Unicode插入、手工Preedit/Text或全局输入源切换。

## 剩余范围

本轮另一个TekesUI原生验证程序重新启动后，未追加当前pin三端真实原生组合批次。
原native-feedback探针的剩余宽度断言已编译，但该模式本轮未执行。
窄列默认文字按钮仍可能耗尽整行；图标 #47、Popover #49、ProcessContainer #48、
富内容和完整Views配对视觉、三端当前原生全套仍未完成。保留同进程可运行路径，PR #10仍为draft。

原始证据在 `.runtime-fixtures/output-information-row-20261022/`，摘要见[机器记录](rust-agent-ui-output-information-20261022.json)。
复用原 UI/root target，没有新建 target；保留两份精确验收程序、固定源、日志、截图与失败。
