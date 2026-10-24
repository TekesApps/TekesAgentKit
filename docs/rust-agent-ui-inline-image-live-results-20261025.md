# #16：原 RunBinding 的真实三端图片协议闭环

起点 `78b7392`，设计见[图片接入契约](rust-agent-ui-inline-image-contract-20261025.md)，机器结果见[JSON](rust-agent-ui-inline-image-live-results-20261025.json)。只消费固定 TekesUI，不改 Kernel/ClientKit/DSH/Pi，不启动桌面。

`ui/demo/examples/inline_image_probe.rs` 读取显式 endpoint 配置文件，两个既有普通会话分别通过原 `begin_inline_images` / Composer / `prepare_send` / `dispatch_prepared_with` 发送中文图文和纯图。每次只使用一个活动会话；原 runtime 重连后重新打开两条会话，要求非 stale、非 provisional 的公开输入及同 turn final 回复，核对原消息 ID 和附件。Native 的 source position 是公开 entry 身份与 ordinal，不能强行要求 numeric sequence，也不把 ordinal 当永久身份。

`scripts/test-real-inline-images.py` 复用 owned fixture 和既有二进制，公开创建会话、选择隔离模型。独立本地 provider 按 `/native/v1`、`/pi/v1`、`/dsh/v1` 分开记录，服务停止后再核对计数，避免迟到请求被下一端掩盖。实际输入是生成的 69-byte、1×1 PNG；不是个人照片或真实付费模型。原 provider 图片、ImageIO 实际转码 PNG、日志、程序和 SHA 都保留在 SSD。

| 端 | 实际固定 runtime | 图片输入 / HTTP 请求 | 公开历史与附件 |
| --- | --- | --- | --- |
| Native | Kernel 0.9.3 | 2 / 2 | 原 `asset` / `mime` / 中文名称；`session.attachment` 读回原 bytes、MIME、尺寸。读取 DTO 不提供名称，名称由原历史引用证明。 |
| Pi | 1.3.0 独立基线 | 2 / 2 | 原 `mimeType` / base64；`session.attachment` 读回原 bytes、名称、MIME、尺寸。没有替换并行保留的 1.4.0 candidate。 |
| DSH | 0.2.1-rc.2 | 2 / 3 | 原 `attachment` 描述符、SHA、69 bytes、1×1、中文名称。当前 SEP 不提供图片字节 read；不读取私有库补齐。 |

六次图片输入均到达 provider，MIME 为 image/png、SHA 为 `c47dd9465c00e9a0c8b85e9ea58d3034a0d23b9cf926113602f3460752a4eb96`，内容顺序和图文/纯图语义正确；重连后未新增图片 prompt。DSH 另一次为已由公开包源码确认的 first-prompt title，独立记录，不计作图片发送或隐藏重试。其公开 runtime/time 上下文后缀和 image handle 也单列，不能从历史里任取一张图冒充当前输入。普通后来 human 输入、任意无图请求、重复图片/标题请求均有拒绝回归。

DSH 隔离测试模型通过显式 `image_input=True` 声明图片能力；helper 默认保持原配置。它的 admission、存储 normalization 和模型 request preview 是不同阶段。本次小 PNG 在 provider 端未改变摘要，不据此宣称所有尺寸或格式永不转换。

**实际验证**：三端同一最终 example 程序通过；Python 274/0（新增 provider observer 10、DSH fixture 两项；fixture 总 11）；demo all-targets 严格 Clippy、example 构建、格式、静态边界和 acceptance matrix 通过。Root380/UI777/feature289、19/44 等属于前一提交的验证，本切片没有重跑或把它们当新增计数。未更改 production presentation/bindings/components。

早期探针曾错误要求 Native numeric sequence、所有历史都内联同名 PNG、Native read 必带名称；DSH 测试模型最初没声明图片能力，provider observer 也曾误读上下文/标题。这些候选失败与程序、原日志全部保留；按公开协议修正后重跑，未放宽 #53，未改真实 harness 或读取私有存储。

最终证据 `.runtime-fixtures/inline-image-live-20261025/final-reviewed/`：原公开协议收据、provider 图片/转码证据、退出状态和归档程序。源码快照及逐文件 SHA 在同级 `final-reviewed-tested-source/` 与 `final-reviewed-source-hashes.json`；只复用 SSD shared-target，未新建 target，删除缓存 0 字节。owned Native/DSH 退出均为 0，未停止机器上其他服务。

仍未验收：真实三端 mixed file/image、JPEG/GIF/WebP 与大图 normalization、真实模型图像理解、系统选择/粘贴/拖放、位图 UI、窗口/物理 IME/OS AX/GPU。#16/#17 继续开放，位图控件等 TekesUI #117；#53 严格阅读仍失败。`78b7392` 两份 CI 已核对，唯一实际失败 #53；新提交 CI 另核对，PR #10 保持 draft，不合并/tag/发布。
