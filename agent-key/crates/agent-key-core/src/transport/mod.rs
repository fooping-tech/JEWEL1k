//! Transport abstraction between the host and the JEWEL1k device.
//!
//! - [`MockTransport`]: no hardware needed; logs outgoing LED packets and
//!   lets tests / the localhost API inject button events.
//! - `SerialTransport` (feature `serial`): USB CDC serial to the CH552E.
//! - `HidRawTransport` (feature `hid`): QMK raw-HID compatible vendor
//!   interface of the composite keyboard firmware.

#[cfg(feature = "hid")]
mod hid;
mod mock;
#[cfg(feature = "serial")]
mod serial;

#[cfg(feature = "hid")]
pub use hid::{list_hid_devices, HidRawTransport};
pub use mock::MockTransport;
#[cfg(feature = "serial")]
pub use serial::SerialTransport;

use crate::protocol::HostPacket;
use crate::types::{DeviceEvent, DeviceInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    Mock,
    Serial,
    Hid,
}

impl TransportKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TransportKind::Mock => "mock",
            TransportKind::Serial => "serial",
            TransportKind::Hid => "hid",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("device not connected")]
    NotConnected,
    #[error("operation not supported by this transport")]
    Unsupported,
    #[error("io error: {0}")]
    Io(String),
    #[error("device not found: {0}")]
    NotFound(String),
}

/// A bidirectional link to one JEWEL1k device. Implementations must be
/// non-blocking: `poll_event` returns immediately with `None` when nothing
/// is pending.
pub trait Transport: Send {
    fn kind(&self) -> TransportKind;

    /// Send one host packet (LED state) to the device.
    fn send_packet(&mut self, packet: &HostPacket) -> Result<(), TransportError>;

    /// Non-blocking read of the next device event, if any.
    fn poll_event(&mut self) -> Result<Option<DeviceEvent>, TransportError>;

    fn is_connected(&self) -> bool;

    fn close(&mut self) {}

    /// Inject a synthetic device event (mock only).
    fn inject_event(&mut self, _event: DeviceEvent) -> Result<(), TransportError> {
        Err(TransportError::Unsupported)
    }

    /// The last packet written, when the transport keeps a log (mock only).
    fn last_packet(&self) -> Option<HostPacket> {
        None
    }
}

/// Raw-HID backend metadata. Implementations speak the same
/// [`HostPacket`]/[`DeviceEvent`] protocol over 32-byte HID reports so the
/// device can stay a composite keyboard+vendor-HID without a CDC port
/// (see `HidRawTransport`, feature `hid`).
pub trait HidTransport: Transport {
    fn vendor_id(&self) -> u16;
    fn product_id(&self) -> u16;
    /// HID usage page of the vendor collection (e.g. 0xFF60 like QMK raw HID).
    fn usage_page(&self) -> u16;
}

/// Enumerate connectable devices. The mock device is always present; serial
/// ports / HID interfaces are listed when the matching feature is enabled.
pub fn list_devices() -> Vec<DeviceInfo> {
    #[allow(unused_mut)]
    let mut devices = vec![DeviceInfo {
        id: "mock".into(),
        name: "Mock JEWEL1k (no hardware)".into(),
        transport: "mock".into(),
        port: None,
    }];
    #[cfg(feature = "serial")]
    devices.extend(serial::list_serial_devices());
    #[cfg(feature = "hid")]
    devices.extend(hid::list_hid_devices());
    devices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_device_is_always_listed() {
        let devices = list_devices();
        assert!(devices.iter().any(|d| d.id == "mock"));
    }
}
