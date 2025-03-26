/*
* Convert data received from the ULD C API to more.. robust formats:
*   - 1D vectors -> 2D matrices
*   - integers -> enums or tuple structs
*   - some squeezing of type safety, e.g. negative 'distance_mm's not accepted
*
* Note: It is by design that these conversions happen already at the ULD level.
*
* References:
*   - vendor's UM2884 > Chapter 5 ("Ranging results"); Rev 5, Feb'24; PDF 18pp.
*       -> https://www.st.com/resource/en/user_manual/um2884-a-guide-to-using-the-vl53l5cx-multizone-timeofflight-ranging-sensor-with-a-wide-field-of-view-ultra-lite-driver-uld-stmicroelectronics.pdf
*/
#[cfg(feature = "_defmt")]
#[allow(unused_imports)]
use defmt::{assert, panic};

use crate::uld_raw::{
    VL53L5CX_ResultsData,
};
use crate::units::TempC;

// Note: We could also take in 'TARGETS_PER_ZONE' from the ULD C API wrapper.
const TARGETS: usize =
         if cfg!(feature = "targets_per_zone_4") { 4 }
    else if cfg!(feature = "targets_per_zone_3") { 3 }
    else if cfg!(feature = "targets_per_zone_2") { 2 }
    else { 1 };

/*
* Results data, in matrix format.
*
* Note: Scalar metadata ('silicon_temp_degc') that ULD C API treats as a result is being delivered
*       separately. This is mainly a matter of taste: many of the matrix "results" are actually
*       also metadata. Only '.distance_mm' and (likely) '.reflectance_percent' can be seen as
*       actual results. It doesn't really matter.
*/
#[derive(Clone, Debug)]
pub struct ResultsData<const DIM: usize> {      // DIM: 4,8
    // Metadata: DIMxDIM matrix, regardless of 'TARGETS'
    //
    #[cfg(feature = "nb_targets_detected")]
    pub targets_detected: [[u8; DIM]; DIM],     // 1..{X in 'targets_per_zone_X' feature}

    // Actual results: DIMxDIMxTARGETS
    #[cfg(feature = "target_status")]
    pub target_status: [[[TargetStatus; DIM]; DIM]; TARGETS],

    #[cfg(feature = "distance_mm")]
    pub distance_mm: [[[u16; DIM]; DIM]; TARGETS],
}

impl<const DIM: usize> ResultsData<DIM> {
    /*
    * Provide an empty buffer-like struct; owned usually by the application and fed via 'feed()'.
    */
    #[cfg(not(all()))]
    fn empty() -> Self {

        Self {
            #[cfg(feature = "nb_targets_detected")]
            targets_detected: [[0;DIM];DIM],

            #[cfg(feature = "target_status")]
            target_status: [[[TargetStatus::NoTarget;DIM];DIM];TARGETS],

            #[cfg(feature = "distance_mm")]
            distance_mm: [[[0;DIM];DIM];TARGETS],
        }
    }

    pub(crate) fn from(raw_results: &VL53L5CX_ResultsData) -> (Self,TempC) {
        use core::mem::MaybeUninit;

        let mut x: Self = {
            let un = MaybeUninit::<Self>::uninit();
            unsafe { un.assume_init() }
        };

        let tempC = x.feed(raw_results);
        (x, tempC)
    }

