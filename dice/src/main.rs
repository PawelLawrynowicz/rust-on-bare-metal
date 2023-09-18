#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

use dice_http::response;
use dice_http::Request;
use dice_http_client::crypto_api_client::CryptoApiClient;
use dice_http_client::cryptocompare_api_client::CryptoCompareApiClient;

use heapless::{FnvIndexMap, String, Vec};

use panic_semihosting as _; // panic handler

mod platform;
use platform::{hal, PINS0, PINS1, PINS2};

use cortex_m_semihosting::hprintln;
use rtic::app;

use embedded_hal::digital::v2::OutputPin;

use dice_common::display::DrawableCrypto;

use hal::gpio::{Output, PushPull};

use hal::gpio::gpiob::*;

use dice_common::network_utils::*;
use dice_common::smoltcp::{
    self,
    dhcp::Dhcpv4Client,
    phy::{Device, DeviceCapabilities},
    socket::{
        IcmpEndpoint, IcmpPacketMetadata, RawPacketMetadata, RawSocketBuffer, SocketSet,
        SocketSetItem, TcpSocket, TcpSocketBuffer,
    },
    wire::{IpCidr, Ipv4Address, Ipv6Cidr},
};

use dice_http::HttpServer;

mod tls_stack;

mod network_stack;
use network_stack::NetworkStack;

extern crate alloc;

use alloc_cortex_m::CortexMHeap;
use core::alloc::Layout;
use dice_common::display::DOUBLE_SCREEN_WIDTH;
use spin::Mutex;
use tls_stack::TlsLayer;

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();

use core::sync::atomic::{AtomicU32, Ordering};
mod webpages;
use dice_common::display as display_abstraction;

static mut INTERFACE_STORAGE: EthernetInterfaceStorage = EthernetInterfaceStorage {
    ip_storage: [IpCidr::Ipv6(Ipv6Cidr::SOLICITED_NODE_PREFIX)],
    neighbor_storage: [None; 8],
    routes_storage: [None],
};

pub const SOCKET_BUFFER_SIZE: usize = 600;

static mut TX_ICMP_BUFFER: [u8; SOCKET_BUFFER_SIZE] = [0; SOCKET_BUFFER_SIZE];
static mut RX_ICMP_BUFFER: [u8; SOCKET_BUFFER_SIZE] = [0; SOCKET_BUFFER_SIZE];

static mut SOCKET_STORAGE: IcmpSocketStorage = IcmpSocketStorage {
    tx_metadata: [IcmpPacketMetadata::EMPTY],
    rx_metadata: [IcmpPacketMetadata::EMPTY],
    tx_buffer: unsafe { &mut TX_ICMP_BUFFER },
    rx_buffer: unsafe { &mut RX_ICMP_BUFFER },
};

const DHCP_TX_BUFFER_SIZE: usize = 600;
const DHCP_RX_BUFFER_SIZE: usize = 900;

static mut TX_DHCP_BUFFER: [u8; DHCP_TX_BUFFER_SIZE] = [0; DHCP_TX_BUFFER_SIZE];
static mut RX_DHCP_BUFFER: [u8; DHCP_RX_BUFFER_SIZE] = [0; DHCP_RX_BUFFER_SIZE];
static mut TX_DHCP_METADATA: [RawPacketMetadata; 1] = [RawPacketMetadata::EMPTY];
static mut RX_DHCP_METADATA: [RawPacketMetadata; 1] = [RawPacketMetadata::EMPTY];

static mut TX_HTTPCLNT_BUFFER: [u8; 2048] = [0; 2048];
static mut RX_HTTPCLNT_BUFFER: [u8; 2048] = [0; 2048];
static mut TX_HTTPSVR_BUFFER: [u8; 16384] = [0; 16384];
static mut RX_HTTPSVR_BUFFER: [u8; 2048] = [0; 2048];

static mut SOCKETS_STORAGE: [Option<SocketSetItem>; 4] = [None, None, None, None];

static TIME: AtomicU32 = AtomicU32::new(0);

const SRC_MAC: [u8; 6] = [0x00, 0x60, 0xEE, 0xAD, 0xBE, 0xEF];

