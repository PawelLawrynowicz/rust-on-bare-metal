use core::{fmt::Write, usize};
use embedded_graphics::drawable::Drawable;
use embedded_graphics::prelude::Primitive;
use heapless::String;
use heapless::Vec;
use tinytga::Tga;

const SINGLE_DISPLAY_RESOLUTION: (usize, usize) = (32, 32);
const HORIZONTAL_DISPLAYS: usize = 4;
const VERTICAL_DISPLAYS: usize = 2;
pub const DOUBLE_SCREEN_WIDTH: usize = 256;
const SCREEN_RESOLUTION: (usize, usize) = (
    SINGLE_DISPLAY_RESOLUTION.0 * HORIZONTAL_DISPLAYS,
    SINGLE_DISPLAY_RESOLUTION.1 * VERTICAL_DISPLAYS,
);
const FONT_SIZE: (usize, usize) = (6, 8);
use embedded_graphics::{
    drawable::Pixel,
    fonts::{Font6x8, Text},
    image::Image,
    pixelcolor::Rgb888,
    prelude::{Point, Size},
    primitives::{Line, Triangle},
    style::PrimitiveStyle,
    style::TextStyle,
    DrawTarget,
};

use embedded_hal::blocking::delay::DelayUs;
use hub75::{Hub75, Pins};

pub struct ConnectedDisplays<
    const PIN_POSITIONS0: Pins,
    const PIN_POSITIONS1: Pins,
    const PIN_POSITIONS2: Pins,
> {
    display0: Hub75<PIN_POSITIONS0, DOUBLE_SCREEN_WIDTH>,
    display1: Hub75<PIN_POSITIONS1, DOUBLE_SCREEN_WIDTH>,
    // Display 2 is not used in current configuration
    display2: Hub75<PIN_POSITIONS2, DOUBLE_SCREEN_WIDTH>,
    brightness_bits: u8,
}

impl<const PIN_POSITIONS0: Pins, const PIN_POSITIONS1: Pins, const PIN_POSITIONS2: Pins>
    ConnectedDisplays<PIN_POSITIONS0, PIN_POSITIONS1, PIN_POSITIONS2>
{
    pub fn new(
        display0: Hub75<PIN_POSITIONS0, DOUBLE_SCREEN_WIDTH>,
        display1: Hub75<PIN_POSITIONS1, DOUBLE_SCREEN_WIDTH>,
        display2: Hub75<PIN_POSITIONS2, DOUBLE_SCREEN_WIDTH>,
        brightness_bits: u8,
    ) -> Self {
        let connected_displays = ConnectedDisplays {
            display0,
            display1,
            display2,
            brightness_bits,
        };
        connected_displays
    }

    pub fn output_bcm<DELAY: DelayUs<u8>>(&mut self, delay: &mut DELAY, delay_base_us: u8) {
        let shift = 8 - self.brightness_bits;

        // PWM cycle
        for bit in 0..self.brightness_bits {
            self.display0.output_single_bcm(delay, bit + shift);
            self.display1.output_single_bcm(delay, bit + shift);
            self.display2.output_single_bcm(delay, bit + shift);
            delay.delay_us(delay_base_us * (1 << bit))
        }
    }
}

impl<const PIN_POSITIONS0: Pins, const PIN_POSITIONS1: Pins, const PIN_POSITIONS2: Pins>
    DrawTarget<Rgb888> for ConnectedDisplays<PIN_POSITIONS0, PIN_POSITIONS1, PIN_POSITIONS2>
{
    //No errors expected
    type Error = core::convert::Infallible;
    fn draw_pixel(&mut self, mut item: Pixel<Rgb888>) -> Result<(), Self::Error> {
        let Pixel(coord, _color) = item;

        let (x, y) = (coord[0], coord[1]);

        //let row_number = y as usize / SINGLE_DISPLAY_RESOLUTION.1;
        let row_on_display = y as usize % SINGLE_DISPLAY_RESOLUTION.1;

        item.0 = Point::new(x, row_on_display as i32);
        //This part will need to be changed when the displays arrive, it is good for now
        if y >= 0 && y < SINGLE_DISPLAY_RESOLUTION.1 as i32 {
            self.display1.draw_pixel(item).ok();
        }
        if y >= SINGLE_DISPLAY_RESOLUTION.1 as i32 && y < (SINGLE_DISPLAY_RESOLUTION.1 as i32) * 2 {
            self.display2.draw_pixel(item).ok();
        }

        Ok(())
    }

    fn draw_iter<T>(&mut self, item: T) -> Result<(), Self::Error>
    where
        T: IntoIterator<Item = Pixel<Rgb888>>,
    {
        let pixels = item.into_iter();

        for pixel in pixels {
            self.draw_pixel(pixel).unwrap();
        }
        Ok(())
    }

    fn size(&self) -> Size {
        Size {
            width: SCREEN_RESOLUTION.0 as u32,
            height: SCREEN_RESOLUTION.1 as u32,
        }
    }

    fn clear(&mut self, _color: Rgb888) -> Result<(), Self::Error> {
        self.display0.clear(Rgb888::new(0, 0, 0)).ok();
        self.display1.clear(Rgb888::new(0, 0, 0)).ok();
        self.display2.clear(Rgb888::new(0, 0, 0)).ok();
        Ok(())
    }
}
pub struct Screen {
    display_regions: Vec<(usize, usize), 32>,
}
impl Screen {
    pub fn new() -> Self {
        let mut screen = Screen {
            display_regions: Vec::<(usize, usize), 32>::new(),
        };
        screen.generate_regions(SCREEN_RESOLUTION.0, SCREEN_RESOLUTION.1, FONT_SIZE);
        screen
    }
    fn generate_regions(
        &mut self,
        screen_width: usize,
        screen_height: usize,
        font_size: (usize, usize),
    ) {
        let mut regions = Vec::<(usize, usize), 32>::new();
        let min_vertical_space = font_size.1 * 2;
        let mut min_horizontal_space = (7 + 9 * font_size.0) / SINGLE_DISPLAY_RESOLUTION.0;

        min_horizontal_space = min_horizontal_space * SINGLE_DISPLAY_RESOLUTION.0;
        if (7 + 9 * font_size.0) % SINGLE_DISPLAY_RESOLUTION.0 != 0 {
            min_horizontal_space += SINGLE_DISPLAY_RESOLUTION.0;
        }

        for i in 0..(screen_height / min_vertical_space) {
            for j in 0..(screen_width / min_horizontal_space) {
                regions
                    .push((j * min_horizontal_space, i * min_vertical_space))
                    .unwrap();
            }
        }

        self.display_regions = regions;
    }
}

