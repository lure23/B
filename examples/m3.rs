/*
* Just read some data from a Satel board.
*/
#![no_std]
#![no_main]

#[allow(unused_imports)]
use defmt::{info, debug, error, warn, panic};

#[cfg(feature = "run_with_espflash")]
use esp_println as _;
#[cfg(feature = "run_with_probe_rs")]
use defmt_rtt as _;

use esp_backtrace as _;

use esp_hal::{
    delay::Delay,
    gpio::{AnyPin, Output, OutputConfig, Level},
    i2c::master::{Config as I2cConfig, I2c},
    main,
    time::Rate,
    Blocking
};

use core::cell::RefCell;

extern crate just_b as uld;
use uld::VL53L5CX;

include!("./pins_gen.in");  // pins!

mod pl;
use pl::MyPlatform;

#[allow(non_snake_case)]
struct Pins {
    SDA: AnyPin,
    SCL: AnyPin,
    PWR_EN: AnyPin,
}

#[allow(non_upper_case_globals)]
const I2C_SPEED: Rate = Rate::from_khz(400);        // use max 400

#[main]
fn main() -> ! {
    init_defmt();   // skip if using 'espflash' 3.x

    let peripherals = esp_hal::init(esp_hal::Config::default());

    #[allow(non_snake_case)]
    let Pins{ SDA, SCL, PWR_EN } = pins!(peripherals);

    #[allow(non_snake_case)]
    let mut PWR_EN = Output::new(PWR_EN, Level::Low, OutputConfig::default());

    let i2c: I2c<'static,Blocking> = {
        let x = I2c::new(peripherals.I2C0, I2cConfig::default()
            .with_frequency(I2C_SPEED)
        ).unwrap();

        x
            .with_sda(SDA)
            .with_scl(SCL)
    };

    // Keep ownership of 'i2c' in 'main', while allowing it to be borrowed. If we moved it to 'MyPlatform',
    // there would be problems in the handling of it (the way the lib uses 'MaybeUninit' vs. 'esp-hal'
    // asserts); this also _feels_ like a better abstraction.
    //
    // Notes:
    //  - 'RefCell'. Even this is just a "struct + counter" wrap and moves the ownership. Not good for us.
    //
    let i2c = RefCell::new(i2c);
    let pl = MyPlatform::new(i2c);

    // Reset VL53L5CX(s) by pulling down their power for a moment
    {
        PWR_EN.set_low();
        blocking_delay_ms(10);      // 10ms based on UM2884 (PDF; 18pp) Rev. 6, Chapter 4.2
        PWR_EN.set_high();
        info!("Target powered off and on again.");
    }

    let /*mut*/ x = VL53L5CX::new_with_ping(pl).unwrap();

    let mut vl = x.init()
        .expect("initialize to succeed");

    info!("Init succeeded");

    // Hits the assert
    {
        vl.i2c_no_op()
            .expect("to pass");
        info!("I2C no-op (get power mode) succeeded");
    }

    //unreachable!();
    loop {}
}

const D_PROVIDER: Delay = Delay::new();

fn blocking_delay_ms(ms: u32) { D_PROVIDER.delay_millis(ms); }

/*
* Tell 'defmt' how to support '{t}' (timestamp) in logging.
*
* Note! 'defmt' sample insists the command to be: "(interrupt-safe) single instruction volatile
*       read operation". Our 'Instant::now' isn't, but sure seems to work.
*
* Reference:
*   - defmt book > ... > Hardware timestamp
*       -> https://defmt.ferrous-systems.com/timestamps#hardware-timestamp
*
* Note: If you use Embassy, a better way is to depend on 'embassy-time' and enable its
*       "defmt-timestamp-uptime-*" feature.
*/
fn init_defmt() {
    use esp_hal::time::Instant;

    defmt::timestamp!("{=u64:us}", {
        let now = Instant::now();
        now.duration_since_epoch().as_micros()
    });
}
