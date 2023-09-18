#![no_std]
#![feature(const_generics)]

pub mod http_utils;
pub use smoltcp;
pub mod display;
#[cfg(test)]
mod mock_ethernet;
pub mod network_fsm;
pub mod network_utils;
