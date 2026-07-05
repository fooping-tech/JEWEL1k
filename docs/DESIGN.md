# agent-key 設計ドキュメント (DESIGN.md)

JEWEL1k を **AIコーディングエージェント用のステータスLED兼物理承認ボタン** として扱うための
Tauri v2 plugin「jewel1k-plugin-agent-key」の設計。

## 目的

- エージェントの状態を LED で常時可視化する
  - working(thinking)中: 青が柔らかく点滅(ブリージング)
  - ツール実行中: 黄色
  - 完了: 緑が柔らかく発光
  - 承認/確認が必要: 赤のダブル点滅(高リスクは高速点滅)
- 物理キーで承認操作を行う
  - 単押し: 承認
  - 長押し(>= 800ms): 拒否
  - 超長押し(>= 3s): 緊急停止(全承認要求を拒否)
  - ダブル押し: 詳細表示(承認待ちが高リスクの場合は2クリック分としてカウント)

## アーキテクチャ

```
Codex / Claude Code / shell hook
  ↓ (プロセス起動 / stdin JSON)
agent-key CLI ──── localhost API (127.0.0.1:43117, JSON/HTTP)
  ↓
Tauri tray app
  ↓ (plugin init)
jewel1k-plugin-agent-key   … Tauri commands / events / state管理 / HTTPサーバ
  ↓
agent-key-core             … types / protocol / led_policy / risk_policy /
  ↓                          approval_queue / transport trait
SerialTransport / HidRawTransport(feature hid) / MockTransport
  ↓ (USB CDC 115200bps または raw HID 32byteレポート, A1/B1 バイナリパケット)
JEWEL1k (CH552E + WS2812 + 1 key)
```

## クレート構成

```
agent-key/
├── Cargo.toml                        # workspace
├── crates/
│   ├── agent-key-core/               # ハードウェア/Tauri非依存のコア
│   │   └── src/
│   │       ├── types.rs              # AgentState, RiskLevel, ApprovalRequest, ...
│   │       ├── protocol.rs           # A1/B1 パケットの encode/decode + 逐次Decoder
│   │       ├── led_policy.rs         # state+risk -> pattern/brightness/color
│   │       ├── risk_policy.rs        # risk -> 承認ルール(クリック数/自動拒否)
│   │       ├── approval_queue.rs     # 承認判定の唯一の実装(FIFO, タイムアウト)
│   │       └── transport/            # Transport trait, Mock, Serial(feature serial),
│   │                                 #   HidRaw(feature hid, QMK raw-HID互換)
│   └── agent-key-cli/                # `agent-key` CLI (localhost APIクライアント)
├── plugins/jewel1k-plugin-agent-key/ # Tauri v2 plugin本体
│   │                                 # (Cargoパッケージ名: tauri-plugin-agent-key)
│   ├── src/
│   │   ├── lib.rs                    # plugin setup, Config, poll thread起動
│   │   ├── manager.rs                # 状態管理・イベントemit・LED送出 (複数デバイス対応)
│   │   ├── commands.rs               # Tauri commands
│   │   ├── server.rs                 # localhost HTTP API (std::netのみ)
│   │   └── error.rs
│   ├── permissions/                  # permission sets (default / allow-*)
│   └── guest-js/index.ts             # TypeScript bindings (dist-jsへビルド)
└── apps/tray/                        # tray常駐アプリ (独立workspace, Phase 4)
```

ファームウェアは2種類: `src/agentkey/agentkey.ino`(CDC版)と
`src/agentkey_hid/agentkey_hid.ino`(キーボード+raw HID 複合デバイス版)。

複数デバイス: manager は複数の transport link を同時に保持できる。LED パケットは
全デバイスへブロードキャストされ、どのデバイスのボタンイベントも単一の
`ApprovalQueue` に入る(先着デバイスの決定が有効)。

## 状態モデル

- `AgentState`: `idle | thinking | tool_running | done | needs_approval | error | off`
- `RiskLevel`: `none | low | medium | high | critical`
- `LedPattern`: `off | solid | breath | blink | double_blink | fast_blink`

LED表示は `led_policy::led_for(state, risk, master_brightness)` が一元的に決める。
承認要求が pending の間は state が強制的に `needs_approval` になり、キューが空になると
エージェントが最後に指定した状態 (`resume_state`) に戻る。

## 安全設計(最重要)

1. **承認判定は Rust の `ApprovalQueue` のみが行う。**
   frontend / CLI / LLM が使えるのは「要求(submit)」「取消(cancel)」だけで、
   承認済みにする API は存在しない(コマンドとしても公開していない)。
2. リスク別ルール (`risk_policy.rs`):
   - none/low/medium: 単押しで承認、長押しで拒否
   - high: **5秒以内に2クリック**で承認、長押しで拒否
   - critical: **デフォルト拒否**(submit と同時に denied で解決。ボタンでも覆せない)
3. 超長押しは緊急停止: pending の全要求を `emergency_stopped` として解決し、
   `agent-key://error` を emit。
4. タイムアウト(デフォルト60s)で自動的に `timed_out`。フックはこれを「拒否」として扱う。
5. `simulate_button`(疑似ボタン)は開発用。permission set `allow-simulate` を
   明示的に付与しない限り webview から呼べない。localhost API 側は開発ビルドの
   Mock 接続時のみ機能する(SerialTransport は inject を拒否する)。
6. localhost API は 127.0.0.1 バインドのみ。必要なら `httpToken` 設定で共有トークンを
   要求できる(ヘッダ `x-agent-key-token`)。

## スレッドモデル

- Tauri commands / HTTP ハンドラ: 呼び出し元スレッドから `Mutex<Inner>` 経由で操作
- poll thread (20ms 周期): transport のイベント読み出し、`ApprovalQueue::tick()`
  によるタイムアウト処理、Tauri イベント emit
- HTTP server: accept ループ + 接続ごとのスレッド。承認要求はスレッド上で
  long-poll (`wait_for_resolution`) するため、ブロックしても他要求に影響しない

## MockTransport

実機なしで全機能を開発・テストするための transport。

- `send_packet` は `log::info!` で hex + デコード内容(色を含む)を出力し、内部ログに保持
- `inject_event` で任意のボタンジェスチャを疑似発火(CLI: `agent-key simulate single`)
- 接続直後に firmware と同じ `Ready` イベントを流す

## 今後の拡張

- **HidRawTransport / 複合デバイス**: 実装済み(core feature `hid` +
  `src/agentkey_hid/agentkey_hid.ino`)。実機検証が残タスク(PLANS.md 参照)。
- **tray app**: `agent-key/apps/tray/` に実装済み(トレイメニュー / 承認トースト /
  自動再接続 / autostart)。組み込み手順は docs/TAURI_PLUGIN.md。
- mobile (Swift/Kotlin) 対応は後回し。plugin は desktop のみを想定。
