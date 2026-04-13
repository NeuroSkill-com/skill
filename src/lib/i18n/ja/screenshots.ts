// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** JA "screenshots" namespace. */
const screenshots: Record<string, string> = {
  "screenshots.title": "スクリーンショットキャプチャ",
  "screenshots.enableToggle": "スクリーンショットキャプチャを有効化",
  "screenshots.enableDesc":
    "アクティブウィンドウを定期的にキャプチャし、ビジョンモデルで埋め込んで視覚類似検索を可能にします。",
  "screenshots.sessionOnlyToggle": "セッション中のみ",
  "screenshots.sessionOnlyDesc": "アクティブなEEG記録セッション中のみキャプチャします。",
  "screenshots.interval": "キャプチャ間隔",
  "screenshots.intervalDesc":
    "EEG埋め込みエポック（各5秒）に揃えられます。1× = 毎エポック、2× = 2エポックごと、最大12×（60秒）。",
  "screenshots.intervalUnit": "秒",
  "screenshots.intervalEpoch": "エポック",
  "screenshots.imageSize": "画像サイズ",
  "screenshots.imageSizeDesc":
    "中間解像度（px）。キャプチャしたウィンドウは保存と埋め込みの前にこの正方形に収まるようにリサイズされます。",
  "screenshots.imageSizeUnit": "px",
  "screenshots.imageSizeRecommended": "現在のモデルの推奨値：",
  "screenshots.quality": "WebP品質",
  "screenshots.qualityDesc": "WebP圧縮品質（0〜100）。低いほどファイルサイズが小さくなります。",
  "screenshots.embeddingModel": "埋め込みモデル",
  "screenshots.embeddingModelDesc": "類似検索用の画像埋め込みを生成するビジョンモデル。",
  "screenshots.backendFastembed": "fastembed（ローカルONNX）",
  "screenshots.backendMmproj": "mmproj（LLMビジョンプロジェクター）",
  "screenshots.backendLlmVlm": "LLM VLM（ビジョンモデルによる埋め込み＋OCR）",
  "screenshots.modelClip": "CLIP ViT-B/32 — 512d（高速、デフォルト）",
  "screenshots.modelNomic": "Nomic Embed Vision v1.5 — 768d",
  "screenshots.reembed": "スクリーンショットの再埋め込み",
  "screenshots.reembedDesc": "現在のモデルを使用してすべての既存スクリーンショットの埋め込みを再計算します。",
  "screenshots.reembedBtn": "再埋め込み＆再インデックス",
  "screenshots.reembedNowBtn": "今すぐ再埋め込み",
  "screenshots.reembedding": "埋め込み中…",
  "screenshots.stale": "古い",
  "screenshots.unembedded": "未埋め込み",
  "screenshots.estimate": "推定時間：",
  "screenshots.modelChanged": "埋め込みモデルが変更されました",
  "screenshots.modelChangedDesc":
    "スクリーンショットは別のモデルで埋め込まれています。一貫した検索結果のために再埋め込みしてください。",
  "screenshots.privacyNote":
    "すべてのスクリーンショットはローカルにのみ保存され、送信されることはありません。オプトイン方式で、デフォルトではセッション限定です。",
  "screenshots.storagePath": "保存先：~/.skill/screenshots/",
  "screenshots.permissionRequired": "画面収録の権限が必要です",
  "screenshots.permissionDesc":
    "macOSでは他のアプリケーションウィンドウをキャプチャするために画面収録とシステム音声録音の権限が必要です。これがないとスクリーンショットが空白になったり、自分のアプリのみが表示される場合があります。",
  "screenshots.permissionGranted": "画面収録の権限が許可されています。",
  "screenshots.openPermissionSettings": "画面収録設定を開く",
  "screenshots.ocrToggle": "OCRテキスト抽出",
  "screenshots.ocrToggleDesc":
    "テキストベースの検索のためにスクリーンショットからテキストを抽出します。ダウンサイズ前のフル解像度画像で実行されます。",
  "screenshots.gpuToggle": "GPUアクセラレーション",
  "screenshots.gpuToggleDesc":
    "画像埋め込みとOCRにGPUを使用します。CPU推論を強制するには無効にしてください（LLM/EEG用にGPUを解放）。",
  "screenshots.ocrEngineSelect": "OCRエンジン",
  "screenshots.ocrEngineAppleVision": "Apple Vision — GPU / Neural Engine（macOSでは推奨）",
  "screenshots.ocrEngineOcrs": "ocrs — ローカルrtenベースCPU（クロスプラットフォーム）",
  "screenshots.ocrAppleVisionHint": "⚡ Apple VisionはGPU/ANEで動作し、macOSではocrsの約10倍高速です",
  "screenshots.ocrActiveModels": "アクティブモデル",
  "screenshots.ocrInference": "推論",
  "screenshots.ocrTitle": "OCRテキスト抽出",
  "screenshots.ocrEngine": "オンデバイスOCR",
  "screenshots.ocrDesc":
    "テキストはocrsエンジンを使用してダウンサイズ前のフル解像度で各スクリーンショットから抽出されます。抽出されたテキストはBGE-Small-EN-v1.5で埋め込まれ、セマンティックテキスト検索用の別のHNSWインデックスにインデックス化されます。OCRモデル（各約10MB）は初回使用時に自動ダウンロードされます。",
  "screenshots.ocrDetModel": "検出モデル",
  "screenshots.ocrRecModel": "認識モデル",
  "screenshots.ocrTextEmbed": "テキスト埋め込み",
  "screenshots.ocrIndex": "テキストインデックス",
  "screenshots.ocrSearchHint": "検索ウィンドウ→画像タブでスクリーンショットテキストを検索できます。",
  "screenshots.ocrSearchTitle": "画面テキストで検索",
  "screenshots.ocrSearchPlaceholder": "スクリーンショットに表示されたテキストを検索…",
  "screenshots.ocrSearchBtn": "検索",
  "screenshots.ocrModeSubstring": "テキストマッチ",
  "screenshots.ocrModeSemantic": "セマンティック",
  "screenshots.ocrNoResults": "一致するスクリーンショットが見つかりません。",
  "screenshots.perfTitle": "パイプラインパフォーマンス",
  "screenshots.perfCapture": "キャプチャスレッド",
  "screenshots.perfEmbed": "埋め込みスレッド",
  "screenshots.perfTotal": "合計",
  "screenshots.perfWindowCapture": "ウィンドウキャプチャ",
  "screenshots.perfOcr": "OCR抽出",
  "screenshots.perfResize": "リサイズ＋パディング",
  "screenshots.perfSave": "保存＋SQLite",
  "screenshots.perfIterTotal": "反復合計",
  "screenshots.perfVisionEmbed": "ビジョン埋め込み",
  "screenshots.perfTextEmbed": "テキスト埋め込み",
  "screenshots.perfQueue": "キュー深度",
  "screenshots.perfDrops": "ドロップ",
  "screenshots.perfBackoff": "バックオフ",
  "screenshots.perfDropsHint": "埋め込みスレッドが遅すぎます — 間隔が自動的に増加し、キューが排出されると回復します",
  "screenshots.perfErrors": "エラー",
  "screenshots.stats": "統計",
  "screenshots.totalCount": "スクリーンショット総数",
  "screenshots.embeddedCount": "埋め込み済み",
  "screenshots.unembeddedCount": "未埋め込み",
  "screenshots.staleCount": "古い（別のモデル）",
};

export default screenshots;
