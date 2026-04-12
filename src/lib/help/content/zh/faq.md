## 我的数据存储在哪里？
所有内容本地存储在 {dataDir}/ 中——原始 CSV 录制、HNSW 向量索引、嵌入 SQLite 数据库、标签、日志和设置。没有任何内容发送到云端。

## ZUNA 编码器做什么？
ZUNA 是一个 GPU 加速的 Transformer 编码器，将 5 秒 EEG 时段转换为紧凑的嵌入向量。这些向量捕捉每个时刻的神经特征，并驱动相似性搜索功能。

## 为什么校准需要已连接的设备？
校准运行一个计时任务（如睁眼/闭眼）并记录带标签的 EEG 数据。没有实时流数据，校准就没有神经信号可以与每个阶段关联。

## 如何从 Python / Node.js 连接？
通过 mDNS 发现 WebSocket 端口（macOS 上运行 dns-sd -B _skill._tcp），然后打开标准 WebSocket 连接。发送 JSON 命令并接收实时事件流。有关传输格式详情，请参阅 API 选项卡。

## 信号质量指示器是什么意思？
每个点代表一个 EEG 电极。绿色 = 良好的皮肤接触，低噪声。黄色 = 有一些运动伪影或佩戴松动。红色 = 高噪声，接触不良。灰色 = 未检测到信号。

## 我可以更改陷波滤波器频率吗？
可以——进入设置 → 信号处理，选择 50 Hz（欧洲、大部分亚洲地区）或 60 Hz（美洲、日本）。这会从显示和频段功率计算中去除工频干扰。

## 如何重置已配对的设备？
打开设置 → 已配对设备，然后点击要忘记的设备旁边的 × 按钮。之后可以重新扫描该设备。

## 为什么托盘图标变红了？
您系统上的蓝牙已关闭。打开系统设置 → 蓝牙并启用它。{app} 将在约 1 秒内自动重新连接。

## 应用一直在旋转但从不连接——我该怎么办？
1. 确保设备已开机（Muse：长按直到感觉到振动；Ganglion/Cyton：检查蓝色 LED）。2. 保持在 5 米范围内。3. 如果仍然失败，请重启设备电源。

## 如何授予蓝牙权限？
macOS 会在 {app} 首次尝试连接时显示权限对话框。如果您之前关闭了它，请进入系统设置 → 隐私与安全性 → 蓝牙并启用 {app}。

## 数据库中存储了哪些指标？
每 2.5 秒时段存储：ZUNA 嵌入向量（32 维）、各通道平均的相对频段功率（delta、theta、alpha、beta、gamma、high-gamma）、以 JSON 形式存储的每通道频段功率、派生评分（放松度、参与度）、FAA、交叉频段比（TAR、BAR、DTR）、频谱形状（PSE、APF、BPS、SNR）、相干性、Mu 抑制、情绪指数，以及可用时的 PPG 平均值。

## 什么是会话比较？
比较（⌘⇧M）让您选择两个时间范围并排比较：带有差值的相对频段功率条、所有派生评分和比率、前额 Alpha 不对称性、睡眠分期图和 Brain Nebula™——一个 3D UMAP 嵌入投影。

## 什么是 Brain Nebula™？
Brain Nebula™（技术上称为：UMAP 嵌入分布）将高维 EEG 嵌入投影到 3D 空间中，使相似的脑状态显示为邻近的点。当会话不同时，范围 A（蓝色）和范围 B（琥珀色）形成不同的聚类。您可以旋转、缩放，并点击标注点以追踪时间连接。可以同时以不同颜色高亮多个标签。

## 为什么 Brain Nebula™ 一开始显示随机云团？
UMAP 投影计算量大，在后台任务队列中运行以保持界面响应。计算期间显示随机占位符云团。投影准备就绪后，点会平滑动画到最终位置。

## 什么是标签，如何使用？
Labels are user-defined tags (e.g. 'meditation', 'reading') attached to a moment during recording. They're stored alongside EEG embeddings. In the UMAP viewer, labelled points appear larger with coloured rings — click one to trace that label through time across both sessions.

## 什么是前额 Alpha 不对称性（FAA）？
FAA 是 ln(AF8 α) − ln(AF7 α)。正值表示趋近动机（参与、好奇）。负值表示回避（逃避、焦虑）。

