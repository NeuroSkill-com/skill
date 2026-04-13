// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** JA "perm" namespace. */
const perm: Record<string, string> = {
  "perm.intro":
    "{app}はキーボード/マウスの活動タイムスタンプや通知などの機能を有効にするために、少数のオプションOS権限を使用します。すべてのデータはデバイス上に保存されます。",
  "perm.granted": "許可済み",
  "perm.denied": "未許可",
  "perm.unknown": "不明",
  "perm.notRequired": "不要",
  "perm.systemManaged": "OSが管理",
  "perm.accessibility": "アクセシビリティ",
  "perm.accessibilityDesc":
    "キーボードとマウスの活動トラッキングは、macOSのCGEventTapを使用して最後のキー押下とマウスイベントのタイムスタンプを記録します。キーストロークやカーソル位置は保存されません — Unixの秒単位のタイムスタンプのみです。macOSではアクセシビリティ権限が必要です。",
  "perm.accessibilityOk": "権限が許可されました。キーボードとマウスの活動タイムスタンプが記録されています。",
  "perm.accessibilityPending": "権限ステータスを確認中…",
  "perm.howToGrant": "この権限を許可する方法：",
  "perm.accessStep1": "下の「アクセシビリティ設定を開く」をクリックしてください。",
  "perm.accessStep2": "リストで{app}を見つけます（または+ボタンをクリックして追加）。",
  "perm.accessStep3": "オンに切り替えます。",
  "perm.accessStep4": "ここに戻ってください — ステータスは自動的に更新されます。",
  "perm.openAccessibilitySettings": "アクセシビリティ設定を開く",
  "perm.bluetooth": "Bluetooth",
  "perm.bluetoothDesc":
    "BluetoothはBCIヘッドセット（Muse、MW75 Neuro、OpenBCI Ganglion、IDUN Guardianなど）に接続するために使用されます。macOSでは、アプリが初めてスキャンする際にシステムがBluetooth アクセスを求めます。LinuxとWindowsでは個別の権限は不要です。",
  "perm.openBluetoothSettings": "Bluetooth設定を開く",
  "perm.notifications": "通知",
  "perm.notificationsDesc":
    "通知はデイリー記録目標の達成時やソフトウェアアップデートが利用可能な時にお知らせするために使用されます。macOSとWindowsでは、最初の通知送信時にOSが権限を求めます。",
  "perm.openNotificationsSettings": "通知設定を開く",
  "perm.matrix": "権限サマリー",
  "perm.feature": "機能",
  "perm.matrixBluetooth": "Bluetooth（BCIデバイス）",
  "perm.matrixKeyboardMouse": "キーボード＆マウスタイムスタンプ",
  "perm.matrixActiveWindow": "アクティブウィンドウトラッキング",
  "perm.matrixNotifications": "通知",
  "perm.matrixNone": "権限不要",
  "perm.matrixAccessibility": "アクセシビリティが必要",
  "perm.matrixOsPrompt": "初回使用時にOSが確認",
  "perm.legendNone": "権限不要",
  "perm.legendRequired": "OS権限が必要 — 不在時は無音で機能低下",
  "perm.legendPrompt": "初回使用時にOSが確認",
  "perm.why": "{app}にこれらの権限が必要な理由",
  "perm.whyBluetooth": "Bluetooth",
  "perm.whyBluetoothDesc": "BLE経由でBCIヘッドセットを検出しデータをストリーミングするため。",
  "perm.whyAccessibility": "アクセシビリティ",
  "perm.whyAccessibilityDesc":
    "活動コンテキスト用にキーボードとマウスイベントにタイムスタンプを付けるため。イベントの時刻のみが保存されます — 入力内容やカーソル位置は保存されません。",
  "perm.whyNotifications": "通知",
  "perm.whyNotificationsDesc": "デイリー記録目標の達成時とアップデートの準備完了時に通知するため。",
  "perm.privacyNote":
    "すべてのデータはデバイス上にローカルに保存され、サーバーに送信されることはありません。設定→活動トラッキングで各機能を無効にできます。",
  "perm.screenRecording": "画面収録",
  "perm.screenRecordingDesc":
    "スクリーンショット埋め込みシステムのために他のアプリケーションウィンドウをキャプチャするのに必要です。macOSではこの権限がないとウィンドウ内容が墨消しされます。",
  "perm.screenRecordingOk": "画面収録の権限が許可されています。スクリーンショットキャプチャは正常に動作します。",
  "perm.screenRecordingStep1": "システム設定→プライバシーとセキュリティ→画面収録と音声録音を開く",
  "perm.screenRecordingStep2": "リストでNeuroSkill™を見つけて有効にする",
  "perm.screenRecordingStep3": "変更を適用するにはアプリを終了して再起動する必要がある場合があります",
  "perm.openScreenRecordingSettings": "画面収録設定を開く",
  "perm.whyScreenRecording": "画面収録",
  "perm.whyScreenRecordingDesc":
    "視覚類似検索とクロスモーダルEEG相関のためにアクティブウィンドウをキャプチャします。オプトインのスクリーンショットのみが保存されます — 連続録画ではありません。",
  "perm.matrixScreenRecording": "スクリーンショットキャプチャ",
  "perm.matrixScreenRecordingReq": "画面収録が必要",
  "perm.calendar": "カレンダー",
  "perm.calendarDesc":
    "カレンダーツールはスケジュールコンテキストのためにイベントを読み取ることができます。必要に応じてmacOSが権限を要求します。",
  "perm.requestCalendarPermission": "カレンダー権限を要求",
  "perm.openCalendarSettings": "カレンダープライバシー設定を開く",
  "perm.location": "位置情報サービス",
  "perm.locationDesc":
    "macOSでは位置情報サービスがCoreLocation（GPS / Wi-Fi / セル）を使用して高精度な位置情報を提供します。LinuxとWindowsではIPベースのジオロケーションを使用し、権限は不要です。位置情報サービスが拒否されたり利用できない場合、アプリは自動的にIPジオロケーションにフォールバックします。",
  "perm.locationOk": "位置情報の権限が許可されています。高精度な位置情報にCoreLocationが使用されます。",
  "perm.locationFallback": "位置情報が許可されていません — IPベースのジオロケーション（市レベルの精度）を使用します。",
  "perm.locationStep1": "システム設定→プライバシーとセキュリティ→位置情報サービスを開く",
  "perm.locationStep2": "リストで{app}を見つけて有効にする",
  "perm.locationStep3": "ここに戻ってください — ステータスは自動的に更新されます",
  "perm.requestLocationPermission": "位置情報の権限を要求",
  "perm.openLocationSettings": "位置情報設定を開く",
  "perm.whyLocation": "位置情報",
  "perm.whyLocationDesc":
    "LLMに正確な位置情報コンテキストを提供し、健康データとともにGPS位置情報を保存するため。拒否された場合はIPジオロケーションにフォールバックします。",
  "perm.matrixLocation": "位置情報（GPS / IP）",
  "perm.matrixLocationReq": "位置情報サービス（オプション — IPにフォールバック）",
  "perm.openInputMonitoringSettings": "入力監視設定を開く",
  "perm.openFocusSettings": "集中モード設定を開く",
  "perm.fullDiskAccess": "フルディスクアクセス",
  "perm.fullDiskAccessDesc":
    "システムデータベース経由で直接集中モードを検出するのに必要です。これがないと、アプリはより遅いレガシー方式にフォールバックします。信頼性の高いおやすみモード統合に推奨されます。",
  "perm.fullDiskAccessStep1": "システム設定→プライバシーとセキュリティ→フルディスクアクセスを開く",
  "perm.fullDiskAccessStep2": "リストでNeuroSkill™（またはデーモンを実行しているターミナル）を見つけて有効にする",
  "perm.fullDiskAccessStep3": "変更を適用するにはアプリを終了して再起動する必要がある場合があります",
  "perm.openFullDiskAccessSettings": "フルディスクアクセス設定を開く",
  "perm.whyCalendar": "カレンダー",
  "perm.whyCalendarDesc": "AIが予定のイベントを参照できるようにLLMツールにスケジュールコンテキストを提供するため。",
  "perm.matrixCalendar": "カレンダーイベント",
  "perm.matrixCalendarReq": "カレンダーアクセスが必要",
};

export default perm;
