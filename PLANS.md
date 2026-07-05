# PLANS.md — agent-key 開発計画

JEWEL1k を「AIコーディングエージェントのステータスLED + 物理承認ボタン」にする
agent-key プロジェクトの計画と進捗。設計の詳細は [docs/DESIGN.md](docs/DESIGN.md)。

## 全体ロードマップ

| Phase | 内容 | 状態 |
|-------|------|------|
| 0 | ブランチ作成・フォルダ構成決定 | ✅ 完了 (`feature/agent-key-plugin`) |
| 1 | MockTransport で全機能を完成させる | ✅ 完了 |
| 2 | SerialTransport 実装 | ✅ 実装済み / 🔲 実機検証 |
| 3 | デバイスファームウェア (CH552E) | ✅ 実装済み / 🔲 実機書き込み・検証 |
| 4 | Tauri tray app 本体 | 🔲 未着手 |
| 5 | HidTransport (QMK raw-HID 互換の複合デバイス化) | 🔲 trait 定義のみ |
| 6 | mobile (Swift/Kotlin) 対応 | 🔲 対象外(後回しと決定) |

## Phase 1: MockTransport (完了)

- [x] agent-key-core: types / protocol (A1/B1 + XOR checksum + 逐次Decoder)
- [x] led_policy (state+risk → pattern/brightness/color)
- [x] risk_policy (medium=1クリック, high=5秒以内2クリック, critical=自動拒否)
- [x] approval_queue (FIFO / タイムアウト / 長押し拒否 / 超長押し緊急停止)
- [x] MockTransport (送信ログ + `inject_event` による疑似ボタン)
- [x] Tauri plugin (commands / events / permissions / poll thread)
- [x] localhost HTTP API (127.0.0.1:43117, 任意トークン)
- [x] agent-key CLI (`status` / `approval` / `simulate` / `hook pre-tool|stop`)
- [x] guest-js TypeScript bindings (`pnpm build` → dist-js)
- [x] docs 4本 + examples (claude-settings / codex-hooks / AGENTS テンプレート)
- [x] MVP受け入れ条件 1〜10 (cargo test 32件 / pnpm build グリーン)

## Phase 2: SerialTransport (実装済み → 実機検証待ち)

- [x] serialport クレートによる実装 (115200bps, 10msノンブロッキング, feature `serial`)
- [x] ポート列挙 (`list_devices` に USB VID/PID 表示)
- [x] I/Oエラー時の切断処理 + `device-disconnected` emit
- [ ] 実機 (COMポート) での疎通確認: `agent-key connect serial COM5`
- [ ] ホットプラグ(抜き差し)の再接続動作確認
- [ ] 自動再接続 (現状は手動 connect のみ。tray app 側で実装予定)

## Phase 3: ファームウェア (実装済み → 実機検証待ち)

`src/agentkey/agentkey.ino` (既存 `src/1key/1key.ino` ベース、CDCシリアル版)

- [x] A1 パケット受信 (再同期 + checksum 検証)
- [x] B1 イベント送信 (single/double/long/very_long/down/up/ready)
- [x] ジェスチャ分類 (ダブル判定350ms / 長押し800ms / 超長押し3s、超長押しは押下中に即送信)
- [x] LEDパターンレンダラ (breath/blink/double/fast、整数演算のみ、~60fps)
- [ ] Arduino IDE (CH55xduino) でのビルド・書き込み確認
- [ ] タイミングパラメータの実機チューニング (デバウンス、ダブル判定窓)
- [ ] 検証後、`setting/` に書き込み手順を追記

## Phase 4: Tauri tray app (未着手)

- [ ] トレイ常駐アプリの雛形 (`agent-key/apps/tray/` を想定)
- [ ] plugin 組み込み + capabilities 設定 (docs/TAURI_PLUGIN.md の手順で)
- [ ] トレイメニュー: デバイス選択 / 接続状態表示 / 輝度調整
- [ ] 承認要求のトースト通知 (ダブル押し「詳細表示」の受け皿)
- [ ] シリアルポート自動検出・自動再接続
- [ ] 起動時自動実行 (autostart)

## Phase 5: HidTransport (trait のみ定義済み)

- [ ] ファームウェアを キーボードHID + vendor HID (usage page 0xFF60) の複合デバイス化
  (1key.ino の QMK/via 機能と agent-key の同居)
- [ ] `hidapi` ベースの HidTransport 実装 (`transport::HidTransport` trait に準拠)
- [ ] CDC 版ファームとの互換維持 (transport 自動判別)

## 継続タスク / 技術的負債

- [ ] CLI の localhost API E2E テスト (現状: plugin 統合テストでAPI検証、CLIは単体テストのみ)
- [ ] `agent-key hook pre-tool` の Claude Code hookOutput JSON 形式対応 (現状 exit code のみ)
- [ ] Codex のフック仕様確定後、examples/codex-hooks.json を実設定に更新
- [ ] approval キューの複数デバイス対応 (現状は単一 transport 前提)
- [ ] CI (GitHub Actions): cargo test + pnpm build + clippy

## 受け入れ条件の対応表 (MVP)

| # | 条件 | 検証 |
|---|------|------|
| 1 | `setStatus({state:"thinking"})` が動作 | plugin統合テスト `set_status_thinking_reaches_mock_transport` |
| 2 | MockTransport で LED 送信ログ出力 | 同上 + `mock_logs_sent_packets` |
| 3 | MockTransport で button イベント疑似発火 | `mock_emits_ready_then_injected_events` / `/simulate` |
| 4 | medium risk がクリックで承認 | `medium_risk_approved_by_simulated_click_over_http` |
| 5 | high risk が2クリックで承認 | `high_risk_needs_two_clicks_over_http` |
| 6 | critical risk が拒否 | `critical_risk_is_denied_without_button` |
| 7 | onButtonEvent でイベント受信 | guest-js `onButtonEvent` + `agent-key://button` emit |
| 8 | CLI から localhost API 経由で approval 要求 | CLI `approval` コマンド + HTTP `/approval` 統合テスト |
| 9 | Codex/Claude hook 例を提供 | examples/claude-settings.json, codex-hooks.json |
| 10 | cargo test と pnpm build が成功 | 32 tests passed / tsc ビルド成功 |
