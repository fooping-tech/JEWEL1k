# フック設定ガイド (HOOKS_SETUP.md)

Claude Code / Codex から JEWEL1k を光らせ、危険な操作の前に物理キーの承認を
挟むまでを **最初から順番に** 設定するガイド。実機がなくても mock デバイスで
最後まで試せる。

- コマンドやリスク判定の詳細なリファレンス → [HOOKS.md](HOOKS.md)
- ビルド・tray app の全体像 → [../agent-key/README.md](../agent-key/README.md)

全体の流れ:

```
[1] CLI をビルドして PATH に通す
        ↓
[2] tray app を起動(localhost API が立つ)
        ↓
[3] 疎通確認(mock デバイスで動作テスト)
        ↓
[4] エージェントのフックを設定(Claude Code / Codex)
        ↓
[5] エンドツーエンドで動作確認
```

---

## 前提

- **Rust** (stable / Windows は MSVC toolchain) — <https://rustup.rs/>
- **エージェント本体** — Claude Code か Codex CLI
- 実機は任意。**なくても mock デバイスで全ステップを試せる**
  (実機を使う場合は [FIRMWARE_FLASHING.md](FIRMWARE_FLASHING.md) でファーム書き込み)

---

## [1] CLI をビルドして PATH に通す

フックは `agent-key` コマンドを呼ぶので、**どのディレクトリからでも起動できる**
状態にしておく必要がある。

GitHub Releases の配布物を使う場合は、先に展開して `agent-key` と
`agent-key-tray` を PATH の通った場所へ置く。詳細は
[../agent-key/README.md](../agent-key/README.md) の「リリース配布物から使う」を参照。

macOS:

```sh
tar -xzf JEWEL1k-agent-key-v0.1.0-macos-arm64.tar.gz
cd JEWEL1k-agent-key-v0.1.0-macos-arm64
mkdir -p "$HOME/.local/bin"
cp agent-key agent-key-tray "$HOME/.local/bin/"
export PATH="$HOME/.local/bin:$PATH"
agent-key --help
```

Windows (PowerShell):

```powershell
Expand-Archive .\JEWEL1k-agent-key-v0.1.0-windows-x86_64.zip
cd .\JEWEL1k-agent-key-v0.1.0-windows-x86_64\JEWEL1k-agent-key-v0.1.0-windows-x86_64
New-Item -ItemType Directory -Force "$env:USERPROFILE\bin"
Copy-Item ".\agent-key.exe" "$env:USERPROFILE\bin\agent-key.exe"
Copy-Item ".\agent-key-tray.exe" "$env:USERPROFILE\bin\agent-key-tray.exe"
[Environment]::SetEnvironmentVariable(
  "Path",
  $env:Path + ";$env:USERPROFILE\bin",
  "User"
)
```

ソースからビルドする場合は以下。

```sh
cd agent-key

# ~/.cargo/bin にインストール(PATH が通っていれば一発)
cargo install --path crates/agent-key-cli
```

すでに `cargo build --release -p agent-key-cli` でビルド済みなら、ビルド成果物を
PATH が通っている場所から見えるようにしてもよい。

macOS / Linux (zsh/bash):

```sh
cd agent-key
mkdir -p "$HOME/.local/bin"
ln -s "$PWD/target/release/agent-key" "$HOME/.local/bin/agent-key"

# ~/.local/bin が PATH に入っていない場合だけ ~/.zshrc などに追記
export PATH="$HOME/.local/bin:$PATH"
```

Windows (PowerShell):

```powershell
cd agent-key
New-Item -ItemType Directory -Force "$env:USERPROFILE\bin"
Copy-Item ".\target\release\agent-key.exe" "$env:USERPROFILE\bin\agent-key.exe"
[Environment]::SetEnvironmentVariable(
  "Path",
  $env:Path + ";$env:USERPROFILE\bin",
  "User"
)
```

確認:

```sh
agent-key --help        # USAGE が出れば OK
```

> `agent-key: command not found` の場合は `~/.cargo/bin`、`~/.local/bin`、
> または Windows の `%USERPROFILE%\bin` / `%USERPROFILE%\.cargo\bin` が
> PATH に入っているか確認する。

---

## [2] tray app を起動する

localhost API (`127.0.0.1:43117`) はこの tray app が提供する。**フックはこの
API を叩く**ので、エージェントを使う間は起動しておく。

```sh
cd agent-key/apps/tray
cargo build --release
```

```sh
# Windows
.\target\release\agent-key-tray.exe
# macOS / Linux
./target/release/agent-key-tray
```

タスクトレイに常駐する。起動すると:

1. localhost API が `127.0.0.1:43117` で立ち上がる
2. 3秒ごとにデバイスを自動探索して接続する(実機を挿していれば繋がる)
3. 承認要求が来るとトースト通知を出す

> 毎回手動で起動したくない場合は、トレイメニューの **「ログイン時に起動」**
> をオンにする。

---

## [3] 疎通確認(mock デバイス)

実機がなくても、mock デバイスに繋げば一連の流れをテストできる。

```sh
agent-key connect mock                    # 疑似デバイスに接続
agent-key status thinking                 # 青ブリージング(ログに出る)
agent-key state                           # 現在の状態を確認

# 承認フローのテスト(high は2クリック必要)
agent-key approval "test push" --risk high &   # 承認待ちでブロック
agent-key simulate single                 # 疑似クリック 1回目
agent-key simulate single                 # 2回目 -> approved / バックグラウンドjobが exit 0
```

