# 隐私概述
{app} 设计为完全本地优先。您的 EEG 数据、嵌入、标签和设置永远不会离开您的设备，除非您明确选择共享。

# 数据存储

## 所有数据都保留在您的设备上
{app} 记录的每一项数据——原始 EEG 样本（CSV）、ZUNA 嵌入（SQLite + HNSW 索引）、文本标签、校准时间戳、日志和设置——都本地存储在 {dataDir}/ 中。没有数据上传到任何云服务、服务器或第三方。

## 无需用户账户
{app} 不需要注册、登录或任何形式的账户创建。不存储或传输任何用户标识符、令牌或身份验证凭据。

## 数据位置
所有文件存储在 macOS 和 Linux 上的 {dataDir}/ 下。每个录制日期都有自己的 YYYYMMDD 子目录，包含 EEG SQLite 数据库和 HNSW 向量索引。标签在 {dataDir}/labels.sqlite 中。日志在 {dataDir}/logs/ 中。您可以随时删除这些文件中的任何一个。

# 网络活动

## 无遥测或分析
{app} 不收集使用分析、崩溃报告、遥测或任何形式的行为追踪。应用中没有嵌入分析 SDK、追踪像素或回传信标。

## 仅限本地的 WebSocket 服务器
{app} 运行一个绑定到本地网络接口的 WebSocket 服务器，用于向局域网配套工具流式传输。此服务器不暴露到互联网。它向同一本地网络上的客户端广播派生的 EEG 指标（频段功率、评分、心率）和状态更新。不广播原始 EEG/PPG/IMU 样本流。

## mDNS / Bonjour 服务
{app} 注册一个 _skill._tcp.local. mDNS 服务，以便局域网客户端自动发现 WebSocket 端口。此广播仅限本地（组播 DNS），在您的网络外部不可见。

## 更新检查
When you click 'Check for Updates' in Settings, {app} contacts the configured update endpoint to check for a newer version. This is the only outbound internet request the app makes, and it only happens when you explicitly trigger it. Update bundles are verified with an Ed25519 signature before installation.

# 蓝牙与设备安全

## 蓝牙低功耗（BLE）
{app} 通过蓝牙低功耗或 USB 串口与您的 BCI 设备通信。连接使用标准的 CoreBluetooth（macOS）或 BlueZ（Linux）系统栈。不会安装自定义蓝牙驱动程序或内核模块。

## 操作系统级权限
蓝牙访问需要明确的系统权限。在 macOS 上，您必须在系统设置 → 隐私与安全性 → 蓝牙中授予蓝牙访问权限。{app} 未经您的同意无法访问蓝牙。

## 设备标识符
设备序列号和 MAC 地址从 BCI 头戴设备接收并显示在界面中。这些标识符仅存储在本地设置文件中，永远不会通过网络传输。

# 设备端处理

## GPU 推理保持本地
ZUNA 嵌入编码器完全通过 wgpu 在本地 GPU 上运行。模型权重从本地 Hugging Face 缓存（~/.cache/huggingface/）加载。没有 EEG 数据发送到任何外部推理 API 或云 GPU。

## 滤波和分析
所有信号处理——重叠保存滤波、FFT 频段功率计算、频谱图生成和信号质量监控——都在您的 CPU/GPU 上本地运行。没有原始或处理过的 EEG 数据离开您的设备。

## 最近邻搜索
用于相似性搜索的 HNSW 向量索引完全在您的设备上构建和查询。搜索查询永远不会离开您的设备。

# 您的数据，您做主

## 访问
您的所有数据都在 {dataDir}/ 中，采用标准格式（CSV、SQLite、二进制 HNSW）。您可以使用任何工具读取、复制或处理它。

## 删除
随时删除 {dataDir}/ 下的任何文件或目录。无需担心云备份。卸载应用仅删除应用程序二进制文件——{dataDir}/ 中的数据不受影响，除非您自己删除。

## 导出
CSV 录制和 SQLite 数据库是可移植的标准格式。将它们复制到任何设备或导入 Python、R、MATLAB 或任何分析工具。

## 加密
{app} 不对静态数据进行加密。如果您需要磁盘级加密，请使用操作系统的全盘加密功能（macOS 上的 FileVault，Linux 上的 LUKS）。

# 活动追踪

## 活动追踪
启用后，NeuroSkill 记录哪个应用处于前台以及键盘和鼠标的最后使用时间。此数据完全保留在您的设备上的 ~/.skill/activity.sqlite 中——永远不会发送到任何服务器、远程记录或包含在任何形式的分析中。活动窗口追踪捕获：应用名称、可执行文件路径、窗口标题以及该窗口变为活动状态的 Unix 时间戳。键盘和鼠标追踪仅捕获两个时间戳（上次键盘事件、上次鼠标事件）——绝不包括按键、输入的文本、光标坐标或点击目标。两项功能可在设置 → 活动追踪中独立禁用；禁用后立即停止收集。现有行不会自动删除，但您可以随时通过删除 activity.sqlite 来移除。

## 辅助功能权限（macOS）
在 macOS 上，键盘和鼠标追踪需要辅助功能权限，因为它安装了 CGEventTap——一个拦截输入事件的系统级钩子。Apple 要求任何读取全局输入的应用都具有此权限。仅在功能启用时请求该权限。如果您拒绝或撤销它，钩子会静默失败：应用其余部分继续正常运行，仅输入活动时间戳保持为零。活动窗口追踪（应用名称/路径）不需要辅助功能权限——它使用 AppleScript/osascript，在正常应用权限范围内运行。

# 摘要

## No cloud
无云端。所有 EEG 数据、嵌入、标签和设置本地存储在 {dataDir}/ 中。

## No telemetry
无遥测。没有分析、崩溃报告或任何形式的使用追踪。

## No accounts
无账户。无需注册、登录或用户标识符。

## One optional network request
一个可选的网络请求。仅在您明确触发时检查更新。

## Fully on-device
完全设备端。GPU 推理、信号处理和搜索全部在本地运行。

## Activity tracking is local-only
活动追踪仅限本地。窗口焦点和输入时间戳写入设备上的 activity.sqlite，永远不会离开设备。
