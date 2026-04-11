---
layout: guide
title: JEWEL1k actkey Troubleshooting
permalink: /troubleshooting.html
brand: JEWEL1k actkey
eyebrow: TROUBLESHOOTING
hero_title: JEWEL1k actkey Troubleshooting
hero_subtitle: README の「動かない場合」をもとにした確認ページです。接続やキー入力の問題を順番に切り分けます。
brand_site:
  label: ブランドサイトへ戻る
  url: https://fooping-tech.github.io/jewel1k-site/
top_link:
  label: actkey トップへ戻る
  url: ./
guide_link:
  label: 設定ガイド
  url: how-to-custom.html
troubleshooting_link:
  label: トラブルシューティング
  url: troubleshooting.html
primary_link:
  label: Keyboard Test を開く
  url: https://www.onlinemictest.com/ja/keyboard-test/
overview:
  title: Troubleshooting
  subtitle: README の確認フローを別ページ化
preparation:
  title: Check First
  subtitle: 最初に見ておくポイント
  items:
    - title: ケーブルを確認
      body: "USB Micro-B ケーブルが通信対応か確認します。充電専用ケーブルでは actkey を認識できません。"
    - title: 接続先を変える
      body: "別の USB ポートに差し替えて、接触や給電の問題を切り分けます。可能なら別 PC でも確認します。"
    - title: 設定状態を切り分ける
      body: "Remap で設定した直後なら、いったん標準状態か別の簡単なキー割り当てにして挙動を確認します。"
  note: "README では「以下のフローに従って確認してください」と案内されています。まずは接続・認識・キー入力の順に切り分けるのが安全です。"
steps_section:
  title: Steps
  subtitle: README のトラブルシューティングを実行しやすい順に整理
steps:
  - no: 1
    eyebrow: CONNECT
    title: ケーブルと接続を確認する
    body: "actkey が動かない場合は、最初に USB Micro-B ケーブルを確認します。通信可能なケーブルを使い、別ポートへの差し替えも試します。"
    points:
      - "充電専用ケーブルは使えません。"
      - "可能なら別の PC や別の USB ポートでも試します。"
  - no: 2
    eyebrow: REMAP
    title: Remap 側で認識するか確認する
    body: "Google Chrome で Remap を開き、`JEWEL 1KEY qmk kbd` が見えるかを確認します。見えない場合は接続段階の問題を疑います。"
  - no: 3
    eyebrow: KEYMAP
    title: 簡単なキー割り当てで試す
    body: "複合キーや特殊な設定ではなく、まずは単純なアルファベット 1 文字などに割り当てて動くかを確認します。"
  - no: 4
    eyebrow: TEST
    title: Keyboard Test で入力を確認する
    body: "README にあるキーボードテストサイトで、押したキーが実際に入力されているか確認します。"
    points:
      - "ブラウザ上でキー入力が見えるので、PC 側の認識確認に向いています。"
      - "入力が出ない場合は、ケーブルか認識の問題に戻って確認します。"
  - no: 5
    eyebrow: RETRY
    title: 再設定して保存する
    body: "Remap で `JEWEL1k.json` を再インポートし、キーを設定し直して `Finish` まで進めます。`success` が出るかを確認します。"
links_section:
  title: Links
  subtitle: トラブル時に使う確認先
links:
  - title: Keyboard Test
    body: "キー入力が PC 側で認識されているか確認します。"
    url: https://www.onlinemictest.com/ja/keyboard-test/
    external: true
  - title: Remap
    body: "JEWEL 1KEY qmk kbd が見えるか、再設定できるかを確認します。"
    url: https://remap-keys.app
    external: true
  - title: JEWEL1k.json
    body: "再設定時に読み込むキーマップ定義ファイルです。"
    url: https://github.com/fooping-tech/JEWEL1k/blob/main/setting/JEWEL1k.json
    external: true
---
JEWEL1k actkey がうまく動かないときは、README にあるフローに沿って確認していくのが最短です。
このページでは、その確認内容を接続、認識、入力テスト、再設定の順に整理しています。
