#![no_std]
//! ESP32 firmware for the MQTT<->CAN home-automation gateway
//! ("gateway-hardware"). Wire format and hardware-independent translation
//! logic live in the `gateway-core` crate; everything in here talks to
//! actual peripherals (CAN/TWAI, WiFi, MQTT/HTTP sockets, flash-backed
//! config storage, OTA, the commissioning console).
pub mod can;
pub mod can_update;
pub mod config;
pub mod console;
pub mod device_list;
pub mod echo;
pub mod flash;
pub mod logging;
pub mod mqtt;
pub mod translate;
pub mod update;
pub mod wifi;
