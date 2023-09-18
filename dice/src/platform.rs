#[cfg(feature = "stm32f429")]
mod stm32f4;
#[cfg(feature = "stm32f429")]
pub use stm32f4::*;

#[cfg(feature = "stm32h743")]
mod stm32h7;
#[cfg(feature = "stm32h743")]
pub use stm32h7::*;
