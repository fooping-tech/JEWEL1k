//! HidRawTransport: QMK raw-HID compatible link to the composite firmware
//! (src/agentkey_hid/agentkey_hid.ino). The device stays a keyboard (via/QMK
//! remappable) and exposes a vendor HID interface (usage page 0xFF60, usage
//! 0x61) carrying the same A1/B1 protocol in fixed 32-byte reports:
//!
//! - host -> device: report = `A1 state risk pattern brightness checksum` +
//!   zero padding to 32 bytes
//! - device -> host: report = `B1 event checksum` + zero padding. Reports not
//!   starting with `B1` are via/QMK traffic and are ignored.

use super::{HidTransport, Transport, TransportError, TransportKind};
use crate::protocol::{HostPacket, DEVICE_HEADER};
use crate::types::{DeviceEvent, DeviceInfo};

/// USB identity shared with the keyboard firmware (see keyboardConfig.h).
pub const USB_VID: u16 = 0x4249;
pub const USB_PID: u16 = 0x4287;
/// Vendor collection, QMK raw-HID compatible.
pub const RAW_USAGE_PAGE: u16 = 0xFF60;
pub const RAW_USAGE: u16 = 0x61;
/// Fixed report payload length (no report IDs).
pub const REPORT_LEN: usize = 32;

pub struct HidRawTransport {
    device: hidapi::HidDevice,
    connected: bool,
    path: String,
    pending: std::collections::VecDeque<DeviceEvent>,
}

fn api() -> Result<hidapi::HidApi, TransportError> {
    hidapi::HidApi::new().map_err(|e| TransportError::Io(e.to_string()))
}

/// True when this HID interface is the JEWEL1k vendor (raw HID) collection.
/// Usage page/usage are not exposed on every platform; fall back to VID/PID
/// and let the protocol's checksums reject a wrong interface.
fn is_agent_key_interface(info: &hidapi::DeviceInfo) -> bool {
    info.vendor_id() == USB_VID
        && info.product_id() == USB_PID
        && ((info.usage_page() == RAW_USAGE_PAGE && info.usage() == RAW_USAGE)
            || (info.usage_page() == 0 && info.usage() == 0))
}

impl HidRawTransport {
    /// Open the vendor HID interface. `path` is the platform device path from
    /// [`list_hid_devices`]; `None` picks the first matching device.
    pub fn open(path: Option<&str>) -> Result<Self, TransportError> {
        let api = api()?;
        let info = api
            .device_list()
            .find(|d| match path {
                Some(p) => d.path().to_string_lossy() == p,
                None => is_agent_key_interface(d),
            })
            .ok_or_else(|| {
                TransportError::NotFound(
                    path.map(str::to_string)
                        .unwrap_or_else(|| format!("HID {USB_VID:04x}:{USB_PID:04x}")),
                )
            })?;
        let device = info
            .open_device(&api)
            .map_err(|e| TransportError::Io(e.to_string()))?;
        device
            .set_blocking_mode(false)
            .map_err(|e| TransportError::Io(e.to_string()))?;
        Ok(Self {
            device,
            connected: true,
            path: info.path().to_string_lossy().into_owned(),
            pending: std::collections::VecDeque::new(),
        })
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

impl Transport for HidRawTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Hid
    }

    fn send_packet(&mut self, packet: &HostPacket) -> Result<(), TransportError> {
        if !self.connected {
            return Err(TransportError::NotConnected);
        }
        // byte 0: report ID (none -> 0x00), then the fixed 32-byte report.
        let mut buf = [0u8; REPORT_LEN + 1];
        buf[1..7].copy_from_slice(&packet.encode());
        self.device.write(&buf).map_err(|e| {
            self.connected = false;
            TransportError::Io(e.to_string())
        })?;
        log::debug!("[hid->{}] {:02X?}", self.path, &buf[1..7]);
        Ok(())
    }

    fn poll_event(&mut self) -> Result<Option<DeviceEvent>, TransportError> {
        if !self.connected {
            return Err(TransportError::NotConnected);
        }
        if let Some(ev) = self.pending.pop_front() {
            return Ok(Some(ev));
        }
        let mut buf = [0u8; REPORT_LEN];
        loop {
            match self.device.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    // One report = at most one frame; via/QMK responses (first
                    // byte != B1) and corrupt frames are skipped.
                    if n >= 3 && buf[0] == DEVICE_HEADER && buf[0] ^ buf[1] == buf[2] {
                        if let Some(ev) = DeviceEvent::from_byte(buf[1]) {
                            self.pending.push_back(ev);
                        }
                    }
                }
                Err(e) => {
                    self.connected = false;
                    return Err(TransportError::Io(e.to_string()));
                }
            }
        }
        Ok(self.pending.pop_front())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn close(&mut self) {
        self.connected = false;
    }
}

impl HidTransport for HidRawTransport {
    fn vendor_id(&self) -> u16 {
        USB_VID
    }

    fn product_id(&self) -> u16 {
        USB_PID
    }

    fn usage_page(&self) -> u16 {
        RAW_USAGE_PAGE
    }
}

/// Enumerate JEWEL1k vendor HID interfaces.
pub fn list_hid_devices() -> Vec<DeviceInfo> {
    let Ok(api) = api() else {
        return Vec::new();
    };
    api.device_list()
        .filter(|d| is_agent_key_interface(d))
        .map(|d| {
            let path = d.path().to_string_lossy().into_owned();
            DeviceInfo {
                id: path.clone(),
                name: format!(
                    "{} (HID {:04x}:{:04x})",
                    d.product_string().unwrap_or("JEWEL1k"),
                    d.vendor_id(),
                    d.product_id()
                ),
                transport: "hid".into(),
                port: Some(path),
            }
        })
        .collect()
}
