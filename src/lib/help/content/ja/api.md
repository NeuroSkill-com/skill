# 概要

## ライブストリーミング
{app}はローカルWebSocketサーバーを通じて、算出されたEEGメトリクスとデバイスステータスをリアルタイムで配信します。ブロードキャストイベントには、eeg-bands（約4 Hz — 60以上のスコア）、device-status（約1 Hz — バッテリー、接続状態）、label-createdがあります。生のEEG/PPG/IMUサンプルはWebSocket APIでは利用できません。サービスはBonjour/mDNSで_skill._tcpとして公開されるため、クライアントは自動的に検出できます。

## コマンド
クライアントはWebSocket経由でJSONコマンドを送信できます: status（システム全体のスナップショット）、calibrate（キャリブレーションを開始）、label（アノテーションを送信）、search（最近傍検索）、sessions（録音一覧）、compare（A/Bメトリクス + 睡眠 + UMAP）、sleep（睡眠ステージング）、umap/umap_poll（3D埋め込み投影）。レスポンスは同じ接続上で"ok"ブール値を含むJSONとして返されます。

# コマンドリファレンス

## status
_(パラメータなし)_

デバイスの状態、セッション情報、埋め込み数（本日と全期間）、ラベル数、最終キャリブレーションのタイムスタンプ、チャネルごとの信号品質を返します。

## calibrate
_(パラメータなし)_

キャリブレーションウィンドウを開きます。接続済みでストリーミング中のデバイスが必要です。

## label
text (string, 必須); label_start_utc (u64, 任意 — デフォルトは現在時刻)

タイムスタンプ付きラベルをラベルデータベースに挿入します。新しいlabel_idを返します。

## search
start_utc, end_utc (u64, 必須); k, ef (u64, 任意)

指定された時間範囲内でHNSW埋め込みインデックスからk個の最近傍を検索します。

## compare
a_start_utc, a_end_utc, b_start_utc, b_end_utc (u64, 必須)

2つの時間範囲を比較し、各範囲の集約されたバンドパワーメトリクス（相対パワー、リラクゼーション/エンゲージメントスコア、FAA）を返します。{ a: SessionMetrics, b: SessionMetrics }を返します。

## sessions
_(パラメータなし)_

毎日のeeg.sqliteデータベースから検出されたすべての埋め込みセッションを一覧表示します。セッションは連続した記録範囲です（2分以上の空白 = 新しいセッション）。最新のものから順に返されます。

## sleep
start_utc, end_utc (u64, 必須)

時間範囲内の各埋め込みエポックをバンドパワー比に基づいて睡眠ステージ（Wake/N1/N2/N3/REM）に分類し、各ステージの要約を含むヒプノグラムを返します。

## umap
a_start_utc, a_end_utc, b_start_utc, b_end_utc (u64, 必須)

2つのセッションの埋め込みから3D UMAP投影をキューに追加します。ポーリング用のjob_idを返します。ノンブロッキングです。

## umap_poll
job_id (string, 必須)

以前にキューに追加されたUMAPジョブの結果をポーリングします。{ status: 'pending' | 'done', points?: [...] }を返します。

## say
text: string (必須)

デバイス上のTTSでテキストを読み上げます。ファイア・アンド・フォーゲット方式で、音声がバックグラウンドで再生される間、即座にレスポンスを返します。初回呼び出し時にTTSエンジンを初期化します。
