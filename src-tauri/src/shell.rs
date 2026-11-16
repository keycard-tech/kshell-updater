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
/// Flashing: both take the image in the same chunked form `load_block` builds.
pub const INS_LOAD_FIRMWARE: u8 = 0xf2;
pub const INS_LOAD_DATABASE: u8 = 0xf4;
const LOAD_CHUNK: usize = 240;
/// The flash declined on the device's own screen.
pub const SW_USER_DECLINED: u16 = 0x6982;
/// A command answers in milliseconds; a chunk write erases flash.
const READ_TIMEOUT: i32 = 5_000;
const LOAD_TIMEOUT: i32 = 30_000;

/// One chunk of a `load`. Only the first carries the image's total length, and that header counts
/// towards the command's own length byte.
pub fn load_block(ins: u8, total: usize, offset: usize, data: &[u8]) -> Vec<u8> {
    let chunk = &data[offset..(offset + LOAD_CHUNK).min(data.len())];
    let header = if offset == 0 {
        (total as u32).to_be_bytes().to_vec()
    } else {
        Vec::new()
    };
    let mut apdu = Vec::with_capacity(chunk.len() + header.len() + 5);
    apdu.extend_from_slice(&[CLA, ins, if offset == 0 { 0x00 } else { 0x80 }, 0x00]);
    // Covers the total-length header too.
    apdu.push((header.len() + chunk.len()) as u8);
    apdu.extend_from_slice(&header);
    apdu.extend_from_slice(chunk);
    apdu
}

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
    /// The number rather than its text: 0x6982 is a decline on the device, shown differently from a
    /// genuine rejection.
    #[error("the Shell rejected the command (SW {0:#06x})")]
    StatusWord(u16),
    #[error("the transfer was cancelled")]
    Cancelled,
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
    /// The path HID uses to open the device; the UI hands it back unchanged.
    pub path: String,
    pub serial_number: Option<String>,
}

/// The one device to talk to. Commands take the first, because a second interface on the same Shell
/// answers the same APDUs and the person only ever updates one.
pub fn only_device(api: &mut hidapi::HidApi) -> Option<DeviceInfo> {
    list_devices(api).into_iter().next()
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

/// hidapi caches the bus as it looked when the context was made, so the list must be re-read:
/// otherwise a Shell plugged in after start-up - or powered on, which is how it appears - stays
/// invisible until the app restarts.
pub fn list_devices(api: &mut hidapi::HidApi) -> Vec<DeviceInfo> {
    let _ = api.refresh_devices();
    let mut devices: Vec<DeviceInfo> = api
        .device_list()
        .filter(|device| is_shell(device))
        .map(|device| DeviceInfo {
            product_name: device
                .product_string()
                .unwrap_or("Keycard Shell")
                .to_string(),
            path: device.path().to_string_lossy().into_owned(),
            serial_number: device.serial_number().map(str::to_string),
        })
        .collect();
    devices.sort_by(|a, b| a.path.cmp(&b.path));
    devices
}

/// Ledger framing: 5 byte header (channel, tag, sequence) then 59 bytes of payload. The APDU is
/// prefixed with its own length and the last block is zero padded.
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
    /// The first packet carries the total length; later ones use the whole block.
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
        // The tail of the last packet is padding, so reaching the length ends the response.
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
        // A fixed-ish channel makes a stale response from a previous session identifiable.
        let channel = (std::process::id() as u16) ^ 0x5cb0;
        Ok(Self { device, channel })
    }

    pub fn exchange(&self, apdu: &[u8]) -> Result<Vec<u8>> {
        self.exchange_with(apdu, READ_TIMEOUT)
    }

    fn exchange_with(&self, apdu: &[u8], timeout: i32) -> Result<Vec<u8>> {
        for packet in request_blocks(self.channel, apdu) {
            // The leading zero is the HID report id, not part of the frame.
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
                .read_timeout(&mut buf, timeout)
                .map_err(|e| ShellError::Open(e.to_string()))?;
            if read == 0 {
                return Err(ShellError::Timeout);
            }
            if let Some(data) = reassembler.push(&buf[..read])? {
                return self.finish(data);
            }
        }
    }

    /// 0x6100 means "ask again", and the answer comes from INS_GET_RESPONSE.
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
        Err(ShellError::StatusWord(sw))
    }

    /// Write an image. Any status but 0x9000 stops the transfer rather than flashing an image the
    /// device has refused. `should_stop` is checked between chunks, because a part-written firmware
    /// is what a cancel mid-chunk leaves behind.
    pub fn load(
        &self,
        ins: u8,
        data: &[u8],
        mut progress: impl FnMut(usize),
        mut should_stop: impl FnMut() -> bool,
    ) -> Result<()> {
        for offset in (0..data.len()).step_by(LOAD_CHUNK) {
            if should_stop() {
                return Err(ShellError::Cancelled);
            }
            self.exchange_with(&load_block(ins, data.len(), offset, data), LOAD_TIMEOUT)?;
            progress((offset + LOAD_CHUNK).min(data.len()));
        }
        Ok(())
    }

    pub fn get_app_configuration(&self) -> Result<AppConfiguration> {
        let response = self.exchange(&[CLA, INS_GET_APP_CONFIGURATION, 0x00, 0x00])?;
        parse_app_configuration(&response)
    }
}

