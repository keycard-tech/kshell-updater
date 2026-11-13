use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const SHELL_VENDOR_ID: u16 = 0x1209;
pub const SHELL_PRODUCT_IDS: [u16; 2] = [0x0001, 0x21f7];

const PACKET_SIZE: usize = 64;
const BLOCK_SIZE: usize = PACKET_SIZE - 5;
const TAG: u8 = 0x05;

const CLA: u8 = 0xe0;
const INS_GET_APP_CONFIGURATION: u8 = 0x06;
const INS_GET_RESPONSE: u8 = 0xc0;

/// 56 payload + 2 status word; firmware 1.5 and later append variant and beta flags.
const APP_CONFIG_LEN: usize = 58;
const APP_CONFIG_EXTENDED_LEN: usize = 60;

#[derive(Debug, Error)]
pub enum ShellError {
    #[error("could not talk to the USB subsystem: {0}")]
    Usb(String),
    #[error("could not open the Keycard Shell: {0}")]
    Open(String),
    #[error("the Shell did not answer in time")]
    Timeout,
    #[error("the Shell sent an unexpected packet ({0})")]
    Framing(&'static str),
    #[error("the Shell rejected the command (SW {0})")]
    StatusWord(String),
    #[error("the Shell answered with {actual} bytes, expected {expected}")]
    Malformed { actual: usize, expected: usize },
}

impl From<hidapi::HidError> for ShellError {
    fn from(error: hidapi::HidError) -> Self {
        ShellError::Usb(error.to_string())
    }
}

type Result<T> = std::result::Result<T, ShellError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub product_name: String,
    pub usb_product_id: u16,
    /// The path HID uses to open the device; the UI hands it back unchanged.
    pub path: String,
    pub serial_number: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfiguration {
    pub fw_version: String,
    pub db_version: u32,
    pub serial_number: String,
    pub public_key: String,
    pub fw_variant: Option<u8>,
    pub fw_beta: Option<bool>,
}

fn is_shell(device: &hidapi::DeviceInfo) -> bool {
    device.vendor_id() == SHELL_VENDOR_ID && SHELL_PRODUCT_IDS.contains(&device.product_id())
}

pub fn list_devices(api: &hidapi::HidApi) -> Vec<DeviceInfo> {
    let mut devices: Vec<DeviceInfo> = api
        .device_list()
        .filter(|device| is_shell(device))
        .map(|device| DeviceInfo {
            product_name: device
                .product_string()
                .unwrap_or("Keycard Shell")
                .to_string(),
            usb_product_id: device.product_id(),
            path: device.path().to_string_lossy().into_owned(),
            serial_number: device.serial_number().map(str::to_string),
        })
        .collect();
    devices.sort_by(|a, b| a.path.cmp(&b.path));
    devices
}

/// Ledger-style framing: a 5 byte header (channel, tag, sequence) then 59 bytes of payload.
/// The whole APDU is prefixed with its own length, and the last block is zero padded.
fn write_block(buf: &mut [u8; PACKET_SIZE], channel: u16, sequence: u16, payload: &[u8]) {
    buf[0..2].copy_from_slice(&channel.to_be_bytes());
    buf[2] = TAG;
    buf[3..5].copy_from_slice(&sequence.to_be_bytes());
    buf[5..5 + payload.len()].copy_from_slice(payload);
}

/// Must stay byte-identical to `hidFraming(channel, 64).makeBlocks` in @choppu/shelljs.
pub fn request_blocks(channel: u16, apdu: &[u8]) -> Vec<[u8; PACKET_SIZE]> {
    let mut payload = Vec::with_capacity(apdu.len() + 2);
    payload.extend_from_slice(&(apdu.len() as u16).to_be_bytes());
    payload.extend_from_slice(apdu);

    (0..payload.len().div_ceil(BLOCK_SIZE))
        .map(|sequence| {
            let start = sequence * BLOCK_SIZE;
            let chunk = &payload[start..start + BLOCK_SIZE.min(payload.len() - start)];
            let mut packet = [0u8; PACKET_SIZE];
            write_block(&mut packet, channel, sequence as u16, chunk);
            packet
        })
        .collect()
}

struct Reassembler {
    channel: u16,
    sequence: u16,
    expected: usize,
    data: Vec<u8>,
}

impl Reassembler {
    /// The first packet carries the total length inside the payload, later ones use the whole
    /// block after the header.
    fn push(&mut self, packet: &[u8]) -> Result<Option<Vec<u8>>> {
        if packet.len() < 7 {
            return Err(ShellError::Framing("short packet"));
        }
        if u16::from_be_bytes([packet[0], packet[1]]) != self.channel {
            return Err(ShellError::Framing("channel mismatch"));
        }
        if packet[2] != TAG {
            return Err(ShellError::Framing("tag mismatch"));
        }
        if u16::from_be_bytes([packet[3], packet[4]]) != self.sequence {
            return Err(ShellError::Framing("sequence mismatch"));
        }
        let header = if self.sequence == 0 { 7 } else { 5 };
        if self.sequence == 0 {
            self.expected = u16::from_be_bytes([packet[5], packet[6]]) as usize;
        }
        self.data.extend_from_slice(&packet[header..]);
        self.sequence += 1;
        // The tail of the last packet is zero padding, so reaching the length ends the response
        // rather than exceeding it.
        if self.data.len() >= self.expected {
            self.data.truncate(self.expected);
            return Ok(Some(std::mem::take(&mut self.data)));
        }
        Ok(None)
    }
}

pub struct Shell {
    device: hidapi::HidDevice,
    channel: u16,
}

impl Shell {
    pub fn open(api: &hidapi::HidApi, path: &str) -> Result<Self> {
        let c_path = std::ffi::CString::new(path)
            .map_err(|_| ShellError::Open("invalid device path".into()))?;
        let device = api
            .open_path(&c_path)
            .map_err(|e| ShellError::Open(e.to_string()))?;
        // A random channel makes stale responses from a previous session identifiable.
        let channel = (std::process::id() as u16) ^ 0x5cb0;
        Ok(Self { device, channel })
    }

