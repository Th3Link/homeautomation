//! Hardware-independent DS18B20 (Maxim/Dallas 1-Wire thermometer) logic:
//! CRC, ROM search, scratchpad decoding, the `TemperatureSensor` CAN
//! payload, a small store for the latest readings, and the "is it time to
//! publish on CAN" timer.
//!
//! The actual bit-banging of the 1-Wire bus lives in
//! `cancomponents-hardware::temperature`; everything here talks to the bus
//! only through the [`OneWireBus`] trait, so the ROM search can be tested
//! on the host against a simulated bus.
//!
//! # CAN wire format
//!
//! One `TemperatureSensor` frame per sensor, 8 bytes — the same layout the
//! original C++ (`THSensor::dispatch`) used and the gateway decodes
//! (`gateway_core::sensor::FixedPointReading`):
//!
//! ```text
//! byte:  0..5                       6..7
//!       ┌──────────────────────────┬────────────────┐
//!       │ ROM bytes 1..=6          │ raw temperature│
//!       │ (serial, no family/CRC)  │ i16 as u16, LE │
//!       └──────────────────────────┴────────────────┘
//! ```
//!
//! `raw` is the DS18B20's native fixed-point value (1/16 °C per LSB), so
//! `°C = raw / 16`. Negative temperatures are sent in two's complement;
//! the gateway currently decodes the field as unsigned, so it shows them as
//! large positive values.

use crate::DecodeError;
use embassy_time::{Duration, Instant};
use heapless::Vec;

/// ROM family code of the DS18B20.
pub const FAMILY_DS18B20: u8 = 0x28;
/// ROM family code of the DS1822 (same scratchpad layout as the DS18B20).
pub const FAMILY_DS1822: u8 = 0x22;

/// 1-Wire ROM/function commands used here.
pub mod command {
    pub const SEARCH_ROM: u8 = 0xF0;
    pub const MATCH_ROM: u8 = 0x55;
    pub const SKIP_ROM: u8 = 0xCC;
    pub const CONVERT_T: u8 = 0x44;
    pub const READ_SCRATCHPAD: u8 = 0xBE;
}

/// Dallas/Maxim CRC-8 (polynomial x⁸+x⁵+x⁴+1, reflected `0x8C`). The CRC of
/// a message followed by its own CRC byte is `0`.
pub fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for &byte in data {
        let mut byte = byte;
        for _ in 0..8 {
            let mix = (crc ^ byte) & 1;
            crc >>= 1;
            if mix != 0 {
                crc ^= 0x8C;
            }
            byte >>= 1;
        }
    }
    crc
}

/// A 64-bit 1-Wire ROM code: `[family, serial × 6, crc]`, in bus order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RomCode([u8; 8]);

impl RomCode {
    /// Wraps raw ROM bytes, rejecting a wrong CRC.
    pub fn new(bytes: [u8; 8]) -> Result<Self, DecodeError> {
        if crc8(&bytes[..7]) == bytes[7] {
            Ok(Self(bytes))
        } else {
            Err(DecodeError)
        }
    }

    pub fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }

    pub fn family(&self) -> u8 {
        self.0[0]
    }

    /// The 48-bit serial number (ROM bytes 1..=6) — what identifies the
    /// sensor in the CAN payload.
    pub fn serial(&self) -> [u8; 6] {
        [
            self.0[1], self.0[2], self.0[3], self.0[4], self.0[5], self.0[6],
        ]
    }

    /// The serial as the little-endian 48-bit `sub_id` that rides in the CAN
    /// payload (and the gateway shows as `FixedPointReading::sub_id`).
    pub fn sub_id(&self) -> u64 {
        let s = self.serial();
        u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], 0, 0])
    }

    /// The low 32 bits of [`sub_id`](Self::sub_id): the `0x…` suffix of the
    /// gateway's MQTT topic (`canbus/temperature/<device>/0x<this>`), which
    /// only keeps that many bits.
    pub fn topic_id(&self) -> u32 {
        self.sub_id() as u32
    }

    /// True for the families whose scratchpad [`parse_scratchpad`] understands.
    pub fn is_ds18b20_compatible(&self) -> bool {
        matches!(self.family(), FAMILY_DS18B20 | FAMILY_DS1822)
    }
}

