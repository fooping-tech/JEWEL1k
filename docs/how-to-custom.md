---
layout: guide
title: JEWEL1k actkey How To Custom
permalink: /how-to-custom.html
brand: JEWEL1k actkey
eyebrow: HOW TO CUSTOM
hero_title: JEWEL1k actkey Custom Guide
hero_subtitle: Remap を使って、JEWEL1k actkey の 1 キーに好きな操作を割り当てる手順です。
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
  label: Remap を開く
  url: https://remap-keys.app
overview:
  title: How To Custom
  subtitle: setting/HowToCustom.md をもとにしたガイドページ
preparation:
  title: Preparation
  subtitle: 先に揃えておくもの
  items:
    - title: JEWEL1k.json を用意
      body: "`setting/JEWEL1k.json` をダウンロードしておきます。Remap でキーボード定義を読み込むために必要です。"
    - title: Google Chrome を使う
      body: "Remap は Google Chrome で開きます。他のブラウザでは設定できません。"
    - title: USB Micro-B ケーブルを用意
      body: "JEWEL1k actkey を接続するために、通信可能な USB Micro-B ケーブルを使います。充電専用ケーブルは使えません。"
  note: "準備ができたら、Remap の画面を開いた状態で actkey を接続するとスムーズです。"
steps_section:
  title: Steps
  subtitle: HowToCustom.md の内容を 6 ステップに整理
steps:
  - no: 1
    eyebrow: DOWNLOAD
    title: 設定ファイルをダウンロードする
    body: "まず `JEWEL1k.json` を手元に保存します。あとで `IMPORT JSON` から読み込みます。"
  - no: 2
    eyebrow: OPEN REMAP
    title: Chrome で Remap を開く
    body: "Google Chrome で `remap-keys.app` にアクセスします。ここから actkey のキー割り当てを変更します。"
  - no: 3
    eyebrow: CUSTOMIZE
    title: Remap の設定画面へ進む
    body: "`CUSTOMIZE YOUR KEYBOARD` をクリックして、デバイス接続待ちの画面まで進みます。"
  - no: 4
    eyebrow: CONNECT
    title: actkey を接続して選択する
    body: "画面が表示された状態で JEWEL1k actkey を接続します。表示された `JEWEL 1KEY qmk kbd` を選択します。"
  - no: 5
    eyebrow: IMPORT JSON
    title: JSON を読み込み、キーを設定する
    body: "`IMPORT JSON` を押してダウンロード済みの `JEWEL1k.json` を選択します。表示されたキー一覧から、好きなキーや複合キーをドラッグ＆ドロップで割り当てます。"
    points:
      - "`Ctrl + C` などの複合キーも登録できます。"
      - "1 キーでもショートカットや定型操作に使えます。"
  - no: 6
    eyebrow: FINISH
    title: 設定を保存して完了する
    body: "`Finish` をクリックし、`success` と表示されれば設定完了です。必要なら Keyboard Test で動作確認します。"
links_section:
  title: Links
  subtitle: カスタム時に使う関連リンク
links:
  - title: JEWEL1k.json
    body: actkey のキーマップ定義ファイルです。先にダウンロードしておきます。
    url: https://github.com/fooping-tech/JEWEL1k/blob/main/setting/JEWEL1k.json
    external: true
  - title: Remap
    body: Google Chrome から開いて、JEWEL1k actkey のキー割り当てを変更します。
    url: https://remap-keys.app
    external: true
  - title: Keyboard Test
    body: 設定後にキー入力が正しく動いているか確認できます。
    url: https://www.onlinemictest.com/ja/keyboard-test/
    external: true
---
JEWEL1k actkey は、ブラウザからキー割り当てを変更できる実用的な 1key キーボードです。
このページでは `setting/HowToCustom.md` の内容を、実際の操作順に合わせて読みやすく整理しています。
