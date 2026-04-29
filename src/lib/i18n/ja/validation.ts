// SPDX-License-Identifier: GPL-3.0-only
/** JA — "validation" namespace. */
const validation: Record<string, string> = {
  "settingsTabs.validation": "検証",
  "validation.title": "検証と研究",
  "validation.intro":
    "ブレイクコーチとフォーカススコアを外部測定で校正するオプトインの研究器具。NeuroSkill の使用に必須ではありません。",
  "validation.disclaimer":
    "研究用ツールのみ — 医療機器ではありません。FDA、CE、またはいかなる規制機関の承認も受けていません。臨床使用は不可。",

  "validation.master.title": "グローバルゲート",
  "validation.master.respectFlow": "フロー状態を尊重",
  "validation.master.respectFlowDesc":
    "フローに入ると、以下のすべてのプロンプトを抑制します。デフォルトで有効 — そのままにしてください。",
  "validation.master.quietBefore": "静音時間 開始",
  "validation.master.quietAfter": "静音時間 終了",
  "validation.master.quietDesc":
    "ローカル時間。このウィンドウ外ではプロンプトは発生しません。開始 = 終了 で静音時間を完全に無効化します。",

  "validation.kss.title": "カロリンスカ眠気尺度 (KSS)",
  "validation.kss.desc": "瞬間的な眠気の5秒自己報告 (1-9)。主観的状態に対するブレイクコーチの校正に使用します。",
  "validation.kss.enabled": "KSS プロンプトを有効化",
  "validation.kss.maxPerDay": "1日あたりの最大プロンプト数",
  "validation.kss.minInterval": "プロンプト間の最小分数",
  "validation.kss.triggerBreakCoach": "ブレイクコーチが疲労を検出したら発動",
  "validation.kss.triggerRandom": "時々のランダム制御サンプルを発動",
  "validation.kss.triggerRandomDesc": "ROC/AUC を計算するために必要 — 陰性なしでは陽性ケースしか見えません。",
  "validation.kss.randomWeight": "ランダムサンプルの重み (0-1)",

  "validation.tlx.title": "NASA-TLX (作業負荷、生6スケール)",
  "validation.tlx.desc": "作業ユニット後の60秒6サブスケール作業負荷自己報告。負荷を測定 — KSS 眠気の相補。",
  "validation.tlx.enabled": "NASA-TLX プロンプトを有効化",
  "validation.tlx.maxPerDay": "1日あたりの最大プロンプト数",
  "validation.tlx.minTaskMin": "尋ねる最小タスク長 (分)",
  "validation.tlx.endOfDay": "1日の終わりの作業負荷サマリーも発動",

  "validation.tlx.form.title": "終わったばかりのタスクを評価してください",
  "validation.tlx.mental": "精神的要求",
  "validation.tlx.physical": "身体的要求",
  "validation.tlx.temporal": "時間的要求",
  "validation.tlx.performance": "パフォーマンス",
  "validation.tlx.effort": "努力",
  "validation.tlx.frustration": "フラストレーション",

  "validation.pvt.title": "精神運動性覚醒度課題 (PVT)",
  "validation.pvt.desc": "3分間の反応時間タスク。客観的な覚醒度測定 — 収集は遅いが文献で最も強力なシグナル。",
  "validation.pvt.enabled": "週次 PVT リマインダーを有効化",
  "validation.pvt.weeklyReminder": "今週 PVT がない場合に1行リマインダーを表示",
  "validation.pvt.runNow": "PVT を今すぐ実行 (3分)",
  "validation.pvt.task.start": "開始",
  "validation.pvt.task.cancel": "キャンセル",
  "validation.pvt.task.close": "閉じる",

  "validation.eeg.title": "EEG 疲労指数 (Jap et al. 2009)",
  "validation.eeg.desc":
    "NeuroSkill ヘッドセットが接続されている場合、バンドパワーストリームから連続的に計算されます。式: (α + θ) / β。受動的 — コストなし。",
  "validation.eeg.enabled": "EEG 疲労指数を計算",
  "validation.eeg.windowSecs": "ローリングウィンドウ (秒)",
  "validation.eeg.current": "現在値",
  "validation.eeg.noHeadset": "EEG ヘッドセットがストリーミングしていません",

  "validation.calibrationWeek.title": "校正ウィーク",
  "validation.calibrationWeek.desc":
    "オプトインの7日間バースト、より高頻度のサンプリング。KSS を 1日8回に増やし、20分以上のフローブロック後ごとに TLX を発動、週半ばに1つの PVT を要求。8日目に通常設定に自動復帰。",
  "validation.calibrationWeek.start": "校正ウィークを開始",

  "validation.results.title": "最近の結果",
  "validation.save.saved": "保存しました",
};
export default validation;
