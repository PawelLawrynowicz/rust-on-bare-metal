#![cfg(feature = "stm32h743")]

use drogue_tls::entropy::{entropy_f, EntropySource};
use drogue_tls_sys::types::{c_int, c_uchar, c_void, size_t};

use hal::device::TIM3;
use hal::timer;
pub use stm32h7xx_hal as hal;

use hub75::{Hub75, Pins};

use cortex_m::peripheral::DWT;
use dice_common::display::DOUBLE_SCREEN_WIDTH;
use dice_common::network_utils::*;
use dice_common::smoltcp::wire::{IpAddress, Ipv4Address};
use dice_common::smoltcp::{iface::EthernetInterface, wire::EthernetAddress};

use hal::{
    device::TIM2,
    ethernet,
    ethernet::{phy::LAN8742A, EthernetDMA, EthernetMAC, PHY},
    gpio::GpioExt,
    gpio::{
        gpiob::{PB0, PB14},
        gpioe::PE1,
        Output, PushPull,
        Speed::VeryHigh,
    },
    delay::DelayFromCountDownTimer,
    prelude::*,
    timer::Timer,
};

pub const PINS0: Pins = Pins {
    r1: 0,
    g1: 1,
    b1: 2,
    r2: 3,
    g2: 4,
    b2: 5,
    a: 12,
    b: 13,
    c: 14,
    clock: 6,
    latch: 7,
    oe: 11,
};
pub const PINS1: Pins = Pins {
    r1: 1,
    g1: 2,
    b1: 3,
    r2: 4,
    g2: 5,
    b2: 6,
    a: 10,
    b: 11,
    c: 12,
    clock: 7,
    latch: 8,
    oe: 9,
};
pub const PINS2: Pins = Pins {
    r1: 0,
    g1: 2,
    b1: 3,
    r2: 4,
    g2: 5,
    b2: 6,
    a: 10,
    b: 11,
    c: 12,
    clock: 7,
    latch: 8,
    oe: 9,
};

pub const CLOCK_FREQ_MHZ: u32 = 480;

pub type EthIface = EthernetInterface<'static, ethernet::EthernetDMA<'static>>;
pub type DisplayDelayProvider = DelayFromCountDownTimer<Timer<TIM2>>;
pub type DisplayTaskTimer = Timer<TIM3>;

pub type EthDeviceT = ethernet::EthernetDMA<'static>;

pub type LedBType = PE1<Output<PushPull>>;

#[link_section = ".sram3.eth"]
static mut DES_RING: ethernet::DesRing = ethernet::DesRing::new();

static mut ETH_PHY: Option<LAN8742A<EthernetMAC>> = None;

static mut RNG: Option<stm32h7xx_hal::rng::Rng> = None;

