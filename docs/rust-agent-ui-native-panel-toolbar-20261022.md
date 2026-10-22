# SidePanel 导航原生补验（2026-10-22）

在 `95ffba1f6460b19982aaec94f1ba64b335934b46` 基础上，仅调整原生验收探针：
关闭动作优先查询已挂载的 `SidePanelNavigation` 原控件，保留原诊断模式路径。
业务组件、状态机与 TekesUI 基础实现不变；公开 SDK 仍精确锁定
`50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`。AgentKit 为仓库内开发路径，未发布新版本。

验收使用独立保留的正常 APPL 程序，SHA256
`cb4c0048ae0ea36c4298228ea7fb348180ac1882d928572b1dbbbb4cc43157c3`。
原始流、前台应用变更轨迹、程序和源码哈希保存在
`.runtime-fixtures/native-panel-toolbar-20261022/`，不覆盖前阶段程序及失败证据。

| 新导航模式的本地受控 SEP 原生检查 | 结果 |
| --- | --- |
| Summary 关闭/重开、任务打开顺序、最后标签选择与空面板保留 | 通过 |
| 原任务/子会话 editor 保留、晚到和重复子会话回传拒绝、关闭后的 pending open 拒绝 | 通过 |
| 非当前父会话任务导航拒绝，当前父会话草稿、焦点及请求保持 | 通过 |
| 原分页按钮、20→44、前插与刷新阅读锚点、A/B 独立阅读和草稿、重连恢复 | 通过 |
| 原回合导航与回到最新、组字期间导航屏障、原 editor/opaque baseline 保留 | 通过（合成 preedit） |
| 只读子历史 38→75、原分页/刷新、跨面板复制与主会话草稿/组字保持 | 通过（合成 preedit） |
| 系统简体拼音 `ni hao`→“你好”，组字期间发送/切会话拦截、提交后单次发送 | 通过（CUA 绑定应用按键、真实系统文本服务） |

真实拼音没有粘贴、直接 Unicode 替换或合成 marked-text；前后输入源均为
`com.apple.inputmethod.SCIM.ITABC`。日志保留原 `Preedit` 与 `Commit`。
这不证明实体键盘、候选窗口或 VoiceOver。分页与子历史的滚轮/preedit 是 SDK 原自测接口。
所有本地检查均来自同进程受控 SEP，不外推为三个真实 harness 的完整组合验收。

这是宿主内容区的原 SDK toolbar，`nativeWindowToolbar=false`，不等同窗口级注册或
ProcessContainer；SideChat 家族原生弹出菜单尚未由本轮验证。
原生 `open -W` 返回值仅为 launcher 退出码，应用退出码仍记为未知，结果以原完整收据及崩溃检查判断。

分页首次 launcher 使用了错误的收据前缀（遗漏 `SELFTEST`），因此包装层报错；原应用已到 phase 16，
完整收据存在且无崩溃。使用正确前缀独立重验原始日志并保存 `reading-revalidated.json`，未重新启动，
不掩盖原包装层失败。

本地 host 回归 14 项、独立 consumer 3 项通过，demo 严格 Clippy 通过。
复用现有共享 target；新增约 69 MiB 为保留的验收程序及原始证据。没有新增 target，未删除证据，释放 0 字节。
PR #10 保持 draft，未合并或打 tag。TekesUI #14/#36/#48 等既有缺口继续按原范围跟踪。

## 三个真实 harness 的流式阅读补验

同一验收程序复用现有 driver，Pi 1.3.0、DSH 0.2.1-rc.2、Native 0.9.3
均取得完整 `NATIVE_STREAM_READING_RECEIPT`，driver 最终退出 0。
每端观察到 3 次追加，原 item/local anchor、原 editor、中文 marked text 与焦点保留，
最终文本精确匹配；切会话草稿及阅读位置保留，重连不重放，每端只提交一次。
使用真实公开 harness 协议和本地受控 provider；Native 为明确 UI-only，未运行 lifecycle/child。

此组沿用原有 demo 诊断布局，`hostPanelNavigation=false`；不与上方新导航受控会话结果混算。
滚轮与 preedit 为 SDK 自测接口，不代表实体输入。三端系统窗口截图均报
`could not create image from window`，退出状态 1；截图未验收。原交互收据、前台轨迹、
失败输出及离屏 GPU 图保留，没有改断言、强制抢焦点或重试应用。

[机器收据](rust-agent-ui-native-panel-toolbar-20261022.json) 汇总各项完整原收据和原始证据哈希。
此前失焦失败仍保留；本轮成功不证明此前前台切换的原因已解决。
