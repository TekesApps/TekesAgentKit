# 正常 app 的系统拼音软件验收（2026-10-22）

上一轮同一程序的直接启动发生过窗口失焦，系统拼音未完成。本轮为现有 probe 增加可选
LaunchServices app 路径，原生输入/焦点/文本服务和所有业务断言均不改动。
这提供了通过验收的正常 app 路径，不能单凭一次通过断定之前失焦的根因，或声称修复了 SDK。

## 运行与责任

```sh
python3 scripts/test-system-ime.py \
  --demo .runtime-fixtures/AgentKitChildOpenFencing20261022.app/Contents/MacOS/AgentKitChildOpenFencing20261022 \
  --native-app .runtime-fixtures/AgentKitChildOpenFencing20261022.app \
  --log /tmp/agentkit-ime-app-new.log \
  --observe-frontmost \
  --receipt /tmp/agentkit-ime-app-new.json
```

日志/收据路径必须尚不存在。默认直接可执行文件模式仍保留；`--log`、`--observe-frontmost`
用于显式 `--native-app` 模式。前置要求仍为当前简体拼音和已有事件投递权限，不切换输入源
或请求权限，不接受真实 endpoint/provider。

新模式复用 `native_probe_launcher` 的精确 APPL/executable 校验、独立 stdout/stderr、panic/
重复或缺失收据拒绝。`open -W -n` 的0只代表 launcher 成功，**applicationExitCode 保持 null**。
对每个 TYPE/COMMIT 原阶段，在每个键码前重新核对前台 bundle、同一 PID、`proc_pidpath`
实际可执行文件和当前输入源；目标失焦/变化立即停止后续注入，不转投新前台、不重抢焦点。
日志按完整 UTF-8 行读取，部分写入不能被误识别为 ready 阶段。

只向验证的原 PID 使用既有 `CGEventPostToPid` 投递正确键码，不用 Unicode 插入、粘贴、AX
替换或合成 Preedit。失败仍等待原 selftest/launcher 的有界结束并保留日志，不重新启动或为
通过断言而提交/取消组字。launcher 超时后需检查原应用进程，不能推断它已停止。

## 本轮证据

未重新编译。使用既有冻结程序 `.runtime-fixtures/AgentKitChildOpenFencing20261022.app`，
SHA256 `75e84a1360b86a0268f7f5c6d513d63eb607d90758f034403a81ba3dc141d911`，
对应上一轮 Rust 源码 `126a79bc315133efe11e16744d4b01b2815a3889`。

同一原生窗口通过全部原断言：系统输入法产生 `ni hao` marked text；点击 Send 和会话 B
均不提前发送或切换；空格确认成 `你好`、marked=false；显式 Send 后仅新增一条用户记录，
且只有一条包含 `你好`，仍选中会话 A。前台轨迹记录此 app 在测试期间保持前台，关闭后返回
Codex；输入源前后均为 `com.apple.inputmethod.SCIM.ITABC`。

最终 CLI 的另一次复验中，原窗口收到 `n`、`i` 并产生 `ni` preedit，随后前台切回 Codex。
逐键 guard 拒绝后续 `h a o` 和确认键；原程序的完整性断言仍失败，无成功收据。
该结果保留为失败，不能把首次完整通过扩展为稳定性通过，也不能据此归因于 SDK 或启动方式。
两次使用同一程序；期间只收紧了 CLI 收据的排他写入和参数检查，没有修改输入/焦点引擎。

这是 **owned SEP fixture、同进程原生、系统软件 IME + PID 定向合成键码** 的完整通过。
不是实体键盘、候选窗视觉、VoiceOver、Enter/取消组字或真实三端原生 IME 验收。
原程序已接受的子 pane 乱序/关闭、readonly分页/组字与三端离屏数据证据保持原归属，
本轮不重复运行或扩大其范围。之前的直接启动失败日志不覆盖、不转记为成功。

52项验收工具测试通过，覆盖错误 app/可执行文件、PID 改变、TYPE中途换前台、源变化、关闭、重复/乱序阶段、
分段 UTF-8 日志、证据保护、正常 app 实时阶段与 launcher crash。首轮单元测试的临时目录
symlink 预期差异已规范化，失败日志仍保留。未改 Rust 或 SDK，不重跑本地既有505项 UI
测试/Clippy；新提交的 CI 独立记录。

当前公开 AgentKit 仍为 v0.3.8；TekesUI main/tag 仍为精确
`09b4bab44b6dfa65487250ea816b610b9df59095`。ProcessContainer #48 仍 OPEN，
完整 Views 对齐、真实三端原生组合、物理 IME/AX 和最终 review/合并/tag 尚未完成。

原始日志、前台轨迹、程序/脚本哈希与收据位于 `.runtime-fixtures/system-ime-app-20261022/`，
摘要见[机器记录](rust-agent-ui-system-ime-app-20261022.json)。没有新建 target、编译或删除缓存，
释放空间0；源码、失败证据及既有验收程序全部保留。
