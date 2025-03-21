/*
* State_HP_Idle
*
* The vendor specs |*| state the sensor has three states (power-off state omitted):
*
*   - HP Idle   // allows transition to the other two
*   - Ranging
*   - LP Idle   // we don't (currently) support this mode
*
* The larger point is that the Rust API reflects the states. You can have the sensor presented
* as 'State_HP_Idle', but if you transit to ranging, you no longer have access to that state (unless
* the ranging is ended, in which case you may take it back).
*
*   [*]: DS13754 - Rev 12, p.9
*/
#[cfg(feature = "_defmt")]
use defmt::panic;

use crate::{
    platform,
    state_ranging::{
        RangingConfig,
        State_Ranging,
    },
    uld_raw::{
        vl53l5cx_get_power_mode,
        VL53L5CX_Configuration
    },
    Error,
    I2cAddr,
    Result,
    ST_OK
};

/*
* The "HP Idle" state (vendor terminology): firmware has been downloaded; ready to range.
*/
#[allow(non_camel_case_types)]
pub struct State_HP_Idle {
    // The vendor ULD driver wants to have a "playing ground" (it's called 'Dev', presumably for
    // "device"), in the form of the "configuration" struct. It's not really configuration;
    // more of a driver working memory area where all the state and buffers exist.
    //
    // The good part of this arrangement is, we have separate state when handling multiple sensors. :)
    //
    // The "state" also carries our 'Platform' struct within it. The ULD code uses it to reach back
    // to the app level, for MCU hardware access.
    //
    // The "state" can be read, but we "MUST not manually change these field[s]". In this Rust API,
    // the whole "state" is kept private, to enforce such read-only nature.
    //
    uld: VL53L5CX_Configuration,
}

impl State_HP_Idle {
    pub(crate) fn new(uld: VL53L5CX_Configuration) -> Self {
        Self{ uld }
    }

    //---
    // Ranging (getting values)
    //
    pub fn start_ranging<const DIM: usize>(/*move*/ self, cfg: &RangingConfig<DIM>) -> Result<State_Ranging<DIM>> {
        let r = State_Ranging::transition_from(self, cfg)?;
        Ok(r)
    }

    /*
    * Change the I2C address on-the-fly and continue the session with the new I2C address.
    *
    * Unlike other functions, we don't refer to the ULD C API because the changing of the address
    * mechanism there.. is very.. intrusive(??). Instead, we do the full bytewise comms here, in
    * Rust side, allowing us to make a callback in the middle of the dance. :)
    */
    pub fn set_i2c_address(&mut self, addr: &I2cAddr) -> Result<()> {

        // Implementation based on ULD C API 'vl53l5cx_set_i2c_address'

        platform::with(&mut self.uld.platform, |pl| -> core::result::Result<(),()> {
            pl.wr_bytes(0x7fff, &[0])?;
            pl.wr_bytes(0x4, &[addr.as_7bit()])?;
            pl.addr_changed(addr);

            pl.wr_bytes(0x7fff, &[2])?;  // now with the new I2C address
            Ok(())
        }).expect("writing to I2C to succeed");

        // Further comms will happen to the new address. Let's still make a small access with the
        // new address, e.g. reading something.
        //
        let _ = self.i2c_no_op().map_err(|_| {
            panic!("Device wasn't reached after its I2C address changed.");
        });

        Ok(())
    }

    /* I2C access without consequences
    */
    pub /*<-- for debugging*/ fn i2c_no_op(&mut self) -> Result<()> {
        let mut tmp: u8 = 0;
        match unsafe { vl53l5cx_get_power_mode(&mut self.uld, &mut tmp) } {
            ST_OK => Ok(()),
            e => Err(Error(e))
        }
    }

    /*R not needed
    pub(crate) fn borrow_uld(&self) -> &VL53L5CX_Configuration {
        &self.uld
    }*/

    pub(crate) fn borrow_uld_mut(&mut self) -> &mut VL53L5CX_Configuration {
        &mut self.uld
    }

    /*** disabled (until we try/need low power)
    // tbd. Does setting low power mode mean transitioning to 'LP_Idle'?  In that case, this should
    //      be state transition for us (and 'get_power_mode' is not needed, since it's implied by
    //      the Rust object the application has access to!

    //---
    // Maintenance; special use
    //
    pub fn get_power_mode(&mut self) -> Result<PowerMode> {
        let mut tmp: u8 = 0;
        match unsafe { vl53l5cx_get_power_mode(&mut self.vl, &mut tmp) } {
            ST_OK => Ok(PowerMode::from_repr(tmp).unwrap()),
            e => Err(Error(e))
        }
    }
    pub fn set_power_mode(&mut self, v: PowerMode) -> Result<()> {
        match unsafe { vl53l5cx_set_power_mode(&mut self.vl, v as u8) } {
            ST_OK => Ok(()),
            e => Err(Error(e))
        }
    }
    ***/

    // tbd. if exposing these, make them into a "dci" feature
    //pub fn dci_read_data(index: u16, buf: &mut [u8]) { unimplemented!() }
    //pub fn dci_write_data(index: u16, buf: &[u8]) { unimplemented!() }

    // 'dci_replace_data' doesn't seem useful; easily reproduced using the 'read' and 'write'. Skip.

    // Remaining to be implemented:
    //  vl53l5cx_enable_internal_cp()
    //  vl53l5cx_disable_internal_cp
    //  vl53l5cx_set_VHV_repeat_count
    //  vl53l5cx_get_VHV_repeat_count
}