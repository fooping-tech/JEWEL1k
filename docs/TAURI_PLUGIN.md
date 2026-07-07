# jewel1k-plugin-agent-key 利用ガイド (TAURI_PLUGIN.md)

Tauri v2 アプリ(トレイ常駐アプリなど)に agent-key plugin を組み込む手順。

組み込み済みの実装例として tray 常駐アプリ `agent-key/apps/tray` がある
(トレイメニュー / 承認トースト / 自動再接続 / autostart。`cargo build` は
apps/tray ディレクトリ内で行う。独立 workspace)。

## 1. インストール

`src-tauri/Cargo.toml`:

```toml
[dependencies]
# パッケージ名は tauri-plugin-agent-key (tauri が permission 名前空間
# "agent-key" をパッケージ名から導出するため)。Rust の crate 名は
# jewel1k_plugin_agent_key のまま。
tauri-plugin-agent-key = { path = "../agent-key/plugins/jewel1k-plugin-agent-key" }
# raw HID (複合デバイスファーム) も使う場合:
# tauri-plugin-agent-key = { path = "...", features = ["serial", "hid"] }
```

`src-tauri/src/main.rs`:

```rust
fn main() {
    tauri::Builder::default()
        .plugin(jewel1k_plugin_agent_key::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

frontend(package.json):

```json
{ "dependencies": { "jewel1k-plugin-agent-key-api": "file:../agent-key/plugins/jewel1k-plugin-agent-key" } }
```

## 2. 設定 (tauri.conf.json)

```json
{
  "plugins": {
    "agent-key": {
      "httpEnabled": true,
      "httpPort": 43117,
      "httpToken": null,
      "autoConnect": "mock"
    }
  }
}
```

- `httpEnabled` / `httpPort`: CLI・フック用 localhost API (127.0.0.1のみ)。`0` で空きポート
- `httpToken`: 設定すると `x-agent-key-token` ヘッダを要求
- `autoConnect`: `"mock"` | `"serial:COM5"` | `"hid"` | `"hid:<PATH>"` | `"none"`

## 3. Permissions (capabilities)

`src-tauri/capabilities/default.json` に追加:

```json
{
  "identifier": "main-capability",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "agent-key:default",
    "agent-key:allow-device",
    "agent-key:allow-config"
  ]
}
```

| set              | 含まれるcommand                              | 備考 |
|------------------|----------------------------------------------|------|
| `default`        | set_status / get_current_state / get_health / list_devices / request_approval / cancel_approval | 通常のUIに十分 |
| `allow-status`   | set_status                                   | |
| `allow-read-state` | get_current_state / get_health / list_devices | |
| `allow-device`   | connect / disconnect                         | デバイス管理UIのみに付与 |
| `allow-approval` | request_approval / cancel_approval           | 承認の**要求/取消のみ**。承認決定のAPIは存在しない |
| `allow-config`   | set_brightness                               | |
| `allow-simulate` | simulate_button                              | **開発専用**。疑似クリックで承認が通るため本番では付与禁止 |

## 4. Commands (TypeScript)

```ts
import {
  listDevices, connect, disconnect, getHealth,
  setStatus, requestApproval, cancelApproval,
  getCurrentState, setBrightness,
  onButtonEvent, onStateChanged, onApprovalChanged,
} from 'jewel1k-plugin-agent-key-api'

await connect({ transport: 'serial', port: 'COM5' })   // 省略時 mock
await connect({ transport: 'hid' })                     // 複合デバイスファーム (raw HID)
// connect は複数回呼べる: 同じ id は置き換え、異なる id は同時接続になり
// LED は全デバイスへブロードキャスト、ボタンはどのデバイスからでも有効
await disconnect('COM5')                                // id 指定で1台だけ切断 (省略で全部)
await setStatus({ state: 'thinking' })
await setBrightness(128)

const outcome = await requestApproval({ title: 'git push --force', risk: 'high' })
if (outcome.status === 'resolved' && outcome.decision === 'denied') {
  // critical は即 denied で返る
}

const un1 = await onButtonEvent((e) => console.log('button:', e.gesture))
const un2 = await onStateChanged((s) => console.log('state:', s.state))
const un3 = await onApprovalChanged((c) => console.log('approval:', c.kind))
```

`requestApproval` は **pending の id を返すだけ**で、決定は
`onApprovalChanged`(`kind === 'resolved'`)または localhost API のブロッキング呼び出しで受け取る。

## 5. Events

| event                              | payload |
|------------------------------------|---------|
| `agent-key://button`               | `{ gesture, timestamp_ms }` |
| `agent-key://state-changed`        | `CurrentState` |
| `agent-key://approval-requested`   | `ApprovalRequest` |
| `agent-key://approval-resolved`    | `{ id, decision, reason? }` |
| `agent-key://device-connected`     | `DeviceInfo` |
| `agent-key://device-disconnected`  | `{ device?: DeviceInfo }` |
| `agent-key://error`                | `{ message }` (緊急停止時にも発火) |

## 6. localhost API

CLI (`agent-key`) が使う HTTP API。`docs/HOOKS.md` と `agent-key --help` を参照。

```
GET  /health | /state | /devices    # /health は devices: DeviceInfo[] を含む
POST /connect | /disconnect | /status | /brightness | /simulate
POST /disconnect {"id": "COM5"}     # id 指定で1台だけ切断 (body 省略で全部)
POST /approval            # 解決までブロック (?wait=false で即時リターン)
POST /approval/<id>/cancel
```

## 7. Rust 側から使う (tray app など)

```rust
use jewel1k_plugin_agent_key::AgentKey;
use tauri::Manager;

let agent_key = app.state::<AgentKey>();
agent_key.0.set_status(agent_key_core::StatusUpdate {
    state: agent_key_core::types::AgentState::Done,
    risk: None,
    message: None,
})?;
```

## 8. 開発フロー

1. `autoConnect: "mock"` のままアプリを起動(実機不要)
2. `agent-key simulate double` などでボタンを疑似発火(承認はダブル押し)
3. 実機を繋いだら `agent-key connect serial COM5`(CDC版ファーム)または
   `agent-key connect hid`(複合デバイス版ファーム。`listDevices()` から選択も可)
4. ファームウェア書き込みは [FIRMWARE_FLASHING.md](FIRMWARE_FLASHING.md) を参照
5. 常用するなら tray app (`agent-key/apps/tray`): シリアル→HID の順に
   自動検出・再接続し、承認要求をトースト通知する
