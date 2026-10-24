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
- mixed file/image 的数量、总 bytes 与顺序要按原消息整体核对；imageLimits 不是已证明的模型 vision 能力。DSH `0.2.1-rc.2` 公开 attachment-local 包已核对：imageLimits 先对原图 bytes/pixels/单边做硬拒绝，之后独立 normalizationPolicy 才可缩放/重编码。Kit不能把 admission 上限当成 resize 目标，也不能猜测模型名或偷偷转换原图。公开源码与npm lock integrity收据在 `.runtime-fixtures/inline-image-contract-20261025/dsh-public-package-source.json`（lib/index.js SHA `cd39685581b4573a453c6126f8ce73da0bf1257c378827ebb431d6d4064f7662`）。
- 用 owned Native/Pi/DSH 的原入口分别验证图文、纯图与混合附件，检查 provider 实际收到的 MIME/digest/order 与公开 history/read。合成 provider 仅证明协议送达，不证明真实模型理解图片。

以上是完整目标。后续[RunBinding本地闭环](rust-agent-ui-inline-image-binding-results-20261025.md)已补图片捕获/CAS与提交前重验；真实三端图片闭环仍待补，不能把最初Pi窄修或fixture成功当完整结果。

## UI 依赖

通用位图控件已去重提交 [TekesUI #117](https://github.com/TekesApps/TekesUI/issues/117)：公开 ImageContent、受控资源生命周期、fit/alignment/clip、fallback 和 AX 语义。AgentKit 负责 Composer/Transcript 的业务组合，不复制基础渲染/资源引擎。SVG IconContent 与低层 ImageQuad 不等同于该能力；自动锚定预览仍归 #46。

系统 picker、粘贴/拖放、实际窗口排版、IME、OS AX、跨进程与性能验收均另列，不从纯组件/离屏结果推断通过。#16/#17 继续开放，PR #10 仍试验状态，#53 严格门不豁免。

## 原 ticket 的混合附件 facts

原 PreparedSubmission 只保留编译 plan/settings，file block 只有 receiptId，原 ComposerArtifact 的 byte_count 不在 ticket 中。为了在host模型准备后校验混合file/image总量，新增只读 `PreparedAttachmentFacts`，仅保存artifact ID与可缺省的AttachmentFacts，按同一锁内原draft的附件顺序与plan同时冻结；不复制图片base64，不读后来草稿，不按可重复receiptId建map。旧file缺facts且需要总量上限时明确失败，不当作零；冻结只证明同票据捕获，不证明任意外部host自报facts真实，SEP仍负责最终验证。

`InlineImageInput`仅接收basename与不可变Arc原bytes；`InlineImageSelection`捕获原binding/baseline/epoch并一次性attach。policy的byte/count/mixed-total先于真实格式/尺寸与完整首帧解码；按真实格式生成MIME，原bytes编码为标准base64，最终原context CAS一次提交全部图片。失败/取消/迟到不部分添加，不伪造Uploading/receipt/uncertain transport，不自动重试。直接消费bindings中的image私有输入校验（PNG/JPEG/GIF/WebP，当前锁内0.25.10）；不引用renderer，暂不把Pi固定policy抽成通用core。

新pre-send重验仅由包含image的plan触发，纯文字和纯file保留原路径；计数和总bytes涵盖同一ticket内所有file/image，file用冻结facts，image用真实base64 bytes与解码尺寸。顺序为原pane/epoch检查→原host.prepare→pane/epoch再查→policy读取/真实ticket校验→pane/epoch再查。policy await、关闭/重连均不可跨原权限。原command附件能力检查不变。此处不声称policy与prompt原子绑定：后续open_session等await期间的并发变更仍由SEP最终拒绝。失败在mark_dispatched之前，复用原Completion/finish恢复附件并保留新文字。

验收要覆盖：原ticket与后来草稿分离；host准备调低policy导致mixed-total失败且零prompt；legacy file facts缺失；policy await时pane关闭/epoch变化；第二张损坏图片不部分CAS；文字编辑允许原CAS但附件context ABA/Send拒绝旧capture；四格式与中文名/顺序/原bytes；纯文本/纯file不新增policy查询；含图command不绕过原能力。实际实现与分层验证见[本地闭环结果](rust-agent-ui-inline-image-binding-results-20261025.md)；真实三端与完整UI仍未通过。

旧图片草稿缺facts时，capture追加也从原encoded data计算真实字节预算；不得把可计算的图片当成未知file或零字节。file的legacy facts缺失则在大小限制下拒绝。最终fixture结果为UI777/0/1，Root380源码未因该兼容修正改变，详见链接收据；真实三端图片仍待后续验证。

## 真实端非桌面验收补充

[三端结果](rust-agent-ui-inline-image-live-results-20261025.md)已通过原 RunBinding 的图文/纯图发送、公开历史和重连。Native 保留 asset/mime 引用，经 session.attachment 读取；Pi 保留 mimeType/data，名称从公开 attachment 读取；DSH 保留完整 attachment 描述符；[后续实际字节回读](rust-agent-ui-dsh-image-read-results-20261025.md)已通过原公开 session.attachment，旧 probe 的 unsupported 标记已纠正。不能要求所有历史都回显同一种 canonical 输入 JSON，也不能把 Native source ordinal 替代永久 entry/message 身份。

provider 按 endpoint 独立 URL 路由并统计全部请求：本次图片输入共六次，DSH 另有一次公开 first-prompt title；已知 runtime/time suffix 和图片 handle 与用户文本分别记录，普通后来 human 输入不可跳过。只声明原输入送达/状态恢复，不声明模型理解图片、完整 UI 或真实三端 mixed file/image 已完成。固定 DSH 测试模型的 image capability 是显式 fixture 输入，默认 helper 行为保持。
