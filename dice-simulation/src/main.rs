use dice_common::display;
use dice_common::display::DrawableCrypto;
use embedded_graphics::{
    pixelcolor::Rgb888,
    prelude::*,
};
use embedded_graphics_simulator::{OutputSettingsBuilder, SimulatorDisplay, Window};
use heapless::FnvIndexMap;

pub fn draw_simulation() {
    let mut display: SimulatorDisplay<Rgb888> = SimulatorDisplay::new(Size::new(128, 64));
    let output_settings = OutputSettingsBuilder::new().build();
    let mut screen = Some(display::Screen::new());

    let mut window = Window::new("DICE Project", &output_settings);
    let crypto_currency = populate_cryptos();

    let mut region = 0;

    for (symbol, (price, change)) in crypto_currency.into_iter() {
        screen.as_mut().unwrap().draw_crypto(
            &mut display,
            symbol.clone(),
            *price,
            *change,
            (6, 8),
            region,
        );
        region += 1;
    }

    window.show_static(&display);
}

fn main() {
    draw_simulation();
}

fn populate_cryptos() -> FnvIndexMap<heapless::String<16>, (Option<f32>, Option<f32>), 32> {
    let mut cryptos = FnvIndexMap::<heapless::String<16>, (Option<f32>, Option<f32>), 32>::new();

    let symbols = [
        "BTC",
        "ETH", //"DOG", "ADA", "AVA", "BTT", "DAI", "EOS", "FIO", "GRT", "LTC", "SXP", "VET",
              //"VIN", "XMR", "XTZ", "ZEC", "ZIL",
    ];
    let price = [
        Some(45087.00),
        None, //123.33, 8.88, 12345.12, 123.87, 0.01, 234.65, 2406.13, 13245.80,
              //6545.13, 123.57, 1987.22, 12.36, 76.34, 213.12, 2.23, 1.23,
    ];
    let change = [
        Some(12.1),
        None,
        //-1.3, 1000.2, 23.4, 233.2, 7.1, -11.4, 99.9, 0.1, 10.2, 1.3, 6.4, 87.4, 3.5,
        //-9999.9, -10.2, -1.2,
    ];
    for (i, sy) in symbols.iter().enumerate() {
        cryptos
            .insert(heapless::String::from(*sy), (price[i], change[i]))
            .unwrap();
    }

    cryptos
}