/// Offsets are fixed by the firmware: version 0..3, database 3..7, serial 7..23, key 23..56, with
/// variant and beta appended by firmware 1.5. The status word stays in the buffer unread, as shelljs
/// leaves it.
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
    crate::image::hex(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte-for-byte reference from @choppu/shelljs 2.0.0:
    /// `hidFraming(0x5cb0, 64).makeBlocks(Buffer.from([0xe0, 0x06, 0x00, 0x00]))`
    const EXPECTED_SINGLE_BLOCK: &str = "5cb00500000004e00600000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";

    #[test]
    fn only_the_first_load_chunk_carries_the_length() {
        let data: Vec<u8> = (0..500u16).map(|i| (i % 251) as u8).collect();
        let first = load_block(INS_LOAD_FIRMWARE, data.len(), 0, &data);
        assert_eq!(&first[..4], &[0xe0, 0xf2, 0x00, 0x00]);
        assert_eq!(
            first[4] as usize,
            4 + 240,
            "the length byte covers its own header"
        );
        assert_eq!(&first[5..9], &500u32.to_be_bytes()[..]);
        assert_eq!(&first[9..13], &data[..4]);

        let second = load_block(INS_LOAD_FIRMWARE, data.len(), 240, &data);
        assert_eq!(&second[..4], &[0xe0, 0xf2, 0x80, 0x00]);
        assert_eq!(second[4] as usize, 240);
        assert_eq!(&second[5..8], &data[240..243]);

        let last = load_block(INS_LOAD_FIRMWARE, data.len(), 480, &data);
        assert_eq!(last[4] as usize, 20, "the tail chunk is short");
        assert_eq!(&last[5..], &data[480..]);
    }

    #[test]
    fn a_load_chunk_needs_no_extended_length() {
        // A length byte over 255 would mean an extended APDU, which this command does not use.
        for total in [1usize, 240, 241, 8192 * 76] {
            let data = vec![0u8; total];
            for offset in (0..total).step_by(LOAD_CHUNK) {
                let block = load_block(INS_LOAD_DATABASE, total, offset, &data);
                assert!(block[4] <= 244, "Lc too big at {total}/{offset}");
                assert_eq!(block.len(), 5 + block[4] as usize);
            }
        }
    }

    #[test]
    fn a_firmware_image_is_2595_chunks() {
        // What a full transfer costs, kept next to the chunk size on purpose: 622592 / 240.
        assert_eq!(8192 * 76 - (8192 * 76 / LOAD_CHUNK) * LOAD_CHUNK, 32);
        assert_eq!(8192 * 76 / LOAD_CHUNK + 1, 2595);
    }

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
