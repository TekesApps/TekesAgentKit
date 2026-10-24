# #16 / #17：inline image 接入边界

参照只读 ClientKit `c289bc9d33edcd83a782ac17927a8c65a027eccd`，AgentKit 起点 `156aff5`。仅 Rust；只消费 TekesUI 精确 pin `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`，不改 Kernel/SDK，不跑桌面。

## 已确认差异与本次窄修

ClientKit PIHostEndpoint 与 PIImageStore 限制单图 4 MiB、每消息 8 附件/16 MiB、单边 8192、总像素 33,554,432，并实际解码首帧后保存原图。Rust Pi 原先只读尺寸、使用 40,000,000 pixels 且缺单边限制，损坏像素数据可能通过。

本次只修 Pi 的公开 policy 和原 materialize 入口：先检查 bytes/尺寸/像素，再用现有 image crate 验证解码；保留原 base64、MIME、文件名、尺寸与 SHA 身份，不重编码。decoder 工作内存 guard 256 MiB 属实现保护；部分 codec 的 max_alloc 是 best effort，不能描述为已证明全 codec 严格内存预算。输入尺寸和像素限制独立先行强制。

验证需包含四种支持格式的小图、头部可读但像素损坏、尺寸/像素界限、公开 policy，以及原 materialize 保存/转发原始数据、损坏数据不落盘。内部方法回归不等于真实 Pi provider/窗口验收。

本切片实际结果：Pi crate 19 项通过（新增 5 项），严格 Clippy/all-targets 通过；四种小图均执行解码，原 materialize 的原 bytes/digest/中文名称/尺寸转发与保存、损坏数据不落盘均通过。证据位于 `.runtime-fixtures/inline-image-contract-20261025/pi-tests.log`、`pi-clippy.log`。没有运行真实 provider 或桌面，没有变更依赖或创建 target。

## 后续原输入闭环

- `AttachmentSelection` 的 uploadFile 能力不代表 inline image 能力；图片使用 `{type:image,mediaType,data,name}`，不能把本地路径文字代替图片内容。
- 开始选择前捕获原 RunBinding、destination、Composer context baseline 与 epoch。文件权限/普通文件与有界读取归宿主；可信格式/实际解码及 session policy 归 Kit，SEP 在最终入口再次校验。
- 使用原 context-at-epoch/CAS 把图片加入 Composer artifact，不新建上传/提交状态机；迟到选择不能写入新 session、新 pane 或已修改草稿。
- 模型准备可能改变 session policy。提交前在原 ScopedPreparation 内、host preparation 之后，以不可变 ticket 的真实图片内容重新校验当前 policy；失败走原恢复/错误路径，不自动重试。不能只在 picker 时检查，也不能依赖可选宿主保证。
- mixed file/image 的数量、总 bytes 与顺序要按原消息整体核对；imageLimits 不是已证明的模型 vision 能力。DSH 的 resize 与 reject 语义先从公开实现核对，不能猜测模型名或偷偷转换原图。
- 用 owned Native/Pi/DSH 的原入口分别验证图文、纯图与混合附件，检查 provider 实际收到的 MIME/digest/order 与公开 history/read。合成 provider 仅证明协议送达，不证明真实模型理解图片。

这些是下一阶段要求，本次 Pi 窄修尚未实现通用 RunBinding 图片选择、提交前 policy 重验与三端图片闭环。

## UI 依赖

通用位图控件已去重提交 [TekesUI #117](https://github.com/TekesApps/TekesUI/issues/117)：公开 ImageContent、受控资源生命周期、fit/alignment/clip、fallback 和 AX 语义。AgentKit 负责 Composer/Transcript 的业务组合，不复制基础渲染/资源引擎。SVG IconContent 与低层 ImageQuad 不等同于该能力；自动锚定预览仍归 #46。

系统 picker、粘贴/拖放、实际窗口排版、IME、OS AX、跨进程与性能验收均另列，不从纯组件/离屏结果推断通过。#16/#17 继续开放，PR #10 仍试验状态，#53 严格门不豁免。
