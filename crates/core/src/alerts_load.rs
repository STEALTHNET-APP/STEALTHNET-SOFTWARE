//! Load-state decisions. `None` means there is no observation to apply.
pub fn load_observation(stale: bool, cpu: Option<f32>, ram: Option<f32>, active: bool) -> Option<bool> {
    if stale { return None; }
    let (cpu, ram) = (cpu?, ram?);
    if !cpu.is_finite() || !ram.is_finite() { return None; }
    Some(cpu >= 95.0 || ram >= 95.0 || (active && (cpu >= 85.0 || ram >= 85.0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_samples_never_resolve_or_open_an_incident() {
        for active in [false, true] {
            for value in [20.0, 99.0] {
                assert_eq!(load_observation(true, Some(value), Some(value), active), None);
            }
        }
    }
    #[test]
    fn missing_or_nonfinite_samples_are_unknown() {
        for active in [false, true] {
            for missing in [None, Some(f32::NAN), Some(f32::INFINITY)] {
                assert_eq!(load_observation(false, missing, Some(20.0), active), None);
                assert_eq!(load_observation(false, Some(20.0), missing, active), None);
            }
        }
    }
    #[test]
    fn fresh_samples_preserve_existing_thresholds_and_hysteresis() {
        assert_eq!(load_observation(false, Some(95.0), Some(20.0), false), Some(true));
        assert_eq!(load_observation(false, Some(20.0), Some(95.0), false), Some(true));
        assert_eq!(load_observation(false, Some(94.9), Some(20.0), false), Some(false));
        assert_eq!(load_observation(false, Some(85.0), Some(20.0), true), Some(true));
        assert_eq!(load_observation(false, Some(20.0), Some(85.0), true), Some(true));
        assert_eq!(load_observation(false, Some(84.9), Some(84.9), true), Some(false));
    }
}
