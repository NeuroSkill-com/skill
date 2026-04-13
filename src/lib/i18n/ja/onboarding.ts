// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** JA "onboarding" namespace. */
const onboarding: Record<string, string> = {
  "onboarding.title": "{app} へようこそ",
  "onboarding.step.welcome": "ようこそ",
  "onboarding.step.bluetooth": "Bluetooth",
  "onboarding.step.fit": "フィットチェック",
  "onboarding.step.calibration": "キャリブレーション",
  "onboarding.step.models": "モデル",
  "onboarding.step.tray": "トレイ",
  "onboarding.step.enable_bluetooth": "Bluetoothを有効化",
  "onboarding.step.done": "完了",
  "onboarding.welcomeTitle": "{app} へようこそ",
  "onboarding.welcomeBody":
    "{app}は対応するBCIデバイスからEEGデータを記録・分析・インデックス化します。いくつかの簡単なステップでセットアップしましょう。",
  "onboarding.bluetoothHint": "BCIデバイスを接続",
  "onboarding.fitHint": "センサー接触品質を確認",
  "onboarding.calibrationHint": "クイックキャリブレーションセッションを実行",
  "onboarding.modelsHint": "推奨ローカルAIモデルをダウンロード",
  "onboarding.bluetoothTitle": "BCIデバイスを接続",
  "onboarding.bluetoothBody":
    "BCIデバイスの電源を入れて装着してください。{app}が近くのデバイスをスキャンして自動的に接続します。",
  "onboarding.enableBluetoothTitle": "MacのBluetoothを有効にする",
  "onboarding.enableBluetoothBody":
    "{app}はBCIデバイスの検出と接続にMacのBluetoothアダプターが必要です。Bluetoothがオフの場合はシステム設定で有効にしてください。",
  "onboarding.enableBluetoothStatus": "Bluetoothアダプター",
  "onboarding.enableBluetoothHint":
    "Bluetooth設定を開いてBluetoothをオンにしてください。ターミナルで開発中の場合は、システムアダプターが有効であることを確認してください。",
  "onboarding.enableBluetoothOpen": "Bluetooth設定を開く",
  "onboarding.btConnected": "{name} に接続しました",
  "onboarding.btScanning": "スキャン中…",
  "onboarding.btReady": "スキャン準備完了",
  "onboarding.btScan": "スキャン",
  "onboarding.btInstructions": "接続方法",
  "onboarding.btStep1":
    "BCIデバイスの電源を入れます（ヘッドセットに応じて電源ボタンを長押し、スイッチを切り替え、またはボタンを押してください）。",
  "onboarding.btStep2": "ヘッドセットを装着します — センサーが耳の後ろと額に当たるようにしてください。",
  "onboarding.btStep3": "上のスキャンをクリックしてください。{app}が最も近いBCIデバイスを見つけて自動接続します。",
  "onboarding.btSuccess": "ヘッドセットが接続されました！次に進めます。",
  "onboarding.fitTitle": "ヘッドセットのフィットを確認",
  "onboarding.fitBody":
    "クリーンなEEGデータにはセンサーの良好な接触が不可欠です。4つのセンサーすべてが緑または黄色を示す必要があります。",
  "onboarding.sensorQuality": "ライブセンサー品質",
  "onboarding.quality.good": "良好",
  "onboarding.quality.fair": "普通",
  "onboarding.quality.poor": "不良",
  "onboarding.quality.no_signal": "信号なし",
  "onboarding.fitNeedsBt": "ライブセンサーデータを表示するには、まずヘッドセットを接続してください。",
  "onboarding.fitTips": "接触改善のヒント",
  "onboarding.fitTip1": "耳センサー（TP9/TP10）：耳の後ろ少し上に配置します。センサーを覆う髪をよけてください。",
  "onboarding.fitTip2": "額センサー（AF7/AF8）：清潔な肌に平らに当ててください — 必要に応じて乾いた布で拭きます。",
  "onboarding.fitTip3": "接触が不良な場合、センサーを湿った指で軽く湿らせてください。導電性が向上します。",
  "onboarding.fitGood": "フィット良好！すべてのセンサーの接触が良好です。",
  "onboarding.calibrationTitle": "キャリブレーションの実行",
  "onboarding.calibrationBody":
    "キャリブレーションは2つの精神状態を交互に行いながらラベル付きEEGを記録します。これにより{app}があなたの脳のベースラインパターンを学習します。",
  "onboarding.openCalibration": "キャリブレーションを開く",
  "onboarding.calibrationNeedsBt": "キャリブレーションを実行するには、まずヘッドセットを接続してください。",
  "onboarding.calibrationSkip": "スキップしてトレイメニューや設定から後でキャリブレーションできます。",
  "onboarding.modelsTitle": "推奨モデルをダウンロード",
  "onboarding.modelsBody":
    "最適なローカル体験のために、Qwen3.5 4B (Q4_K_M)、ZUNAエンコーダー、NeuTTS、Kitten TTSをダウンロードしてください。",
  "onboarding.models.downloadAll": "推奨セットをダウンロード",
  "onboarding.models.download": "ダウンロード",
  "onboarding.models.downloading": "ダウンロード中…",
  "onboarding.models.downloaded": "ダウンロード済み",
  "onboarding.models.qwenTitle": "Qwen3.5 4B (Q4_K_M)",
  "onboarding.models.qwenDesc": "推奨チャットモデル。ほとんどのノートPCで最適な品質/速度バランスのQ4_K_Mを使用します。",
  "onboarding.models.zunaTitle": "ZUNA EEGエンコーダー",
  "onboarding.models.zunaDesc": "EEG埋め込み、セマンティック履歴、脳状態分析に必要です。",
  "onboarding.models.neuttsTitle": "NeuTTS (Nano Q4)",
  "onboarding.models.neuttsDesc": "より高品質でクローニング対応の推奨多言語音声エンジン。",
  "onboarding.models.kittenTitle": "Kitten TTS",
  "onboarding.models.kittenDesc":
    "軽量で高速な音声バックエンド。クイックフォールバックや低リソースシステムに便利です。",
  "onboarding.models.ocrTitle": "OCRモデル",
  "onboarding.models.ocrDesc":
    "スクリーンショットからテキストを抽出するためのテキスト検出+認識モデル。キャプチャ画面のテキスト検索を可能にします（各約10MB）。",
  "onboarding.screenRecTitle": "画面収録の権限",
  "onboarding.screenRecDesc":
    "macOSでスクリーンショットシステムのために他のアプリケーションウィンドウをキャプチャするのに必要です。これがないとスクリーンショットが空白になる場合があります。",
  "onboarding.screenRecOpen": "設定を開く",
  "onboarding.trayTitle": "トレイでアプリを見つける",
  "onboarding.trayBody":
    "{app}はバックグラウンドで静かに動作します。セットアップ後は、メニューバー（macOS）またはシステムトレイ（Windows/Linux）のアイコンからアプリに戻れます。",
  "onboarding.tray.states": "アイコンの色でステータスを表示します：",
  "onboarding.tray.grey": "グレー — 切断中",
  "onboarding.tray.amber": "黄色 — スキャンまたは接続中",
  "onboarding.tray.green": "緑 — 接続して記録中",
  "onboarding.tray.red": "赤 — Bluetoothがオフ",
  "onboarding.tray.open": "トレイアイコンをクリックすると、いつでもメインダッシュボードを表示/非表示にできます。",
  "onboarding.tray.menu":
    "アイコンを右クリック（Windows/Linuxでは左クリック）でクイックアクション — 接続、ラベル、キャリブレーションなど。",
  "onboarding.downloadsComplete": "すべてのダウンロードが完了しました！",
  "onboarding.downloadsCompleteBody":
    "推奨モデルがダウンロードされ、使用準備ができました。さらにモデルをダウンロードしたり別のモデルに切り替えるには、",
  "onboarding.downloadMoreSettings": "アプリ設定",
  "onboarding.doneTitle": "準備完了！",
  "onboarding.doneBody": "{app}はメニューバーで動作しています。いくつかの注意点：",
  "onboarding.doneTip.tray":
    "{app}はメニューバートレイにあります。アイコンをクリックでダッシュボードを表示/非表示にできます。",
  "onboarding.doneTip.shortcuts": "⌘Kでコマンドパレットを開くか、?ですべてのキーボードショートカットを確認できます。",
  "onboarding.doneTip.help": "トレイメニューからヘルプを開くと、すべての機能の完全なリファレンスが表示されます。",
  "onboarding.back": "戻る",
  "onboarding.next": "次へ",
  "onboarding.getStarted": "始める",
  "onboarding.finish": "完了",
};

export default onboarding;
