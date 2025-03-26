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
    gpio::{AnyPin, Input, InputConfig, Output, OutputConfig, Level},
    i2c::master::{Config as I2cConfig, I2c},
    main,
    time::{Instant, Rate}
};

extern crate just_b as uld;
use uld::{VL53L5CX, State_Ranging};

include!("./pins_gen.in");  // pins!

mod common;
use common::MyPlatform;

#[allow(non_snake_case)]
struct Pins {
    SDA: AnyPin,
    SCL: AnyPin,
    PWR_EN: AnyPin,
    INT: AnyPin
}

#[allow(non_upper_case_globals)]
const I2C_SPEED: Rate = Rate::from_khz(400);        // use max 400

#[main]
fn main() -> ! {
    #[cfg(feature="run_with_probe_rs")]
    init_defmt();

    let peripherals = esp_hal::init(esp_hal::Config::default());

    #[allow(non_snake_case)]
    let Pins{ SDA, SCL, PWR_EN, INT } = pins!(peripherals);

    #[allow(non_snake_case)]
    let mut PWR_EN = Output::new(PWR_EN, Level::Low, OutputConfig::default());

    #[allow(non_snake_case)]
    let INT = Input::new(INT, InputConfig::default());  // no pull

    let pl = {
        let x = I2c::new(peripherals.I2C0, I2cConfig::default()
            .with_frequency(I2C_SPEED)
        ).unwrap();

        let i2c_bus = x
            .with_sda(SDA)
            .with_scl(SCL);

        MyPlatform::new(i2c_bus)
    };

    // Reset VL53L5CX(s) by pulling down their power for a moment
    {
        PWR_EN.set_low();
        blocking_delay_ms(10);      // 10ms based on UM2884 (PDF; 18pp) Rev. 6, Chapter 4.2
        PWR_EN.set_high();
        info!("Target powered off and on again.");
    }

    let /*mut*/ vl = VL53L5CX::new_with_ping(pl).unwrap().init()
        .expect("initialize to succeed");

    info!("Init succeeded");

    // Extra test, to see basic comms work
    #[cfg(not(all()))]
    {
        vl.i2c_no_op()
            .expect("to pass");
        info!("I2C no-op (get power mode) succeeded");
    }

    //--- ranging loop
    //
    let mut ring: State_Ranging<4> = vl.start_ranging()
        .expect("to start ranging");

    //unreachable!();

    for round in 0..3 {
        let t0= Instant::now();

        // wait for 'INT' to fall
        loop {
            if INT.is_low() {
                debug!("INT after: {}", t0.elapsed());
                break;
            } else if t0.elapsed().as_millis() > 1000 {
                panic!("No INT detected");
            }
            blocking_delay_us(20);   // < 100us
        }

        let x = ring.is_ready().unwrap();
        assert!(x == true, "Interrupt seen but data isn't available");

        let (res, temp_degc) = ring.get_data()
            .expect("Failed to get data");

        info!("Data #{} ({})", round, temp_degc);

        #[cfg(feature = "target_status")]
        info!(".target_status:    {}", res.target_status);
        #[cfg(feature = "nb_targets_detected")]
        info!(".targets_detected: {}", res.targets_detected);

        #[cfg(feature = "distance_mm")]
        info!(".distance_mm:      {}", res.distance_mm);
    }

    loop {}
}

const D_PROVIDER: Delay = Delay::new();

fn blocking_delay_ms(ms: u32) { D_PROVIDER.delay_millis(ms); }
fn blocking_delay_us(us: u32) { D_PROVIDER.delay_micros(us); }

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
#[cfg(feature="run_with_probe_rs")]
fn init_defmt() {
    defmt::timestamp!("{=u64:us}", {
        let now = Instant::now();
        now.duration_since_epoch().as_micros()
    });
}
