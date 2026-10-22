# 连续工具 URL 预览与 Foundation 对照

本轮修正连续工具常显摘要的 URL 显示，不改变请求、导航或可执行行为。只读对照 ClientKit 的 `TranscriptToolRowModel.continuousSummary` 和 Tekes 的宿主适配边界。

此前使用 WHATWG URL 解析并连接虚构 base，会消去 `../`、规范化点路径、补根 `/`、改变 ASCII 主机名大小写，并让 opaque scheme 的正文进入预览。现在用精确固定的 `iri-string = 0.7.14` 读取原引用的组件；不解析相对 base、不规范化原路径。用户名/密码/端口、query 与 fragment 不进入常显标题。Unicode 域名通过既有 URL Host 服务做 IDNA；路径保留原相对/点段关系，并按 Foundation 的目录末尾 delimiter 与解码规则显示。无效 UTF-8 不替换成猜测文本。

61 组 JSON 事实来自 macOS 26.6.2 的 Foundation NSURL `host`/`path` 属性，覆盖空/root/相对/点路径、重复与编码末尾斜杠、IPv6、中文/组合 emoji/IDNA、凭据过滤、opaque scheme、坏 percent 和坏 UTF-8。三个工具 family 分别比较。`scripts/check-tool-url-foundation.py` 默认只读核对；`--write` 才刷新数据，更新前需核对真实属性结果。脚本只创建 Foundation 数据对象，无 Swift、窗口、键鼠或网络操作；跨平台 Rust 测试只消费冻结 JSON，不依赖 macOS。

这不证明全部非 RFC 输入等价：例如非数字 port，Foundation 可以生成 host/path 为空的对象，而严格引用 parser 当前回退到安全标题。该边界仍是 AgentKit 自身待办，不转交为 TekesUI 缺口；也不将只读 Foundation 对照称为 Swift 宿主或原生 UI 验收。

验证：根 workspace 251 项通过；宿主 Transcript/Output/RunReading 与第二公开 consumer 的 114 项通过，保留 1 项既有 #49 ignore。默认分组标题经原 SDK 字段/CoreText 标签确认保留主机名/路径拼写且没有凭据或 query。根与 UI all-targets Clippy、格式/依赖边界、实际八包/243 输入核验及 demo 构建通过。最终 JSON 格式化后再次运行 5 项 tool-run 测试通过。

上一提交 `c23bab6` 两个 Linux CI 通过，两个 macOS CI 均在必需的严格 nested-reading 步骤失败。本轮严格示例仍退出 1：父跟随未暂停、append 后 offset 634、详情不可见。#53 不忽略、不放宽。既有原生冻结程序的收据不转移到本轮；没有重跑原生或真实三端，完整交付仍未达到。

## 构建缓存清理

所有验证复用 `/tmp/agentkit-layout-text-292-target`，使用 `CARGO_INCREMENTAL=0`，未新建 target 或验收程序副本。验证结束后检查进程、文件占用并持有现有 `debug/.cargo-lock` 的单一 flock：

- 删除三处旧 `debug/incremental/tekes_agent_{contract,session,presentation}-*`，移除 47,136,768 bytes（44.95 MiB）。首次检查发现三个硬链接便停止；确认另一端是保留的 `debug/deps/*.rmeta` 后，才删除缓存端链接，并从释放计量中排除仍保留的元数据。
- 根据每个 dep-info 的实际源根及新 `tool_url.rs` 缺席，删除 11 个已被替代 presentation unit 的 16 个 `.rlib/.rmeta` 文件，移除 86,900,736 bytes（82.88 MiB）。仅删除编译中间产物；当前 unit、dep-info/fingerprint、所有测试/验收程序都保留。

合计移除 134,037,504 bytes，127.83 MiB，按删除文件分配块计量；没有把共享卷可用空间变化归为独占释放。WIP 前后指纹、当前库、三个元数据锚点及冻结原生程序 SHA 均复核。现有共享 target 约 33 GiB，保留复用的 SDK/依赖、各验证配置、验收可执行文件及用途未充分判明的历史产物，没有整删。源码、固定快照、日志、收据、截图保留。

逐文件清单、占用/锁/WIP 检查及数据哈希见[机器记录](rust-agent-ui-tool-url-20261023.json)。
