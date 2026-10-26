# 系统拼音软件探针

在 macOS 26.6.2 / 25G83、TekesUI 固定公开 `8cf0840790334ba224afc0595897c74231c712e6` 上通过。组件仍消费公共平台事件和文本服务；没有修改 TekesUI、Kernel 或 Swift，也没有新增 Agent 状态机。

## 复跑

```sh
cargo +1.95.0 build --manifest-path ui/Cargo.toml -p tekes-agent-ui-demo --locked
python3 scripts/test-system-ime.py --demo ui/target/debug/tekes-agent-ui-demo --receipt /tmp/agentkit-system-ime.json
```

需要带 tomllib 支持的 Python 3.11+ 跑仓库其它检查；本工具本身只用标准库。执行前由用户选择系统“拼音－简体”，并已有系统事件投递权限。工具只检查权限，不请求授权、不切换输入源、不改变输入法设置。输入源不匹配、权限缺失或非 macOS 时，在启动窗口前失败。只运行隔离 fixture，不接受真实 endpoint 配置或付费 provider。

driver 通过公开 Carbon TIS API 核对当前输入源为 `com.apple.inputmethod.SCIM.ITABC`；启动独占测试进程，在两个明确阶段用 CGEventPostToPid 向该 PID 投递正确物理键码，清空修饰键 flags。不会向当前任意应用发全局按键，不使用粘贴、Unicode 直接插入、AXSetValue 或 setMarkedText。25 秒超时或失败只终止自己创建的测试进程，输入源在注入前和退出后再次核对；不自动重试。

## 通过的断言

1. `n i h a o` 经系统输入法成为 `ni hao`，原生 NSTextInputClient 查询返回 marked=true。
2. 组字期间点击 Send 及会话 B，文本保持 `ni hao`、marked=true，原会话仍接收后续操作。
3. 空格确认候选后，查询为 `你好`、marked=false。
4. 显式点击 Send，最终用户记录数等于初始 fixture 记录数加一，且恰好一条包含 `你好`；仍选中会话 A。不能把 fixture 预置的十条历史误计成本次提交。

成功收据见[JSON](rust-agent-ui-system-ime-20261020.json)，保留二进制和源码摘要、原生查询日志及输入源前后值。三个失败前置条件另有无注入的自动测试。公共组件和第二 consumer 沿用现有实现，严格 UI Clippy、原生旧模式回归及公开 consumer 检查分别记录。

## 限制与早期失败

这是系统软件输入法配合进程定向合成按键的集成证据，不能宣称实体键盘、候选窗视觉、VoiceOver、所有导航入口、Enter 确认/换行、组字取消、全部 input-source 或完整三端原生 UX 已验收。此探针固定检查当前简体拼音的分词显示及候选，用户词频或输入法配置不同会明确失败，不用放宽断言冒充通过。

探索时使用 TekesUI selftest 的 TypeAscii 曾得到非组字文本；其事件对所有字符使用同一键码，不适合这项系统 IME 证据。切换正确键码后先观察到真实 `ni hao`，随后修正探针预期；首次关闭时又因把十条预置历史计作新增而失败，已改为与启动基线比较。早期日志在 `/tmp/agentkit-system-ime-*` 保留，本次结果不追溯改写先前 Preedit 异常的未确认原因。
