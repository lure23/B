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
use crate::{
    state_ranging::{
        State_Ranging,
    },
    uld_raw::{
        vl53l5cx_get_power_mode,
        VL53L5CX_Configuration
    },
    Error,
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
    pub fn start_ranging<const DIM: usize>(/*move*/ self) -> Result<State_Ranging<DIM>> {
        let r = State_Ranging::transition_from(self)?;
        Ok(r)
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

    pub(crate) fn borrow_uld_mut(&mut self) -> &mut VL53L5CX_Configuration {
        &mut self.uld
    }
}

