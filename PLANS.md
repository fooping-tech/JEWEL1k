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
| 4 | Tauri tray app 本体 | ✅ 実装済み (`agent-key/apps/tray`) / 🔲 実運用検証 |
| 5 | HidTransport (QMK raw-HID 互換の複合デバイス化) | ✅ 実装済み (host + firmware) / 🔲 実機検証 |
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
- [x] 実機 CDC 認識確認 (macOS: `/dev/cu.usbmodemCH55x1`)
- [x] 実機への LED コマンド送信確認 (A1 packet write)
- [x] 実機からのボタンイベント受信確認 (B1 packet read: single/double/long)
- [x] 承認待ち表示の実機確認 (`needs_approval` が赤速点滅)
- [x] 自動再接続 (tray app 側で実装: 3秒ポーリングで serial→hid の順に自動検出)
- [ ] 実機 (COMポート) での疎通確認: `agent-key connect serial COM5`
- [ ] ホットプラグ(抜き差し)の再接続動作確認 (tray app の自動再接続で実機確認)

## Phase 3: ファームウェア (実装済み → 実機検証待ち)

`src/agentkey/agentkey.ino` (既存 `src/1key/1key.ino` ベース、CDCシリアル版)

- [x] A1 パケット受信 (再同期 + checksum 検証)
- [x] B1 イベント送信 (single/double/long/very_long/down/up/ready)
- [x] ジェスチャ分類 (ダブル判定350ms / 長押し800ms / 超長押し3s、超長押しは押下中に即送信)
- [x] LEDパターンレンダラ (breath/blink/double/fast、整数演算のみ、~60fps)
- [x] 実機検証で判明した LED 色順ズレを修正・再書き込み確認 (`set_pixel_for_GRB_LED` へ RGB 順で渡す)
- [x] Arduino IDE (CH55xduino) でのビルド・書き込み確認 (IDE 2.3.8。2.3.10 はコンパイルエラーあり)
- [x] `docs/FIRMWARE_FLASHING.md` に書き込み手順を追記
- [ ] タイミングパラメータの実機チューニング (デバウンス、ダブル判定窓)

## Phase 4: Tauri tray app (実装済み → 実運用検証待ち)

`agent-key/apps/tray/`。独立 workspace(wry/WebView2 をリンクするため plugin の
テストビルドから分離)。ビルドは `cd agent-key/apps/tray && cargo build`。

- [x] トレイ常駐アプリの雛形 (`agent-key/apps/tray/`)
- [x] plugin 組み込み + capabilities 設定 (docs/TAURI_PLUGIN.md の手順で)
- [x] トレイメニュー: 接続状態表示 / Mock接続・切断 / 輝度調整 (25/50/100%)
- [x] 承認要求・承認結果のトースト通知 + ダブル押しでステータスウィンドウ表示
  (「詳細表示」の受け皿)
- [x] シリアルポート自動検出・自動再接続 (3秒ポーリング、serial→hid の順、
  VID/PID 4249:4287 / "CH55" / "JEWEL" でマッチ。トレイの「自動接続」でon/off)
- [x] 起動時自動実行 (tauri-plugin-autostart。トレイメニューから切替)
- [x] ステータスウィンドウ (dist/index.html, withGlobalTauri):
  LED状態 / 承認要求の詳細と取消 / デバイス一覧から接続 / 輝度スライダ / イベントログ
- [ ] 実機を挿しての自動検出・トースト通知の実運用確認
- [ ] 配布用 bundle (現状 bundle.active=false。必要になったら icons 一式を揃えて有効化)

## Phase 5: HidTransport (実装済み → 実機検証待ち)

- [x] ファームウェアを キーボードHID + vendor HID (usage page 0xFF60, usage 0x61) の
  複合デバイス化: `src/agentkey_hid/agentkey_hid.ino`
  (1key.ino の QMK/via 機能と同居。via の raw HID に A1/B1 を相乗りし、
  コマンドID 0xA1 は via の 0x01..0x12/0xFF と非衝突。デフォルトキーマップは
  KEY_NONE にして承認クリックでの誤入力を防止)
- [x] `hidapi` ベースの HidRawTransport 実装 (core feature `hid`、
  `transport::HidTransport` trait 準拠。32byte固定レポート、先頭が B1 でない
  レポート = via トラフィックは無視)
- [x] CDC 版ファームとの互換維持 (tray app が serial→hid の順に自動判別。
  `agent-key connect hid` / plugin の `autoConnect: "hid"` も追加)
- [ ] 複合デバイスファームの実機書き込み・検証 (**未実機検証**: この環境では
  コンパイル不可。Arduino IDE 2.3.8 + CH55xduino、USB Settings=user266。
  docs/FIRMWARE_FLASHING.md 参照)

## 継続タスク / 技術的負債

- [x] CLI の localhost API E2E テスト (`crates/agent-key-cli/tests/e2e.rs`:
  実バイナリをモックAPIサーバに対して起動し、リクエスト内容・exit code・
  hookOutput JSON を検証。9件)
- [x] `agent-key hook pre-tool` の Claude Code hookOutput JSON 形式対応
  (`--json` フラグ: permissionDecision allow/deny を stdout に出して exit 0。
  examples/claude-settings.json も更新)
- [x] Codex 連携を実設定に更新 (`notify = ["agent-key", "hook", "codex-notify"]`。
  CLI に `hook codex-notify` を追加、agent-turn-complete → done)
- [x] approval キューの複数デバイス対応 (manager が複数 links を保持。LED は
  全デバイスへブロードキャスト、ボタンはどのデバイスからでも同一キューに入る。
  `disconnect(id)` / `Health.devices` を追加)
- [x] CI (GitHub Actions): `.github/workflows/ci.yml`
  (ubuntu+windows で cargo test --all-features / clippy -D warnings / pnpm build)
- [x] plugin の Cargo パッケージ名を `tauri-plugin-agent-key` に修正
  (tauri が permission 名前空間をパッケージ名から導出するため。旧名のままだと
  capability の `agent-key:*` が解決できず webview invoke が ACL で全拒否になる
  潜在バグだった。Rust lib 名 `jewel1k_plugin_agent_key` は維持)
- [ ] tray app の実機での実運用確認 (自動再接続 / トースト / autostart)

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
| 10 | cargo test と pnpm build が成功 | 42 tests passed (core 23 / plugin 8 / CLI 2 + E2E 9) / tsc ビルド成功 |
