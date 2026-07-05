//! SerialTransport: USB CDC serial link to the CH552E firmware
//! (src/agentkey/agentkey.ino). 115200 baud 8N1, non-blocking reads with a
//! short timeout, incremental [`Decoder`] for the B1 event stream.

use super::{Transport, TransportError, TransportKind};
use crate::protocol::{Decoder, HostPacket};
use crate::types::{DeviceEvent, DeviceInfo};
use std::io::{Read, Write};
use std::time::Duration;

pub const BAUD_RATE: u32 = 115_200;

pub struct SerialTransport {
    port: Box<dyn serialport::SerialPort>,
    decoder: Decoder,
    connected: bool,
    port_name: String,
    /// Events decoded but not yet handed out by `poll_event`.
    pending: std::collections::VecDeque<DeviceEvent>,
}

impl SerialTransport {
    pub fn open(port_name: &str) -> Result<Self, TransportError> {
        let port = serialport::new(port_name, BAUD_RATE)
            .timeout(Duration::from_millis(10))
            .open()
            .map_err(|e| match e.kind {
                serialport::ErrorKind::NoDevice => {
                    TransportError::NotFound(port_name.to_string())
                }
                _ => TransportError::Io(e.to_string()),
            })?;
        Ok(Self {
            port,
            decoder: Decoder::new(),
            connected: true,
            port_name: port_name.to_string(),
            pending: std::collections::VecDeque::new(),
        })
    }

    pub fn port_name(&self) -> &str {
        &self.port_name
    }
}

impl Transport for SerialTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Serial
    }

    fn send_packet(&mut self, packet: &HostPacket) -> Result<(), TransportError> {
        if !self.connected {
            return Err(TransportError::NotConnected);
        }
        let bytes = packet.encode();
        self.port.write_all(&bytes).map_err(|e| {
            self.connected = false;
            TransportError::Io(e.to_string())
        })?;
        let _ = self.port.flush();
        log::debug!("[serial->{}] {:02X?}", self.port_name, bytes);
        Ok(())
    }

    fn poll_event(&mut self) -> Result<Option<DeviceEvent>, TransportError> {
        if !self.connected {
            return Err(TransportError::NotConnected);
        }
        if let Some(ev) = self.pending.pop_front() {
            return Ok(Some(ev));
        }
        let mut buf = [0u8; 64];
        match self.port.read(&mut buf) {
            Ok(0) => {}
            Ok(n) => {
                self.pending.extend(self.decoder.feed(&buf[..n]));
                if let Some(ev) = self.pending.pop_front() {
                    return Ok(Some(ev));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => {
                self.connected = false;
                return Err(TransportError::Io(e.to_string()));
            }
        }
        Ok(None)
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn close(&mut self) {
        self.connected = false;
    }
}

/// Enumerate serial ports that could be a JEWEL1k. USB details (VID/PID) are
/// included in the name when the OS exposes them.
pub fn list_serial_devices() -> Vec<DeviceInfo> {
    let Ok(ports) = serialport::available_ports() else {
        return Vec::new();
    };
    ports
        .into_iter()
        .map(|p| {
            let name = match &p.port_type {
                serialport::SerialPortType::UsbPort(usb) => format!(
                    "{} (USB {:04x}:{:04x}{})",
                    p.port_name,
                    usb.vid,
                    usb.pid,
                    usb.product
                        .as_deref()
                        .map(|s| format!(" {s}"))
                        .unwrap_or_default()
                ),
                _ => p.port_name.clone(),
            };
            DeviceInfo {
                id: p.port_name.clone(),
                name,
                transport: "serial".into(),
                port: Some(p.port_name),
            }
        })
        .collect()
}