    fn feed(&mut self, rr: &VL53L5CX_ResultsData) -> TempC {
        use core::convert::identity;

        // helpers
        //
        // The ULD C API matrix layout is,
        //  - looking _out_ through the sensor so that the SATEL mini-board's PCB text is horizontal
        //    and right-way-up
        //      ^-- i.e. what the sensor "sees" (not how we look at the sensor)
        //  - for a fictional 2x2x2 matrix = only the corner zones
        //
        // Real world:
        //      [A B]   // A₁..D₁ = first targets; A₂..D₂ = 2nd targets; i.e. same target zone
        //      [C D]
        //
        // ULD C API vector:
        //      [A₁ A₂ B₁ B₂ C₁ C₂ D₁ D₂]   // every "zone" is first covered; then next zone
        //
        // Rust note:
        //      'const DIM' generic needs to be repeated for each 'fn'; we cannot use the "outer":
        //          <<
        //              error[E0401]: can't use generic parameters from outer item
        //          <<
        //
        #[allow(dead_code)]
        fn into_matrix_map_o<IN: Copy, OUT, const DIM: usize>(raw: &[IN], offset: usize, out: &mut [[OUT; DIM]; DIM], f: impl Fn(IN) -> OUT) {
            let raw = &raw[..DIM * DIM * TARGETS];      // take only the beginning of the C buffer

            for r in 0..DIM {
                for c in 0..DIM {
                    out[r][c] = f(raw[(r * DIM + c) * TARGETS + offset]);
                }
            }
        }
        #[inline]
        #[allow(dead_code)]
        fn into_matrix_o<X: Copy, const DIM: usize>(raw: &[X], offset: usize, out: &mut [[X; DIM]; DIM]) {     // no mapping
            into_matrix_map_o(raw, offset, out, identity)
        }
        // Zone metadata: 'TARGETS' (and 'offset', by extension) are not involved.
        #[allow(dead_code)]
        fn into_matrix<X: Copy, const DIM: usize>(raw: &[X], out: &mut [[X; DIM]; DIM]) {
            let raw = &raw[..DIM * DIM];      // take only the beginning of the C buffer

            for r in 0..DIM {
                for c in 0..DIM {
                    out[r][c] = raw[r*DIM+c];
                }
            }
        }

        // Results: DIMxDIMxTARGETS
        //
        for i in 0..TARGETS {
            #[cfg(feature = "target_status")]
            into_matrix_map_o(&rr.target_status, i, &mut self.target_status[i], TargetStatus::from_uld);

            // We tolerate '.distance_mm' == 0 for non-existing data (where '.target_status' is 0); no need to check.
            //
            #[cfg(feature = "distance_mm")]
            into_matrix_map_o(&rr.distance_mm, i, &mut self.distance_mm[i],
            |v: i16| -> u16 {
                assert!(v >= 0, "Unexpected 'distance_mm' value: {} < 0", v); v as u16
            });
        }

        TempC(rr.silicon_temp_degc)
    }
}

//---
// Target status
//
// Note: Vendor docs (UM2884 Rev.5; chapter 5.5; Table 4) gives detailed explanations for values
//      0..13 and 255. We intend to provide enums for values that are _actually seen_, so that
//      application code doesn't need to deal with integers. Where multiple choices exist, they
//      are provided  as the inner values.
//
#[derive(Copy, Clone, Debug)]       // 'Clone' needed for 'ResultsData' to be cloneable.
#[cfg_attr(feature = "_defmt", derive(defmt::Format))]
pub enum TargetStatus {
    NotUpdated,         // 0    "Ranging data are not updated" (O)
    Valid,              // 5    "Range valid" = 100% valid
    SemiValid(u8),      // 6    "Wrap around not performed (typically the first range)"
                        // 9    "Range valid with large pulse (may be due to a merged target)"
    NoTarget,           // 255  "No target detected (only if number of targets detected is enabled)"
    Error(u8),          // 1    "Signal rate too slow on SPAD array"
                        // 2    "Target phase"
                        // 3    "Sigma estimator too high"
                        // 4    "Target consistency failed" (O)
                        // 7    "Rate consistency failed"
                        // 8    "Signal rate too low for the current target"
                        // 10   "Range valid, but no target detected at previous range"
                        // 11   "Measurement consistency failed"
                        // 12   "Target blurred by another one, due to sharpener"
                        // 13   "Target detected but inconsistent data. Frequently happens for secondary targets." (O)
                        //
                        //      (O): Observed in wild
}

impl TargetStatus {
    fn from_uld(v: u8) -> Self {
        match v {
            0 => { Self::NotUpdated }
            5 => { Self::Valid },
            6 | 9 => { Self::SemiValid(v) },
            255 => { Self::NoTarget },
            ..=13 => { Self::Error(v) },
            v => panic!("Unexpected value {} for target status", v),
        }
    }
}