/// The primitive operations of a 1-Wire master. Implemented by the
/// bit-banging driver on the device and by a simulated bus in the tests.
pub trait OneWireBus {
    /// Sends a reset pulse; returns whether any device answered with a
    /// presence pulse.
    fn reset(&mut self) -> bool;
    /// Writes one time slot. `true` is a "1" slot.
    fn write_bit(&mut self, bit: bool);
    /// Reads one time slot.
    fn read_bit(&mut self) -> bool;

    /// Writes a byte, least significant bit first.
    fn write_byte(&mut self, byte: u8) {
        for i in 0..8 {
            self.write_bit((byte >> i) & 1 != 0);
        }
    }

    /// Reads a byte, least significant bit first.
    fn read_byte(&mut self) -> u8 {
        let mut byte = 0;
        for i in 0..8 {
            if self.read_bit() {
                byte |= 1 << i;
            }
        }
        byte
    }
}

/// Why a [`RomSearch`] step failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchError {
    /// Nothing answered the reset pulse (empty or broken bus).
    NoPresence,
    /// Every device dropped out mid-search (both bit reads returned 1) —
    /// noise, or a device was unplugged during the search.
    BusError,
    /// A ROM was assembled but its CRC doesn't match.
    CrcMismatch,
}

/// Iterative Search ROM (Maxim application note 187): each [`next`](Self::next)
/// call finds one more device, until it returns `Ok(None)`.
#[derive(Debug, Clone)]
pub struct RomSearch {
    rom: [u8; 8],
    last_discrepancy: u8,
    done: bool,
}

impl Default for RomSearch {
    fn default() -> Self {
        Self::new()
    }
}

impl RomSearch {
    pub fn new() -> Self {
        Self {
            rom: [0; 8],
            last_discrepancy: 0,
            done: false,
        }
    }

    /// Finds the next device on the bus. After an error the search is over
    /// (further calls return `Ok(None)`); start a new [`RomSearch`] to retry.
    pub fn next<B: OneWireBus>(&mut self, bus: &mut B) -> Result<Option<RomCode>, SearchError> {
        if self.done {
            return Ok(None);
        }
        // Whatever happens below, this search ends unless we return Ok(Some)
        // with devices left over.
        self.done = true;

        if !bus.reset() {
            return Err(SearchError::NoPresence);
        }
        bus.write_byte(command::SEARCH_ROM);

        let mut last_zero = 0u8;
        for id_bit_number in 1..=64u8 {
            let byte = usize::from((id_bit_number - 1) / 8);
            let mask = 1u8 << ((id_bit_number - 1) % 8);

            let id_bit = bus.read_bit();
            let cmp_id_bit = bus.read_bit();
            if id_bit && cmp_id_bit {
                return Err(SearchError::BusError);
            }

            let direction = if id_bit != cmp_id_bit {
                // All remaining devices agree on this bit.
                id_bit
            } else {
                // Discrepancy: devices with both 0 and 1 here.
                let direction = if id_bit_number < self.last_discrepancy {
                    self.rom[byte] & mask != 0
                } else {
                    id_bit_number == self.last_discrepancy
                };
                if !direction {
                    last_zero = id_bit_number;
                }
                direction
            };

            if direction {
                self.rom[byte] |= mask;
            } else {
                self.rom[byte] &= !mask;
            }
            bus.write_bit(direction);
        }

        let rom = RomCode::new(self.rom).map_err(|_| SearchError::CrcMismatch)?;
        self.last_discrepancy = last_zero;
        self.done = last_zero == 0;
        Ok(Some(rom))
    }
}

/// Why a scratchpad couldn't be turned into a temperature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScratchpadError {
    CrcMismatch,
    /// The CRC is fine but the configuration register isn't one a DS18B20
    /// can hold. This is what an idle bus reads as: all zeros (line stuck
    /// low) is a valid CRC, and would otherwise look like a genuine 0 °C.
    Implausible,
}

/// A decoded temperature in the sensor's native fixed-point format
/// (1/16 °C per LSB).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Temperature {
    raw: i16,
}

impl Temperature {
    pub const fn from_raw(raw: i16) -> Self {
        Self { raw }
    }

    pub const fn raw(self) -> i16 {
        self.raw
    }

    /// Temperature in milli-degrees Celsius, for float-free control logic
    /// (e.g. a mixer controller). 1/16 °C = 62.5 m°C, so the half
    /// millidegree is truncated.
    pub const fn millicelsius(self) -> i32 {
        self.raw as i32 * 125 / 2
    }

