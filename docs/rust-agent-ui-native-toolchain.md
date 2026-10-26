# 原生验收工具链

原生桌面验收使用一个可更新的宿主身份，历史 `.app` 与 binary 仅作归档。
组件、第二公开 API consumer 和离屏验证通过后，才进入必要的桌面验收。
构建复用 `/tmp/agentkit-layout-text-292-target`，不按验收轮次创建 target。

## 固定宿主

- 路径：仓库 `.runtime-fixtures/native-host/AgentKitAcceptance.app`。
- bundle ID：`org.tekesapps.agentkit.acceptance`。
- 程序：`Contents/MacOS/tekes-agent-ui-demo`。
- 签名：当前机器的 Apple Development 证书
  `F091EF63D82913C1B9E9371BDF6533F4B727FF20`，Team `9HFX228644`。
  不允许 ad-hoc 签名或自动切换证书。证书轮换需要显式更新策略和重新核验权限。

安装入口为 `scripts/native_acceptance_host.py --install <已验证程序绝对路径>`。
安装只复制并签名私有副本，不在 Cargo/历史程序的硬链接上签名，不启动窗口。
签名访问私钥可能出现 Keychain 提示，因此不将安装纳入普通测试或 CI。
本次修改仅交付安装工具及其模拟测试，**尚未安装或签名实际宿主**。

安装/验收使用同一排他锁。安装前检查全部可见进程的文件占用，检查失败则拒绝替换；
已签名宿主及身份收据一起归档，保留新旧程序 SHA-256、固定证书和 designated requirement。
更新必须维持同一 designated requirement。重复安装同一输入不产生新归档。
升级、回退后再次升级时，归档目录使用程序哈希加唯一后缀，避免覆盖旧收据或因相同哈希而拒绝安装。
只有固定活动路径可用于验收；归档目录名称变化不产生新的活动宿主，也不启动历史应用。
原生 launcher 在启动前核验路径、bundle ID、签名、Team、程序 SHA 与安装收据；
历史 bundle 和直接 binary 启动均不允许进入正式原生验收。

## 截图入口

验收程序通过 `acceptance_script` 把 SDK `Action::Capture` 转换为 SDK `Action::Log`，
继续使用 SDK 的原动作时间表，不自行实现输入、焦点、IME 或调度。
日志载荷为 `AGENTKIT_EXTERNAL_CAPTURE_REQUEST`，包含原始目标路径及
`capturePerformed: false`。程序不会执行 `screencapture`，也不会为截图请求权限。

桌面截图统一使用 Codex Computer Use 固定服务：
`/Users/frankqin/.codex/computer-use/Codex Computer Use.app/Contents/MacOS/SkyComputerUseService`，
bundle ID `com.openai.sky.CUAService`，OpenAI Developer ID，Team `2DC432GLL2`。
通过工具的原生截图接口消费阶段请求，不能从验收程序、shell 或历史 binary 再调用系统截图。
本次未调用该服务截图，当前 TCC 授权状态未读取成功，不能声称已核验授权。

截图请求日志不等于截图成功。需要另存固定工具返回的原始图片，核对请求阶段、目标窗口、
PNG 可解码及非零尺寸，并记录实际工具身份/窗口/时间。未完成的请求保留为缺失证据；
等待图片的探针保持原等待和超时，不创建占位文件、不伪造成功。
行为收据与截图证据分别验收。历史 `could not create image from window` 和退出码 1
继续保留，不反写为成功。

固定截图服务若尚未获授权，只在正式桌面验收时由用户授予该固定身份；
签名、组件测试、离屏输出均不授予 Screen Recording 权限。不重置或直接修改 TCC。

## 保留控件与可见阅读行

`TranscriptContent::retained_tool_activity(id)` 可查询被折叠历史保留的原业务控件。
返回控件不意味着已挂载或可见；`tool_run`、`tool_activity_id` 仍从可见行所属组查询，
其中返回的子控件 ID 也不证明子控件当前挂载。
滚动/阅读定位继续使用 `row_id` 和 SDK 原阅读事务，不用隐藏控件 ID 充当阅读项。
已结束回合展开后仍显示历史推理；实时 `None`/`Compact` 偏好遵循现有 presentation。

## 本次验证范围（2026-10-23）

UI workspace 614 项通过、1 项忽略（既有 TekesUI #49 Popover 子控件路由门槛），
含原生截图动作的纯转换测试及第二公开 consumer 的隐藏工具控件查询。
严格 Clippy、格式及 UI 边界检查通过；Python 脚本测试 67 项通过。
日志位于 `.runtime-fixtures/global-tool-runs-20261023/`。
这些结果不代表原生窗口、物理中文输入、截图、实际宿主签名安装或跨进程验收。
TekesUI #53 嵌套阅读的严格 CI 门槛保持不变。

补充安装器回归：模拟 A→B→A→C 的签名安装及同输入重复安装，复现并修复了原哈希目录
`FileExistsError`。旧程序和三份收据均保留，活动路径/bundle ID/证书要求不变，
重复安装不再次签名。Python 全套现为 68 项通过；签名命令均为模拟，实际宿主仍未安装。