    pub fn exchange(&self, apdu: &[u8]) -> Result<Vec<u8>> {
        for packet in request_blocks(self.channel, apdu) {
            // The leading zero is the report id the HID report carries, not part of the frame.
            let mut out = Vec::with_capacity(PACKET_SIZE + 1);
            out.push(0);
            out.extend_from_slice(&packet);
            self.device
                .write(&out)
                .map_err(|e| ShellError::Open(e.to_string()))?;
        }

        let mut reassembler = Reassembler {
            channel: self.channel,
            sequence: 0,
            expected: 0,
            data: Vec::new(),
        };
        let mut buf = [0u8; PACKET_SIZE];
        loop {
            let read = self
                .device
                .read_timeout(&mut buf, 5_000)
                .map_err(|e| ShellError::Open(e.to_string()))?;
            if read == 0 {
                return Err(ShellError::Timeout);
            }
            if let Some(data) = reassembler.push(&buf[..read])? {
                return self.finish(data);
            }
        }
    }

    /// A short status word with no payload means "ask again", the answer comes from INS_GET_RESPONSE.
    fn finish(&self, data: Vec<u8>) -> Result<Vec<u8>> {
        if data.len() < 2 {
            return Err(ShellError::Malformed {
                actual: data.len(),
                expected: 2,
            });
        }
        let sw = u16::from_be_bytes([data[data.len() - 2], data[data.len() - 1]]);
        if sw == 0x9000 {
            return Ok(data);
        }
        if sw == 0x6100 {
            return self.exchange(&[CLA, INS_GET_RESPONSE, 0x00, 0x00]);
        }
        Err(ShellError::StatusWord(format!("{sw:04x}")))
    }