    pub fn celsius(self) -> f32 {
        f32::from(self.raw) / 16.0
    }
}

/// Decodes the 9-byte scratchpad read after `Convert T`.
///
/// Verifies the CRC and masks the bits that are undefined at the sensor's
/// configured resolution (9/10/11 bit modes leave the low bits unspecified).
pub fn parse_scratchpad(raw: &[u8; 9]) -> Result<Temperature, ScratchpadError> {
    if crc8(&raw[..8]) != raw[8] {
        return Err(ScratchpadError::CrcMismatch);
    }
    // Configuration register: bit 7 and bits 4..0 are fixed (0 and 0b11111).
    if raw[4] & 0x9F != 0x1F {
        return Err(ScratchpadError::Implausible);
    }
    let value = i16::from_le_bytes([raw[0], raw[1]]);
    // Configuration register bits 6..5 (R1, R0): 00 = 9 bit … 11 = 12 bit.
    let undefined_bits: i16 = match (raw[4] >> 5) & 0b11 {
        0b00 => 0b111,
        0b01 => 0b011,
        0b10 => 0b001,
        _ => 0,
    };
    Ok(Temperature::from_raw(value & !undefined_bits))
}

/// Builds the 8-byte `TemperatureSensor` CAN payload (see the module docs).
pub fn can_payload(rom: &RomCode, temperature: Temperature) -> [u8; 8] {
    let serial = rom.serial();
    let value = (temperature.raw() as u16).to_le_bytes();
    [
        serial[0], serial[1], serial[2], serial[3], serial[4], serial[5], value[0], value[1],
    ]
}

/// One sensor's latest measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading {
    pub rom: RomCode,
    pub temperature: Temperature,
    /// When the measurement was taken.
    pub at: Instant,
}

/// The sensor bank is full (more distinct sensors than `N`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BankFull;

/// Latest reading per sensor, at most `N` sensors, in discovery order.
#[derive(Debug, Clone)]
pub struct SensorBank<const N: usize> {
    readings: Vec<Reading, N>,
}

impl<const N: usize> Default for SensorBank<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> SensorBank<N> {
    pub const fn new() -> Self {
        Self {
            readings: Vec::new(),
        }
    }

    /// Stores a fresh reading, replacing the previous one of the same sensor.
    pub fn update(&mut self, reading: Reading) -> Result<(), BankFull> {
        match self.readings.iter_mut().find(|r| r.rom == reading.rom) {
            Some(slot) => {
                *slot = reading;
                Ok(())
            }
            None => self.readings.push(reading).map_err(|_| BankFull),
        }
    }

    /// Drops one sensor's reading, if it has one.
    pub fn remove(&mut self, rom: &RomCode) {
        self.readings.retain(|r| &r.rom != rom);
    }

    /// Drops every reading whose sensor isn't in `present` (e.g. after a
    /// bus rescan found a sensor gone).
    pub fn retain_present(&mut self, present: &[RomCode]) {
        self.readings.retain(|r| present.contains(&r.rom));
    }

    pub fn get(&self, rom: &RomCode) -> Option<&Reading> {
        self.readings.iter().find(|r| &r.rom == rom)
    }

    pub fn as_slice(&self) -> &[Reading] {
        &self.readings
    }

    pub fn len(&self) -> usize {
        self.readings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.readings.is_empty()
    }
}

/// Consecutive failed measurement cycles after which a sensor counts as lost.
pub const LOST_AFTER_FAILED_CYCLES: u8 = 3;
/// While a sensor stays lost, its loss is re-announced every this many cycles.
pub const REMIND_EVERY_CYCLES: u16 = 6;

/// A change in a sensor's health worth telling the bus about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthEventKind {
    /// The sensor just failed [`LOST_AFTER_FAILED_CYCLES`] cycles in a row.
    Lost,
    /// The sensor is still lost; repeated every [`REMIND_EVERY_CYCLES`]
    /// cycles so a gateway that (re)started later still learns about it.
    StillLost,
    /// A lost sensor delivered a valid reading again.
    Recovered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealthEvent {
    pub kind: HealthEventKind,
    /// The sensor's position in [`SensorHealth`] (stable while no sensor is
    /// forgotten).
    pub slot: usize,
    pub rom: RomCode,
}

