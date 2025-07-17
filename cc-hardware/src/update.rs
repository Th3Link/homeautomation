use cancomponents_core::can_id::CanId;
use crate::error::{Component, ErrorCode, ErrorReport, Severity};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use esp_hal_ota::Ota;
use esp_storage::FlashStorage;

static UPDATE: Mutex<CriticalSectionRawMutex, Option<Update>> = Mutex::new(None);

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum UpdateErrorCode {
    Unknown = 0,
    InvalidData = 1,
    Begin = 2,
    Init = 3,
    Write = 4,
    NotStarted = 5,
}

impl From<u8> for UpdateErrorCode {
    fn from(value: u8) -> Self {
        match value {
            1 => UpdateErrorCode::InvalidData,
            2 => UpdateErrorCode::Begin,
            3 => UpdateErrorCode::Init,
            4 => UpdateErrorCode::Write,
            5 => UpdateErrorCode::NotStarted,
            _ => UpdateErrorCode::Unknown,
        }
    }
}
pub async fn init() {
    let mut update_guard = UPDATE.lock().await;

    if update_guard.is_none() {
        let update = Update { ota: None };
        *update_guard = Some(update);
    }
}

pub async fn update(
) -> embassy_sync::mutex::MappedMutexGuard<'static, CriticalSectionRawMutex, Update> {
    let guard = UPDATE.lock().await;
    embassy_sync::mutex::MutexGuard::map(guard, |opt| opt.as_mut().expect("Update not initialized"))
}

pub struct Update {
    ota: Option<Ota<FlashStorage>>,
}

impl Update {
    pub async fn start(&mut self, id: CanId, data: &[u8], _remote_request: bool) {
        if data.len() < 8 {
            ErrorReport::send(
                Component::Update,
                ErrorCode::InvalidData,
                Severity::Warning,
                UpdateErrorCode::InvalidData as u8,
                &[id.msg_type as u8, data.len() as u8, 0u8],
            )
            .await;
            return;
        }

        let size = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let crc = u32::from_le_bytes(data[4..8].try_into().unwrap());

        match Ota::new(FlashStorage::new()) {
            Ok(mut ota) => {
                if ota.ota_begin(size, crc).is_ok() {
                    self.ota = Some(ota);
                } else {
                    // ota_begin fehlgeschlagen
                    ErrorReport::send(
                        Component::Ota,
                        ErrorCode::Unknown,
                        Severity::RecoverableError,
                        UpdateErrorCode::Begin as u8,
                        &[0u8, 0u8, 0u8],
                    )
                    .await;
                    self.ota = None;
                }
            }
            Err(_) => {
                ErrorReport::send(
                    Component::Ota,
                    ErrorCode::Unknown,
                    Severity::RecoverableError,
                    UpdateErrorCode::Init as u8,
                    &[0u8, 0u8, 0u8],
                )
                .await;
                // OTA-Initialisierung fehlgeschlagen
                self.ota = None;
            }
        }
    }

    pub async fn write(&mut self, _id: CanId, data: &[u8], _remote_request: bool) {
        if let Some(ota) = self.ota.as_mut() {
            match ota.ota_write_chunk(data) {
                Ok(true) => {
                    // Letzter Chunk – flush und reboot
                    if ota.ota_flush(true, true).is_ok() {
                        esp_hal::system::software_reset();
                    }
                }
                Ok(false) => {
                    // Weiter schreiben
                }
                Err(_) => {
                    ErrorReport::send(
                        Component::Ota,
                        ErrorCode::Unknown,
                        Severity::RecoverableError,
                        UpdateErrorCode::Write as u8,
                        &[0u8, 0u8, 0u8],
                    )
                    .await;
                    // Fehler beim Schreiben
                    self.ota = None;
                }
            }
        } else {
            ErrorReport::send(
                Component::Ota,
                ErrorCode::Unknown,
                Severity::RecoverableError,
                UpdateErrorCode::NotStarted as u8,
                &[0u8, 0u8, 0u8],
            )
            .await;
            // OTA nicht gestartet
        }
    }

    pub async fn progress(&mut self, _id: CanId, _data: &[u8], _remote_request: bool) {}
    pub async fn select(&mut self, _id: CanId, _data: &[u8], _remote_request: bool) {}
    pub async fn erase(&mut self, _id: CanId, _data: &[u8], _remote_request: bool) {}
    pub async fn read(&mut self, _id: CanId, _data: &[u8], _remote_request: bool) {}
    pub async fn verify(&mut self, _id: CanId, _data: &[u8], _remote_request: bool) {}
}
