#![no_std]
#![allow(non_snake_case)]

mod platform;
mod state_hp_idle;
mod uld_raw;

use defmt::{debug, error, Format};

use core::{
    fmt::{Display, Formatter},
    result::Result as CoreResult,
};

pub use {
    platform::Custom,
    state_hp_idle::State_HP_Idle,
};

use crate::uld_raw::{
    VL53L5CX_Configuration,
    vl53l5cx_init,
    ST_OK, ST_ERROR,
};

pub type Result<T> = core::result::Result<T,Error>;

#[cfg_attr(feature = "_defmt", derive(defmt::Format))]
#[derive(core::fmt::Debug)]
pub struct Error(pub u8);

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "ULD driver or hardware error ({})", self.0)
    }
}

pub const DEFAULT_I2C_ADDR: I2cAddr = I2cAddr::from_8bit(0x52);    // default after each power on

/*
* Adds a method to the ULD C API struct.
*
* Note: Since the C-side struct has quite a lot of internal "bookkeeping" fields, we don't expose
*       this directly to Rust customers, but wrap it. We *could* also consider making
*       those fields non-pub in the 'bindgen' phase, and be able to pass this struct, directly. #design
*/
impl VL53L5CX_Configuration {
    /** @brief Returns a default 'VL53L5CX_Configuration' struct, spiced with the application
       * provided 'Platform'-derived state (opaque to us, except for its size).
       *
       * Initialized state is (as per ULD C code):
       *   <<
       *       .platform: dyn Platform     = anything the app keeps there
       *       .streamcount: u8            = 0 (undefined by ULD C code)
       *       .data_read_size: u32        = 0 (undefined by ULD C code)
       *       .default_configuration: *mut u8 = VL53L5CX_DEFAULT_CONFIGURATION (a const table)
       *       .default_xtalk: *mut u8     = VL53L5CX_DEFAULT_XTALK (a const table)
       *       .offset_data: [u8; 488]     = data read from the sensor
       *       .xtalk_data: [u8; 776]      = copy of 'VL53L5CX_DEFAULT_XTALK'
       *       .temp_buffer: [u8; 1452]    = { being used for multiple things }
       *       .is_auto_stop_enabled: u8   = 0
       *   <<
       *
       * Side effects:
       *   - the sensor is reset, and firmware uploaded to it
       *   - NVM (non-volatile?) data is read from the sensor to the driver
       *   - default Xtalk data programmed to the sensor
       *   - default configuration ('.default_configuration') written to the sensor
       *   - four bytes written to sensor's DCI memory at '0xDB80U' ('VL53L5CX_DCI_PIPE_CONTROL'):
       *       {VL53L5CX_NB_TARGET_PER_ZONE, 0x00, 0x01, 0x00}
       *   - if 'NB_TARGET_PER_ZONE' != 1, 1 byte updated at '0x5478+0xc0' ('VL53L5CX_DCI_FW_NB_TARGET'+0xc0)  // if I got that right!?!
       *       {VL53L5CX_NB_TARGET_PER_ZONE}
       *   - one byte written to sensor's DCI memory at '0xD964' ('VL53L5CX_DCI_SINGLE_RANGE'):
       *       {0x01}
       *   - two bytes updated at sensor's DCI memory at '0x0e108' ('VL53L5CX_GLARE_FILTER'):
       *       {0x01, 0x01}
    */
    fn init_with(mut p: impl Custom) -> Result<Self> {
        use core::{
            mem::MaybeUninit,
            ptr::addr_of_mut
        };
        #[allow(unused_imports)]
        use core::ffi::c_void;

        let ret: Result<VL53L5CX_Configuration> = unsafe {
            let mut uninit = MaybeUninit::<VL53L5CX_Configuration>::uninit();
                // note: use '::zeroed()' in place of '::uninit()' to get more predictability
            let up = uninit.as_mut_ptr();

            // Check that the size and alignments are as expected.
            {
                let pp = addr_of_mut!((*up).platform);

                // Check size and alignment
                //
                // Note: It's difficult (it seems) to make the C side both 8-aligned and 20-wide.
                //      The compiler makes the struct 24-wide, in that case. So we allow the gap.
                //
                let sz_c = size_of_val(&(*up).platform);
                let sz_rust = size_of_val(&p);
                assert!(sz_c >= sz_rust, "Tunnel C side isn't wide enough");   // edit 'platform.h' to adjust

                let al_rust = align_of_val(&p);
                assert!( (pp as usize)%al_rust == 0 ||false, "bad alignment on C side (needs {})", al_rust );

                debug!("C size: {}, Rust size and alignment: {} {}", sz_c, sz_rust, al_rust );  // 24 20 4
            }

            // Make a bitwise copy of 'Custom' in 'uninit.platform'; ULD C 'vl.._init()' will need it,
            // to access the I2C bus (below).
            //
            // Note! Very important 'Custom' doesn't get dropped.
            {
                let pp = addr_of_mut!((*up).platform);

                *(pp as *mut &mut dyn Custom) = (&mut p) as &mut dyn Custom;
                core::mem::forget(p);

                /*** shoo off
                // "cannot transmute between types of different sizes, or dependently-sized types"
                // *pp = unsafe { transmute(p) };

                //(*pp).__ = unsafe { transmute(&mut p as *mut dyn Custom) };

                // works, but allows a Drop
                // *(pp as *mut &mut dyn Custom) = (&mut p) as &mut dyn Custom;

                // this is just for getting fields from within struct
                //let mut a = unsafe { core::ptr::read(&p) };     // moves data out, ensures it’s not dropped

                //let dd: &dyn Custom = unsafe { transmute(p) };

                *(_pp as *mut &mut dyn Custom) = (&mut p) as &mut dyn Custom;
                ***/
            }

            // Initialize those fields we know C API won't touch (just in case)
            addr_of_mut!((*up).streamcount).write(u8::MAX);
            addr_of_mut!((*up).data_read_size).write(u32::MAX);

            // Call ULD C API to arrange the rest
            //
            // Note: Already this will call the platform methods (via the tunnel).
            //
            match vl53l5cx_init(up) {
                ST_OK => Ok(uninit.assume_init()),  // we guarantee it's now initialized
                e => Err(Error(e))
            }
        };
        ret
    }
}

