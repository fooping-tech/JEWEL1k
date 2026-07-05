# ファームウェア書き込み手順

JEWEL1k のファームウェアは Arduino IDE + CH55xduino で書き込みます。

このリポジトリには用途の違うファームウェアが3つあります。書き込むファイルに合わせて
`USB Settings` を変えてください。

| 用途 | ファイル | USB Settings |
|------|----------|--------------|
| 1キーキーボード / Remap 対応 | `src/1key/1key.ino` | `USER CODE w/266B USB ram` |
| agent-key / ステータスLED + 承認ボタン (CDC) | `src/agentkey/agentkey.ino` | `Default CDC` など CDC 系 |
| agent-key 複合デバイス (キーボード + raw HID) | `src/agentkey_hid/agentkey_hid.ino` | `USER CODE w/266B USB ram` |

`src/agentkey/agentkey.ino` は HID キーボードではなく USB CDC シリアルデバイスとして動きます。
`USER CODE w/266B USB ram` で書き込むと、macOS で `/dev/cu.usbmodem*` が出ず、
agent-key の `SerialTransport` から接続できません。

## 確認済みの Arduino IDE

- Arduino IDE 2.3.8: 書き込み確認済み
- Arduino IDE 2.3.10: コンパイルエラーが出る場合あり

2.3.10 で失敗する場合は、まず Arduino IDE 2.3.8 で試してください。

## 書き込みモードで起動する

1. JEWEL1k を USB から抜く
2. 書き込み用の端子をショートしたまま USB を差し込む
3. 書き込みモードに入ったら、Arduino IDE でアップロードする

CH552 のブートローダは待ち時間が短いため、書き込みモードに入れたらすぐアップロードしてください。
タイムアウトした場合は、いったん USB を抜いて同じ手順をやり直します。

通常の Arduino ボードと違い、IDE 上でシリアルポートを選んでからアップロードする必要はありません。
接続状態に関係なくアップロードボタンを押すと、書き込み待ちの CH552 に対して書き込みが始まります。

## agent-key ファームを書き込む

1. Arduino IDE で `src/agentkey/agentkey.ino` を開く
2. ボードを CH552 に設定する
3. `USB Settings` を `Default CDC` など CDC 系に設定する
4. JEWEL1k を書き込みモードで起動する
5. すぐにアップロードボタンを押す
6. 書き込み完了後、USB を抜き差しする

macOS では、CDC として正しく起動すると次のようなポートが出ます。

```sh
ls /dev/cu.*
```

例:

```text
/dev/cu.usbmodemXXXX
```

このポートを指定して agent-key 側から接続します。

```sh
agent-key connect serial /dev/cu.usbmodemXXXX
```

## agent-key 複合デバイスファーム (agentkey_hid) を書き込む

キーボード(QMK/via 互換)のまま agent-key も使いたい場合はこちら。
CDC ポートは出ず、raw HID (usage page 0xFF60) で agent-key と通信します。

1. Arduino IDE で `src/agentkey_hid/agentkey_hid.ino` を開く
2. ボードを CH552 に設定する
3. `USB Settings` を `USER CODE w/266B USB ram` に設定する(1key.ino と同じ)
4. JEWEL1k を書き込みモードで起動し、すぐにアップロードする
5. 書き込み完了後、USB を抜き差しする

接続確認:

```sh
agent-key devices          # "JEWEL1k ... (HID 4249:4287)" が見えること
agent-key connect hid
```

- ボタンはデフォルトで文字を入力しません (キーマップ初期値 KEY_NONE)。
  キーとしても使う場合は https://usevia.app/ で割り当ててください
- ※ このファームは未実機検証です。書き込み後、LED 表示とボタンイベント
  (`agent-key state` で確認) の動作確認が必要です

## 通常の 1キーキーボードファームを書き込む

1. Arduino IDE で `src/1key/1key.ino` を開く
2. ボードを CH552 に設定する
3. `USB Settings` を `USER CODE w/266B USB ram` に設定する
4. JEWEL1k を書き込みモードで起動する
5. すぐにアップロードボタンを押す
6. 書き込み完了後、USB を抜き差しする

通常ファームは HID キーボードとして起動します。CDC シリアルポートは出ません。
