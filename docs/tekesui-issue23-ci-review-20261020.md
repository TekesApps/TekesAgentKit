# TekesUI #23 CI 后继候选复验

公开提交 [8cf0840790334ba224afc0595897c74231c712e6](https://github.com/TekesApps/TekesUI/commit/8cf0840790334ba224afc0595897c74231c712e6) 已修复此前 61ddf7c 的公共类型登记失败。[远端 CI](https://github.com/TekesApps/TekesUI/actions/runs/39365340268) 成功，消费侧独立复验也通过。建议将它建立为单独的开发冻结候选，供正式组件继续接入；本次没有替换产品的 4310937 pin、合入上游 main、打 tag 或关闭 Issue。

## 输入与隔离

独立 clone 固定该 SHA，工作树干净；相对 61ddf7c 只有公开的 8 文件增量。公共生产库和 native consumer 未变，没有混入 Gallery 后续 Button/Link/required 等 WIP。发布方完整清单 201 项、`ae0b1055118d50d4d6e28a3d47e5e40bab254c7c6721d0c483e2a49f78114a4e` 已独立逐项验证。

另将当前 AgentKit 源码复制到隔离目录，只在副本中显式更换精确 Git rev、解析锁文件和加入既有 plain-content 探针。实际 Cargo 八个 TekesUI 包均来自 8cf0840，非 UI 的 299 个锁定包身份未变；所有 Rust 源码与当前工作区匹配。正式 manifest、lock 和冻结清单没有修改。

直接把发布方 201 清单作为运行时闭包时，现有严格检查拒绝了 Gallery 这个非依赖包。没有放宽检查：完整 201 项验证独立保留，再为实际八包生成明确的 199 项消费投影，差集仅 Gallery 的 Cargo.toml 和 snapshot.rs，SHA 为 `f0af230a1efe97334b90eb193f77ca0d30dec43ef09e487ad6948c38a938257f`。实际 Git 来源、全部消费文件哈希、额外文件和符号链接检查继续执行。

## 消费侧结果

- 公共 API / registry 共 6 tests、controls 核心 233 tests、上游独立 consumer 134 tests 通过。
- 隔离 AgentKit UI workspace 186 tests（含第二公开 consumer 和 shared-history/transient plain-content 探针）通过；AgentKit 与上游 consumer 两处 strict all-target Clippy 通过。
- 原生使用该精确 clone 的未修改公开 `composition-consumer` plain 例子。核对了实际 bundle 可执行文件哈希、进程路径、窗口标题和新写入的 snapshot，不使用仅返回的旧 AX 树作为活窗口证明。测试剪贴板为进程私有，不读写用户通用剪贴板。

按 #23 原验收项记录如下：

| 条件 | 本次证据与边界 |
| --- | --- |
| 无装饰正文置于普通圆角 Surface，与按钮、footnote 组合，普通表单保持默认呈现 | 独立公开 consumer 的 API/布局检查和原生画面通过；普通字段默认外观仍在，输入/粘贴的 CUA 尝试未形成新的原生编辑通过项 |
| 不增加表单装饰空隙，宽度变化使用原文本几何，父文档唯一滚动 | 公开布局测试通过；原生较小窗口在 version 11 的父偏移为 158、正文内部偏移为 0。随后放大窗口，正文宽度由 663 变为 1863，内容放得下时父偏移归零是合法 clamp，不是滚动失败 |
| 中文/emoji 选择、复制、AX 文本与 UTF-16 range，原生文本服务 | 原生指针选中并以 Cmd-C 复制“中文😀”，实际选区 `[8,12]`，私有剪贴板内容正确；AX 展示原正文与选中文本。未验证物理 IME、VoiceOver 或全部 AX 属性 |
| 受控追加保留既有选区，复用原编辑/布局/焦点/滚动 | 原生 version 2 → 3 后选区仍为 `[8,12]`；隔离 AgentKit 探针以现有 RunController 的历史与 transient 流验证受控追加/旧版本拒绝，没有复制状态机 |

技术结论是该公开只读内容 API 与修复后的注册/输入门槛可接收，建议建立新的开发冻结候选。它不等于完整 AgentKit 原生验收：#22 混合文档前插/稳定锚点事务、AgentKit 的流/持久记录阅读关联、正式 Transcript / RunView 和物理 IME 仍未完成。当前产品仍固定 4310937。

本轮不做性能验收。原生验证窗口已经关闭；隔离 checkout、target、测试 bundle 和 sidecar 作为证据保留，未清理主仓库或 Gallery WIP。完整输入、实际解析、测试和原生状态摘要见[机器收据](tekesui-issue23-ci-review-20261020.json)。
