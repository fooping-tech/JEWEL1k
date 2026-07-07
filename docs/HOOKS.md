# エージェントフック連携 (HOOKS.md)

Claude Code / Codex などのコーディングエージェントから JEWEL1k を光らせ、
危険な操作の前に物理キーの承認を挟むための設定。

> **初めて設定する場合は [HOOKS_SETUP.md](HOOKS_SETUP.md)(番号付きセットアップ
> ガイド)から。** このファイルはコマンド・リスク判定・各エージェント連携の
> リファレンス。

前提: agent-key plugin を組み込んだ Tauri アプリ(付属の tray app
`agent-key/apps/tray` で可)が起動しており、localhost API
(デフォルト `127.0.0.1:43117`) が有効なこと。
CLI `agent-key` (`cargo install --path agent-key/crates/agent-key-cli` または
`cargo build --release` した `agent-key.exe`) が PATH にあること。

## CLI チートシート

```sh
agent-key status thinking            # 青ブリージング
agent-key status tool_running        # 黄
agent-key status done                # 緑
agent-key approval "git push --force" --risk high --timeout 60000
#   -> ボタンの決定までブロック。exit code: 0=承認 2=拒否 3=タイムアウト
agent-key simulate double            # mock接続時の疑似ダブル押し(承認)
```

環境変数: `AGENT_KEY_PORT`, `AGENT_KEY_TOKEN`

## Claude Code

`examples/claude-settings.json` を `~/.claude/settings.json`(または
プロジェクトの `.claude/settings.json`)にマージ。

仕組み:

- `PreToolUse` (matcher なし = 全ツール): `agent-key status tool_running`
- `PreToolUse` (matcher `Bash`): `agent-key hook pre-tool --risk high --json`
  - stdin の hook JSON から `tool_name` / `tool_input` を読み取り、承認要求を発行
  - `--json`: Claude Code の hookOutput JSON を stdout に出して **常に exit 0**:

    ```json
    {"hookSpecificOutput": {"hookEventName": "PreToolUse",
      "permissionDecision": "allow" | "deny",
      "permissionDecisionReason": "Approved by physical key (JEWEL1k)."}}
    ```

    承認= `allow`、拒否/緊急停止/タイムアウト= `deny`
  - `--json` なし(レガシー): 拒否/タイムアウト時は **exit 2** で返り、
    Claude Code がツール実行をブロックする(stderr がモデルへのフィードバック)
  - API に到達できない時はどちらのモードでも exit 1(非ブロッキングエラー)。
    tray app が落ちていてもエージェントがロックされない
- `Stop`: `agent-key hook stop` (= `status done`)
- `Notification`: `agent-key status needs_approval` (Claude Code 自身の確認待ち)

### auto-accept(権限バイパス)モードとの関係

Claude Code を auto-accept モードで使うと、ツールは Claude 自身が承認して即実行され、
物理ボタンによる承認ゲートは本質的に意味を持たない(誰もボタンを押さない)。

そのため `hook pre-tool` は、hook JSON の `permission_mode` が
**`auto` / `dontAsk` / `bypassPermissions` のときは承認要求を出さず、即 `allow` を返す**
(LED は `needs_approval` の赤ではなく `tool_running` の黄を表示)。
`default` / `plan` / `acceptEdits` では従来どおり物理キーでゲートする。

- auto モードでも物理キーを必須にしたい場合は `--always-gate` を付ける
- `--risk critical` は auto モードでもゲートされ、従来どおり自動拒否される
  (auto モードで critical が素通りすることはない)

さらに保険として plugin 側は、エージェントが次のステータス(`thinking` /
`tool_running` / `done` など needs_approval 以外)を送ってきた時点で、**未解決のまま
残っている承認要求を supersede(Cancelled として破棄)する**。これにより赤点滅が
残り続けず、LED は実際のステータス色に戻る。破棄は必ず Cancelled であって Approved には
しないので、承認の安全性は保たれる(通常のブロッキング承認フローでは、承認待ちの間
エージェントは停止していてステータスを送らないため、正規のゲートが誤って打ち切られる
ことはない)。

auto-accept を常用するなら、そもそも Bash の `PreToolUse` 承認ゲートを外して
ステータス表示のフックだけ残す運用も可。

リスクの目安:

| 操作                         | risk     | 挙動 |
|------------------------------|----------|------|
| 読み取り・検索               | none/low | フック不要(光らせるだけ) |
| ファイル編集                 | medium   | ダブル押しで承認 |
| Bash / git push / デプロイ   | high     | ダブル押しで承認 |
| 破壊的操作 (rm -rf, force push to main) | critical | 常に自動拒否 |

## Codex

Codex は lifecycle hooks を読める。リポジトリ内の `.codex/hooks.json` に置くと、
Codex がこのプロジェクトを trusted として扱う場合に読み込まれる。
`.codex/hooks.json` は個人の trust 状態に関わるため git 管理せず、サンプルからコピーする。

```sh
mkdir -p .codex
cp examples/codex-hooks.json .codex/hooks.json
```

このリポジトリの推奨設定:

| Event | command | 効果 |
|-------|---------|------|
| `UserPromptSubmit` | `agent-key status thinking --risk none` | 思考開始で青 |
| `PermissionRequest` | `agent-key status needs_approval --risk high` | Codex の承認待ちで赤 |
| `PreToolUse` | `agent-key status tool_running --risk none` | ツール実行中に黄 |
| `PostToolUse` | `agent-key status thinking --risk none` | ツール完了後に青へ戻す |
| `Stop` | `agent-key status done --risk none` | ターン完了で緑 |

初回または hook 変更後は Codex CLI で `/hooks` を開き、該当 hook を trust する。
hook が動かない場合は、`agent-key` が hook 実行環境の PATH に入っていない可能性がある。
その場合は `.codex/hooks.json` の `command` を
`/Users/.../.local/bin/agent-key status ...` のような絶対パスに置き換える。

`PermissionRequest` は Codex 自身の承認 UI と同期して赤にするだけで、物理キー承認の
結果を Codex へ返すものではない。物理キーで別途ゲートしたい処理は、シェルやスクリプトで
`agent-key approval ...` を明示的に挟む。

古い Codex や通知だけで使う場合は、`~/.codex/config.toml` に:

```toml
notify = ["agent-key", "hook", "codex-notify"]
```

この行は TOML のトップレベルに置く。`[projects.'...']` セクションの後ろに追記すると
そのプロジェクトテーブル内の値になるため、通知設定として効かない場合がある。

Codex は通知のたびに JSON を1引数で渡してくる。`agent-key hook codex-notify` は
`{"type":"agent-turn-complete", ...}` を `status done`(緑)にマップし、
未知の type は無視する(exit 0)。

## シェルからの直接利用

どんなエージェントでも、シェルが挟めるなら使える:

```sh
# 長時間タスクのラッパー
agent-key status tool_running
./deploy.sh; rc=$?
if [ $rc -eq 0 ]; then agent-key status done; else agent-key status error; fi

# 危険コマンドのゲート
agent-key approval "DBマイグレーション実行" --risk high || exit 1
./migrate.sh
```

PowerShell:

```powershell
agent-key approval "prod deploy" --risk high
if ($LASTEXITCODE -ne 0) { throw "denied on the physical key" }
```

## curl での直接アクセス

```sh
curl -s localhost:43117/state
curl -s -X POST localhost:43117/status -d '{"state":"thinking"}'
curl -s -X POST "localhost:43117/approval?wait=false" -d '{"title":"x","risk":"medium"}'
```
