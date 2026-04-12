# 概述

## 实时流式传输
{app} 通过本地 WebSocket 服务器流式传输派生的 EEG 指标和设备状态。广播事件包括：eeg-bands（约 4 Hz — 60 多项评分）、device-status（约 1 Hz — 电池、连接状态）和 label-created。原始 EEG/PPG/IMU 样本不通过 WebSocket API 提供。该服务通过 Bonjour/mDNS 以 _skill._tcp 广播，以便客户端自动发现。

## 命令
客户端可以通过 WebSocket 发送 JSON 命令：status（完整系统快照）、calibrate（打开校准）、label（提交标注）、search（最近邻查询）、sessions（列出录制）、compare（A/B 指标 + 睡眠 + UMAP）、sleep（睡眠分期）、umap/umap_poll（3D 嵌入投影）。响应以 JSON 形式在同一连接上到达，包含 "ok" 布尔值。

# 命令参考

## status
_(无)_

返回设备状态、会话信息、嵌入计数（今日和全部）、标签数量、上次校准时间戳以及每通道信号质量。

## calibrate
_(无)_

打开校准窗口。需要已连接且正在流式传输的设备。

## label
text（字符串，必填）；label_start_utc（u64，可选——默认为当前时间）

向标签数据库插入一个带时间戳的标签。返回新的 label_id。

## search
start_utc、end_utc（u64，必填）；k、ef（u64，可选）

在给定时间范围内搜索 HNSW 嵌入索引中的 k 个最近邻。

## compare
a_start_utc、a_end_utc、b_start_utc、b_end_utc（u64，必填）

通过返回每个时间范围的聚合频段功率指标（相对功率、放松/参与度评分和 FAA）来比较两个时间范围。返回 { a: SessionMetrics, b: SessionMetrics }。

## sessions
_(无)_

列出从每日 eeg.sqlite 数据库中发现的所有嵌入会话。会话是连续录制范围（间隔 > 2 分钟 = 新会话）。按最新优先返回。

## sleep
start_utc、end_utc（u64，必填）

使用频段功率比将时间范围内的每个嵌入时段分类为睡眠阶段（Wake/N1/N2/N3/REM），并返回包含每阶段摘要的睡眠图。

## umap
a_start_utc、a_end_utc、b_start_utc、b_end_utc（u64，必填）

将两个会话嵌入的 3D UMAP 投影加入队列。返回用于轮询的 job_id。非阻塞。

## umap_poll
job_id（字符串，必填）

轮询先前入队的 UMAP 任务的结果。返回 { status: 'pending' | 'done', points?: [...] }。

## say
text: string（必填）

通过设备端 TTS 朗读文本。即发即忘——立即返回，音频在后台播放。首次调用时初始化 TTS 引擎。