## 睡眠分期如何工作？
每个 EEG 时段根据相对 delta、theta、alpha 和 beta 功率被分类为清醒、N1、N2、N3 或 REM。比较视图显示每个会话的睡眠图，包含阶段分解和时间百分比。

## 键盘快捷键有哪些？
⌘⇧O——打开 {app} 窗口。⌘⇧M——打开会话比较。在设置 → 快捷键中自定义快捷键。

## 什么是 WebSocket API？
{app} exposes a JSON WebSocket API on the local network (mDNS: _skill._tcp). Commands: status, label, search, compare (metrics + sleep + UMAP ticket), sessions, sleep, umap (enqueue 3D projection), umap_poll (retrieve result). Run 'node test.js' to smoke-test.

## 什么是放松度和参与度评分？
放松度 = α/(β+θ)，衡量平静的清醒状态。参与度 = β/(α+θ)，衡量持续的心理投入。两者通过 sigmoid 映射到 0–100。

## 什么是 TAR、BAR 和 DTR？
TAR（Theta/Alpha）——越高 = 越困倦或更冥想。BAR（Beta/Alpha）——越高 = 越紧张或专注。DTR（Delta/Theta）——越高 = 更深睡眠或放松。所有值为各通道平均。

## 什么是 PSE、APF、BPS 和 SNR？
PSE（功率谱熵，0–1）——频谱复杂度。APF（Alpha 峰值频率，Hz）——最大 Alpha 功率频率。BPS（频段功率斜率）——1/f 非周期性指数。SNR（信噪比，dB）——宽带功率与线路噪声的比较。

## 什么是 Theta/Beta 比率（TBR）？
TBR 是绝对 theta 与绝对 beta 功率的比率。较高的值表示皮层唤醒降低——升高的 TBR 与嗜睡和注意力失调有关。参考：Angelidis 等（2016）。

## 什么是 Hjorth 参数？
来自 Hjorth（1970）的三个时域特征：Activity（信号方差/总功率）、Mobility（平均频率估计）和 Complexity（带宽/与纯正弦波的偏差）。它们计算开销低，广泛用于 EEG 机器学习管线。

## 计算了哪些非线性复杂度指标？
四种指标：排列熵（序列模式复杂度，Bandt & Pompe 2002）、Higuchi 分形维数（信号分形结构，Higuchi 1988）、DFA 指数（长程时间相关性，Peng 等 1994）和样本熵（信号规律性，Richman & Moorman 2000）。所有指标为 4 个 EEG 通道的平均值。

## 什么是 SEF95、频谱重心、PAC 和偏侧化指数？
SEF95（频谱边缘频率）是总功率 95% 以下的频率——用于麻醉监测。频谱重心是功率加权平均频率（唤醒指标）。PAC（相位-幅度耦合）衡量与记忆编码相关的 theta-gamma 交叉频率交互。偏侧化指数是所有频段的广义左/右功率不对称性。

## 计算了哪些 PPG 指标？
在 Muse 2/S（带 PPG 传感器）上：心率（bpm，来自红外峰值检测）、RMSSD/SDNN/pNN50（心率变异性——副交感神经张力）、LF/HF 比率（交感迷走平衡）、呼吸频率（次/分钟，来自 PPG 包络）、SpO₂ 估计（未校准的血氧，来自红色/红外比）、灌注指数（外周血流）和 Baevsky 压力指数（自主神经压力）。当连接配备 PPG 的头带时，这些出现在 PPG 生命体征部分。

## 如何使用专注计时器？
通过托盘菜单、命令面板（⌘K → "专注计时器"）或全局快捷键（默认 ⌘⇧P）打开专注计时器。选择预设——番茄钟（25/5）、深度工作（50/10）或短暂专注（15/5）——或设置自定义时长。启用"自动标注 EEG"可让 NeuroSkill™ 在每个专注阶段开始和结束时自动标记 EEG 录制。会话点追踪您完成的轮次。您的预设和自定义设置会自动保存，下次打开计时器时恢复。