pub trait DrawableCrypto<DisplayT: DrawTarget<Rgb888>> {
    /// Methods incorportaing text objects and DrawablePrimitives
    fn draw_crypto(
        &mut self,
        display: &mut DisplayT,
        symbol: String<16>,
        price: Option<f32>,
        change: Option<f32>,
        font_size: (i32, i32),
        region: usize,
    );
    fn draw_intro(&mut self, display: &mut DisplayT);
    fn draw_wallet(&mut self, display: &mut DisplayT, wallet_value: f32, daily_change: f32);
    fn clear(&mut self, display: &mut DisplayT);
}
pub trait DrawablePrimitives<DisplayT: DrawTarget<Rgb888>> {
    /// Primitive drawing methods for non-text objects on the screen
    fn draw_down_arrow(&mut self, display: &mut DisplayT, x: i32, y: i32);
    fn draw_up_arrow(&mut self, display: &mut DisplayT, x: i32, y: i32);
    fn draw_line(&mut self, display: &mut DisplayT, x: i32, y: i32);
}

pub trait DrawHelpers {
    /// Helper methods
    fn calculate_change_x_pos(symbol_len: usize, change: f32) -> usize;
}

impl<DisplayT: DrawTarget<Rgb888>> DrawablePrimitives<DisplayT> for Screen {
    /// Draws a red arrow aimed downwards with base from x to y, looks best if y-x is odd
    /// *`x` - base beginning
    /// *`y` - base end
    fn draw_down_arrow(&mut self, display: &mut DisplayT, x: i32, y: i32) {
        let _triangle = Triangle::new(
            Point::new(x, y + 3),
            Point::new(x + 2, y + 5),
            Point::new(x + 4, y + 3),
        )
        .into_styled::<Rgb888>(PrimitiveStyle::with_fill(Rgb888::new(255, 0, 0)))
        .draw(display);
    }
    /// Draws a green arrow aimed upwards with base from x to y, looks best if y-x is odd
    /// *`x` - base beginning
    /// *`y` - base end
    fn draw_up_arrow(&mut self, display: &mut DisplayT, x: i32, y: i32) {
        let _triangle = Triangle::new(
            Point::new(x, y + 3),
            Point::new(x + 2, y + 1),
            Point::new(x + 4, y + 3),
        )
        .into_styled::<Rgb888>(PrimitiveStyle::with_fill(Rgb888::new(0, 255, 0)))
        .draw(display);
    }
    /// Draws a line for x to y
    /// *`x` - beginning of the line
    /// *`y` - end of the line (that's what Graves says when you ban him)
    fn draw_line(&mut self, display: &mut DisplayT, x: i32, y: i32) {
        let _line = Line::new(Point::new(x, y + 3), Point::new(x + 4, y + 3))
            .into_styled(PrimitiveStyle::with_stroke(Rgb888::new(0, 0, 255), 1))
            .draw(display);
    }
}
impl DrawHelpers for Screen {
    fn calculate_change_x_pos(symbol_len: usize, change: f32) -> usize {
        let mut offset = 3;
        if ((change < 10.0 && change > -10.0) && symbol_len != 4)
            || (symbol_len == 4 && (change > 9.9 || change < -9.9))
        {
            offset += 6;
        }
        if symbol_len < 3 && (change < 10.0 && change > -10.0) {
            offset += 6;
        }
        let change_x_pos = 7 + FONT_SIZE.0 as usize * symbol_len + offset;
        change_x_pos
    }
}