static mut NETWORK_STACK: Option<NetworkStack<platform::EthDeviceT>> = None;
static mut TLS_LAYER: Option<TlsLayer<NetworkStack<platform::EthDeviceT>>> = None;

const ALL_SYMBOLS: [&str; 128] = [
    "ETH", "BTC", "BNB", "XRP", "MATIC", "DOGE", "ETC", "ADA", "LTC", "DOT", "BCH", "EOS", "LINK",
    "FIL", "UNI", "XLM", "VET", "TRX", "BTT", "SOL", "LUNA", "OMG", "CAKE", "YFI", "HT", "QTUM",
    "NEO", "SUSHI", "OKB", "BSV", "AAVE", "THETA", "ONT", "ZEC", "KSM", "XVS", "RUNE", "DASH",
    "CHZ", "MKR", "ATOM", "SXP", "BAKE", "ENJ", "WAVES", "XMR", "MANA", "ONE", "WRX", "XTZ", "CRV",
    "FTT", "HOT", "IOST", "AVAX", "HBAR", "FTM", "ZEN", "MIOTA", "ZIL", "CHR", "GRT", "KAVA", "SC",
    "ALGO", "WBTC", "1INCH", "COMP", "LSK", "ZRX", "SRM", "FLOW", "BAT", "NANO", "SNX", "XEM",
    "ICX", "RSR", "REEF", "BURGER", "RLC", "TRB", "KLAY", "EGLD", "ONGAS", "NEAR", "DENT", "RVN",
    "LRC", "VTHO", "KNC", "WIN", "ANKR", "BAND", "JST", "BTG", "BNT", "REP", "CRO", "ALPHA",
    "OCEAN", "HIVE", "TFUEL", "STORJ", "YFII", "SUN", "OGN", "STMX", "COTI", "GT", "MTL", "MLK",
    "MONA", "SNT", "DGB", "QKC", "CELR", "INJ", "SOC", "PAXG", "REN", "UNFI", "NKN", "CELO", "BAL",
    "STEEM", "DAI", "GERO",
];

static mut CANVAS: Option<display_abstraction::Screen> = None;
static mut CONNECTED_DISPLAYS: Option<display_abstraction::ConnectedDisplays<PINS0, PINS1, PINS2>> =
    None;

static mut CONFIG_SYMBOLS: Option<Mutex<Vec<String<16>, 64>>> = None;

pub fn index_get<const SIZE: usize>(_request: Request, _body: &[u8]) -> String<SIZE> {
    webpages::index_get(&ALL_SYMBOLS)
}

pub fn index_post<const SIZE: usize>(_request: Request, body: &[u8]) -> String<SIZE> {
    #[cfg(feature = "use_semihosting")]
    hprintln!("{}", core::str::from_utf8(body).unwrap()).ok();

    let symbols = webpages::parse_post_body(core::str::from_utf8(body).unwrap());
    unsafe {
        let cs = CONFIG_SYMBOLS.as_mut().unwrap();
        let mut csval = cs.lock();
        *csval = symbols;
    }

    return response::redirect_response("/");
}

