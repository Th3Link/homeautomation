use crate::can::send_can_message;
use cancomponents_core::can_message_type::CanMessageType;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Instant};
use heapless::FnvIndexMap;

type ErrorKey = (Component, ErrorCode, u8);

const MAX_TRACKED_ERRORS: usize = 16;

static ERROR_TIMESTAMPS: Mutex<
    CriticalSectionRawMutex,
    FnvIndexMap<ErrorKey, Instant, MAX_TRACKED_ERRORS>,
> = Mutex::new(FnvIndexMap::new());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorReport {
    pub component: Component,
    pub code: ErrorCode,
    pub severity: Severity,
    pub local_code: u8,
    pub details: [u8; 4],
}

impl ErrorReport {
    pub async fn send(
        component: Component,
        code: ErrorCode,
        severity: Severity,
        local_code: u8,
        details: &[u8],
    ) {
        // deduplication and rate limiting
        let key = (component, code, local_code);
        let now = Instant::now();
        let map = &mut ERROR_TIMESTAMPS.lock().await;
        match map.get(&key) {
            Some(&last) if now.duration_since(last) < Duration::from_secs(1) => return,
            _ => {
                let _ = map.insert(key, now);
            }
        }

        let mut d = [0u8; 4];
        d[..details.len().min(4)].copy_from_slice(&details[..details.len().min(4)]);
        let s = Self {
            component,
            code,
            severity,
            local_code,
            details: d,
        };
        let data = s.to_bytes();
        send_can_message(CanMessageType::DeviceError, &data, false).await;
    }

    pub fn to_bytes(&self) -> [u8; 8] {
        [
            self.component as u8,
            self.code as u8,
            self.severity as u8,
            self.local_code,
            self.details[0],
            self.details[1],
            self.details[2],
            self.details[3],
        ]
    }
}

impl TryFrom<&[u8]> for ErrorReport {
    type Error = ();

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != 8 {
            return Err(());
        }

        Ok(Self {
            component: Component::from(value[0]),
            code: ErrorCode::from(value[1]),
            severity: Severity::from(value[2]),
            local_code: value[3],
            details: [value[4], value[5], value[6], value[7]],
        })
    }
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    Unknown = 0,
    InvalidData = 1,
}

impl From<u8> for ErrorCode {
    fn from(value: u8) -> Self {
        match value {
            1 => ErrorCode::InvalidData,
            _ => ErrorCode::Unknown,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Severity {
    Unknown = 0,
    Warning = 1,
    RecoverableError = 2,
    RepeatingError = 3,
    Error = 4,
    CriticalError = 5,
}

impl From<u8> for Severity {
    fn from(value: u8) -> Self {
        match value {
            1 => Severity::Warning,
            2 => Severity::RepeatingError,
            3 => Severity::RecoverableError,
            4 => Severity::Error,
            5 => Severity::CriticalError,
            _ => Severity::Unknown,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Component {
    Unknown = 0,
    Can = 1,
    Device = 2,
    Update = 3,
    Storage = 4,
    Ota = 5,
    Relais = 6,
}

impl From<u8> for Component {
    fn from(value: u8) -> Self {
        match value {
            1 => Component::Can,
            2 => Component::Device,
            3 => Component::Update,
            4 => Component::Storage,
            5 => Component::Ota,
            6 => Component::Relais,
            _ => Component::Unknown,
        }
    }
}
