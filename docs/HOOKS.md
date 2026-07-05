# エージェントフック連携 (HOOKS.md)

Claude Code / Codex などのコーディングエージェントから JEWEL1k を光らせ、
危険な操作の前に物理キーの承認を挟むための設定。

前提: agent-key plugin を組み込んだ Tauri アプリが起動しており、
localhost API (デフォルト `127.0.0.1:43117`) が有効なこと。
CLI `agent-key` (`cargo install --path agent-key/crates/agent-key-cli` または
`cargo build --release` した `agent-key.exe`) が PATH にあること。

## CLI チートシート

```sh
agent-key status thinking            # 青ブリージング
agent-key status tool_running        # 黄
agent-key status done                # 緑
agent-key approval "git push --force" --risk high --timeout 60000
#   -> ボタンの決定までブロック。exit code: 0=承認 2=拒否 3=タイムアウト
agent-key simulate single            # mock接続時の疑似クリック
```

環境変数: `AGENT_KEY_PORT`, `AGENT_KEY_TOKEN`

## Claude Code

`examples/claude-settings.json` を `~/.claude/settings.json`(または
プロジェクトの `.claude/settings.json`)にマージ。

仕組み:

- `PreToolUse` (matcher なし = 全ツール): `agent-key status tool_running`
- `PreToolUse` (matcher `Bash`): `agent-key hook pre-tool --risk high`
  - stdin の hook JSON から `tool_name` / `tool_input` を読み取り、承認要求を発行
  - 拒否/タイムアウト時は **exit 2** で返り、Claude Code がツール実行をブロックする
    (stderr がモデルへのフィードバックになる)
- `Stop`: `agent-key hook stop` (= `status done`)
- `Notification`: `agent-key status needs_approval` (Claude Code 自身の確認待ち)

リスクの目安:

| 操作                         | risk     | 挙動 |
|------------------------------|----------|------|
| 読み取り・検索               | none/low | フック不要(光らせるだけ) |
| ファイル編集                 | medium   | 単押しで承認 |
| Bash / git push / デプロイ   | high     | 5秒以内に2クリック |
| 破壊的操作 (rm -rf, force push to main) | critical | 常に自動拒否 |

## Codex

`examples/codex-hooks.json` を参照。Codex の `~/.codex/config.toml` の `notify` /
exec ラッパー等、フック機構の呼び出し先として `agent-key` CLI を指定する
(Codex 側のフック仕様はバージョンで変わるため、JSON はコマンド定義の
リファレンスとして使うこと)。

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