fn get_default_symbols() -> Vec<String<16>, 64> {
    let mut vector = Vec::new();
    for i in 0..8 {
        vector.push(String::from(ALL_SYMBOLS[i])).unwrap();
    }

    vector
}
#[app(device = crate::hal::stm32, peripherals = true, monotonic = rtic::cyccnt::CYCCNT)]
const APP: () = {
    struct Resources {
        led_r: PB14<Output<PushPull>>,
        led_g: PB0<Output<PushPull>>,
        led_b: platform::LedBType,
        selected_symbols: Vec<String<16>, 64>,
        //tuple contains actual price and a base price updated every 24 hours used to calculate 24h% change
        prices: FnvIndexMap<String<16>, (Option<f32>, Option<f32>), 16>,
        device_capabilities: DeviceCapabilities,
        http_server: HttpServer<128, 16384, 16, 2048, 20>,
        display_delay: platform::DisplayDelayProvider,
        display_task_timer: platform::DisplayTaskTimer,
    }

    #[init(schedule = [stack_poll, server_poll, time_tick, update_24h, update_prices_task, config_update_task])]
    fn init(cx: init::Context) -> init::LateResources {
        //initialize the allocator
        let start = cortex_m_rt::heap_start() as usize;
        let size = 64 * 1024; // in bytes
        unsafe { ALLOCATOR.init(start, size) }

        let ip = Ipv4Address::UNSPECIFIED.into();
        let gw_ip = Ipv4Address::UNSPECIFIED;

        //take ownership of peripherals
        let cp = cx.core;
        let dp = cx.device;

        let (
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
        ) = platform::init(
            cp,
            dp,
            &SRC_MAC,
            ip,
            gw_ip,
            unsafe { &mut INTERFACE_STORAGE },
            platform::CLOCK_FREQ_MHZ,
        );

        unsafe {
            CANVAS = Some(dice_common::display::Screen::new());
        }

        unsafe {
            CONNECTED_DISPLAYS = Some(
                display_abstraction::ConnectedDisplays::<PINS0, PINS1, PINS2>::new(
                    display0, display1, display2, 4,
                ),
            );
        }

        unsafe {
            CANVAS
                .as_mut()
                .unwrap()
                .draw_intro(CONNECTED_DISPLAYS.as_mut().unwrap())
        }
        unsafe {
            CONFIG_SYMBOLS = Some(Mutex::new(get_default_symbols()));
        }

        let device_capabilities = iface.device().capabilities();

        let mut socket_set = SocketSet::new(unsafe { &mut SOCKETS_STORAGE[..] });

        let httpsvr_tcp_rx_buffer = TcpSocketBuffer::new(unsafe { &mut RX_HTTPSVR_BUFFER[..] });
        let httpsvr_tcp_tx_buffer = TcpSocketBuffer::new(unsafe { &mut TX_HTTPSVR_BUFFER[..] });

        let httpsvr_tcp_socket = TcpSocket::new(httpsvr_tcp_rx_buffer, httpsvr_tcp_tx_buffer);

        let http_socket_handle = socket_set.add(httpsvr_tcp_socket);

        let mut http_server = HttpServer::<128, 16384, 16, 2048, 20>::new(http_socket_handle, 80);

        http_server.add_route("GET", "/", index_get).ok();
        http_server.add_route("POST", "/", index_post).ok();
        http_server
            .add_route("GET", "/styles.css", webpages::styles_get)
            .ok();

        let mut icmp_socket = create_icmp_socket(unsafe { &mut SOCKET_STORAGE });
        icmp_socket.bind(IcmpEndpoint::Ident(1)).unwrap();

        socket_set.add(icmp_socket);

        let httpclnt_tcp_rx_buffer = TcpSocketBuffer::new(unsafe { &mut RX_HTTPCLNT_BUFFER[..] });
        let httpclnt_tcp_tx_buffer = TcpSocketBuffer::new(unsafe { &mut TX_HTTPCLNT_BUFFER[..] });

        let httpclnt_tcp_socket = TcpSocket::new(httpclnt_tcp_rx_buffer, httpclnt_tcp_tx_buffer);

        let http_client_socket_handle = socket_set.add(httpclnt_tcp_socket);

        let dhcp_rx_buffer = RawSocketBuffer::new(unsafe { &mut RX_DHCP_METADATA[..] }, unsafe {
            &mut RX_DHCP_BUFFER[..]
        });
        let dhcp_tx_buffer = RawSocketBuffer::new(unsafe { &mut TX_DHCP_METADATA[..] }, unsafe {
            &mut TX_DHCP_BUFFER[..]
        });

        let dhcp_client = Dhcpv4Client::new(
            &mut socket_set,
            dhcp_rx_buffer,
            dhcp_tx_buffer,
            smoltcp::time::Instant::from_millis(0),
        );

        let socket_handles = [http_client_socket_handle];

        let network_stack =
            NetworkStack::new(iface, socket_set, &socket_handles, Some(dhcp_client));

        unsafe {
            NETWORK_STACK = Some(network_stack);
            TLS_LAYER = Some(TlsLayer::new(NETWORK_STACK.as_mut().unwrap()));

            TLS_LAYER.as_mut().unwrap().init(entropy);
        }

        //TODO: Cap max selected cryptos to 60 (limited by API Url param length) or divide into chunks in the driver and make multiple requests

        let prices = FnvIndexMap::new();

        let period = rtic::cyccnt::U32Ext::cycles(platform::CLOCK_FREQ_MHZ * 1000);
        //start counting milliseconds
        cx.schedule.time_tick(cx.start + period).unwrap();

        //start polling tasks

        cx.schedule.stack_poll(cx.start + period).unwrap();
        cx.schedule.server_poll(cx.start + period).unwrap();
        cx.schedule.config_update_task(cx.start + period).unwrap();

        cx.schedule.update_prices_task(cx.start + period).unwrap();

        init::LateResources {
            led_r,
            led_g,
            led_b,
            device_capabilities,
            selected_symbols: Vec::new(),
            prices,
            http_server,
            display_delay,
            display_task_timer,
        }
    }

    #[idle(resources = [led_g])]
    fn idle(cx: idle::Context) -> ! {
        let led_g = cx.resources.led_g;

        //Spectacular animation
        loop {
            led_g.set_high().unwrap();
            cortex_m::asm::delay(platform::CLOCK_FREQ_MHZ * 500000);
            led_g.set_low().unwrap();
            cortex_m::asm::delay(platform::CLOCK_FREQ_MHZ * 500000);
        }
    }

    #[task(resources=[selected_symbols, prices], schedule=[update_prices_task], priority=1)]
    fn update_prices_task(cx: update_prices_task::Context) {
        let period = rtic::cyccnt::U32Ext::cycles(platform::CLOCK_FREQ_MHZ * 1000000 * 4);

        //unsafe only because we access static mutables
        unsafe {
            if NETWORK_STACK.as_mut().unwrap().is_ip_unspecified() {
                cx.schedule
                    .update_prices_task(cx.scheduled + period)
                    .unwrap();
                return;
            }

            let tls = TLS_LAYER.as_mut().unwrap();

            let result = CryptoCompareApiClient::get_current_prices(
                tls,
                cx.resources.selected_symbols,
                &"USD",
            );

            if let Ok(res) = result {
                let prices = cx.resources.prices;
                for (key, val) in res.iter() {
                    if prices.contains_key(key) {
                        let (price, _change) = prices.get_mut(key).unwrap();
                        *price = Some(val.clone());
                    }
                }

                CANVAS
                    .as_mut()
                    .unwrap()
                    .clear(CONNECTED_DISPLAYS.as_mut().unwrap());

                let mut region = 0;

                for (symbol, (price_tick, price_24h)) in prices.into_iter() {
                    unsafe {
                        CANVAS.as_mut().unwrap().draw_crypto(
                            CONNECTED_DISPLAYS.as_mut().unwrap(),
                            symbol.clone(),
                            *price_tick,
                            *price_24h,
                            (6, 8),
                            region,
                        );
                    }
                    #[cfg(feature = "use_semihosting")]
                    hprintln!("{:?}", region).ok();
                    region += 1;
                }

                // #[cfg(feature = "use_semihosting")]
                // hprintln!("{:?}", prices).ok();
            }
        }

        cx.schedule
            .update_prices_task(cx.scheduled + period)
            .unwrap();
    }

    #[task(resources=[selected_symbols, prices], schedule=[update_24h], priority=1)]
    fn update_24h(cx: update_24h::Context) {
        let period = rtic::cyccnt::U32Ext::cycles(platform::CLOCK_FREQ_MHZ * 1000000);

        //unsafe only because we access static mutables
        unsafe {
            if NETWORK_STACK.as_mut().unwrap().is_ip_unspecified() {
                cx.schedule.update_24h(cx.scheduled + period).unwrap();
                return;
            }

            let tls = TLS_LAYER.as_mut().unwrap();

            let result = CryptoCompareApiClient::get_openday_price(
                tls,
                cx.resources.selected_symbols,
                &"USD",
            );

            if let Ok(res) = result {
                let prices = cx.resources.prices;

                for (key, val) in res.iter() {
                    if prices.contains_key(key) {
                        let (_price, base_24) = prices.get_mut(key).unwrap();
                        *base_24 = Some(val.clone());
                    }
                }
                //schedule for the next day
                return;
            }

            //If failed, try again in 1 second
            cx.schedule.update_24h(cx.scheduled + period).unwrap();
        }
    }

    #[task(resources=[], schedule=[stack_poll], priority=3)]
    fn stack_poll(cx: stack_poll::Context) {
        let stack = unsafe { NETWORK_STACK.as_mut().unwrap() };
        let tls_stack = unsafe { TLS_LAYER.as_mut().unwrap() };

        match platform::check_ethernet_state() {
            true => {
                stack.poll(TIME.load(Ordering::Relaxed)).ok();
            }

            false => {
                tls_stack.handle_disconnected();
                stack.handle_link_reset();
            }
        };

        //1ms period
        let period = rtic::cyccnt::U32Ext::cycles(platform::CLOCK_FREQ_MHZ * 1000);

        cx.schedule.stack_poll(cx.scheduled + period).unwrap();
    }

    #[task(resources = [device_capabilities, http_server], schedule=[server_poll], priority=2)]
    fn server_poll(cx: server_poll::Context) {
        let socket_set_ref = unsafe { NETWORK_STACK.as_mut().unwrap().get_socket_set() };

        if let Some(mut set) = socket_set_ref {
            cx.resources.http_server.poll(&mut set);
            //drop the mutexguard to prevent deadlock
            drop(set);
        }

        let period = rtic::cyccnt::U32Ext::cycles(platform::CLOCK_FREQ_MHZ * 10000);
        cx.schedule.server_poll(cx.scheduled + period).unwrap();
    }

    #[task(resources = [selected_symbols, prices], schedule=[config_update_task], spawn=[update_24h], priority=1)]
    fn config_update_task(cx: config_update_task::Context) {
        let period = rtic::cyccnt::U32Ext::cycles(platform::CLOCK_FREQ_MHZ * 1000);

        let cs = unsafe { CONFIG_SYMBOLS.as_mut().unwrap() };
        let cs = cs.try_lock();

        if let Some(mut vec) = cs {
            if vec.len() > 0 {
                let selected = cx.resources.selected_symbols;
                let prices = cx.resources.prices;

                *selected = Vec::from_slice(&vec.clone()).unwrap();

                vec.clear();
                prices.clear();

                //reset prices map
                for element in selected {
                    prices.insert(element.clone(), (None, None)).unwrap();
                }

                cx.spawn.update_24h().unwrap();

                #[cfg(feature = "use_semihosting")]
                hprintln!("{:?}", prices).ok();
            }
        }

        cx.schedule
            .config_update_task(cx.scheduled + period)
            .unwrap();
    }

    #[task(schedule=[time_tick], priority = 3)]
    fn time_tick(cx: time_tick::Context) {
        TIME.fetch_add(1, Ordering::Relaxed);
        let period = rtic::cyccnt::U32Ext::cycles(platform::CLOCK_FREQ_MHZ * 1000);
        cx.schedule.time_tick(cx.scheduled + period).unwrap();
    }

    #[task(binds = ETH)]
    fn ethernet_event(_ctx: ethernet_event::Context) {
        platform::ethernet_interrupt_handler();
    }

    #[task(binds = TIM3, resources = [display_delay, display_task_timer, led_b], priority=4)]
    fn display(cx: display::Context) {
        cx.resources.led_b.set_high().unwrap();
        unsafe {
            CONNECTED_DISPLAYS
                .as_mut()
                .unwrap()
                .output_bcm(cx.resources.display_delay, 1)
        };
        cx.resources.led_b.set_low().unwrap();

        platform::clear_display_timer_int(cx.resources.display_task_timer);
    }

    //usused interrupts used to dispatch software tasks
    extern "C" {
        fn EXTI0();
        fn EXTI1();
        fn EXTI2();
        fn EXTI3();
        fn EXTI4();
    }
};

#[alloc_error_handler]
fn oom(_: Layout) -> ! {
    panic!("Allocation error!");
}
