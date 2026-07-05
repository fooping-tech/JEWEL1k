#ifndef __VIA_H__
#define __VIA_H__

#include <stdint.h>

void raw_hid_receive(void);
void via_process(void);
// uint16_t dynamic_keymap_get_keycode(uint8_t layer, uint8_t row, uint8_t col);
void press_qmk_key(__data uint8_t row, __xdata uint8_t col,
                   __xdata uint8_t layer, __xdata uint8_t press);

// ---- agent-key extension (docs/PROTOCOL.md) ----------------------------
// Host packets `A1 state risk pattern brightness checksum` arrive on the
// same raw-HID (via) interface, zero-padded to the 32-byte report.
// This hook is implemented in agentkey_hid.ino.
void agentkey_handle_host_packet(uint8_t state, uint8_t risk, uint8_t pattern,
                                 uint8_t brightness);
// Send a `B1 event checksum` report to the host (zero-padded to 32 bytes).
void agentkey_send_event(uint8_t ev);

#endif