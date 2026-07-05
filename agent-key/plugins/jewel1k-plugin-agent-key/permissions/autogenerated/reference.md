## Default Permission

Default permissions for the agent-key plugin: push status updates, read
state/health and manage approval requests. Device control, brightness and
button simulation must be granted explicitly.

#### This default permission set includes the following:

- `allow-set-status`
- `allow-get-current-state`
- `allow-get-health`
- `allow-list-devices`
- `allow-request-approval`
- `allow-cancel-approval`

## Permission Table

<table>
<tr>
<th>Identifier</th>
<th>Description</th>
</tr>


<tr>
<td>

`jewel1k-plugin-agent-key:allow-cancel-approval`

</td>
<td>

Enables the cancel_approval command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:deny-cancel-approval`

</td>
<td>

Denies the cancel_approval command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:allow-connect`

</td>
<td>

Enables the connect command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:deny-connect`

</td>
<td>

Denies the connect command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:allow-disconnect`

</td>
<td>

Enables the disconnect command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:deny-disconnect`

</td>
<td>

Denies the disconnect command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:allow-get-current-state`

</td>
<td>

Enables the get_current_state command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:deny-get-current-state`

</td>
<td>

Denies the get_current_state command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:allow-get-health`

</td>
<td>

Enables the get_health command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:deny-get-health`

</td>
<td>

Denies the get_health command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:allow-list-devices`

</td>
<td>

Enables the list_devices command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:deny-list-devices`

</td>
<td>

Denies the list_devices command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:allow-request-approval`

</td>
<td>

Enables the request_approval command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:deny-request-approval`

</td>
<td>

Denies the request_approval command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:allow-set-brightness`

</td>
<td>

Enables the set_brightness command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:deny-set-brightness`

</td>
<td>

Denies the set_brightness command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:allow-set-status`

</td>
<td>

Enables the set_status command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:deny-set-status`

</td>
<td>

Denies the set_status command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:allow-simulate-button`

</td>
<td>

Enables the simulate_button command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:deny-simulate-button`

</td>
<td>

Denies the simulate_button command without any pre-configured scope.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:allow-status`

</td>
<td>

Push agent status updates to the device LED.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:allow-read-state`

</td>
<td>

Read current state, health and the device list.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:allow-device`

</td>
<td>

Connect and disconnect devices. Grant only to trusted device-management UI.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:allow-approval`

</td>
<td>

Submit and cancel approval requests (never resolve them).

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:allow-config`

</td>
<td>

Change device configuration such as LED brightness.

</td>
</tr>

<tr>
<td>

`jewel1k-plugin-agent-key:allow-simulate`

</td>
<td>

DANGER (dev only): inject synthetic button gestures into the mock transport. A synthetic click can approve a pending request — never grant in production builds.

</td>
</tr>
</table>
