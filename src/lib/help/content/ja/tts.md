# オンデバイス音声ガイダンス（TTS）

## オンデバイス音声ガイダンス（TTS）
NeuroSkill™には完全にオンデバイスの英語テキスト読み上げエンジンが搭載されています。キャリブレーションフェーズ（アクションラベル、休憩、完了）を音声で案内し、WebSocketまたはHTTP API経由で任意のスクリプトからリモートでトリガーできます。約30 MBのモデルを一度ダウンロードした後は、すべての合成がローカルで実行されます — インターネットは不要です。

## 仕組み
テキスト前処理 → 文チャンキング（400文字以下）→ libespeak-ng（Cライブラリ、インプロセス、en-usボイス）による音素化 → トークン化（IPA → 整数ID）→ ONNX推論（KittenTTSモデル: input_ids + style + speed → f32波形）→ 1秒の無音パディング → rodioがシステムデフォルトの音声出力で再生。

## モデル
HuggingFace HubのKittenML/kitten-tts-mini-0.8。ボイス: Jasper（英語 en-us）。サンプルレート: 24,000 Hz モノラル float32。INT8 ONNX量子化 — CPU専用、GPU不要。初回ダウンロード後は~/.cache/huggingface/hub/にキャッシュされます。

## 要件
espeak-ngがインストールされPATH上にある必要があります — インプロセスIPA音素化を提供します（サブプロセスとして起動されるのではなく、Cライブラリとしてリンクされます）。macOS: brew install espeak-ng。Ubuntu/Debian: apt install libespeak-ng-dev。Alpine: apk add espeak-ng-dev。Fedora: dnf install espeak-ng-devel。

## キャリブレーション連携
キャリブレーションセッションが開始されると、エンジンがバックグラウンドでプリウォームされます（必要に応じてモデルをダウンロード）。各フェーズでキャリブレーションウィンドウがtts_speakを呼び出し、アクションラベル、休憩案内、完了メッセージ、またはキャンセル通知を発話します。音声がキャリブレーションをブロックすることはありません — すべてのTTS呼び出しはファイア・アンド・フォーゲットです。

## API — sayコマンド
外部スクリプト、自動化ツール、またはLLMエージェントから音声をトリガーします。コマンドは音声が再生される間、即座にレスポンスを返します。WebSocket: {"command":"say","text":"メッセージ"}。HTTP: POST /say、ボディは{"text":"メッセージ"}。CLI (curl): curl -X POST http://localhost:<port>/say -d '{"text":"hello"}' -H 'Content-Type: application/json'。

## デバッグログ
設定 → 音声でTTS合成ログを有効にすると、イベント（発話テキスト、サンプル数、推論レイテンシー）がNeuroSkill™のログファイルに書き込まれます。レイテンシーの測定や問題の診断に役立ちます。

## ここでテスト
下のウィジェットを使用して、このヘルプウィンドウから直接TTSエンジンをテストできます。