pub fn init<'a>(
    mut cp: rtic::Peripherals,
    dp: hal::device::Peripherals,
    src_mac: &[u8],
    ip: IpAddress,
    gateway_ip: Ipv4Address,
    interface_storage: &'a mut EthernetInterfaceStorage,
    clock_frequency_mhz: u32,
) -> (
    PB14<Output<PushPull>>,
    PB0<Output<PushPull>>,
    PE1<Output<PushPull>>,
    EthernetInterface<'a, EthernetDMA<'a>>,
    HardwareEntropy,
    Hub75<PINS0, DOUBLE_SCREEN_WIDTH>,
    Hub75<PINS1, DOUBLE_SCREEN_WIDTH>,
    Hub75<PINS2, DOUBLE_SCREEN_WIDTH>,
    DelayFromCountDownTimer<Timer<TIM2>>,
    Timer<TIM3>,
) {
    //setup power
    let pwr = dp.PWR.constrain();
    let pwrcfg = pwr.vos0(&dp.SYSCFG).freeze();

    // Link the SRAM3 power state to CPU1
    dp.RCC.ahb2enr.modify(|_, w| w.sram3en().set_bit());

    // Set up the system clock.
    let rcc = dp.RCC.constrain();

    let ccdr = rcc
        .sys_ck(clock_frequency_mhz.mhz())
        .hclk((clock_frequency_mhz / 2).mhz())
        .pll1_strategy(hal::rcc::PllConfigStrategy::Iterative)
        .freeze(pwrcfg, &dp.SYSCFG);

    let display_timer = dp.TIM2.timer(2.ms(), ccdr.peripheral.TIM2, &ccdr.clocks);
    let display_delay = DelayFromCountDownTimer::new(display_timer);

    // Initialise system...
    cp.SCB.invalidate_icache();
    cp.SCB.enable_icache();

    // Initialize (enable) the monotonic timer (CYCCNT)
    cp.DCB.enable_trace();
    cp.DWT.enable_cycle_counter();
    DWT::unlock();

    let gpioa = dp.GPIOA.split(ccdr.peripheral.GPIOA);
    let gpiob = dp.GPIOB.split(ccdr.peripheral.GPIOB);
    let gpioc = dp.GPIOC.split(ccdr.peripheral.GPIOC);
    let gpiod = dp.GPIOD.split(ccdr.peripheral.GPIOD);
    let gpioe = dp.GPIOE.split(ccdr.peripheral.GPIOE);
    let gpiog = dp.GPIOG.split(ccdr.peripheral.GPIOG);
    let _gpiof = dp.GPIOF.split(ccdr.peripheral.GPIOF);

    let _r1 = gpiod.pd0.into_push_pull_output().set_speed(VeryHigh);
    let _g1 = gpiod.pd1.into_push_pull_output().set_speed(VeryHigh);
    let _b1 = gpiod.pd2.into_push_pull_output().set_speed(VeryHigh);
    let _r2 = gpiod.pd3.into_push_pull_output().set_speed(VeryHigh);
    let _g2 = gpiod.pd4.into_push_pull_output().set_speed(VeryHigh);
    let _b2 = gpiod.pd5.into_push_pull_output().set_speed(VeryHigh);
    let _a = gpiod.pd12.into_push_pull_output().set_speed(VeryHigh);
    let _b = gpiod.pd13.into_push_pull_output().set_speed(VeryHigh);
    let _c = gpiod.pd14.into_push_pull_output().set_speed(VeryHigh);
    let _clk = gpiod.pd6.into_push_pull_output().set_speed(VeryHigh);
    let _strobe = gpiod.pd7.into_push_pull_output().set_speed(VeryHigh);
    let _oe = gpiod.pd11.into_push_pull_output().set_speed(VeryHigh);

    //initialize display 0
    let display0 =
        Hub75::<PINS0, DOUBLE_SCREEN_WIDTH>::new(4, unsafe { &mut *(0x58020C14 as *mut u16) });

    let _r1 = gpiob.pb1.into_push_pull_output().set_speed(VeryHigh);
    let _g1 = gpiob.pb2.into_push_pull_output().set_speed(VeryHigh);
    let _b1 = gpiob.pb3.into_push_pull_output().set_speed(VeryHigh);
    let _r2 = gpiob.pb4.into_push_pull_output().set_speed(VeryHigh);
    let _g2 = gpiob.pb5.into_push_pull_output().set_speed(VeryHigh);
    let _b2 = gpiob.pb6.into_push_pull_output().set_speed(VeryHigh);
    let _a = gpiob.pb10.into_push_pull_output().set_speed(VeryHigh);
    let _b = gpiob.pb11.into_push_pull_output().set_speed(VeryHigh);
    let _c = gpiob.pb12.into_push_pull_output().set_speed(VeryHigh);
    let _clk = gpiob.pb7.into_push_pull_output().set_speed(VeryHigh);
    let _strobe = gpiob.pb8.into_push_pull_output().set_speed(VeryHigh);
    let _oe = gpiob.pb9.into_push_pull_output().set_speed(VeryHigh);

    //initialize display 1
    let display1 =
        Hub75::<PINS1, DOUBLE_SCREEN_WIDTH>::new(4, unsafe { &mut *(0x58020414 as *mut u16) });

    let _r1 = gpioe.pe0.into_push_pull_output().set_speed(VeryHigh);
    let _g1 = gpioe.pe2.into_push_pull_output().set_speed(VeryHigh);
    let _b1 = gpioe.pe3.into_push_pull_output().set_speed(VeryHigh);
    let _r2 = gpioe.pe4.into_push_pull_output().set_speed(VeryHigh);
    let _g2 = gpioe.pe5.into_push_pull_output().set_speed(VeryHigh);
    let _b2 = gpioe.pe6.into_push_pull_output().set_speed(VeryHigh);
    let _a = gpioe.pe10.into_push_pull_output().set_speed(VeryHigh);
    let _b = gpioe.pe11.into_push_pull_output().set_speed(VeryHigh);
    let _c = gpioe.pe12.into_push_pull_output().set_speed(VeryHigh);
    let _clk = gpioe.pe7.into_push_pull_output().set_speed(VeryHigh);
    let _strobe = gpioe.pe8.into_push_pull_output().set_speed(VeryHigh);
    let _oe = gpioe.pe9.into_push_pull_output().set_speed(VeryHigh);

    //initialize display 2
    let display2 =
        Hub75::<PINS2, DOUBLE_SCREEN_WIDTH>::new(4, unsafe { &mut *(0x58021014 as *mut u16) });

    let _rmii_ref_clk = gpioa.pa1.into_alternate_af11().set_speed(VeryHigh);
    let _rmii_mdio = gpioa.pa2.into_alternate_af11().set_speed(VeryHigh);
    let _rmii_crs_dv = gpioa.pa7.into_alternate_af11().set_speed(VeryHigh);
    let _rmii_mdc = gpioc.pc1.into_alternate_af11().set_speed(VeryHigh);
    let _rmii_rxd0 = gpioc.pc4.into_alternate_af11().set_speed(VeryHigh);
    let _rmii_rxd1 = gpioc.pc5.into_alternate_af11().set_speed(VeryHigh);
    let _rmii_tx_en = gpiog.pg11.into_alternate_af11().set_speed(VeryHigh);
    let _rmii_txd0 = gpiog.pg13.into_alternate_af11().set_speed(VeryHigh);
    let _rmii_txd1 = gpiob.pb13.into_alternate_af11().set_speed(VeryHigh);

    let mac_addr = EthernetAddress::from_bytes(src_mac);
    let (eth_dma, eth_mac) = unsafe {
        ethernet::new_unchecked(
            dp.ETHERNET_MAC,
            dp.ETHERNET_MTL,
            dp.ETHERNET_DMA,
            &mut DES_RING,
            mac_addr.clone(),
            ccdr.peripheral.ETH1MAC,
            &ccdr.clocks,
        )
    };

    unsafe {
        ETH_PHY = Some(ethernet::phy::LAN8742A::new(eth_mac));
        ETH_PHY.as_mut().unwrap().phy_reset();
        ETH_PHY.as_mut().unwrap().phy_init();
        ethernet::enable_interrupt();
    }

    let iface = create_ethernet_iface(
        eth_dma,
        EthernetAddress::from_bytes(src_mac),
        ip,
        0,
        gateway_ip,
        interface_storage,
    );

    unsafe {
        RNG = Some(dp.RNG.constrain(ccdr.peripheral.RNG, &ccdr.clocks));
    }

    let led_r = gpiob.pb14.into_push_pull_output(); //ld3 - red
    let led_g = gpiob.pb0.into_push_pull_output(); //ld1 - green
    let led_b = gpioe.pe1.into_push_pull_output();
    let entropy = HardwareEntropy;

    let mut display_task_timer = dp
        .TIM3
        .tick_timer(30.mhz(), ccdr.peripheral.TIM3, &ccdr.clocks);
    display_task_timer.listen(timer::Event::TimeOut);

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

pub fn clear_display_timer_int(timer: &mut Timer<TIM3>){
    timer.clear_irq();
}

pub fn ethernet_interrupt_handler() {
    unsafe {
        ETH_PHY.as_mut().unwrap().poll_link();
        ethernet::interrupt_handler();
    }
}

pub fn check_ethernet_state() -> bool {
    unsafe {
        let phy_status = ETH_PHY.as_mut().unwrap().link_established();

        phy_status
    }
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
    let mut bytes_left = len;

    while bytes_left > 0 {
        let chunk = RNG.as_mut().unwrap().next().unwrap();
        let array = u32_to_u8_array(chunk);

        for i in 0..4 {
            if bytes_left <= 0 {
                break;
            }
            *data.offset((len - bytes_left) as isize) = array[i];
            bytes_left -= 1;
        }
    }

    *olen = len;

    return 0;
}
