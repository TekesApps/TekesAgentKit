# #18 / #23：同一只读面板前插与重连的 CPU 阅读验证

新增`ui/demo/tests/readonly_paged_reading.rs`，消费既有ReadOnlyTranscript/ChildPaneBinding和TekesUI50ec49f公开reading transaction，无生产组件、SDK或Kernel修改。ClientKit只读基线c289bc9的ThreadTranscriptPresenter+Scroll.swift:575–620先固定visible turn及item-local offset，owner仍有效时完成前插；本切片用原SDK读取锚点与屏幕相对位置验证对应行为，不复制滚动状态机或手工补offset。

这是内存SEP fixture与CoreText/Scene的CPU排版测试，不是实际Kernel/原生窗口。先通过原wheel事件滚到内容底部，再向上180pt，确认offset1330→1150且实际文档溢出；通过原ReadOnlyTranscript Earlier intent执行ChildPaneBinding，并补入真实可见消息行24→48。原锚点仍可见，viewport内的相对Y保持-22pt，offset由SDK补偿为3130；runtime reconnect、原Refresh后仍为相同Y/offset，following=false。相同pane root、viewport、reading document key与保留行ID均核对，reading revisions为1→3→7。具体值见[机器收据](rust-agent-ui-readonly-paged-reading-results-20261025.json)。

焦点按公开SDK的真实行为验收：ContentHost::accessibility_action对显式Press聚焦按钮，因此Earlier publication须保留该按钮焦点；捕获Refresh后，显式把焦点移回父editor，再执行读取和安装，返回帧须保留这个更新的焦点选择。父非空中文草稿、原selection及parent binding保持。没有把AX方法调用当作OS AX，也没有系统IME组字验收。

主会话修复了候选的Shaper导入/版本借用、错用TranscriptContent root查ReadOnlyTranscript、错误parent owner、未完成宿主自动读取及不足溢出的页大小。按SDK真实scroll事件方向建立测试，保留全部失败日志；按源码确认显式Press焦点语义后区分主动聚焦与异步回帧，原锚点/1pt位置门禁保持。

默认和acceptance-source-witness两配置均1/1通过，测得JSON完全一致；严格Clippy、格式与diff检查通过。591项固定源码输入核对，两个测试程序和日志归档于SSD `.runtime-fixtures/acceptance-refresh-20261025/readonly-paged-reading/`，未新建target。此前实际Native两页/一次Earlier证据仍单列，不能将本受控可见前插变成真实Native多页验收。

同时对前一阶段验收归档做只读占用/所属进程退出检查，再用硬链接去重13份逐字节相同的归档程序。全部16个归档路径及哈希保留，共享target/日志/截图/WIP未改；回收唯一inode分配块736514048字节，文件系统前后实测可用空间增加736432128字节（约702MiB）。审计与执行收据存同SSD根，缓存目录删除0字节。

#18/#23保持开放。未验收：跨新Host持久checkpoint（TekesUI#42）、嵌套内层阅读通知父容器（#53）、真实Native可见消息前插/多次Earlier、桌面/物理IME/OS AX、完整Views/ProcessContainer、性能。现有#53严格失败不跳过、不豁免，不新增重复上游Issue。固定1026格矩阵不自动升级。
