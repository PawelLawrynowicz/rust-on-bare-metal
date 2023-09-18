use embedded_hal::digital::v2::OutputPin;

/*
    Machine for indicating network states on the device.

    Only 1 of 3 States are possible at any given moment:
        - Device physically disconnected (Disconnected)
        - Device physically connected, but without a configuration (Unconfigured)
        - Device physically connected and configured (Configured)
        Configured state is indicated by chosen LED

        At startup the device is always given disconnected status.

        Only given state transitions are legal:
        Disconnected -> Unconfigured
        Unconfigured -> Configured
        Unconfigured -> Disconnected
        Configured -> Disconnected

        If any other transition happens the program will panic.
*/

#[derive(PartialEq)]
pub enum NetworkStatus {
    Disconnected,
    Unconfigured,
    Configured,
}

pub struct NetworkConnection<T: OutputPin> {
    pub status: NetworkStatus,
    pub led: T,
}
impl<T: OutputPin> NetworkConnection<T> {
    pub fn new(mut led: T) -> Self {
        led.set_low().ok();
        NetworkConnection {
            status: NetworkStatus::Disconnected,
            led,
        }
    }
    pub fn to_configured(&mut self) {
        self.status = match self.status {
            NetworkStatus::Unconfigured => NetworkStatus::Configured,
            _ => panic!("Invalid state transition (to_configured)"),
        };
        self.led.set_high().ok();
    }
    pub fn to_unconfigured(&mut self) {
        self.status = match self.status {
            NetworkStatus::Disconnected => NetworkStatus::Unconfigured,
            _ => panic!("Invalid state transition (to_unconfigured)"),
        };
        self.led.set_low().ok();
    }
    pub fn to_disconnected(&mut self) {
        self.status = match self.status {
            NetworkStatus::Unconfigured => NetworkStatus::Disconnected,
            NetworkStatus::Configured => NetworkStatus::Disconnected,
            _ => panic!("Invalid state transmition (to_disconnected)"),
        };
        self.led.set_low().ok();
    }
}
