/*
* state_ranging.rs:
*
*   'RangingConfig':  how the ranging should be done
*   'State_Ranging':  handle to the sensor once ranging is ongoing
*/
#![allow(for_loops_over_fallibles)]

#[cfg(feature = "_defmt")]
#[allow(unused_imports)]
use defmt::{assert, panic, trace, debug};

use crate::uld_raw::{
    VL53L5CX_Configuration,
    vl53l5cx_start_ranging,
    vl53l5cx_check_data_ready,
    vl53l5cx_get_ranging_data,
    vl53l5cx_stop_ranging,
    ST_OK,
    VL53L5CX_ResultsData
};

use crate::{
    results_data::ResultsData,
    state_hp_idle::State_HP_Idle,
    units::TempC,
    Error,
    Result,
};

#[allow(non_camel_case_types)]
pub struct State_Ranging<const DIM: usize> {    // DIM: 4|8
    // Access to 'VL53L5CX_Configuration'.
    // The 'Option' is needed to have both explicit '.stop()' and an implicit 'Drop'.
    outer_state: Option<State_HP_Idle>,
}

impl<const DIM: usize> State_Ranging<DIM> {
    pub(crate) fn transition_from(/*move*/ mut st: State_HP_Idle) -> Result<Self> {
        let vl: &mut VL53L5CX_Configuration = st.borrow_uld_mut();

        match unsafe { vl53l5cx_start_ranging(vl) } {
            ST_OK => {
                let x = Self{
                    outer_state: Some(st),
                };
                Ok(x)
            },
            e => Err(Error(e))
        }
    }

    /*
    * Used by the app-level, to see that data actually is available.
    */
    pub fn is_ready(&mut self) -> Result<bool> {
        let mut tmp: u8 = 0;
        match unsafe { vl53l5cx_check_data_ready(self.borrow_uld_mut(), &mut tmp) } {
            ST_OK => Ok(tmp != 0),
            e => Err(Error(e))
        }
    }

    /*
    * Collect results from the last successful scan.
    */
    pub fn get_data(&mut self) -> Result<(ResultsData<DIM>, TempC)> {
        use core::mem::MaybeUninit;
        use core::ptr::addr_of_mut;

        // The 'i8' field within the struct needs explicit initialization.
        // See -> https://doc.rust-lang.org/std/mem/union.MaybeUninit.html#initializing-a-struct-field-by-field
        //
        let mut buf: VL53L5CX_ResultsData = {
            let mut un = MaybeUninit::<VL53L5CX_ResultsData>::uninit();
            let up = un.as_mut_ptr();
            unsafe {
                addr_of_mut!((*up).silicon_temp_degc).write(0);
                un.assume_init()
            }
        };

        match unsafe { vl53l5cx_get_ranging_data(self.borrow_uld_mut(), &mut buf) } {
            ST_OK => {
                let tuple = ResultsData::<DIM>::from(&buf);
                Ok(tuple)
            },
            e => Err(Error(e))
        }
    }

    /*
    * Stop the ranging; provides access back to the 'HP Idle' state of the sensor.
    */
    pub fn stop(mut self) -> Result<State_HP_Idle> {
        match Self::_stop(self.outer_state.as_mut().unwrap()) {
            Ok(()) => {
                Ok( self.outer_state.take().unwrap() )  // leave 'None' for the 'Drop' handler
            },
            Err(e) => Err(e)
        }
    }

    /*
    * Lower level "stop", usable by both the explicit '.stop()' and 'Drop' handler.
    *
    * Takes '&mut Self': 'Drop' handler cannot call the normal '.stop()' that consumes the struct.
    */
    fn _stop(outer: &mut State_HP_Idle) -> Result<()> {
        match unsafe { vl53l5cx_stop_ranging(outer.borrow_uld_mut()) } {
            ST_OK => Ok(()),
            e => Err(Error(e))
        }
    }

    fn borrow_uld_mut(&mut self) -> &mut VL53L5CX_Configuration {
        self.outer_state.as_mut().unwrap().borrow_uld_mut()
    }
}

/*
* A Drop handler, so the ranging will seize (on the sensor) if the application simply drops the
* state (instead of turning it back to 'HP Idle').
*/
impl<const DIM: usize> Drop for State_Ranging<DIM> {
    fn drop(&mut self) {
        #[cfg(feature = "_defmt")]
        debug!("Drop handler called!");

        for mut outer in self.outer_state.as_mut() {
            match Self::_stop(&mut outer) {
                Ok(_) => {},
                Err(Error(e)) => { panic!("Stop ranging failed; st={}", e) }
            }
        }
    }
}