## 如何管理或编辑我的标注？
Open the Labels window via the Command Palette (⌘K → "All Labels"). It shows all annotations with inline text editing (click a label, press ⌘↵ to save or Esc to cancel), delete (with confirmation), and metadata showing the EEG time-range. Use the search box to filter by text. Labels are paginated at 50 per page for large archives.

## 如何并排比较两个特定会话？
From the History page, click "Quick Compare" to enter compare mode. Checkboxes appear on each session row — select exactly two, then click "Compare Selected" to open the Compare window pre-loaded with both sessions. Alternatively open Compare from the tray or Command Palette and use the session dropdowns manually.

## 文本嵌入搜索如何工作？
Your query is converted to a vector by the same sentence-transformer model that indexes your labels. That vector is then searched against the HNSW label index using approximate nearest-neighbour lookup. Results are your own annotations ranked by semantic similarity — so searching "calm and focused" will surface labels like "deep reading" or "meditation" even if those exact words never appeared in your query. Requires the embedding model to be downloaded and the label index to be built (Settings → Embeddings).

## 交互式跨模态搜索如何工作？
Interactive search bridges text, EEG, and time in a single query. Step 1: your text query is embedded. Step 2: the top text-k semantically similar labels are found. Step 3: for each label, {app} computes the mean EEG embedding over its recording window and retrieves the top eeg-k nearest EEG epochs from all daily indices — crossing from language into brain-state space. Step 4: for each EEG moment found, any annotations within ±reach minutes are collected as "found labels". The four node layers (Query → Text Matches → EEG Neighbors → Found Labels) are rendered as a 4-layer directed graph. Export as SVG for a static image or as DOT source for further processing in Graphviz.

## 如何从脚本或自动化工具触发 TTS 语音？
使用 WebSocket 或 HTTP API。WebSocket：发送 {"command":"say","text":"your message"}。HTTP（curl）：curl -X POST http://localhost:<port>/say -H 'Content-Type: application/json' -d '{"text":"your message"}'。say 命令是即发即忘——立即响应，音频在后台播放。

## 为什么 TTS 没有声音？
检查 espeak-ng 是否已安装在 PATH 中（macOS 上 brew install espeak-ng，Ubuntu 上 apt install espeak-ng）。检查系统音频输出是否静音或路由到其他设备。首次运行时，模型（约 30 MB）必须完成下载才能发出声音。在设置 → 语音中启用 TTS 调试日志以查看日志文件中的合成事件。

## 我可以更改 TTS 声音或语言吗？
当前版本使用 KittenML/kitten-tts-mini-0.8 模型中的 Jasper 英语（en-us）声音。仅英语文本能正确进行音素化。未来版本计划支持更多声音和语言。

## TTS 需要互联网连接吗？
仅需一次，用于从 HuggingFace Hub 初始下载约 30 MB 模型。之后，所有合成完全离线运行。模型缓存在 ~/.cache/huggingface/hub/ 中，每次后续启动时重复使用。

## NeuroSkill™ 支持哪些 OpenBCI 开发板？
NeuroSkill™ 通过已发布的 openbci crate（crates.io/crates/openbci）支持 OpenBCI 生态系统中的所有开发板：Ganglion（4 通道，BLE）、Ganglion + WiFi Shield（4 通道，1 kHz）、Cyton（8 通道，USB 加密狗）、Cyton + WiFi Shield（8 通道，1 kHz）、Cyton+Daisy（16 通道，USB 加密狗）、Cyton+Daisy + WiFi Shield（16 通道，1 kHz）和 Galea（24 通道，UDP）。任何开发板均可与其他 BCI 设备同时使用。在设置 → OpenBCI 中选择开发板，然后点击连接。

## 如何通过蓝牙连接 Ganglion？
1. 开启 Ganglion 电源——蓝色 LED 应缓慢闪烁。2. 在设置 → OpenBCI 中选择 "Ganglion — 4ch · BLE"。3. 保存设置，然后点击连接。NeuroSkill™ 会在配置的超时时间内扫描（默认 10 秒）。将开发板保持在 3–5 米范围内。在 macOS 上，提示时授予蓝牙权限（或前往系统设置 → 隐私与安全性 → 蓝牙）。