`approval` の終了コード: **0=承認 / 2=拒否・緊急停止 / 3=タイムアウト・取消**。
フックはこの終了コードで挙動を分岐する。

> ここまで動けば API との疎通は OK。実機がある場合は
> `agent-key devices` で認識状態を確認し、自動接続に任せるか
> `agent-key connect serial COM5`(Windows)/ `agent-key connect hid` で繋ぐ。

---

## [4] エージェントのフックを設定する

### Claude Code

同梱の [../examples/claude-settings.json](../examples/claude-settings.json) を
`~/.claude/settings.json`(またはプロジェクトの `.claude/settings.json`)に
**マージ**する。既に `hooks` がある場合は各イベントの配列に追記する。

登録されるフックの意味:

| イベント | コマンド | 効果 |
|----------|----------|------|
| `UserPromptSubmit` | `agent-key status thinking` | 思考開始で青 |
| `PreToolUse` (Bash) | `agent-key hook pre-tool --risk high --json` | Bash 実行前に**物理キーで承認**(2クリック) |
| `PreToolUse` (Write\|Edit) | `agent-key status tool_running` | 編集開始で黄 |
| `PostToolUse` | `agent-key status thinking` | ツール完了で青に戻す |
| `Notification` | `agent-key status needs_approval` | Claude 自身の確認待ちで赤 |
| `Stop` | `agent-key hook stop` | 応答完了で緑 |

キモは Bash の `PreToolUse`。`--json` を付けると **常に exit 0** で返り、承認結果を
hookOutput の `permissionDecision`(`allow` / `deny`)で Claude Code に伝える:

```json
{"hookSpecificOutput": {"hookEventName": "PreToolUse",
  "permissionDecision": "allow",
  "permissionDecisionReason": "Approved by physical key (JEWEL1k)."}}
```

> **tray app が落ちていても安全**: API に到達できないと exit 1(非ブロッキング
> エラー)を返すので、エージェントがロックされることはない。

対象ツールやリスクは matcher / `--risk` で調整する。目安:

| 操作 | risk | 承認方法 |
|------|------|----------|
| 読み取り・検索 | none/low | フック不要(光らせるだけ) |
| ファイル編集 | medium | 単押し |
| Bash / git push / デプロイ | high | 5秒以内に2クリック |
| rm -rf / force push to main | critical | 常に自動拒否 |

### Codex

Codex の lifecycle hooks で LED を細かく更新する場合は、サンプルをローカル設定へ
コピーする。`.codex/hooks.json` は個人の trust 状態に関わるため git 管理しない。

```sh
mkdir -p .codex
cp examples/codex-hooks.json .codex/hooks.json
```

| イベント | 効果 |
|----------|------|
| `UserPromptSubmit` | 青: 思考開始 |
| `PreToolUse` | 黄: ツール実行中 |
| `PermissionRequest` | 赤: Codex の承認待ち |
| `PostToolUse` | 青: ツール完了後に思考へ戻す |
| `Stop` | 緑: ターン完了 |

初回または hook 変更後は Codex CLI で `/hooks` を開き、このリポジトリの hook を
trust する。hook は `agent-key` CLI を呼ぶので、tray app が起動していて
`agent-key` が PATH にある必要がある。
hook が動かない場合は、`.codex/hooks.json` の `command` を
`/Users/.../.local/bin/agent-key status ...` のような絶対パスに置き換える。

古い Codex や通知だけで使う場合は、`~/.codex/config.toml` に:

```toml
notify = ["agent-key", "hook", "codex-notify"]
```

これは **トップレベル** に置く。`[projects.'...']` の下に書くと、そのプロジェクト
テーブルの項目になり、Codex 全体の通知設定として効かない場合がある。

`agent-turn-complete` 通知を緑(done)にマップする。危険コマンドのゲートは
シェルラッパーで `agent-key approval ...` を挟む(→ [HOOKS.md](HOOKS.md) の
「シェルからの直接利用」)。

---

## [5] エンドツーエンドで動作確認

tray app を起動したまま、エージェントを普段どおり使う。

**Claude Code の例**: `git push` のような Bash を実行させると、`PreToolUse` フックが
承認要求を出す。

- 実機なし(mock 接続時): 別ターミナルで `agent-key simulate single` を
  2回 → LED ログが承認を示し、Claude はツールを続行する
- 実機あり: JEWEL1k が赤く速点滅する。**5秒以内に2回クリック**で承認、
  長押しで拒否

承認すれば `permissionDecision: "allow"` が返ってツールが走り、拒否すれば
`deny` でブロックされる。

---

## つまずいたら

| 症状 | 確認すること |
|------|--------------|
| フックが何も起きない | tray app が起動しているか / `agent-key state` が応答するか |
| `command not found` | `agent-key` が PATH にあるか([1]) |
| ポートが違う | `AGENT_KEY_PORT` を tray app と CLI で揃える(既定 43117) |
| 承認が常に通る/拒否される | `--risk` の値と、mock なら `simulate` のクリック回数 |
| 実機が繋がらない | `agent-key devices` で認識確認 / シリアルは `connect serial <PORT>` |

環境変数 `AGENT_KEY_PORT`(既定 43117)/ `AGENT_KEY_TOKEN`(plugin が
トークンを要求する場合)は CLI と tray app で一致させること。

コマンド一覧・curl での直接アクセス・シェルからの利用は [HOOKS.md](HOOKS.md) を参照。
