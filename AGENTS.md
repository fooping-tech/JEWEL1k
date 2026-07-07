# AGENTS.md — このリポジトリで作業するエージェントへ

JEWEL1k は CH552E 採用の1キーキーボード。このリポジトリには2つの顔がある:

1. **1keyキーボード本体** — `src/1key/`(QMK/via互換ファームウェア)、`docs/`(Jekyllの製品ページ)、`setting/`
2. **agent-key** — JEWEL1k を「AIエージェントのステータスLED + 物理承認ボタン」にする
   Tauri v2 plugin 一式(`agent-key/`)と専用ファームウェア(`src/agentkey/`)

計画と進捗は [PLANS.md](PLANS.md)、設計は [docs/DESIGN.md](docs/DESIGN.md) を参照。
※ `examples/AGENTS.md` はエンドユーザーのプロジェクトに貼るテンプレートであり、
このファイル(リポジトリ開発用)とは別物。

## ビルド・テスト

```sh
cd agent-key
cargo test          # Rust workspace 全体 (core / cli / plugin統合テスト)
cargo test --workspace --all-features   # hid (hidapi) も含める
pnpm install && pnpm build   # guest-js TypeScript bindings -> dist-js/
cd apps/tray && cargo build  # tray app (独立workspace。wryをリンクするため分離)
```

- Rust は MSVC toolchain。`serialport` は core の feature `serial`(plugin では default on)、
  `hidapi` は feature `hid`(default off。tray app / CI の all-features で有効)
- CI: `.github/workflows/ci.yml`(cargo test / clippy `-D warnings` / pnpm build)
- ファームウェア (`src/**/*.ino`) はここではビルドできない。Arduino IDE + CH55xduino が必要。
  変更したらコンパイル確認は人間に依頼すること

## 構成マップ

```
agent-key/
├── crates/agent-key-core/        # Tauri非依存のコア。types / protocol / led_policy /
│                                 #   risk_policy / approval_queue /
│                                 #   transport(Mock / Serial[serial] / HidRaw[hid])
├── crates/agent-key-cli/         # `agent-key` CLI。localhost APIクライアント (std::netのみ)
│   └── tests/e2e.rs              # CLI実バイナリ vs モックAPIサーバの E2E テスト
├── plugins/jewel1k-plugin-agent-key/  # ※Cargoパッケージ名は tauri-plugin-agent-key (下記注意)
│   ├── src/manager.rs            # 状態管理の中心。複数デバイス links / イベントemit / poll tick
│   ├── src/server.rs             # localhost HTTP API (127.0.0.1:43117)
│   ├── src/commands.rs           # Tauri commands (managerの薄いラッパ)
│   ├── permissions/              # default.toml + sets.toml (allow-*)
│   └── guest-js/index.ts         # TypeScript bindings
└── apps/tray/                    # Phase 4 tray常駐アプリ (独立workspace)
src/agentkey/agentkey.ino         # agent-key用ファームウェア (CDC版)
src/agentkey_hid/agentkey_hid.ino # 複合デバイス版 (キーボード+raw HID。via と同居)
docs/{DESIGN,PROTOCOL,TAURI_PLUGIN,HOOKS,HOOKS_SETUP}.md
```

## 変更時の同期ポイント(壊しやすい所)

以下は**手動で同期している**。片方だけ変えるとプロトコル不整合になる:

- 色テーブル: `led_policy.rs::color_for` ⇔ `agentkey.ino::stateColor` ⇔
  `agentkey_hid.ino::stateColor` ⇔ docs/PROTOCOL.md
- state/risk/pattern/event のワイヤバイト値: `types.rs`(`#[repr(u8)]`) ⇔ 両 .ino の
  #define ⇔ docs/PROTOCOL.md
- Tauri commands 追加時: `build.rs` の COMMANDS / `commands.rs` / `lib.rs` の generate_handler /
  `permissions/sets.toml` / `guest-js/index.ts` / docs/TAURI_PLUGIN.md の5箇所
- ジェスチャ閾値 (350ms/800ms/3s): 両 .ino ⇔ docs/PROTOCOL.md ⇔ `types.rs` のdocコメント
- USB VID/PID `0x4249:0x4287`: `keyboardConfig.h`(1key / agentkey_hid) ⇔
  `transport/hid.rs` の定数 ⇔ tray app の自動検出ヒューリスティック。
  ※ CDC 版ファーム `src/agentkey/agentkey.ino` は "Default CDC" 設定で VID/PID を
  上書きしないため、CH55xduino 既定の `1209:c550` として列挙される。tray の
  `looks_like_jewel` はこの CDC 既定 ID も併せてマッチさせている(でないと
  「USB シリアル デバイス」名で自動接続に拾われない)
- **plugin の Cargo パッケージ名は `tauri-plugin-agent-key` から変えない。**
  tauri は permission 名前空間を CARGO_PKG_NAME から導出(`tauri-plugin-` のみ strip)
  するため、これを変えると capability の `agent-key:*` と runtime 名 "agent-key" が
  不一致になり webview からの invoke が全部 ACL で落ちる(Rust lib 名は
  `jewel1k_plugin_agent_key` のまま)

## 安全設計の不変条件(絶対に破らない)

- **承認判定は `agent-key-core::ApprovalQueue` のみが行う。**
  「承認済みにする」Tauri command / HTTP エンドポイントを追加してはならない。
  frontend・CLI・LLM に許されるのは要求(submit)と取消(cancel)だけ
- critical リスクのデフォルト拒否を緩和しない。承認は物理ボタンの
  **ダブル押し(`ButtonGesture::Double`)のみ**で成立させる(単押しは承認に使わない)
- `simulate_button` は開発用。permission set `allow-simulate` を default に含めない
- localhost API は 127.0.0.1 バインドのみ。外部公開しない

## 実装上の注意

- plugin の `tauri` 依存は `default-features = false`(wry/WebView2 をリンクすると
  Windows でテストバイナリが STATUS_ENTRYPOINT_NOT_FOUND で落ちる)
- plugin のテストは `src/tests.rs`(クレート内モジュール)。tauri MockRuntime +
  ephemeral port で HTTP E2E を回す。テスト実行時に default port 43117 の bind 失敗ログが
  出るが無害
- `ApprovalQueue` は時刻を ms 引数で受け取る決定論的設計。テストでは実時間 sleep ではなく
  now_ms を進めること
- `ManagerCore` は非ジェネリック(emit はクロージャ注入)。State のみを引数に取る
  ジェネリック command は tauri の generate_handler で型推論できないため、この形を維持する
- コミットは `fooping-tech <fukuhala@gmail.com>`(リポジトリローカル設定済み)

## 動作確認(実機なし)

plugin を組み込んだアプリ起動後(autoConnect: "mock"):

```sh
agent-key status thinking                       # 送信ログが青breathingを示す
agent-key approval "test" --risk medium &       # ブロッキング承認要求
agent-key simulate single                       # 単押し -> 承認されない
agent-key simulate double                       # ダブル押し -> approved / exit 0
```
