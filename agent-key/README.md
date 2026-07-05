# agent-key

JEWEL1k を「AIコーディングエージェントのステータスLED + 物理承認ボタン」にする
ツール一式です。エージェント(Claude Code / Codex など)の状態を LED 色で表示し、
危険な操作の前に **物理キーの単押し / 長押し** で承認・拒否を挟めます。

- **tray app** (`apps/tray/`): 常駐アプリ本体。デバイスを自動検出し、承認要求を
  トースト通知する。CLI/フック用の localhost API (127.0.0.1:43117) を提供
- **CLI** (`crates/agent-key-cli/`): `agent-key` コマンド。フックやシェルから叩く
- **plugin** (`plugins/jewel1k-plugin-agent-key/`): Tauri v2 plugin。自作アプリに
  組み込む場合は [docs/TAURI_PLUGIN.md](../docs/TAURI_PLUGIN.md)
- **firmware** (`../src/agentkey/`, `../src/agentkey_hid/`): デバイス側ファーム

LED の色: 青=思考中 / 黄=ツール実行中 / 緑=完了 / 赤=承認待ち・エラー。

## 1. 必要なもの

- **Rust** (stable, MSVC toolchain on Windows) — [rustup](https://rustup.rs/)
- **Node.js 20+** と **pnpm** (`corepack enable` で有効化) — plugin の TS bindings をビルドする場合のみ
- Windows では WebView2 ランタイム(Windows 11 は標準搭載)
- 実機で使う場合: ファームウェアを書き込んだ JEWEL1k
  ([docs/FIRMWARE_FLASHING.md](../docs/FIRMWARE_FLASHING.md))。
  **実機がなくても mock デバイスで全機能を試せます。**

## 2. ビルド

```sh
cd agent-key

# CLI (agent-key コマンド)
cargo build --release -p agent-key-cli
# -> target/release/agent-key(.exe)

# tray app 本体(独立 workspace。必ず apps/tray 内でビルドする)
cd apps/tray
cargo build --release
# -> apps/tray/target/release/agent-key-tray(.exe)
```

CLI をどこからでも呼べるように PATH を通します(フックから使うため必須):

```sh
# 方法A: cargo install で ~/.cargo/bin に入れる
cargo install --path crates/agent-key-cli

# 確認
agent-key --help
```

`cargo install` ではなく、上でビルドした `target/release/agent-key` をそのまま
使いたい場合は、PATH が通っているディレクトリへリンクを置きます。

macOS / Linux (zsh/bash):

```sh
mkdir -p "$HOME/.local/bin"
ln -s "$PWD/target/release/agent-key" "$HOME/.local/bin/agent-key"

# ~/.local/bin が PATH に入っていない場合だけ ~/.zshrc などに追記
export PATH="$HOME/.local/bin:$PATH"

# 確認
agent-key --help
```

Windows (PowerShell):

```powershell
# 例: ユーザー配下の bin にコピーして PATH に追加する
New-Item -ItemType Directory -Force "$env:USERPROFILE\bin"
Copy-Item ".\target\release\agent-key.exe" "$env:USERPROFILE\bin\agent-key.exe"
[Environment]::SetEnvironmentVariable(
  "Path",
  $env:Path + ";$env:USERPROFILE\bin",
  "User"
)

# 新しい PowerShell を開いて確認
agent-key --help
```

## 3. tray app の起動

ビルドした `agent-key-tray` を実行するとタスクトレイに常駐します
(ウィンドウは最初は非表示)。

```sh
# Windows
.\apps\tray\target\release\agent-key-tray.exe

# macOS / Linux
./apps/tray/target/release/agent-key-tray
```

起動すると:

1. localhost API が `127.0.0.1:43117` で立ち上がる(CLI/フックの接続先)
2. 自動接続スレッドが 3秒ごとにデバイスを探索し、見つかれば接続する
   (シリアル CDC版ファーム → raw HID 複合デバイス版 の順)
3. 承認要求が来るとトースト通知が出る

トレイアイコンをクリックするとメニューが開きます:

| メニュー | 動作 |
|----------|------|
| (先頭の行) | 現在の接続状態を表示 |
| 自動接続 (シリアル/HID) | デバイス自動検出のオン/オフ |
| Mock デバイスに接続 | 実機なしで動作確認する用 |
| 切断 | デバイスを切断(自動接続もオフになる) |
| 輝度 | 25% / 50% / 100% |
| ステータスウィンドウ | LED状態・承認詳細・デバイス一覧・ログを表示 |
| ログイン時に起動 | autostart のオン/オフ(次項) |
| 終了 | アプリ終了 |

物理キーを **ダブル押し** すると承認要求の詳細ウィンドウが開きます。

## 4. 初期設定

### ログイン時に自動起動する

トレイメニューの **「ログイン時に起動」** をオンにすると、次回ログインから
tray app が自動で立ち上がります(tauri-plugin-autostart)。エージェントを
使うたびに手動起動する必要がなくなります。

### 実機を接続する

ファームを書き込んだ JEWEL1k を USB で挿すと、自動接続が有効なら数秒で
つながります。手動でつなぐ場合は CLI かステータスウィンドウから:

```sh
agent-key devices                 # 認識中のデバイス一覧
agent-key connect serial COM5     # CDC版ファーム (Windows は COMn)
agent-key connect hid             # raw HID 複合デバイス版ファーム
```

macOS のシリアルポートは `/dev/cu.usbmodemXXXX` の形式です。

### 動作確認(実機なし)

mock デバイスに接続すれば、LED 送信ログと疑似ボタンで一連の流れを試せます:

```sh
agent-key connect mock
agent-key status thinking                 # 青ブリージング(ログに出る)
agent-key approval "test" --risk high &   # 承認待ち(ブロック)
agent-key simulate single                 # 疑似クリック
agent-key simulate single                 # high は2クリックで承認 -> exit 0
```

## 5. エージェントのフック設定

tray app を起動した状態で、`agent-key` CLI を各エージェントのフックに登録します。

### Claude Code

[examples/claude-settings.json](../examples/claude-settings.json) を
`~/.claude/settings.json`(またはプロジェクトの `.claude/settings.json`)に
マージします。Bash 実行前に物理キーの承認を挟み、状態変化を LED に反映します。

### Codex

Codex の lifecycle hook から LED を細かく更新する場合は、サンプルを
ローカル設定へコピーします。`.codex/hooks.json` は個人の trust 状態に関わるため
git 管理しません。

```sh
mkdir -p .codex
cp examples/codex-hooks.json .codex/hooks.json
```

設定内容:

| Codex event | LED |
|-------------|-----|
| `UserPromptSubmit` | 青: 思考開始 |
| `PreToolUse` | 黄: ツール実行中 |
| `PermissionRequest` | 赤: Codex の承認待ち |
| `PostToolUse` | 青: ツール完了後に思考へ戻る |
| `Stop` | 緑: ターン完了 |

初回または hook 変更後は Codex CLI で `/hooks` を開き、このリポジトリの hook を
trust してください。

古い Codex や通知だけで使う場合は、`~/.codex/config.toml` に追記:

```toml
notify = ["agent-key", "hook", "codex-notify"]
```

`notify` はトップレベルに置きます。`[projects.'...']` の下に書くと、そのプロジェクト
テーブル内の設定になり、通知として動かない場合があります。`notify` だけで反応するのは
主にターン完了時の緑(done)です。

詳細と、シェルから直接使う方法は [docs/HOOKS.md](../docs/HOOKS.md) を参照。

## 6. CLI チートシート

```sh
agent-key status thinking|tool_running|done|needs_approval|error|idle|off
agent-key approval "<タイトル>" --risk medium|high|critical [--timeout 60000]
#   exit code: 0=承認 / 2=拒否・緊急停止 / 3=タイムアウト・取消
agent-key state | health | devices
agent-key brightness 0-255
agent-key simulate single|double|long|very_long   # mock 接続時のみ
```

環境変数: `AGENT_KEY_PORT`(既定 43117)/ `AGENT_KEY_TOKEN`(plugin が
`httpToken` を要求する場合)。

## 7. 開発

```sh
cargo test --workspace                 # デフォルト機能
cargo test --workspace --all-features  # hid (hidapi) 込み
cargo clippy --workspace --all-features --all-targets -- -D warnings
pnpm install && pnpm build             # guest-js TypeScript bindings -> dist-js/
```

設計・不変条件・同期ポイントは [../AGENTS.md](../AGENTS.md) と
[docs/DESIGN.md](../docs/DESIGN.md) を参照。承認判定は Rust の `ApprovalQueue`
のみが行う設計で、「承認済みにする」API は存在しません。
