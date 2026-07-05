# CLAUDE.md

このリポジトリのエージェント向けガイドは @AGENTS.md にまとめてある(構成マップ、
ビルド/テスト、変更時の同期ポイント、安全設計の不変条件)。必ずそちらに従うこと。

## クイックリファレンス

- テスト: `cd agent-key && cargo test`(Rust)/ `pnpm build`(TS bindings)
- 計画・進捗: @PLANS.md
- 設計: docs/DESIGN.md、プロトコル: docs/PROTOCOL.md
- 最重要の不変条件: 承認判定は Rust の `ApprovalQueue` のみ。
  「承認済みにする」command/API を追加しない。critical は常に自動拒否。

## Claude Code 固有の注意

- コミット author は `fooping-tech <fukuhala@gmail.com>`(設定済み)。勝手に変えない
- `cargo test` は初回 tauri のビルドで数分かかる。タイムアウトは長めに(10分)
- ファームウェア(*.ino)はこの環境ではコンパイル不可。変更したら「実機確認が必要」と
  明示的に報告すること