#[derive(Debug, Clone, Copy)]
struct Health {
    rom: RomCode,
    failures: u8,
    lost: bool,
    cycles_since_report: u16,
}

/// Every sensor ever seen on the bus, in discovery order, and whether it's
/// currently answering. A sensor stays in here after it stops answering —
/// that's what makes its failure reportable.
#[derive(Debug, Clone)]
pub struct SensorHealth<const N: usize> {
    sensors: Vec<Health, N>,
}

impl<const N: usize> Default for SensorHealth<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> SensorHealth<N> {
    pub const fn new() -> Self {
        Self {
            sensors: Vec::new(),
        }
    }

    /// Starts tracking `rom` if it's new. Returns whether it was.
    pub fn register(&mut self, rom: RomCode) -> Result<bool, BankFull> {
        if self.sensors.iter().any(|s| s.rom == rom) {
            return Ok(false);
        }
        self.sensors
            .push(Health {
                rom,
                failures: 0,
                lost: false,
                cycles_since_report: 0,
            })
            .map_err(|_| BankFull)?;
        Ok(true)
    }

    pub fn len(&self) -> usize {
        self.sensors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sensors.is_empty()
    }

    /// The tracked sensors, in slot order.
    pub fn roms(&self) -> impl Iterator<Item = RomCode> + '_ {
        self.sensors.iter().map(|s| s.rom)
    }

    pub fn is_lost(&self, rom: &RomCode) -> bool {
        self.sensors.iter().any(|s| &s.rom == rom && s.lost)
    }

    pub fn lost_count(&self) -> usize {
        self.sensors.iter().filter(|s| s.lost).count()
    }

    /// Records the outcome of one measurement cycle for `rom`. Returns an
    /// event if this outcome should be announced.
    pub fn record(&mut self, rom: &RomCode, ok: bool) -> Option<HealthEvent> {
        let slot = self.sensors.iter().position(|s| &s.rom == rom)?;
        let sensor = &mut self.sensors[slot];
        let event = |kind| HealthEvent {
            kind,
            slot,
            rom: *rom,
        };

        if ok {
            sensor.failures = 0;
            if sensor.lost {
                sensor.lost = false;
                return Some(event(HealthEventKind::Recovered));
            }
            return None;
        }

        sensor.failures = sensor.failures.saturating_add(1);
        if sensor.lost {
            sensor.cycles_since_report += 1;
            if sensor.cycles_since_report >= REMIND_EVERY_CYCLES {
                sensor.cycles_since_report = 0;
                return Some(event(HealthEventKind::StillLost));
            }
        } else if sensor.failures >= LOST_AFTER_FAILED_CYCLES {
            sensor.lost = true;
            sensor.cycles_since_report = 0;
            return Some(event(HealthEventKind::Lost));
        }
        None
    }

    /// Stops tracking every lost sensor (for sensors that were removed on
    /// purpose). Returns how many were dropped.
    pub fn forget_lost(&mut self) -> usize {
        let before = self.sensors.len();
        self.sensors.retain(|s| !s.lost);
        before - self.sensors.len()
    }
}

/// Decides when the readings are due on the CAN bus. Measurements happen on
/// a fixed, faster cadence; this just tells the caller which of them should
/// also be published.
///
/// Measurement cycles never land exactly `interval` apart (they take a
/// variable time), so a publish counts as due once the elapsed time is
/// within `tolerance` of the interval — without that, a 30 s interval on a
/// 10 s cycle could slip to 40 s on a few milliseconds of jitter.
#[derive(Debug, Clone, Copy)]
pub struct PublishTimer {
    interval: Duration,
    tolerance: Duration,
    last: Option<Instant>,
}

impl PublishTimer {
    pub const fn new(interval: Duration, tolerance: Duration) -> Self {
        Self {
            interval,
            tolerance,
            last: None,
        }
    }

    pub fn set_interval(&mut self, interval: Duration) {
        self.interval = interval;
    }

    /// True if a publish is due at `now`. The very first call is always due.
    pub fn due(&self, now: Instant) -> bool {
        match self.last {
            None => true,
            Some(last) => now.duration_since(last) + self.tolerance >= self.interval,
        }
    }