## 我的 Ganglion 已开机但 NeuroSkill™ 找不到它——我该怎么办？
1. 确认蓝色 LED 正在闪烁（常亮或熄灭表示它未在广播——按按钮唤醒）。2. 在设置 → OpenBCI 中增加 BLE 扫描超时时间。3. 将开发板移至 2 米范围内。4. 退出 NeuroSkill™ 并重新打开以重置 BLE 适配器。5. 在系统设置中关闭蓝牙再重新打开。6. 确保没有其他应用（OpenBCI GUI、另一个 NeuroSkill™ 实例）已经连接——BLE 每次只允许一个主机。7. 在 macOS 14+ 上，检查 NeuroSkill™ 在系统设置 → 隐私与安全性 → 蓝牙中是否有蓝牙权限。

## 如何通过 USB 连接 Cyton？
1. 将 USB 无线电加密狗插入计算机（加密狗是无线电——Cyton 开发板本身没有 USB 端口）。2. 开启 Cyton 电源——将电源开关滑到 PC。3. 在设置 → OpenBCI 中选择 "Cyton — 8ch · USB serial"。4. 点击刷新列出串口，然后选择端口（macOS 上为 /dev/cu.usbserial-…，Linux 上为 /dev/ttyUSB0，Windows 上为 COM3）或留空自动检测。5. 保存设置并点击连接。

## 串口未列出或出现权限被拒绝错误——如何解决？
macOS：加密狗显示为 /dev/cu.usbserial-*。如果不存在，请从芯片制造商网站安装 CP210x 或 FTDI VCP 驱动程序。Linux：运行 sudo usermod -aG dialout $USER，然后注销再登录。插入后验证设备出现在 /dev/ttyUSB0 或 /dev/ttyACM0。Windows：安装 CP2104 USB 转 UART 驱动程序；COM 端口会出现在设备管理器 → 端口（COM 和 LPT）中。

## 如何通过 OpenBCI WiFi Shield 连接？
1. 将 WiFi Shield 堆叠在 Cyton 或 Ganglion 顶部并为开发板通电。2. 在计算机上连接到 Shield 广播的 WiFi 网络（SSID：OpenBCI-XXXX，通常无密码）。3. 在设置 → OpenBCI 中选择匹配的 WiFi 开发板变体。4. 输入 IP 192.168.4.1（Shield 默认值）或留空自动发现。5. 点击连接。WiFi Shield 以 1000 Hz 流式传输——在信号处理中将低通滤波器设为 ≤ 500 Hz 以避免混叠。

## 什么是 Galea 开发板，如何设置？
OpenBCI 的 Galea 是一款 24 通道研究级生物信号头戴设备，结合 EEG、EMG 和 AUX 传感器，通过 UDP 流式传输。连接方法：1. 开启 Galea 电源并将其连接到本地网络。2. 在设置 → OpenBCI 中选择 "Galea — 24ch · UDP"。3. 输入 Galea IP 地址（或留空接受来自任何发送者的数据）。4. 点击连接。通道 1–8 为 EEG（驱动实时分析）；9–16 为 EMG；17–24 为 AUX。所有 24 个通道保存至 CSV。

## 我可以同时使用两个 BCI 设备吗？
可以——NeuroSkill™ 可以同时从两者流式传输。先连接的设备驱动实时仪表盘、频段功率显示和 ZUNA 嵌入管线。第二个设备的数据记录到 CSV 以供离线分析。实时管线中的同时多设备分析计划在未来版本中推出。

## 为什么 Cyton 的 8 个通道中只有 4 个用于实时分析？
实时分析管线（滤波器、频段功率、ZUNA 嵌入、信号质量点）目前设计为 4 通道输入，以匹配 Muse 头戴设备格式。对于 Cyton（8 通道）和 Cyton+Daisy（16 通道），通道 1–4 供实时管线使用；所有通道写入 CSV 供离线使用。完整的多通道管线支持在路线图中。

## 如何改善 OpenBCI 开发板的信号质量？
1. 在每个电极位点涂抹导电凝胶或膏体，拨开头发使其直接接触头皮。2. 录制前使用 OpenBCI GUI 阻抗检查验证阻抗——目标 < 20 kΩ。3. 将 SRB 偏置电极连接到乳突（耳后）以获得稳固的参考。4. 保持电极线缆短且远离电源。5. 在设置 → 信号处理中使用陷波滤波器（欧洲 50 Hz，美洲 60 Hz）。6. 对于 Ganglion BLE：将开发板远离 USB 3.0 端口，因其发射 2.4 GHz 干扰。

