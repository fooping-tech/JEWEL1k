/**
 * TypeScript bindings for jewel1k-plugin-agent-key (Tauri v2).
 *
 * ```ts
 * import { setStatus, requestApproval, onButtonEvent } from 'jewel1k-plugin-agent-key-api'
 *
 * await setStatus({ state: 'thinking' })
 * const outcome = await requestApproval({ title: 'git push --force', risk: 'high' })
 * const unlisten = await onButtonEvent((e) => console.log(e.gesture))
 * ```
 */
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

// ---- types ---------------------------------------------------------------

export type AgentState =
  | 'idle'
  | 'thinking'
  | 'tool_running'
  | 'done'
  | 'needs_approval'
  | 'error'
  | 'off'

export type RiskLevel = 'none' | 'low' | 'medium' | 'high' | 'critical'

export type ButtonGesture = 'single' | 'double' | 'long' | 'very_long' | 'down' | 'up'

export type Decision =
  | 'approved'
  | 'denied'
  | 'cancelled'
  | 'timed_out'
  | 'emergency_stopped'

export interface StatusUpdate {
  state: AgentState
  risk?: RiskLevel
  /** Free-form context for UI listeners; never sent to the device. */
  message?: string
}

export interface ConnectOptions {
  /** 'mock' (default), 'serial' or 'hid'. */
  transport?: 'mock' | 'serial' | 'hid'
  /** Serial port name ('COM5', '/dev/tty.usbmodem1101') or HID device path.
   *  For 'hid' it may be omitted to auto-pick the first JEWEL1k. */
  port?: string
}

export interface ApprovalRequest {
  /** Leave undefined to let the plugin assign an id. */
  id?: string
  title: string
  description?: string
  risk?: RiskLevel
  /** Auto-timeout in ms (default 60000). */
  timeout_ms?: number
  /** Who is asking, e.g. 'claude-code'. */
  source?: string
}

export interface ApprovalResolution {
  id: string
  decision: Decision
  reason?: string
}

/** Result of requestApproval: pending, or resolved immediately by policy
 *  (critical risk is always denied without asking the device). */
export type ApprovalOutcome =
  | { status: 'pending'; id: string }
  | ({ status: 'resolved' } & ApprovalResolution)

export interface DeviceInfo {
  id: string
  name: string
  transport: 'mock' | 'serial' | 'hid' | string
  port?: string
}

export interface Health {
  /** True when at least one device link is up. */
  connected: boolean
  /** First connected device (kept for backward compatibility). */
  device?: DeviceInfo
  /** All connected devices. */
  devices?: DeviceInfo[]
  /** ms since the last successful device I/O. */
  last_io_ms?: number
}

export interface CurrentState {
  state: AgentState
  risk: RiskLevel
  brightness: number
  connected: boolean
  pending_approval?: ApprovalRequest
}

export interface ButtonEvent {
  gesture: ButtonGesture
  timestamp_ms: number
}

export interface PluginError {
  message: string
}

// ---- commands --------------------------------------------------------------

const cmd = <T>(name: string, args?: Record<string, unknown>): Promise<T> =>
  invoke<T>(`plugin:agent-key|${name}`, args)

/** List connectable devices (the mock device is always present). */
export function listDevices(): Promise<DeviceInfo[]> {
  return cmd('list_devices')
}

/** Connect a transport. Defaults to the mock transport. */
export function connect(options?: ConnectOptions): Promise<DeviceInfo> {
  return cmd('connect', { options })
}

/** Disconnect one device by id, or every device when `id` is omitted. */
export function disconnect(id?: string): Promise<void> {
  return cmd('disconnect', { id })
}

export function getHealth(): Promise<Health> {
  return cmd('get_health')
}

/** Push the agent status shown on the LED, e.g. `setStatus({ state: 'thinking' })`. */
export function setStatus(status: StatusUpdate): Promise<CurrentState> {
  return cmd('set_status', { status })
}

/**
 * Submit an approval request. The decision is made ONLY by the physical
 * button (double press approve, long press deny); a single press never
 * approves. Critical risk resolves as denied immediately.
 */
export function requestApproval(request: ApprovalRequest): Promise<ApprovalOutcome> {
  return cmd('request_approval', { request })
}

export function cancelApproval(id: string): Promise<ApprovalResolution> {
  return cmd('cancel_approval', { id })
}

export function getCurrentState(): Promise<CurrentState> {
  return cmd('get_current_state')
}

/** Master LED brightness, 0-255. */
export function setBrightness(value: number): Promise<CurrentState> {
  return cmd('set_brightness', { value })
}

/** Dev helper (mock transport + allow-simulate permission only). */
export function simulateButton(gesture: ButtonGesture): Promise<void> {
  return cmd('simulate_button', { gesture })
}

// ---- events ---------------------------------------------------------------

export function onButtonEvent(
  callback: (event: ButtonEvent) => void,
): Promise<UnlistenFn> {
  return listen<ButtonEvent>('agent-key://button', (e) => callback(e.payload))
}

export function onStateChanged(
  callback: (state: CurrentState) => void,
): Promise<UnlistenFn> {
  return listen<CurrentState>('agent-key://state-changed', (e) => callback(e.payload))
}

/** Fires for every approval lifecycle change (requested, resolved). */
export function onApprovalChanged(
  callback: (change: {
    kind: 'requested' | 'resolved'
    request?: ApprovalRequest
    resolution?: ApprovalResolution
  }) => void,
): Promise<UnlistenFn> {
  const subs = Promise.all([
    listen<ApprovalRequest>('agent-key://approval-requested', (e) =>
      callback({ kind: 'requested', request: e.payload }),
    ),
    listen<ApprovalResolution>('agent-key://approval-resolved', (e) =>
      callback({ kind: 'resolved', resolution: e.payload }),
    ),
  ])
  return subs.then((fns) => () => fns.forEach((f) => f()))
}

export function onApprovalRequested(
  callback: (request: ApprovalRequest) => void,
): Promise<UnlistenFn> {
  return listen<ApprovalRequest>('agent-key://approval-requested', (e) =>
    callback(e.payload),
  )
}

export function onApprovalResolved(
  callback: (resolution: ApprovalResolution) => void,
): Promise<UnlistenFn> {
  return listen<ApprovalResolution>('agent-key://approval-resolved', (e) =>
    callback(e.payload),
  )
}

export function onDeviceConnected(
  callback: (device: DeviceInfo) => void,
): Promise<UnlistenFn> {
  return listen<DeviceInfo>('agent-key://device-connected', (e) => callback(e.payload))
}

export function onDeviceDisconnected(
  callback: (payload?: { device?: DeviceInfo }) => void,
): Promise<UnlistenFn> {
  return listen<{ device?: DeviceInfo }>('agent-key://device-disconnected', (e) =>
    callback(e.payload),
  )
}

export function onError(callback: (error: PluginError) => void): Promise<UnlistenFn> {
  return listen<PluginError>('agent-key://error', (e) => callback(e.payload))
}
