# Native 有界子历史当前程序补验

本轮使用已安装的精确 Kernel 0.9.3 supervisor/worker 与受控本地 provider，生成真实 task 子运行，
从公开 `subagents.history` 读取原子历史及其 sourceCall。没有替换 Kernel 或编辑 TekesUI。
旧程序的 UI-only 和历史原生结论保持原范围，不能移到当前程序上。

## 修复旧验收探针

- `row_id(Tool(...))` 是兼容阅读目标，可能指向 ToolRun，折叠时也可能不在树上。
  改用 `retained_tool_activity` 查询原业务控件；有 Activity 折叠时通过原按钮展开。
- 工具 body 现为结构化详情；使用原 `tool_detail_sections()` 检查每个实际文本面只读、非空，
  并将 Input 文本解析回 JSON 与 sourceCall 原 arguments 比较，不把容器当成一个 TextField。
- 重连后正常执行父 Run 的原自动反馈读取 callback，仍拒绝其他未知后台命令。
- 明确 page size 为 1，要求实际存在更早一页；通过原分页按钮和原捕获 read intent 读取。
  不以 refresh 代替分页。不复制滚动状态机。
- 父 Run 使用非空中文草稿，分页和重连后必须完整保留；有界 pane 不创建 Composer，Send 被拒绝。

新增回归证明折叠/展开前后查询同一原 ToolActivity，阅读行身份不能当作控件身份；外来 tool ID 拒绝。
以上变化仅限 demo/验收代码，不改变用户的业务状态或基础组件。

## 本轮结果与界限

最终驱动退出 0，原收据保存在 `.runtime-fixtures/native-bounded-ui-20261023/`：

- 真实 task、sourceCall、只读有界 history、重连及错误 parent 的所有权拒绝通过。
- 原只读组件的真实单消息分页、原参数详情、既有可见行身份保留、非空父草稿、无 Composer 通过。
- 同一次既有 Native 驱动还验证了历史、发送/幂等、文件、SideChat、模型/命令/设置/上传、
  临时 fork/discard、rename、archive/unarchive 等协议场景，未使用 `--kernel-ui-only`。
  这些是列出的受控协议场景，不代表整个 Kernel 的所有能力验收。
- 普通主 Transcript 及有界 pane 的离屏 GPU 运行通过；原始 child 输入可能属于隐藏 prompt，
  因此 `messages:1` 是最终可见消息控件数，不是原始页数或读取次数。
- 5 项 demo lib（含新回归）、3 项 child binding、严格 demo all-target Clippy、fmt、静态边界通过。
  根协议和第二公开 consumer 未改，本轮不重复声称全部 workspace 在本地重跑。

确切程序、Kernel 身份、源码哈希及收据见[JSON](rust-agent-ui-native-bounded-20261023.json)。
早期只读详情/后台命令失败、编译诊断、原程序和日志保留，未将旧失败改写为成功。
构建只复用共享 target，没有新建独立缓存。

`nativeInteraction`、`physicalIme`、`readingAnchorAcceptance`、`crossProcess` 均为 false。
没有打开桌面、安装/签名宿主或触发权限。分页保持视觉锚点、Task tab 的当前真实组合、
独立子 Run 的全部控件、ProcessContainer 及最终配对布局仍待验收。
PR #10 的 3e40ad6 checks 已通过，UI 仍失败于 #53 的严格嵌套阅读门槛；不放宽，不合并。