/*
* Access to a single VL53L5CX sensor.
*/
pub struct VL53L5CX<P: Custom + 'static> {
    p: P
}

impl<P: Custom + 'static> VL53L5CX<P> {
    /*
    * Instead of just creating this structure, this already pings the bus to see, whether there's
    * a suitable sensor out there.
    */
    pub fn new_with_ping(/*move*/ mut p: P) -> Result<Self> {
        match Self::ping(&mut p) {
            Err(_) => Err(Error(ST_ERROR)),
            Ok(()) => Ok(Self{ p })
        }
    }

    pub fn init(self) -> Result<State_HP_Idle> {
        let uld = VL53L5CX_Configuration::init_with(self.p)?;

        Ok( State_HP_Idle::new(uld) )
    }

    fn ping(p: &mut P) -> CoreResult<(),()> {
        match vl53l5cx_ping(p)? {
            (a@ 0xf0, b@ 0x02) => {     // vendor driver ONLY proceeds with this
                debug!("Ping succeeded: {=u8:#04x},{=u8:#04x}", a,b);
                Ok(())
            },
            t => {
                error!("Unexpected '(device id, rev id)': {:#04x}", t);
                Err(())
            }
        }
    }
}

/**
* Function, modeled akin to the vendor ULD 'vl53l5cx_is_alive()', but:
*   - made in Rust
*   - returns the device and revision id's
*
* This is the only code that the ULD C driver calls on the device, prior to '.init()', i.e. it
* is supposed to be functioning also before the firmware and parameters initialization.
*
* Note:
*   - Vendor's ULD C driver expects '(0xf0, 0x02)'.
*/
fn vl53l5cx_ping<P : Custom>(pl: &mut P) -> CoreResult<(u8,u8),()> {
    let mut buf = [u8::MAX;2];

    pl.wr_bytes(0x7fff, &[0x00]);
    pl.rd_bytes(0, &mut buf);   // [dev_id, rev_id]
    pl.wr_bytes(0x7fff, &[0x02]);

    Ok( (buf[0], buf[1]) )
}

/*
* Wrapper to eliminate 8-bit vs. 7-bit I2C address misunderstandings.
*
* Note: Not using 'esp-hal' 'i2c::master::I2cAddress' to keep the door ever so slightly ajar for
*       other MCU families. If someone wants to do the work.
*/
#[derive(Copy,Clone,Eq,PartialEq)]
pub struct I2cAddr(u8);     // stored as 7-bit (internal detail)

impl I2cAddr {
    pub const fn from_8bit(v: u8) -> Self {
        assert!(v % 2 == 0, "8-bit I2C address is expected to be even");
        Self(v >> 1)
    }
    pub fn from_7bit(v: u8) -> Self {
        assert!(v < 0x80, "not 7-bit");
        Self(v)
    }
    pub const fn as_7bit(&self) -> u8 { self.0 }      // used by platform code (needs to be 'pub')
    //fn as_8bit(&self) -> u8 { self.0 << 1 }
}

#[cfg(feature = "_defmt")]
impl Format for I2cAddr {
    fn format(&self, fmt: defmt::Formatter) {
        // 'esp-hal' (as most of the world) uses 7-bit I2C addresses, but the vendor uses 8-bit.
        // It IS confusing, but don't want to go full 8-bit. Treating vendor as the exception!
        defmt::write!(fmt, "{=u8:#04x}_u7", self.as_7bit());
    }
}
