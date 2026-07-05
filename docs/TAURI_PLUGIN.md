# jewel1k-plugin-agent-key 利用ガイド (TAURI_PLUGIN.md)

Tauri v2 アプリ(トレイ常駐アプリなど)に agent-key plugin を組み込む手順。

## 1. インストール

`src-tauri/Cargo.toml`:

```toml
[dependencies]
jewel1k-plugin-agent-key = { path = "../agent-key/plugins/jewel1k-plugin-agent-key" }
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
- `autoConnect`: `"mock"` | `"serial:COM5"` | `"none"`

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
| `agent-key://approval-progress`    | `{ id, clicks, required }` (high risk の1クリック目など) |
| `agent-key://approval-resolved`    | `{ id, decision, reason? }` |
| `agent-key://device-connected`     | `DeviceInfo` |
| `agent-key://device-disconnected`  | `{}` |
| `agent-key://error`                | `{ message }` (緊急停止時にも発火) |

## 6. localhost API

CLI (`agent-key`) が使う HTTP API。`docs/HOOKS.md` と `agent-key --help` を参照。

```
GET  /health | /state | /devices
POST /connect | /disconnect | /status | /brightness | /simulate
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
2. `agent-key simulate single` などでボタンを疑似発火
3. 実機を繋いだら `agent-key connect serial COM5`(または `listDevices()` から選択)
4. ファームウェア書き込みは [FIRMWARE_FLASHING.md](FIRMWARE_FLASHING.md) を参照
