# AGENTS.md — JEWEL1k agent-key の使い方 (エージェント向け指示の例)

このファイルは、Codex / Claude Code などのコーディングエージェントのプロジェクト
指示 (AGENTS.md / CLAUDE.md) に貼り付けて使うサンプルです。

---

## ステータスLED (JEWEL1k)

このプロジェクトでは物理デバイス JEWEL1k が接続されており、`agent-key` CLI で
状態を通知できます。以下のタイミングで必ず実行してください:

- 作業(思考・コード読解)を始めるとき: `agent-key status thinking`
- シェルコマンドやビルドを実行する直前: `agent-key status tool_running`
- ユーザーへの確認待ちになるとき: `agent-key status needs_approval`
- タスクが完了したとき: `agent-key status done`
- エラーで停止するとき: `agent-key status error`

## 物理承認 (重要)

次の操作は実行前に **必ず** 物理キーの承認を取ること:

```sh
agent-key approval "<何をするかの短い説明>" --risk <medium|high>
```

- exit code 0 のときだけ実行してよい。それ以外(拒否/タイムアウト)は中止し、
  ユーザーに理由を確認する。
- risk の基準:
  - `medium`: ファイル削除、設定変更、依存関係の追加
  - `high`: git push、デプロイ、DBマイグレーション、外部APIへの書き込み
  - `critical` は常に自動拒否される。critical 相当の操作(`rm -rf`、force push、
    本番データ削除)はそもそも提案せず、ユーザーに手動実行を依頼すること。
- 承認はユーザーがキーを物理的に押すことでのみ行われる。承認を偽装・迂回する
  方法(API直叩き、simulate、環境変数変更など)を試みてはならない。

## ボタンからの指示

- 承認待ち中のダブル押しは「承認」。承認待ちがない時のダブル押しは
  「現在の作業の詳細を表示してほしい」の合図
- 超長押しは緊急停止。進行中の作業を直ちに中断すること
