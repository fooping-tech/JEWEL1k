# agent-key ワイヤプロトコル (PROTOCOL.md)

JEWEL1k (CH552E) とホスト間のバイナリプロトコル。トランスポートは2種類あり、
どちらも同じ A1/B1 フレームを運ぶ:

- **USB CDC シリアル** 115200bps 8N1 (`src/agentkey/agentkey.ino`)
- **raw HID** (usage page 0xFF60, usage 0x61, 32byteレポート) —
  キーボード複合デバイス版 (`src/agentkey_hid/agentkey_hid.ino`)

実装: `agent-key/crates/agent-key-core/src/protocol.rs` /
`transport/serial.rs` / `transport/hid.rs`

## PC -> JEWEL1k: LEDコマンド (6 bytes)

```
+------+-------+------+---------+------------+----------+
| 0xA1 | state | risk | pattern | brightness | checksum |
+------+-------+------+---------+------------+----------+
```

- `checksum` = 先行5バイトの XOR
- 不正な checksum のフレームはデバイス側で破棄(応答なし)
- デバイスはヘッダ `0xA1` で再同期する

### state (色はデバイス側でこの表から決定)

| 値 | state           | 色            |
|----|-----------------|---------------|
| 0  | idle            | 薄い白 (40,40,40) |
| 1  | thinking        | 青 (0,60,255)  |
| 2  | tool_running    | 黄 (255,180,0) |
| 3  | done            | 緑 (0,255,60)  |
| 4  | needs_approval  | 赤 (255,0,0)   |
| 5  | error           | 赤 (255,0,30)  |
| 6  | off             | 消灯           |

### risk

| 値 | risk     |
|----|----------|
| 0  | none     |
| 1  | low      |
| 2  | medium   |
| 3  | high     |
| 4  | critical |

risk は現状 LED 表現に直接は使われない(ホスト側 led_policy が pattern に反映済み)が、
将来のファーム拡張(リスク別演出)のために送信する。

### pattern

| 値 | pattern      | 説明                                   |
|----|--------------|----------------------------------------|
| 0  | off          | 消灯                                   |
| 1  | solid        | 常時点灯                               |
| 2  | breath       | 2.4s周期の三角波ブリージング(最小40)   |
| 3  | blink        | 1Hz 点滅                               |
| 4  | double_blink | 120ms x2 フラッシュ + 休止 (周期1.2s)  |
| 5  | fast_blink   | 5Hz 点滅                               |

### brightness

0-255 のマスタ輝度。デバイスは `色 × パターンエンベロープ × brightness` で出力する。

## JEWEL1k -> PC: イベント (3 bytes)

```
+------+-------+----------+
| 0xB1 | event | checksum |
+------+-------+----------+
```

- `checksum` = `0xB1 ^ event`

### event

| 値   | イベント  | 意味                                             |
|------|-----------|--------------------------------------------------|
| 0x01 | single    | 単押し (<800ms、350msのダブル判定後に確定) = 承認 |
| 0x02 | double    | ダブル押し (350ms以内に2回) = 詳細表示 / 2クリック |
| 0x03 | long      | 長押し (>=800ms、離した時に送信) = 拒否           |
| 0x04 | very_long | 超長押し (>=3000ms、**押下中に即時送信**) = 緊急停止 |
| 0x05 | down      | 生イベント: 押下                                 |
| 0x06 | up        | 生イベント: 解放                                 |
| 0x10 | ready     | 起動完了通知 (ホストは現在のLED状態を再送する)    |

## ジェスチャと承認ルールの関係

ファームウェアはジェスチャ分類まで、**承認判定はホスト(Rust `ApprovalQueue`)のみ**が行う。

- high リスクの「5秒以内に2クリック」は、単押し2回(それぞれ `single`)でも
  ダブル押し1回(`double` = 2クリック分)でもよい。
- ダブル押しの判定ウィンドウ(350ms)は 2クリック承認のウィンドウ(5s)より
  十分短いため衝突しない。
- 承認待ちがない時の `double` はホスト側で「詳細表示」イベントとして扱われる。

## シリアル設定

- 115200 bps / 8N1 (CDC のため実際は仮想)
- ホスト側は 10ms タイムアウトのノンブロッキング読み出し + 逐次 `Decoder`
- ホットプラグ: I/O エラー時に transport を切断し `device-disconnected` を emit

## raw HID トランスポート (複合デバイスファームウェア)

`src/agentkey_hid/agentkey_hid.ino` は キーボードHID + vendor HID
(QMK raw HID 互換: usage page `0xFF60`, usage `0x61`, 32byte固定レポート) の
複合デバイス。USB VID/PID は `0x4249:0x4287` (keyboardConfig.h)。

- **PC -> JEWEL1k**: レポート先頭6バイトに上記 A1 フレーム、残りは 0 埋め。
  via コマンドと同じ OUT レポートに載る(コマンドID `0xA1` は via の
  0x01..0x12 / 0xFF と衝突しない)。checksum 不正は無応答で破棄
- **JEWEL1k -> PC**: レポート先頭3バイトに B1 フレーム、残りは 0 埋め。
  ホスト側 (`HidRawTransport`) は先頭が `0xB1` でないレポート
  (= via/QMK トラフィック) を無視する
- via クライアント (usevia.app) と同時使用可能だが、ボタンイベントは
  非同期 IN レポートとして混在する点に注意
- ボタンは via キーマップのキー入力としても動作する(デフォルトは
  KEY_NONE 無入力。usevia.app で任意に割り当て可能)
