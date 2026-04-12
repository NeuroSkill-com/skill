## 钩子如何触发？
工作线程将每个新的 EEG 嵌入与通过关键词 + 文本相似度选择的最近标签样本进行比较。如果最佳余弦距离低于您的阈值，钩子就会触发。

## 为什么托盘图标变红了？
您 Mac 上的蓝牙已关闭。打开系统设置 → 蓝牙并启用它。{app} 将在约 1 秒内自动重新连接。

## 应用一直在旋转但从不连接——我该怎么办？
1. 确保 BCI 设备已开机（Muse：长按直到感觉到振动；Ganglion/Cyton：检查蓝色 LED）。2. 保持在 5 米范围内。3. 如果仍然失败，请重启设备电源。

## 如何授予蓝牙权限？
macOS 会在 {app} 首次尝试连接时显示权限对话框。如果您之前关闭了它，请进入系统设置 → 隐私与安全性 → 蓝牙并启用 {app}。

## 我可以在同一网络上的其他应用中接收 EEG 数据吗？
可以。将 WebSocket 客户端连接到 Bonjour 发现输出中显示的地址（参见上方本地网络流式传输部分）。您将收到派生指标（约 4 Hz eeg-bands 事件，包含 60 多项评分）和设备状态（约 1 Hz）。注意：原始 EEG/PPG/IMU 样本流不通过 WebSocket API 提供——仅提供处理后的评分和频段功率。

## 我的 EEG 录制保存在哪里？
原始（未过滤）样本写入应用数据文件夹中的 CSV 文件（macOS/Linux 上为 {dataDir}/）。每个会话创建一个文件。

## 信号质量点是什么意思？
每个点代表一个 EEG 通道（TP9、AF7、AF8、TP10）。绿色 = 良好（低噪声，良好皮肤接触）。黄色 = 一般（有一些运动伪影或电极松动）。红色 = 差（高噪声，接触非常松或电极脱离皮肤）。灰色 = 无信号。

## 工频陷波滤波器有什么用？
市电在 EEG 录制中引入 50 或 60 Hz 噪声。陷波滤波器从波形显示中去除该频率（及其谐波）。选择 60 Hz（美国/日本）或 50 Hz（欧盟/英国）以匹配当地电网。

## 数据库中存储了哪些指标？
每 2.5 秒时段存储：ZUNA 嵌入向量（32 维）、各通道平均的相对频段功率（delta、theta、alpha、beta、gamma、high-gamma）、以 JSON 形式存储的每通道频段功率、派生评分（放松度、参与度）、前额 Alpha 不对称性（FAA）、交叉频段比（TAR、BAR、DTR、TBR）、频谱形状（PSE、APF、SEF95、频谱重心、BPS、SNR）、相干性、Mu 抑制、情绪综合指数、Hjorth 参数（activity、mobility、complexity）、非线性复杂度（排列熵、Higuchi FD、DFA、样本熵）、PAC（θ–γ）、偏侧化指数、PPG 平均值，以及连接 Muse 2/S 时的 PPG 派生指标（HR、RMSSD、SDNN、pNN50、LF/HF、呼吸频率、SpO₂、灌注指数、压力指数）。

## 什么是会话比较功能？
会话比较（⌘⇧M）让您选择任意两个录制会话并排比较。显示内容包括：带差值的相对频段功率条、所有派生评分和比率、前额 Alpha 不对称性、睡眠分期图和 3D UMAP 嵌入投影，可视化两个会话在高维特征空间中的相似程度。

## 什么是 3D UMAP 查看器？
UMAP 查看器将高维 EEG 嵌入投影到 3D 空间中，使相似的脑状态显示为邻近的点。如果会话不同，会话 A（蓝色）和会话 B（琥珀色）形成不同的聚类。您可以旋转、缩放，并点击标注点查看其时间连接。

## 为什么 UMAP 查看器一开始显示随机云团？
UMAP 计算量大——它在后台任务队列中运行以保持界面响应。计算期间显示随机高斯占位符云团。真正的投影准备就绪后，点会平滑动画到最终位置。

## 什么是标签，如何使用？
Labels are user-defined tags (e.g. 'meditation', 'reading', 'anxious') that you attach to a moment in time during a recording. They're stored alongside the EEG embeddings in the database. In the UMAP viewer, labelled points appear as larger dots with coloured rings.

## 什么是前额 Alpha 不对称性（FAA）？
FAA 是 ln(AF8 α) − ln(AF7 α)。正值表示左半球 Alpha 抑制更大，与趋近动机（参与、好奇）相关。负值表示回避（逃避、焦虑）。

## 睡眠分期如何工作？
{app} 根据相对 delta、theta、alpha 和 beta 功率比将每个 EEG 时段分类为清醒、N1（浅睡）、N2、N3（深睡）或 REM 睡眠。比较视图显示每个会话的睡眠图，包含颜色编码的阶段分解和时间百分比。

## 键盘快捷键有哪些？
⌘⇧O——打开 {app} 窗口。⌘⇧M——打开会话比较。您可以在设置 → 快捷键中自定义快捷键。

## 什么是 WebSocket API？
{app} 在本地网络上公开基于 JSON 的 WebSocket API（mDNS：_skill._tcp）。命令包括：status、label、search、compare、sessions、sleep、umap 和 umap_poll。从项目目录运行 'node test.js' 以冒烟测试所有命令。

## 什么是派生评分（放松度、参与度）？
放松度 = α / (β + θ)，衡量平静的清醒状态。参与度 = β / (α + θ)，衡量持续的心理投入。两者映射到 0–100 的范围。

## 什么是交叉频段比？
TAR（Theta/Alpha）——较高值表示困倦或冥想状态。BAR（Beta/Alpha）——较高值表示紧张或专注。DTR（Delta/Theta）——较高值表示深睡或深度放松。所有值为各通道平均。

## 什么是 PSE、APF、BPS 和 SNR？
PSE（功率谱熵，0–1）衡量频谱复杂度。APF（Alpha 峰值频率，Hz）是最大 Alpha 功率的频率。BPS（频段功率斜率）是 1/f 非周期性指数。SNR（信噪比，dB）比较宽带功率与 50–60 Hz 线路噪声。
