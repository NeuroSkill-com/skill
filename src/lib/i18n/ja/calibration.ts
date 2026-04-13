// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** JA "calibration" namespace. */
const calibration: Record<string, string> = {
  "calibration.title": "キャリブレーション",
  "calibration.profiles": "キャリブレーションプロファイル",
  "calibration.newProfile": "新しいプロファイル",
  "calibration.editProfile": "プロファイルを編集",
  "calibration.profileName": "プロファイル名",
  "calibration.profileNamePlaceholder": "例：開眼 / 閉眼",
  "calibration.addAction": "アクションを追加",
  "calibration.actionLabel": "アクションラベル…",
  "calibration.breakLabel": "休憩",
  "calibration.selectProfile": "プロファイル",
  "calibration.moveUp": "上に移動",
  "calibration.moveDown": "下に移動",
  "calibration.removeAction": "アクションを削除",
  "calibration.descriptionN": "このプロトコルは{actions}を<strong>{count}</strong>回繰り返します。",
  "calibration.timingDescN": "{loops}ループ · {actions}アクション · 各間{breakSecs}秒の休憩",
  "calibration.notifActionBody": "ループ {loop} / {total}",
  "calibration.notifBreakBody": "次：{next}",
  "calibration.notifDoneBody": "全{n}ループが完了しました。",
  "calibration.recording": "● 記録中",
  "calibration.neverCalibrated": "未キャリブレーション",
  "calibration.lastAgo": "最終：{ago}",
  "calibration.eegCalibration": "EEGキャリブレーション",
  "calibration.description":
    'このタスクは<strong class="text-blue-600 dark:text-blue-400">{action1}</strong>と<strong class="text-violet-600 dark:text-violet-400">{action2}</strong>を休憩を挟んで交互に行い、<strong>{count}</strong>回繰り返します。',
  "calibration.timingDesc":
    "各アクションは{actionSecs}秒、休憩は{breakSecs}秒です。ラベルは自動的に保存されます。",
  "calibration.startCalibration": "キャリブレーション開始",
  "calibration.complete": "キャリブレーション完了",
  "calibration.completeDesc":
    "全{n}回の反復が正常に完了しました。各アクションフェーズのラベルが保存されました。",
  "calibration.runAgain": "再実行",
  "calibration.iteration": "反復",
  "calibration.break": "休憩",
  "calibration.nextAction": "次：{action}",
  "calibration.secondsRemaining": "秒残り",
  "calibration.ready": "準備完了",
  "calibration.lastCalibrated": "最終キャリブレーション",
  "calibration.lastAtAgo": "最終：{date}（{ago}）",
  "calibration.noPrevious": "過去のキャリブレーション記録はありません",
  "calibration.footer": "Escで閉じる · イベントはWebSocket経由でブロードキャスト",
  "calibration.presets": "クイックプリセット",
  "calibration.presetsDesc":
    "目的、年齢、用途に基づいたキャリブレーション設定を選択してください。下記で設定を調整できます。",
  "calibration.applyPreset": "適用",
  "calibration.orCustom": "または手動で設定：",
  "calibration.preset.baseline": "開眼 / 閉眼",
  "calibration.preset.baselineDesc":
    "クラシックなベースライン：安静時の開眼vs閉眼。初心者や初回キャリブレーションに最適です。",
  "calibration.preset.focus": "集中 / リラックス",
  "calibration.preset.focusDesc": "ニューロフィードバック：暗算vs穏やかな呼吸。一般的な用途に。",
  "calibration.preset.meditation": "瞑想",
  "calibration.preset.meditationDesc": "能動的思考vsマインドフルネス瞑想。瞑想実践者向け。",
  "calibration.preset.sleep": "入眠前 / 眠気",
  "calibration.preset.sleepDesc": "覚醒状態vs眠気。睡眠研究やリラックストラッキングに。",
  "calibration.preset.gaming": "ゲーム / パフォーマンス",
  "calibration.preset.gamingDesc": "高負荷タスクvs受動的休息。eスポーツやピークパフォーマンスのバイオフィードバックに。",
  "calibration.preset.children": "子供 / 短い注意力",
  "calibration.preset.childrenDesc": "集中力の持続が難しい子供やユーザー向けの短いフェーズ（10秒）。",
  "calibration.preset.clinical": "臨床 / 研究",
  "calibration.preset.clinicalDesc":
    "研究や臨床ベースライン用の、長いアクションフェーズを持つ5回反復の拡張プロトコル。",
  "calibration.preset.stress": "ストレス / 不安",
  "calibration.preset.stressDesc":
    "安静時のリラックスvs軽度の認知ストレッサー。不安やストレス反応のトラッキングに。",
};

export default calibration;
