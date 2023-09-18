#![cfg(feature = "stm32f429")]

use alloc::slice;
use drogue_tls::entropy::{entropy_f, EntropySource};
use drogue_tls_sys::types::{c_int, c_uchar, c_void, size_t};

use dice_common::display::DOUBLE_SCREEN_WIDTH;
use hub75::{Hub75, Pins};

pub use stm32f4xx_hal as hal;

use embedded_hal::blocking::delay::{DelayMs, DelayUs};

pub const CLOCK_FREQ_MHZ: u32 = 180;

use dice_common::{
    network_utils::*,
    smoltcp::{
        iface::EthernetInterface,
        wire::{EthernetAddress, IpAddress, Ipv4Address},
    },
};

use hal::{
    dwt::{ClockDuration, Delay, Dwt, DwtExt},
    gpio::{
        gpiob::{PB0, PB14, PB7},
        Output, PushPull,
    },
    prelude::*,
    stm32::{TIM3, *},
    timer::{Event, Timer},
};
use hal::{gpio::GpioExt, rcc::RccExt};

//B
//1, 2, 3, 4, 5, 6, 8, 9, 10, 11, 12, 15
pub const PINS0: Pins = Pins {
    r1: 1,
    g1: 2,
    b1: 3,
    r2: 4,
    g2: 5,
    b2: 6,
    a: 8,
    b: 9,
    c: 10,
    clock: 11,
    latch: 12,
    oe: 15,
};

//D
//0, 1, 2, 3, 4, 5, 6, 7, 11, 12, 13, 14

pub const PINS1: Pins = Pins {
    r1: 0,
    g1: 1,
    b1: 2,
    r2: 3,
    g2: 4,
    b2: 5,
    a: 6,
    b: 7,
    c: 11,
    clock: 12,
    latch: 13,
    oe: 14,
};

//E
//0, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 13

pub const PINS2: Pins = Pins {
    r1: 0,
    g1: 2,
    b1: 3,
    r2: 4,
    g2: 5,
    b2: 6,
    a: 7,
    b: 8,
    c: 9,
    clock: 11,
    latch: 12,
    oe: 13,
};

static mut RNG: Option<hal::rng::Rng> = None;

use stm32_eth::{Eth, EthPins, PhyAddress, RingEntry, RxDescriptor, TxDescriptor};

pub type EthIface = EthernetInterface<'static, &'static mut Eth<'static, 'static>>;
pub type DisplayDelayProvider = hal::dwt::Delay;
pub type DisplayTaskTimer = Timer<TIM3>;

pub type EthDeviceT = &'static mut Eth<'static, 'static>;

pub type LedBType = PB7<Output<PushPull>>;

static mut ETH: Option<Eth> = None;

static mut RX_RING: [RingEntry<RxDescriptor>; 8] = [
    RingEntry::<RxDescriptor>::new(),
    RingEntry::<RxDescriptor>::new(),
    RingEntry::<RxDescriptor>::new(),
    RingEntry::<RxDescriptor>::new(),
    RingEntry::<RxDescriptor>::new(),
    RingEntry::<RxDescriptor>::new(),
    RingEntry::<RxDescriptor>::new(),
    RingEntry::<RxDescriptor>::new(),
];

static mut TX_RING: [RingEntry<TxDescriptor>; 2] = [
    RingEntry::<TxDescriptor>::new(),
    RingEntry::<TxDescriptor>::new(),
];

