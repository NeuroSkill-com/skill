// SPDX-License-Identifier: GPL-3.0-only
/** JA "virtual-eeg" namespace. */
const virtualEeg: Record<string, string> = {
  "settingsTabs.virtualEeg": "仮想EEG",

  "veeg.title": "仮想EEGデバイス",
  "veeg.desc":
    "テスト、デモ、開発用にEEGヘッドセットをシミュレートします。完全な信号パイプラインを通じて合成データを生成します。",

  "veeg.status": "ステータス",
  "veeg.running": "実行中",
  "veeg.stopped": "停止",
  "veeg.start": "開始",
  "veeg.stop": "停止",

  "veeg.channels": "チャンネル",
  "veeg.channelsDesc": "シミュレートするEEG電極の数。",
  "veeg.sampleRate": "サンプルレート（Hz）",
  "veeg.sampleRateDesc": "チャンネルあたりの1秒間のサンプル数。",

  "veeg.template": "信号テンプレート",
  "veeg.templateDesc": "生成する合成信号の種類を選択します。",
  "veeg.templateSine": "正弦波",
  "veeg.templateSineDesc": "標準周波数帯（デルタ、シータ、アルファ、ベータ、ガンマ）のクリーンな正弦波。",
  "veeg.templateGoodQuality": "良好なEEG",
  "veeg.templateGoodQualityDesc": "アルファリズム優位でピンクノイズ背景のリアルな安静時EEG。",
  "veeg.templateBadQuality": "不良なEEG",
  "veeg.templateBadQualityDesc": "筋アーティファクト、50/60 Hzラインノイズ、電極ポップを含むノイジーな信号。",
  "veeg.templateInterruptions": "間欠的接続",
  "veeg.templateInterruptionsDesc":
    "電極のゆるみやワイヤレス干渉をシミュレートする周期的なドロップアウトを含む良好な信号。",
  "veeg.templateFile": "ファイルから",
  "veeg.templateFileDesc": "CSVまたはEDFファイルからサンプルを再生します。",

  "veeg.quality": "信号品質",
  "veeg.qualityDesc": "信号対雑音比を調整します。高いほどクリーンな信号です。",
  "veeg.qualityPoor": "不良",
  "veeg.qualityFair": "普通",
  "veeg.qualityGood": "良好",
  "veeg.qualityExcellent": "優秀",

  "veeg.chooseFile": "ファイルを選択",
  "veeg.noFile": "ファイルが選択されていません",
  "veeg.fileLoaded": "{name}（{channels}ch、{samples}サンプル）",

  "veeg.advanced": "詳細設定",
  "veeg.amplitudeUv": "振幅（µV）",
  "veeg.amplitudeDesc": "生成信号のピーク間振幅。",
  "veeg.noiseUv": "ノイズフロア（µV）",
  "veeg.noiseDesc": "加法ガウスノイズのRMS振幅。",
  "veeg.lineNoise": "ラインノイズ",
  "veeg.lineNoiseDesc": "50 Hzまたは60 Hzの商用電源干渉を追加します。",
  "veeg.lineNoise50": "50 Hz",
  "veeg.lineNoise60": "60 Hz",
  "veeg.lineNoiseNone": "なし",
  "veeg.dropoutProb": "ドロップアウト確率",
  "veeg.dropoutDesc": "1秒あたりの信号ドロップアウトの確率（0 = なし、1 = 常時）。",

  "veeg.preview": "信号プレビュー",
  "veeg.previewDesc": "最初の4チャンネルのライブプレビュー。",

  "window.title.virtualDevices": "{app} – 仮想デバイス",

  "vdev.title": "仮想デバイス",
  "vdev.desc":
    "物理的なEEGハードウェアなしでNeuroSkillをテストします。実際のデバイスに合わせたプリセットを選ぶか、独自の合成信号ソースを設定します。",

  "vdev.presets": "デバイスプリセット",
  "vdev.statusRunning": "仮想デバイスがストリーミング中",
  "vdev.statusStopped": "仮想デバイスは実行されていません",
  "vdev.selected": "準備完了",
  "vdev.configure": "設定",
  "vdev.customConfig": "カスタム設定",

  "vdev.presetMuse": "Muse S",
  "vdev.presetMuseDesc": "4チャンネルヘッドバンドレイアウト — TP9、AF7、AF8、TP10。",
  "vdev.presetCyton": "OpenBCI Cyton",
  "vdev.presetCytonDesc": "8チャンネル研究グレード信号、フル前頭/中央モンタージュ。",
  "vdev.presetCap32": "32チャンネルEEGキャップ",
  "vdev.presetCap32Desc": "フル10-20国際式システム、32電極。",
  "vdev.presetAlpha": "強いアルファ",
  "vdev.presetAlphaDesc": "顕著な10 Hzアルファリズム — リラックスした閉眼ベースライン。",
  "vdev.presetArtifact": "アーティファクトテスト",
  "vdev.presetArtifactDesc": "筋アーティファクトと50 Hzラインノイズを含むノイジーな信号。",
  "vdev.presetDropout": "ドロップアウトテスト",
  "vdev.presetDropoutDesc": "電極のゆるみをシミュレートする周期的な信号損失。",
  "vdev.presetMinimal": "最小（1ch）",
  "vdev.presetMinimalDesc": "1チャンネル正弦波 — 最も軽い負荷。",
  "vdev.presetCustom": "カスタム",
  "vdev.presetCustomDesc": "チャンネル数、レート、テンプレート、ノイズレベルを自由に定義。",

  "vdev.lslSourceTitle": "仮想LSLソース",
  "vdev.lslRunning": "LSL経由で合成EEGをストリーミング中",
  "vdev.lslStopped": "仮想LSLソースが停止中",
  "vdev.lslDesc": "LSLストリームの検出と接続をテストするためのローカルLab Streaming Layerソースを起動します。",
  "vdev.lslHint":
    'メイン設定→LSLタブで「ネットワークスキャン」をクリックし、ストリームリストにSkillVirtualEEGが表示されたら接続してください。',
  "vdev.lslStarted": "仮想LSLソースがローカルネットワークでストリーミングを開始しました。",

  "vdev.statusSource": "LSLソース",
  "vdev.statusSession": "セッション",
  "vdev.sessionConnected": "接続済み",
  "vdev.sessionConnecting": "接続中…",
  "vdev.sessionDisconnected": "切断済み",
  "vdev.startBtn": "仮想デバイスを開始",
  "vdev.stopBtn": "仮想デバイスを停止",
  "vdev.autoConnect": "ダッシュボードに自動接続",
  "vdev.autoConnectDesc": "開始直後にダッシュボードをこのソースに接続します。",

  "vdev.previewOffline": "信号プレビュー（オフライン）",
  "vdev.previewOfflineDesc":
    "クライアント側の波形プレビュー — 接続前の信号形状を表示します。データはまだストリーミングされていません。",

  "vdev.cfgChannels": "チャンネル",
  "vdev.cfgChannelsDesc": "シミュレートするEEG電極の数。",
  "vdev.cfgRate": "サンプルレート",
  "vdev.cfgRateDesc": "チャンネルあたりの1秒間のサンプル数。",

  "vdev.cfgQuality": "信号品質",
  "vdev.cfgQualityDesc": "信号対雑音比。高いほどクリーンな信号です。",

  "vdev.cfgTemplate": "信号テンプレート",
  "vdev.cfgTemplateSine": "正弦波",
  "vdev.cfgTemplateSineDesc": "デルタ、シータ、アルファ、ベータ、ガンマ周波数の純粋な正弦波。",
  "vdev.cfgTemplateGood": "良好なEEG",
  "vdev.cfgTemplateGoodDesc": "アルファ優位でピンクノイズ背景のリアルな安静状態。",
  "vdev.cfgTemplateBad": "不良なEEG",
  "vdev.cfgTemplateBadDesc": "筋アーティファクト、ラインノイズ、電極ポップを含むノイジーな信号。",
  "vdev.cfgTemplateInterruptions": "間欠的接続",
  "vdev.cfgTemplateInterruptionsDesc": "電極のゆるみをシミュレートする周期的ドロップアウトを含む良好な信号。",

  "vdev.cfgAdvanced": "詳細設定",
  "vdev.cfgAmplitude": "振幅（µV）",
  "vdev.cfgAmplitudeDesc": "シミュレート信号のピーク間振幅。",
  "vdev.cfgNoise": "ノイズフロア（µV）",
  "vdev.cfgNoiseDesc": "加法ガウスバックグラウンドノイズのRMS振幅。",
  "vdev.cfgLineNoise": "ラインノイズ",
  "vdev.cfgLineNoiseDesc": "50 Hzまたは60 Hzの商用電源干渉を注入します。",
  "vdev.cfgLineNoiseNone": "なし",
  "vdev.cfgLineNoise50": "50 Hz",
  "vdev.cfgLineNoise60": "60 Hz",
  "vdev.cfgDropout": "ドロップアウト確率",
  "vdev.cfgDropoutDesc": "1秒あたりの信号ドロップアウトの確率（0 = なし、1 = 常時）。",
};

export default virtualEeg;