impl<DisplayT: DrawTarget<Rgb888>> DrawableCrypto<DisplayT> for Screen {
    fn draw_crypto(
        &mut self,
        display: &mut DisplayT,
        mut symbol: String<16>,
        price_tick: Option<f32>,
        price_24h: Option<f32>,
        font_size: (i32, i32),
        region: usize,
    ) {
        if self.display_regions.is_empty() {
            panic!("Display_regions empty Bruh");
        }
        // Selecting regions of the screen
        let region = self.display_regions.get(region).unwrap();
        let (x, y) = (region.0 as i32, region.1 as i32);

        // Shortening too long cryptocurrency symbols
        if symbol.len() > 5 {
            symbol = String::from(&symbol[0..5]);
        }
        // Drawing name
        let _crypto_name = Text::new(&symbol, Point::new(7 + x, y))
            .into_styled(TextStyle::new(Font6x8, Rgb888::new(255, 170, 0)))
            .draw(display);

        // Drawing price change in [%]
        match (price_tick, price_24h) {
            (Some(pt), Some(p24h)) => {
                // c stands for change
                let mut c = (pt - p24h) * 100.0 / pt;
                {
                    let temp = (c * 10.0) as i32;
                    c = (temp as f32) / 10.0
                }
                /*
                    Handling edge cases when:
                        - Change is over 99.99%; it would take too much space on the screen
                        - Different lengths of cryptocurrency symbols,
                          if the length is 4 or more the decimal point
                          is dropped depending on the change
                */
                if c > 99.99 {
                    c = 99.99;
                } else if c < -99.99 {
                    c = -99.99;
                }
                if c > 0.0 {
                    self.draw_up_arrow(display, 1 + x, y);
                } else if c < 0.0 {
                    self.draw_down_arrow(display, 1 + x, y);
                } else {
                    self.draw_line(display, 1 + x, y)
                }

                // If cryptocurrency symbol is longer than 4 letters, the change is rounded to an integer
                // .round() doesn't work, sadge
                if symbol.len() > 4 || (symbol.len() == 4 && (c > 9.9 || c < -9.9)) {
                    let temp = c as i32;
                    let decimal: f32 = c - temp as f32;
                    if decimal < 0.5 {
                        c = temp as f32;
                    } else {
                        c = (temp + 1) as f32;
                    }
                }
                // Drawing the artithmetic symbol for the change
                let mut change_string = if c >= 0.0 {
                    String::<32>::from("+")
                } else {
                    String::<32>::from("")
                };

                // Workaround reason: if symbol.len == 3 and the change is round it wont draw out ".0"
                // I don't have an idea how to do it otherwise
                if symbol.len() == 3 {
                    core::write!(change_string, "{:.1}", c).ok();
                } else {
                    core::write!(change_string, "{}", c).ok();
                }

                core::write!(change_string, "{}", "%").ok();

                let _change = Text::new(
                    change_string.as_str(),
                    Point::new(
                        x + Screen::calculate_change_x_pos(symbol.len(), c) as i32,
                        y,
                    ),
                )
                .into_styled(TextStyle::new(Font6x8, Rgb888::new(255, 170, 0)))
                .draw(display);

                /* !!!IF THE CODE WORKS WITHOUT THIS COMMENT, YOU CAN YEET IT OUT!!!
                // let mut price_string = String::<U32>::from("$");
                // core::write!(price_string, "{:.2}", pt).ok();
                // let _value = Text::new(price_string.as_str(), Point::new(1 + x, font_size.1 + y))
                //     .into_styled(TextStyle::new(Font6x8, Rgb888::new(255, 170, 0)))
                //     .draw(display);
                 */
            }
            // Drawing price
            (Some(pt), None) => {
                let mut price_string = String::<32>::from("$");
                core::write!(price_string, "{:.2}", pt).ok();
                let _value = Text::new(price_string.as_str(), Point::new(1 + x, font_size.1 + y))
                    .into_styled(TextStyle::new(Font6x8, Rgb888::new(255, 170, 0)))
                    .draw(display);
            }
            // If prices for the display have not been fetched yet "PENDING..." is drawn out instead
            _ => {
                let _value = Text::new("PENDING...", Point::new(1 + x, font_size.1 + y))
                    .into_styled(TextStyle::new(Font6x8, Rgb888::new(255, 170, 0)))
                    .draw(display);
            }
        }
    }

    // Drawing intro screen
    fn draw_intro(&mut self, display: &mut DisplayT) {
        let thaumatec_logo = include_bytes!("../img/thaumatec_tg_box.tga");
        let tga = Tga::from_slice(thaumatec_logo).unwrap();
        let image: Image<Tga, Rgb888> = Image::new(&tga, Point::zero());
        match image.draw(display) {
            Ok(()) => (),
            _ => panic!("Failed at drawing intro screen"),
        }
    }
    ///TODO:
    #[allow(unused_variables)]
    fn draw_wallet(&mut self, display: &mut DisplayT, wallet_value: f32, daily_change: f32) {
        unimplemented!();
    }
    // Clearing screen
    fn clear(&mut self, display: &mut DisplayT) {
        display.clear(Rgb888::new(0, 0, 0)).ok();
    }
}