    pub fn mark_published(&mut self, now: Instant) {
        self.last = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rom_with_crc(family: u8, serial: [u8; 6]) -> [u8; 8] {
        let mut rom = [
            family, serial[0], serial[1], serial[2], serial[3], serial[4], serial[5], 0,
        ];
        rom[7] = crc8(&rom[..7]);
        rom
    }

    #[test]
    fn crc8_matches_datasheet_vectors() {
        // Maxim's documented example ROM: 02 1C B8 01 00 00 00 -> A2.
        assert_eq!(crc8(&[0x02, 0x1C, 0xB8, 0x01, 0x00, 0x00, 0x00]), 0xA2);
        // Data followed by its own CRC checks to zero.
        assert_eq!(crc8(&[0x02, 0x1C, 0xB8, 0x01, 0x00, 0x00, 0x00, 0xA2]), 0);
    }

    #[test]
    fn rom_code_rejects_bad_crc_and_splits_serial() {
        let bytes = rom_with_crc(FAMILY_DS18B20, [1, 2, 3, 4, 5, 6]);
        let rom = RomCode::new(bytes).unwrap();
        assert_eq!(rom.family(), FAMILY_DS18B20);
        assert_eq!(rom.serial(), [1, 2, 3, 4, 5, 6]);
        assert!(rom.is_ds18b20_compatible());

        let mut bad = bytes;
        bad[7] ^= 1;
        assert!(RomCode::new(bad).is_err());
    }

    #[test]
    fn other_families_are_not_ds18b20_compatible() {
        // 0x10 = DS18S20, which has a different scratchpad interpretation.
        let rom = RomCode::new(rom_with_crc(0x10, [0; 6])).unwrap();
        assert!(!rom.is_ds18b20_compatible());
    }

    /// A simulated 1-Wire bus with several devices, implementing just
    /// enough of Search ROM for the algorithm under test.
    struct SimBus {
        devices: Vec<[u8; 8], 32>,
        active: [bool; 32],
        /// Next bit index (0..64) of the search in progress.
        bit: usize,
        phase: Phase,
        resets: usize,
    }

    #[derive(PartialEq)]
    enum Phase {
        Idle,
        ReadId,
        ReadCmp,
        WriteDirection,
    }

    impl SimBus {
        fn new(roms: &[[u8; 8]]) -> Self {
            Self {
                devices: Vec::from_slice(roms).unwrap(),
                active: [false; 32],
                bit: 0,
                phase: Phase::Idle,
                resets: 0,
            }
        }

        fn device_bit(&self, device: usize) -> bool {
            self.devices[device][self.bit / 8] >> (self.bit % 8) & 1 != 0
        }
    }

    impl OneWireBus for SimBus {
        fn reset(&mut self) -> bool {
            self.resets += 1;
            self.phase = Phase::Idle;
            !self.devices.is_empty()
        }

        fn write_byte(&mut self, byte: u8) {
            assert_eq!(byte, command::SEARCH_ROM);
            self.active = [true; 32];
            self.bit = 0;
            self.phase = Phase::ReadId;
        }

        fn read_bit(&mut self) -> bool {
            // Open-drain wired-AND: any active device sending 0 wins. With
            // no active device the line floats high.
            let invert = self.phase == Phase::ReadCmp;
            let mut line = true;
            for d in 0..self.devices.len() {
                if self.active[d] {
                    line &= self.device_bit(d) != invert;
                }
            }
            self.phase = match self.phase {
                Phase::ReadId => Phase::ReadCmp,
                Phase::ReadCmp => Phase::WriteDirection,
                _ => panic!("read_bit outside a search"),
            };
            line
        }

        fn write_bit(&mut self, bit: bool) {
            assert!(self.phase == Phase::WriteDirection);
            for d in 0..self.devices.len() {
                if self.active[d] && self.device_bit(d) != bit {
                    self.active[d] = false;
                }
            }
            self.bit += 1;
            self.phase = Phase::ReadId;
        }
    }

    fn find_all(bus: &mut SimBus) -> Result<Vec<RomCode, 32>, SearchError> {
        let mut search = RomSearch::new();
        let mut found = Vec::new();
        while let Some(rom) = search.next(bus)? {
            found.push(rom).unwrap();
        }
        Ok(found)
    }

    fn pseudo_random_roms<const N: usize>() -> Vec<[u8; 8], N> {
        let mut state = 0x1234_5678u32;
        let mut roms = Vec::new();
        for _ in 0..N {
            let mut serial = [0u8; 6];
            for byte in &mut serial {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                *byte = (state >> 24) as u8;
            }
            roms.push(rom_with_crc(FAMILY_DS18B20, serial)).unwrap();
        }
        roms
    }

    #[test]
    fn search_finds_a_single_device() {
        let rom = rom_with_crc(FAMILY_DS18B20, [9, 8, 7, 6, 5, 4]);
        let mut bus = SimBus::new(&[rom]);
        let found = find_all(&mut bus).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].as_bytes(), &rom);
    }

    #[test]
    fn search_finds_every_one_of_twenty_devices_exactly_once() {
        let roms = pseudo_random_roms::<20>();
        let mut bus = SimBus::new(&roms);
        let found = find_all(&mut bus).unwrap();

        assert_eq!(found.len(), 20);
        for rom in &roms {
            assert_eq!(
                found.iter().filter(|f| f.as_bytes() == rom).count(),
                1,
                "{rom:02X?}"
            );
        }
        // One reset per device plus the final "nothing left" step needs none:
        // the search ends as soon as the last branch is exhausted.
        assert_eq!(bus.resets, 20);
    }

    #[test]
    fn search_on_an_empty_bus_reports_no_presence() {
        let mut bus = SimBus::new(&[]);
        let mut search = RomSearch::new();
        assert_eq!(search.next(&mut bus), Err(SearchError::NoPresence));
        // The search is over after an error.
        assert_eq!(search.next(&mut bus), Ok(None));
    }

    #[test]
    fn search_reports_a_rom_with_bad_crc() {
        let mut rom = rom_with_crc(FAMILY_DS18B20, [1, 1, 1, 1, 1, 1]);
        rom[7] ^= 0xFF;
        let mut bus = SimBus::new(&[rom]);
        let mut search = RomSearch::new();
        assert_eq!(search.next(&mut bus), Err(SearchError::CrcMismatch));
    }

    /// Builds a valid scratchpad for `temp_raw` with the given config byte.
    fn scratchpad(temp_raw: i16, config: u8) -> [u8; 9] {
        let t = temp_raw.to_le_bytes();
        let mut sp = [t[0], t[1], 0x4B, 0x46, config, 0xFF, 0x0C, 0x10, 0];
        sp[8] = crc8(&sp[..8]);
        sp
    }

    #[test]
    fn scratchpad_decodes_datasheet_power_on_value() {
        // Datasheet: reset value 0550h = +85 °C, scratchpad ends ... 1C.
        let sp = [0x50, 0x05, 0x4B, 0x46, 0x7F, 0xFF, 0x0C, 0x10, 0x1C];
        let t = parse_scratchpad(&sp).unwrap();
        assert_eq!(t.raw(), 0x0550);
        assert_eq!(t.celsius(), 85.0);
        assert_eq!(t.millicelsius(), 85_000);
    }

    #[test]
    fn idle_bus_scratchpads_are_not_mistaken_for_a_reading() {
        // A bus stuck low reads all zeros, whose CRC is (unluckily) valid.
        assert_eq!(parse_scratchpad(&[0; 9]), Err(ScratchpadError::Implausible));
        // A floating/idle-high bus reads all ones (CRC mismatch).
        assert!(parse_scratchpad(&[0xFF; 9]).is_err());
    }

    #[test]
    fn rom_ids_match_what_the_gateway_shows() {
        let rom = RomCode::new(rom_with_crc(
            FAMILY_DS18B20,
            [0x22, 0xB0, 0xC3, 0x32, 0x20, 0x01],
        ))
        .unwrap();
        assert_eq!(rom.sub_id(), 0x0120_32C3_B022);
        assert_eq!(rom.topic_id(), 0x32C3_B022);
    }

    #[test]
    fn scratchpad_decodes_negative_temperatures() {
        // Datasheet: FF5Eh = -10.125 °C.
        let t = parse_scratchpad(&scratchpad(-162, 0x7F)).unwrap();
        assert_eq!(t.raw(), -162);
        assert_eq!(t.celsius(), -10.125);
        assert_eq!(t.millicelsius(), -10_125);
    }

    #[test]
    fn scratchpad_with_bad_crc_is_rejected() {
        let mut sp = scratchpad(400, 0x7F);
        sp[0] ^= 1;
        assert_eq!(parse_scratchpad(&sp), Err(ScratchpadError::CrcMismatch));
    }

    #[test]
    fn scratchpad_masks_undefined_bits_at_lower_resolution() {
        // 9-bit mode (config 0x1F): low 3 bits are undefined garbage.
        assert_eq!(
            parse_scratchpad(&scratchpad(0x0197, 0x1F)).unwrap().raw(),
            0x0190
        );
        // 10-bit (0x3F): low 2 bits. 11-bit (0x5F): low bit. 12-bit: none.
        assert_eq!(
            parse_scratchpad(&scratchpad(0x0197, 0x3F)).unwrap().raw(),
            0x0194
        );
        assert_eq!(
            parse_scratchpad(&scratchpad(0x0197, 0x5F)).unwrap().raw(),
            0x0196
        );
        assert_eq!(
            parse_scratchpad(&scratchpad(0x0197, 0x7F)).unwrap().raw(),
            0x0197
        );
    }

    #[test]
    fn can_payload_layout_matches_the_legacy_format() {
        let rom = RomCode::new(rom_with_crc(
            FAMILY_DS18B20,
            [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF],
        ))
        .unwrap();
        // 25.0625 °C = 401 = 0x0191.
        let payload = can_payload(&rom, Temperature::from_raw(0x0191));
        assert_eq!(payload, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x91, 0x01]);
    }

    #[test]
    fn can_payload_sends_negative_values_as_twos_complement() {
        let rom = RomCode::new(rom_with_crc(FAMILY_DS18B20, [0; 6])).unwrap();
        let payload = can_payload(&rom, Temperature::from_raw(-162));
        assert_eq!(&payload[6..], &(-162i16 as u16).to_le_bytes());
    }

    fn reading(serial: u8, raw: i16, at_ms: u64) -> Reading {
        Reading {
            rom: RomCode::new(rom_with_crc(FAMILY_DS18B20, [serial, 0, 0, 0, 0, 0])).unwrap(),
            temperature: Temperature::from_raw(raw),
            at: Instant::from_millis(at_ms),
        }
    }

    #[test]
    fn bank_replaces_the_reading_of_a_known_sensor() {
        let mut bank = SensorBank::<4>::new();
        bank.update(reading(1, 100, 0)).unwrap();
        bank.update(reading(2, 200, 0)).unwrap();
        bank.update(reading(1, 150, 10_000)).unwrap();

        assert_eq!(bank.len(), 2);
        let r = bank.get(&reading(1, 0, 0).rom).unwrap();
        assert_eq!(r.temperature.raw(), 150);
        assert_eq!(r.at, Instant::from_millis(10_000));
    }

    #[test]
    fn bank_rejects_sensors_beyond_its_capacity() {
        let mut bank = SensorBank::<2>::new();
        bank.update(reading(1, 0, 0)).unwrap();
        bank.update(reading(2, 0, 0)).unwrap();
        assert_eq!(bank.update(reading(3, 0, 0)), Err(BankFull));
        // Known sensors can still be updated when full.
        assert!(bank.update(reading(2, 5, 1)).is_ok());
    }

    #[test]
    fn bank_drops_sensors_that_disappeared() {
        let mut bank = SensorBank::<4>::new();
        bank.update(reading(1, 0, 0)).unwrap();
        bank.update(reading(2, 0, 0)).unwrap();
        bank.retain_present(&[reading(2, 0, 0).rom]);
        assert_eq!(bank.len(), 1);
        assert!(bank.get(&reading(1, 0, 0).rom).is_none());
    }

    #[test]
    fn publish_timer_fires_first_then_every_interval() {
        let mut timer = PublishTimer::new(Duration::from_secs(30), Duration::from_secs(5));
        assert!(timer.due(Instant::from_millis(900)));
        timer.mark_published(Instant::from_millis(900));

        assert!(!timer.due(Instant::from_millis(10_900)));
        assert!(!timer.due(Instant::from_millis(20_900)));
        assert!(timer.due(Instant::from_millis(30_900)));
    }

    #[test]
    fn publish_timer_tolerates_cycle_jitter() {
        let mut timer = PublishTimer::new(Duration::from_secs(30), Duration::from_secs(5));
        timer.mark_published(Instant::from_millis(900));
        // A cycle finishing 40 ms "early" must not slip a whole extra cycle.
        assert!(timer.due(Instant::from_millis(30_860)));
    }

    #[test]
    fn publish_timer_follows_a_changed_interval() {
        let mut timer = PublishTimer::new(Duration::from_secs(30), Duration::from_secs(5));
        timer.mark_published(Instant::from_millis(0));
        assert!(!timer.due(Instant::from_millis(10_000)));
        timer.set_interval(Duration::from_secs(10));
        assert!(timer.due(Instant::from_millis(10_000)));
    }

    fn health_rom(serial: u8) -> RomCode {
        reading(serial, 0, 0).rom
    }

    fn health_with(serial: u8) -> (SensorHealth<4>, RomCode) {
        let mut health = SensorHealth::<4>::new();
        let rom = health_rom(serial);
        assert_eq!(health.register(rom), Ok(true));
        (health, rom)
    }

    #[test]
    fn health_registers_each_sensor_once() {
        let (mut health, rom) = health_with(1);
        assert_eq!(health.register(rom), Ok(false));
        assert_eq!(health.len(), 1);
    }

    #[test]
    fn sensor_is_lost_after_three_failed_cycles_not_before() {
        let (mut health, rom) = health_with(1);
        assert_eq!(health.record(&rom, false), None);
        assert_eq!(health.record(&rom, false), None);
        let event = health.record(&rom, false).unwrap();
        assert_eq!(event.kind, HealthEventKind::Lost);
        assert_eq!(event.slot, 0);
        assert_eq!(event.rom, rom);
        assert!(health.is_lost(&rom));
    }

    #[test]
    fn a_single_good_reading_resets_the_failure_count() {
        let (mut health, rom) = health_with(1);
        health.record(&rom, false);
        health.record(&rom, false);
        assert_eq!(health.record(&rom, true), None);
        assert_eq!(health.record(&rom, false), None);
        assert_eq!(health.record(&rom, false), None);
        assert!(!health.is_lost(&rom));
    }

    #[test]
    fn lost_sensor_is_reminded_periodically_then_recovers() {
        let (mut health, rom) = health_with(1);
        for _ in 0..3 {
            health.record(&rom, false);
        }
        // Quiet for REMIND_EVERY_CYCLES - 1 cycles, then a reminder.
        for _ in 0..REMIND_EVERY_CYCLES - 1 {
            assert_eq!(health.record(&rom, false), None);
        }
        assert_eq!(
            health.record(&rom, false).unwrap().kind,
            HealthEventKind::StillLost
        );
        for _ in 0..REMIND_EVERY_CYCLES - 1 {
            assert_eq!(health.record(&rom, false), None);
        }
        assert_eq!(
            health.record(&rom, false).unwrap().kind,
            HealthEventKind::StillLost
        );

        assert_eq!(
            health.record(&rom, true).unwrap().kind,
            HealthEventKind::Recovered
        );
        assert!(!health.is_lost(&rom));
        // And a healthy sensor reports nothing further.
        assert_eq!(health.record(&rom, true), None);
    }

    #[test]
    fn health_events_carry_the_sensors_slot() {
        let mut health = SensorHealth::<4>::new();
        health.register(health_rom(1)).unwrap();
        health.register(health_rom(2)).unwrap();
        let rom = health_rom(2);
        health.record(&rom, false);
        health.record(&rom, false);
        assert_eq!(health.record(&rom, false).unwrap().slot, 1);
        assert_eq!(health.lost_count(), 1);
    }

    #[test]
    fn forgetting_drops_only_lost_sensors() {
        let mut health = SensorHealth::<4>::new();
        health.register(health_rom(1)).unwrap();
        health.register(health_rom(2)).unwrap();
        for _ in 0..3 {
            health.record(&health_rom(1), false);
        }
        assert_eq!(health.forget_lost(), 1);
        assert_eq!(health.len(), 1);
        assert_eq!(health.roms().next(), Some(health_rom(2)));
    }

    #[test]
    fn recording_an_unknown_sensor_is_ignored() {
        let mut health = SensorHealth::<4>::new();
        assert_eq!(health.record(&health_rom(9), false), None);
    }

    #[test]
    fn bank_remove_drops_one_reading() {
        let mut bank = SensorBank::<4>::new();
        bank.update(reading(1, 0, 0)).unwrap();
        bank.update(reading(2, 0, 0)).unwrap();
        bank.remove(&health_rom(1));
        assert_eq!(bank.len(), 1);
        assert!(bank.get(&health_rom(2)).is_some());
    }
}
