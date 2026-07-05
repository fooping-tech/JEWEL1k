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
pnpm install && pnpm build   # guest-js TypeScript bindings -> dist-js/
```

- Rust は MSVC toolchain。`serialport` は core の feature `serial`(plugin では default on)
- ファームウェア (`src/**/*.ino`) はここではビルドできない。Arduino IDE + CH55xduino が必要。
  変更したらコンパイル確認は人間に依頼すること

## 構成マップ

```
agent-key/
├── crates/agent-key-core/        # Tauri非依存のコア。types / protocol / led_policy /
│                                 #   risk_policy / approval_queue / transport(Mock,Serial,HID trait)
├── crates/agent-key-cli/         # `agent-key` CLI。localhost APIクライアント (std::netのみ)
└── plugins/jewel1k-plugin-agent-key/
    ├── src/manager.rs            # 状態管理の中心。イベントemit / LED送出 / poll tick
    ├── src/server.rs             # localhost HTTP API (127.0.0.1:43117)
    ├── src/commands.rs           # Tauri commands (managerの薄いラッパ)
    ├── permissions/              # default.toml + sets.toml (allow-*)
    └── guest-js/index.ts         # TypeScript bindings
src/agentkey/agentkey.ino         # agent-key用ファームウェア (1key.inoベース, CDC)
docs/{DESIGN,PROTOCOL,TAURI_PLUGIN,HOOKS}.md
```

## 変更時の同期ポイント(壊しやすい所)

以下は**手動で同期している**。片方だけ変えるとプロトコル不整合になる:

- 色テーブル: `led_policy.rs::color_for` ⇔ `agentkey.ino::stateColor` ⇔ docs/PROTOCOL.md
- state/risk/pattern/event のワイヤバイト値: `types.rs`(`#[repr(u8)]`) ⇔ `agentkey.ino` の #define ⇔ docs/PROTOCOL.md
- Tauri commands 追加時: `build.rs` の COMMANDS / `commands.rs` / `lib.rs` の generate_handler /
  `permissions/sets.toml` / `guest-js/index.ts` / docs/TAURI_PLUGIN.md の5箇所
- ジェスチャ閾値 (350ms/800ms/3s): `agentkey.ino` ⇔ docs/PROTOCOL.md ⇔ `types.rs` のdocコメント

## 安全設計の不変条件(絶対に破らない)

- **承認判定は `agent-key-core::ApprovalQueue` のみが行う。**
  「承認済みにする」Tauri command / HTTP エンドポイントを追加してはならない。
  frontend・CLI・LLM に許されるのは要求(submit)と取消(cancel)だけ
- critical リスクのデフォルト拒否、high の2クリック要件を緩和しない
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
agent-key simulate single                       # 疑似クリック -> approved / exit 0
```
