# 原生中文 IME 验收步骤与证据边界

状态：已增加并通过[系统拼音软件探针](rust-agent-ui-system-ime-20261020.md)：正确键码进入系统输入法，检查真实 marked text、发送/导航屏障及确认后的单次提交。人工实体键盘、可见候选窗及完整 IME 场景验收仍未完成；合成 Preedit 和本次软件探针均不替代这些证据。正式 Transcript 的混合阅读/分页锚点和可选中富内容仍需 TekesUI #22/#25 后独立验收。

## 当前可运行的隔离宿主

在 AgentKit 根目录：

```sh
CARGO_TARGET_DIR=/tmp/agentkit-final-receive-target cargo +1.95.0 build --manifest-path ui/Cargo.toml -p tekes-agent-ui-demo --locked
/tmp/agentkit-final-receive-target/debug/tekes-agent-ui-demo --components --file-attachments
```

未传 endpoint-config 时使用本地 fixture，不连接用户的真实会话或付费模型。这里是公共 Composer/Indicators 及 pane 控件验证宿主，诊断文字不是正式 Transcript。以被测提交及 binary SHA256 标记证据，不能将旧 binary 的结果归给最新提交。

## 必须先证明进入真实组字

1. 从系统输入菜单显式选择“拼音－简体”，保存输入菜单选中项和系统版本。仅“已安装拼音”或“输入源快捷键已启用”不能证明当前使用拼音。
2. 聚焦会话 A 的 Composer，用实体键盘逐键输入 `nihao`。必须实际看到系统候选窗和/或 marked text 下划线。剪贴板粘贴、AXSetValue、直接插入中文、合成 Preedit 不算真实 IME 证据。
3. 记录候选/marked text 与当前会话、草稿、发送状态的同一时刻截图。候选 UI 可能不出现在宿主 AX 树，应另记录屏幕上的候选；不能仅用“中文最终出现了”推断经历了组字。

## 组字、确认和草稿隔离

- 保持 marked text 时，点击发送不能产生提交；Enter 用于确认候选，不应发送消息。确认后再按 Enter 应换行；只有明确点击发送才调用既有共享发送路径一次。
- 组字期间尝试会话 B、模型/推理、附件、新会话和 SidePanel 导航。仍处于 marked 状态时，入口应被阻止。若系统先提交/取消组字再投递导航，记录该顺序并确认文字只归原会话，不能把这种平台顺序误判为“marked 状态下强行导航”。
- 确认或取消组字后切到 B 输入另一份草稿，再回 A，核对正文、附件及模型/推理设置分别恢复；没有输入落到另一会话，也没有自动发送。
- 分别测试无前缀、已提交中文前缀、替换已选中文本、Escape 取消及连续两轮组字。再次打开模型/推理内容仍只使用对应会话设置。
- 对 Main、可写任务、独立子代理和 SideChat 分别记录；有界只读子历史没有 Composer。三端真实会话联调需独立运行，fixture 通过不提升为三端原生验收。

记录 OS/输入法、提交与 binary hash、实际输入来源、marked/candidate 证据、每次显式发送的次数及 A/B 草稿结果。失败时记录事件顺序再判断属于 AgentKit 组合/回调还是 TekesUI 基础文本服务；基础缺口提 TekesUI issue，不在 AgentKit 绕过或复制 IME。

## 2026-10-20 自动化尝试

被测代码 `43ee06805147b87411d7ed8189f640d40ea48a19`，binary SHA256 `72053f64e57a19931bb5c2c737969a89be5e7fb7b053945ace8f1a0f3ba87fc2`，TekesUI 固定 `431093786de47b48cd566d0d9c0eafa088b10d68`。

只读系统 UI 检查发现 ABC、Pinyin – Simplified、Japanese – Romaji 三个输入源；前/下一输入源快捷键已启用，自动切换文档输入源关闭。没有修改设置、安装输入法或更改快捷键。

自动化工具在隔离宿主中发送输入源快捷键和逐字母按键，观察到普通 `nihao` 文本及正常发送可用状态，没有看到候选窗、marked text 或可确认的活动拼音输入源。系统输入菜单的 AX 读取超时。这些证据不能区分工具未切换输入源、工具输入方式绕过 IME，还是其他平台条件；**不能据此判定 AgentKit/TekesUI 的真实 IME 通过或失败**。

初始 previous-source 快捷键成对发送；随后 next-source 快捷键完成三次循环。由于活动输入源身份未能读取，恢复身份也未被独立确认；没有宣称验证了系统输入源切换。测试文字最后清空，发送按钮恢复 disabled，内容诊断仍为 20 条/0 条流式，随后退出本任务测试宿主。[最终 AX 记录](rust-agent-ui-ime-attempt-native-20261020.txt) 仅作观测，不作 IME 通过收据。

同一 binary 的 `--components --native-selftest` 正常退出，5 个原生回归标记通过：Composer/Indicators、Sidebar/Summary、SideChat、Task、Child。它覆盖真实原生窗口及注入的文本/组字服务、发送/流式/取消/草稿隔离/重连；标记明确保留 `physical-ime=false`、`transcript=false`、`reading-anchor=false`。这与上面的真实输入法尝试分开，不能互相替代。

`43ee068` 的远端 CI 四项均通过：[push run](https://github.com/TekesApps/TekesAgentKit/actions/runs/39334181995)、[PR run](https://github.com/TekesApps/TekesAgentKit/actions/runs/39334189574)。CI job 本身标记无原生交互，这也不构成真实 IME 证明。本文为其后的文档增量，不把该 CI 结果归给后续提交。