pub fn init<'a>(
    mut cp: rtic::Peripherals,
    dp: hal::stm32::Peripherals,
    src_mac: &[u8],
    ip: IpAddress,
    gateway_ip: Ipv4Address,
    interface_storage: &'a mut EthernetInterfaceStorage,
    clock_frequency_mhz: u32,
) -> (
    PB14<Output<PushPull>>,
    PB0<Output<PushPull>>,
    PB7<Output<PushPull>>,
    EthernetInterface<'a, &'a mut Eth<'static, 'static>>,
    HardwareEntropy,
    Hub75<PINS0, DOUBLE_SCREEN_WIDTH>,
    Hub75<PINS1, DOUBLE_SCREEN_WIDTH>,
    Hub75<PINS2, DOUBLE_SCREEN_WIDTH>,
    Delay,
    Timer<TIM3>,
) {
    // Initialize (enable) the monotonic timer (CYCCNT)
    cp.DCB.enable_trace();
    cp.DWT.enable_cycle_counter();

    // Set up the system clock.
    let rcc = dp.RCC.constrain();

    let clocks = rcc
        .cfgr
        .sysclk(clock_frequency_mhz.mhz())
        .hclk(clock_frequency_mhz.mhz())
        .freeze();

    let gpioa = dp.GPIOA.split();
    let gpiob = dp.GPIOB.split();
    let gpioc = dp.GPIOC.split();
    let gpiod = dp.GPIOD.split();
    let gpioe = dp.GPIOE.split();
    let gpiog = dp.GPIOG.split();

    let _r1 = gpiob.pb1.into_push_pull_output();
    let _g1 = gpiob.pb2.into_push_pull_output();
    let _b1 = gpiob.pb3.into_push_pull_output();
    let _r2 = gpiob.pb4.into_push_pull_output();
    let _g2 = gpiob.pb5.into_push_pull_output();
    let _b2 = gpiob.pb6.into_push_pull_output();
    let _a = gpiob.pb8.into_push_pull_output();
    let _b = gpiob.pb9.into_push_pull_output();
    let _c = gpiob.pb10.into_push_pull_output();
    let _clock = gpiob.pb11.into_push_pull_output();
    let _strobe = gpiob.pb12.into_push_pull_output();
    let _oe = gpiob.pb15.into_push_pull_output();

    let _r1 = gpiod.pd0.into_push_pull_output();
    let _g1 = gpiod.pd1.into_push_pull_output();
    let _b1 = gpiod.pd2.into_push_pull_output();
    let _r2 = gpiod.pd3.into_push_pull_output();
    let _g2 = gpiod.pd4.into_push_pull_output();
    let _b2 = gpiod.pd5.into_push_pull_output();
    let _a = gpiod.pd6.into_push_pull_output();
    let _b = gpiod.pd7.into_push_pull_output();
    let _c = gpiod.pd11.into_push_pull_output();
    let _clock = gpiod.pd12.into_push_pull_output();
    let _strobe = gpiod.pd13.into_push_pull_output();
    let _oe = gpiod.pd14.into_push_pull_output();

    let _r1 = gpioe.pe0.into_push_pull_output();
    let _g1 = gpioe.pe2.into_push_pull_output();
    let _b1 = gpioe.pe3.into_push_pull_output();
    let _r2 = gpioe.pe4.into_push_pull_output();
    let _g2 = gpioe.pe5.into_push_pull_output();
    let _b2 = gpioe.pe6.into_push_pull_output();
    let _a = gpioe.pe7.into_push_pull_output();
    let _b = gpioe.pe8.into_push_pull_output();
    let _c = gpioe.pe9.into_push_pull_output();
    let _clock = gpioe.pe11.into_push_pull_output();
    let _strobe = gpioe.pe12.into_push_pull_output();
    let _oe = gpioe.pe13.into_push_pull_output();

    let display0 =
        Hub75::<PINS0, DOUBLE_SCREEN_WIDTH>::new(4, unsafe { &mut *(0x40020414 as *mut u16) });

    //initialize display 1
    let display1 =
        Hub75::<PINS1, DOUBLE_SCREEN_WIDTH>::new(4, unsafe { &mut *(0x40020C14 as *mut u16) });

    //initialize display 2
    let display2 =
        Hub75::<PINS2, DOUBLE_SCREEN_WIDTH>::new(4, unsafe { &mut *(0x40021014 as *mut u16) });

    let eth_pins = EthPins {
        ref_clk: gpioa.pa1,
        md_io: gpioa.pa2,
        md_clk: gpioc.pc1,
        crs: gpioa.pa7,
        tx_en: gpiog.pg11,
        tx_d0: gpiog.pg13,
        tx_d1: gpiob.pb13,
        rx_d0: gpioc.pc4,
        rx_d1: gpioc.pc5,
    };

    unsafe {
        ETH = Some(
            Eth::new(
                dp.ETHERNET_MAC,
                dp.ETHERNET_DMA,
                &mut RX_RING,
                &mut TX_RING,
                PhyAddress::_0,
                clocks,
                eth_pins,
            )
            .unwrap(),
        );
        match &ETH {
            Some(e) => e.enable_interrupt(),
            None => {
                #[cfg(debug_assertions)]
                #[cfg(feature = "use_semihosting")]
                hprintln!("Error initalizing ethernet interface").ok();
            }
        }
    }
    let iface = create_ethernet_iface(
        unsafe { ETH.as_mut().unwrap() },
        EthernetAddress::from_bytes(src_mac),
        ip,
        0,
        gateway_ip,
        interface_storage,
    );

    unsafe {
        RNG = Some(dp.RNG.constrain(clocks));
    }

    let led_r = gpiob.pb14.into_push_pull_output(); //ld3 - red
    let led_g = gpiob.pb0.into_push_pull_output(); //ld1 - green
    let led_b = gpiob.pb7.into_push_pull_output(); //ld2 - blue

    let entropy = HardwareEntropy;

    let dwt = cp.DWT.constrain(cp.DCB, clocks);
    let mut display_delay = dwt.delay();

    let mut display_task_timer = Timer::tim3(dp.TIM3, 30.mhz(), clocks);
    display_task_timer.listen(Event::TimeOut);

    return (
        led_r,
        led_g,
        led_b,
        iface,
        entropy,
        display0,
        display1,
        display2,
        display_delay,
        display_task_timer,
    );
}

pub fn clear_display_timer_int(timer: &mut Timer<TIM3>) {
    timer.clear_interrupt(Event::TimeOut);
}

pub fn ethernet_interrupt_handler() {
    unsafe {
        ETH.as_mut().unwrap().interrupt_handler();
    }
}

pub fn check_ethernet_state() -> bool {
    let phy_status = unsafe { ETH.as_ref().unwrap().status() };

    phy_status.link_detected()
}

pub struct HardwareEntropy;

impl EntropySource for HardwareEntropy {
    fn get_f(&self) -> entropy_f {
        f_source
    }
}

impl EntropySource for &mut HardwareEntropy {
    fn get_f(&self) -> entropy_f {
        f_source
    }
}

fn u32_to_u8_array(x: u32) -> [u8; 4] {
    let b1: u8 = ((x >> 24) & 0xff) as u8;
    let b2: u8 = ((x >> 16) & 0xff) as u8;
    let b3: u8 = ((x >> 8) & 0xff) as u8;
    let b4: u8 = (x & 0xff) as u8;
    return [b1, b2, b3, b4];
}

#[no_mangle]
pub unsafe extern "C" fn f_source(
    _user_data: *mut c_void,
    data: *mut c_uchar,
    len: size_t,
    olen: *mut usize,
) -> c_int {
    let buffer = slice::from_raw_parts_mut(data, len);

    RNG.as_mut().unwrap().read(buffer);
    return 0;
}