    pub fn get_app_configuration(&self) -> Result<AppConfiguration> {
        let response = self.exchange(&[CLA, INS_GET_APP_CONFIGURATION, 0x00, 0x00])?;
        parse_app_configuration(&response)
    }
}

/// Offsets are fixed by the firmware: version 0..3, database 3..7, serial 7..23, key 23..56,
/// and firmware 1.5 appends variant and beta flags. The status word stays in the buffer unread,
/// which is what shelljs does.
fn parse_app_configuration(response: &[u8]) -> Result<AppConfiguration> {
    let extended = match response.len() {
        APP_CONFIG_LEN => false,
        APP_CONFIG_EXTENDED_LEN => true,
        actual => {
            return Err(ShellError::Malformed {
                actual,
                expected: APP_CONFIG_LEN,
            })
        }
    };
    Ok(AppConfiguration {
        fw_version: format!("{}.{}.{}", response[0], response[1], response[2]),
        db_version: u32::from_be_bytes([response[3], response[4], response[5], response[6]]),
        serial_number: hex(&response[7..23]),
        public_key: hex(&response[23..56]),
        fw_variant: extended.then(|| response[56]),
        fw_beta: extended.then(|| response[57] != 0),
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte-for-byte reference from @choppu/shelljs 2.0.0:
    /// `hidFraming(0x5cb0, 64).makeBlocks(Buffer.from([0xe0, 0x06, 0x00, 0x00]))`
    const EXPECTED_SINGLE_BLOCK: &str = "5cb00500000004e00600000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";

    fn reassembler(channel: u16) -> Reassembler {
        Reassembler {
            channel,
            sequence: 0,
            expected: 0,
            data: Vec::new(),
        }
    }

    fn packet(channel: u16, sequence: u16, total: Option<u16>, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![0u8; PACKET_SIZE];
        out[0..2].copy_from_slice(&channel.to_be_bytes());
        out[2] = TAG;
        out[3..5].copy_from_slice(&sequence.to_be_bytes());
        let header = match total {
            Some(len) => {
                out[5..7].copy_from_slice(&len.to_be_bytes());
                7
            }
            None => 5,
        };
        out[header..header + payload.len()].copy_from_slice(payload);
        out
    }

    #[test]
    fn single_block_matches_shelljs() {
        let blocks = request_blocks(0x5cb0, &[0xe0, 0x06, 0x00, 0x00]);
        assert_eq!(blocks.len(), 1);
        assert_eq!(hex(&blocks[0]), EXPECTED_SINGLE_BLOCK);
    }

    #[test]
    fn long_apdu_spans_four_blocks_and_zero_pads() {
        let blocks = request_blocks(0x5cb0, &[0xab; 200]);
        assert_eq!(blocks.len(), 4);
        // 2 byte length prefix plus the APDU, five bytes of header per block.
        assert_eq!(&blocks[0][5..7], &[0x00, 0xc8]);
        assert_eq!(&blocks[1][0..5], &[0x5c, 0xb0, 0x05, 0x00, 0x01]);
        assert_eq!(&blocks[3][5..30], &[0xab; 25]);
        assert_eq!(&blocks[3][30..], &[0u8; 34][..]);
    }

    #[test]
    fn channel_tag_and_sequence_are_big_endian() {
        let blocks = request_blocks(0x1234, &[0xe0, 0x06, 0x00, 0x00]);
        assert_eq!(&blocks[0][0..5], &[0x12, 0x34, 0x05, 0x00, 0x00]);
    }

    #[test]
    fn response_is_reassembled_and_padding_dropped() {
        let mut reassembler = reassembler(0x9000);
        let body: Vec<u8> = (0..57u8).collect();
        assert!(reassembler
            .push(&packet(0x9000, 0, Some(58), &body))
            .unwrap()
            .is_none());

        // One byte completes the 58; the rest of the packet is padding the device does not mean.
        let last = packet(0x9000, 1, None, &[0x42]);
        let data = reassembler.push(&last).unwrap().expect("complete");
        assert_eq!(data.len(), 58);
        assert_eq!(&data[..57], &body[..]);
        assert_eq!(data[57], 0x42);
    }

    #[test]
    fn wrong_sequence_is_an_error() {
        let mut reassembler = reassembler(0x9000);
        let out = reassembler.push(&packet(0x9000, 7, Some(58), &[]));
        assert!(matches!(out, Err(ShellError::Framing("sequence mismatch"))));
    }

    #[test]
    fn foreign_channel_is_an_error() {
        let mut reassembler = reassembler(0x9000);
        let out = reassembler.push(&packet(0x0001, 0, Some(58), &[]));
        assert!(matches!(out, Err(ShellError::Framing("channel mismatch"))));
    }

    #[test]
    fn app_configuration_offsets() {
        let mut raw = vec![0u8; APP_CONFIG_LEN];
        raw[0..3].copy_from_slice(&[1, 4, 0]);
        raw[3..7].copy_from_slice(&20260812u32.to_be_bytes());
        for (i, byte) in raw.iter_mut().enumerate().take(23).skip(7) {
            *byte = i as u8;
        }
        raw[56] = 0x90;
        raw[57] = 0x00;

        let config = parse_app_configuration(&raw).unwrap();
        assert_eq!(config.fw_version, "1.4.0");
        assert_eq!(config.db_version, 20260812);
        assert_eq!(config.serial_number.len(), 32);
        assert_eq!(config.public_key.len(), 66);
        assert_eq!(config.fw_variant, None);
        assert_eq!(config.fw_beta, None);
    }

    #[test]
    fn extended_app_configuration_carries_variant_and_beta() {
        let mut raw = vec![0u8; APP_CONFIG_EXTENDED_LEN];
        raw[0..3].copy_from_slice(&[1, 5, 2]);
        raw[56] = 1;
        raw[57] = 1;
        let config = parse_app_configuration(&raw).unwrap();
        assert_eq!(config.fw_version, "1.5.2");
        assert_eq!(config.fw_variant, Some(1));
        assert_eq!(config.fw_beta, Some(true));
    }

    #[test]
    fn wrong_length_response_is_malformed() {
        assert!(matches!(
            parse_app_configuration(&[0u8; 40]),
            Err(ShellError::Malformed {
                actual: 40,
                expected: APP_CONFIG_LEN
            })
        ));
    }
}
