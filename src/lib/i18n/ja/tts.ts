// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** JA "tts" namespace. */
const tts: Record<string, string> = {
  "ttsTab.backendSection": "音声エンジン",
  "ttsTab.backendKitten": "KittenTTS",
  "ttsTab.backendKittenTag": "ONNX · 英語 · 約30MB",
  "ttsTab.backendKittenDesc": "コンパクトなONNXモデル。あらゆるCPUで高速。英語のみ。",
  "ttsTab.backendNeutts": "NeuTTS",
  "ttsTab.backendNeuttsTag": "GGUF · 声のクローニング · 多言語",
  "ttsTab.backendNeuttsDesc":
    "GGUFのLLMバックボーンとNeuCodecデコーダー。任意の声をクローン可能。英語、ドイツ語、フランス語、スペイン語対応。",
  "ttsTab.statusSection": "エンジン状態",
  "ttsTab.statusReady": "準備完了",
  "ttsTab.statusLoading": "読み込み中…",
  "ttsTab.statusIdle": "アイドル",
  "ttsTab.statusUnloaded": "未読み込み",
  "ttsTab.statusError": "失敗",
  "ttsTab.preloadButton": "プリロード",
  "ttsTab.retryButton": "再試行",
  "ttsTab.unloadButton": "アンロード",
  "ttsTab.errorTitle": "読み込みエラー",
  "ttsTab.preloadOnStartup": "起動時にエンジンをプリロード",
  "ttsTab.preloadOnStartupDesc": "アプリ起動時にアクティブなエンジンをバックグラウンドでウォームアップします",
  "ttsTab.requirements": "PATHにespeak-ngが必要です",
  "ttsTab.requirementsDesc": "macOS: brew install espeak-ng · Ubuntu: apt install espeak-ng",
  "ttsTab.kittenConfigSection": "KittenTTS設定",
  "ttsTab.kittenVoiceLabel": "音声",
  "ttsTab.kittenModelInfo": "KittenML/kitten-tts-mini-0.8 · 24 kHz · 約30MB",
  "ttsTab.neuttsConfigSection": "NeuTTS設定",
  "ttsTab.neuttsModelLabel": "バックボーンモデル",
  "ttsTab.neuttsModelDesc": "小さいGGUFほど高速、大きいほど自然。ほとんどのシステムにはQ4を推奨。",
  "ttsTab.neuttsVoiceSection": "リファレンス音声",
  "ttsTab.neuttsVoiceDesc": "プリセット音声を選択するか、声のクローニング用に独自のWAVクリップを提供します。",
  "ttsTab.neuttsPresetLabel": "プリセット音声",
  "ttsTab.neuttsCustomOption": "カスタムWAV…",
  "ttsTab.neuttsRefWavLabel": "リファレンスWAV",
  "ttsTab.neuttsRefWavNone": "ファイルが選択されていません",
  "ttsTab.neuttsRefWavBrowse": "参照…",
  "ttsTab.neuttsRefTextLabel": "トランスクリプト",
  "ttsTab.neuttsRefTextPlaceholder": "WAVクリップで話されている内容を正確に入力してください",
  "ttsTab.neuttsSaveButton": "保存",
  "ttsTab.neuttsSaved": "保存済み",
  "ttsTab.voiceJo": "Jo",
  "ttsTab.voiceDave": "Dave",
  "ttsTab.voiceGreta": "Greta",
  "ttsTab.voiceJuliette": "Juliette",
  "ttsTab.voiceMateo": "Mateo",
  "ttsTab.voiceCustom": "カスタム…",
  "ttsTab.testSection": "音声テスト",
  "ttsTab.testDesc": "テキストを入力して「再生」を押すと、アクティブなエンジンの音声を確認できます。",
  "ttsTab.startupSection": "起動",
  "ttsTab.loggingSection": "デバッグログ",
  "ttsTab.loggingLabel": "TTS合成ログ",
  "ttsTab.loggingDesc": "合成イベント（テキスト、サンプル数、レイテンシー）をログファイルに書き込みます。",
  "ttsTab.apiSection": "API",
  "ttsTab.apiDesc": "WebSocketまたはHTTP API経由で任意のスクリプトやツールから音声をトリガーできます：",
  "ttsTab.apiExampleWs": 'WebSocket:  {"command":"say","text":"目を閉じてください。"}',
  "ttsTab.apiExampleHttp": 'HTTP (curl): POST /say  body: {"text":"目を閉じてください。"}',

  "helpTts.overviewTitle": "オンデバイス音声ガイダンス（TTS）",
  "helpTts.overviewBody":
    "NeuroSkill™には完全にオンデバイスの英語テキスト読み上げエンジンが含まれています。キャリブレーションフェーズを音声でアナウンスし（アクションラベル、休憩、完了）、WebSocketまたはHTTP API経由で任意のスクリプトからリモートでトリガーできます。すべての合成はローカルで実行されます — 約30MBのモデルを一度ダウンロードした後はインターネット不要です。",
  "helpTts.howItWorksTitle": "仕組み",
  "helpTts.howItWorksBody":
    "テキスト前処理→文チャンキング（400文字以下）→libespeak-ng（Cライブラリ、インプロセス、en-us音声）によるフォネミゼーション→トークン化（IPA→整数ID）→ONNX推論（KittenTTSモデル：input_ids + style + speed → f32波形）→1秒無音パッド→rodioがシステムデフォルト音声出力で再生。",
  "helpTts.modelTitle": "モデル",
  "helpTts.modelBody":
    "HuggingFace HubのKittenML/kitten-tts-mini-0.8。音声：Jasper（英語en-us）。サンプルレート：24,000 Hzモノフロート32。量子化INT8 ONNX — CPUのみ、GPU不要。初回ダウンロード後~/.cache/huggingface/hub/にキャッシュ。",
  "helpTts.requirementsTitle": "要件",
  "helpTts.requirementsBody":
    "espeak-ngがインストールされPATHに存在する必要があります — インプロセスIPAフォネミゼーションを提供します（Cライブラリとしてリンク、サブプロセスとして起動されない）。macOS: brew install espeak-ng。Ubuntu/Debian: apt install libespeak-ng-dev。Alpine: apk add espeak-ng-dev。Fedora: dnf install espeak-ng-devel。",
  "helpTts.calibrationTitle": "キャリブレーション連携",
  "helpTts.calibrationBody":
    "キャリブレーションセッション開始時にエンジンがバックグラウンドでプリウォーム（必要に応じてモデルをダウンロード）されます。各フェーズでキャリブレーションウィンドウがtts_speakを呼び出し、アクションラベル、休憩アナウンス、完了メッセージ、キャンセル通知を発声します。音声はキャリブレーションをブロックしません — すべてのTTS呼び出しはファイア＆フォーゲットです。",
  "helpTts.apiTitle": "API — say コマンド",
  "helpTts.apiBody":
    '任意の外部スクリプト、自動化ツール、LLMエージェントから音声をトリガーします。コマンドは音声再生中に即座に返されます。WebSocket: {"command":"say","text":"your message"}。HTTP: POST /say body {"text":"your message"}。CLI (curl): curl -X POST http://localhost:<port>/say -d \'{"text":"hello"}\' -H \'Content-Type: application/json\'。',
  "helpTts.loggingTitle": "デバッグログ",
  "helpTts.loggingBody":
    "設定→音声でTTS合成ログを有効にすると、イベント（発声テキスト、サンプル数、推論レイテンシー）がNeuroSkill™ログファイルに書き込まれます。レイテンシーの測定や問題の診断に便利です。",
  "helpTts.testTitle": "ここでテスト",
  "helpTts.testBody": "下のウィジェットを使用して、このヘルプウィンドウから直接TTSエンジンをテストできます。",
};

export default tts;