## 我的 OpenBCI 连接反复中断——如何稳定它？
Ganglion BLE：将开发板保持在 2 米范围内；将主机的 BLE 适配器插入 USB 2.0 端口（USB 3.0 发射 2.4 GHz 噪声可能干扰 BLE）。Cyton USB：使用短的高质量 USB 线缆，直接连接到计算机而非通过集线器。WiFi Shield：确保 Shield 的 2.4 GHz 频道不与路由器重叠；将开发板移近。所有开发板：录制期间避免运行其他无线密集型应用（视频通话、文件同步）。

## 活动追踪到底记录了什么？
活动窗口追踪在每次前台应用或窗口标题变化时向 activity.sqlite 写入一行。每行包含：应用显示名称（如 "Safari"、"VS Code"）、二进制文件或应用包的完整路径、窗口标题（如文档名称或网页标题——沙盒应用可能为空），以及其变为活动状态的 Unix 秒时间戳。键盘和鼠标追踪每 60 秒写入一次周期性样本，但仅在自上次刷新以来有活动时才写入。每个样本存储两个 Unix 秒时间戳——上次键盘事件和上次鼠标/触控板事件。它不记录您按了哪些键、输入了什么文本、光标在哪里或点击了哪些按钮。两项功能默认启用，可在设置 → 活动追踪中独立关闭。

## 为什么 macOS 要求辅助功能权限来进行输入追踪？
Keyboard and mouse tracking uses a CGEventTap — a macOS API that intercepts system-wide input events before they reach individual apps. Apple requires the Accessibility permission for any application that reads global input, regardless of what that app does with it. Without Accessibility access the tap fails silently: NeuroSkill continues to work normally, but last-keyboard and last-mouse timestamps stay at zero. To grant access: System Settings → Privacy & Security → Accessibility → find NeuroSkill → toggle on. If you prefer not to grant it, disable the "Track Keyboard & Mouse Activity" toggle in Settings — this prevents the hook from being installed in the first place. Active-window tracking (app name and path) uses AppleScript/osascript and does not require Accessibility permission.

## 如何清除或删除活动追踪数据？
所有活动追踪数据存储在单个文件中：~/.skill/activity.sqlite。要删除所有内容：退出 NeuroSkill，删除该文件，然后重新启动——下次启动时会自动创建空数据库。要停止未来的收集而不触及现有数据，请关闭设置 → 活动追踪中的两个开关；更改立即生效，无需重启。要选择性地删除行，可以在任何 SQLite 浏览器（如 DB Browser for SQLite）中打开文件并从 active_windows 或 input_activity 中 DELETE。

## 为什么 {app} 要求 macOS 上的辅助功能权限？
{app} 使用 macOS CGEventTap API 记录上次按键或鼠标移动的时间。这用于计算活动追踪面板中显示的键盘和鼠标活动时间戳。仅存储时间戳——不记录按键、不记录光标位置。如果未授予权限，该功能会静默降级。

## {app} 需要蓝牙权限吗？
需要。{app} 使用蓝牙低功耗（BLE）连接您的 BCI 头戴设备。在 macOS 上，应用首次尝试扫描时系统会显示一次性蓝牙权限提示。在 Linux 和 Windows 上不需要显式蓝牙权限。

## 如何在 macOS 上授予辅助功能权限？
Open System Settings → Privacy & Security → Accessibility. Find {app} in the list and toggle it on. You can also click "Open Accessibility Settings" in the Permissions tab inside the app.

## 如果我拒绝辅助功能权限会怎样？
键盘和鼠标活动时间戳将不会被记录，并保持为零。所有其他功能——EEG 流式传输、频段功率、校准、TTS、搜索——继续正常工作。您可以在设置 → 活动追踪中完全禁用该功能。

## 我可以在授予权限后撤销吗？
可以。打开系统设置 → 隐私与安全性 → 辅助功能（或通知）并关闭 {app}。相关功能将立即停止工作，无需重启。
