# 当前公开 reading/reveal 接收复核（2026-10-22）

后续同一程序的独立导航与阅读复验见[SidePanel 导航记录](rust-agent-ui-panel-navigation-20261022.md)：
原生分页和 Sidebar reveal 后续通过，系统拼音仍失败。下文保留本次原始失败归属。

公开全部分支、tags、releases 已重新检查。main 与 checkpoint tag 仍为
`09b4bab44b6dfa65487250ea816b610b9df59095`，与当前 Cargo 精确依赖一致，无需重复升级。
#22/#28 仍 OPEN；公开 API 已可消费，issue 状态不等于缺少实现。
匿名读取该 SHA 的 composition/content_reading.rs、content_reveal.rs，字节哈希与审阅清单一致。
实际 Cargo 解析闭包为8包、241文件，边界检查通过。

当前宿主已接通布局后的 Sidebar 选中项自动 reveal、Run rail follow 和原混合文档阅读事务。
滚动几何、clamping 与编辑焦点保护沿用 SDK，不借 focus 跳转或手算 offset。
Sidebar attachment 只承担显式导航，不代表完整库存分页的连续阅读事务；
销毁后新宿主的持久阅读 checkpoint 仍跟踪 TekesUI #42。本次未发现可独立证明的新基础组件缺口。

在保留当前 SidePanel 导航 WIP 的工作区，完整 UI 测试502项通过，1项既有 #49 Popover
接收门禁显式忽略；all-target 严格 Clippy、原生程序构建通过。组件测试包含分页、
阅读锚点、原编辑器/焦点/marked baseline、自动 reveal 及第二公开 API consumer。
这些不替代原生窗口验收。

本次冻结程序为 `.runtime-fixtures/AgentKitRevealCurrent20261022.app`，SHA256
`b1092549757381b3df33d65ddf377590acdc0402225e912f6135cf5f0deb647a`。
本次三条原生验证均未完成，不能报告通过：

- 阅读/分页探针停在 phase 0，未取得激活窗口，最终完整性断言失败。
- Sidebar 原目标 reveal 返回 Completed；前台轨迹随后从 demo 切至 Codex，焦点断言失败。
- 系统拼音产生 `ni hao` preedit，失焦后 marked-text 断言失败，进程退出3，无完整 IME 收据。

未强制重抢焦点、弱化断言或覆盖失败日志；这些结果尚不足以断定 SDK reveal 清除了焦点。
后续需要在窗口持续激活的验证环境重验同一程序。已有历史成功收据保持原程序归属，
不转记为本次成功。实体键盘、候选窗和 VoiceOver 均未验收。

完整记录见[机器记录](rust-agent-ui-reveal-current-20261022.json)，原始日志位于
`.runtime-fixtures/reveal-current-20261022/`。复用既有共享 target，未新建 target，
未删除缓存，释放空间0；保留源码、WIP、冻结程序及所有证据。UI 保持实验状态。
