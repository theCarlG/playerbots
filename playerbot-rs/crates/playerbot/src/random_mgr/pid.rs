//! PID controller — direct port of the legacy `botPID` /
//! `botPIDImpl` classes in `playerbot/RandomPlayerbotMgr.cpp`.
//!
//! The legacy manager uses this to scale bot activity against the
//! `CMaNGOS` world-diff counter: a setpoint of "wanted diff" vs. the
//! measured average diff, producing a percentage modifier that the
//! manager then adds to a baseline 50% activity floor.
//!
//! Source of the original math:
//!   <https://gist.github.com/bradley219/5373998>
//!
//! This port mirrors the original API verbatim (`calculate`, `adjust`,
//! `reset`). Integral wind-up at the clipping edge is preserved. The
//! only differences:
//!
//! * Rust `f64` everywhere (matches the C++ `double`).
//! * `new()` takes a `dt, max, min, kp, ki, kd` tuple, not a
//!   constructor with six positional args — easier to read at call
//!   sites like `PidController::new(1.0, 50.0, -50.0, 0.0, 0.0, 0.0)`.
//! * `#[derive(Clone, Debug)]` so the struct is testable without
//!   boxing, and so it can participate in `Default` for the
//!   `RandomMgrState` container.

/// Classical PID controller.
///
/// `calculate(setpoint, pv)` returns `Pout + Iout + Dout`, clipped into
/// `[min, max]`. At the clipping edge the integral term backs out the
/// most recent contribution so it doesn't accumulate on a rail (the
/// "integral wind-up" guard).
#[derive(Clone, Debug)]
pub struct PidController {
    dt: f64,
    max: f64,
    min: f64,
    kp: f64,
    ki: f64,
    kd: f64,
    pre_error: f64,
    integral: f64,
}

impl PidController {
    /// Build a controller from the same six values the C++ constructor
    /// took: loop interval `dt`, clipping range `[min, max]`, and gains
    /// `kp, ki, kd`.
    #[must_use]
    pub const fn new(dt: f64, max: f64, min: f64, kp: f64, ki: f64, kd: f64) -> Self {
        Self {
            dt,
            max,
            min,
            kp,
            ki,
            kd,
            pre_error: 0.0,
            integral: 0.0,
        }
    }

    /// Re-tune the gains without resetting state. Mirrors `botPID::adjust`.
    pub const fn adjust(&mut self, kp: f64, ki: f64, kd: f64) {
        self.kp = kp;
        self.ki = ki;
        self.kd = kd;
    }

    /// Reset the integral accumulator. Mirrors `botPID::reset`. The
    /// previous-error history is intentionally preserved — the C++
    /// implementation does the same.
    pub const fn reset(&mut self) {
        self.integral = 0.0;
    }

    /// Run one iteration. Returns the clipped control output.
    #[must_use]
    pub fn calculate(&mut self, setpoint: f64, pv: f64) -> f64 {
        let error = setpoint - pv;

        let p_out = self.kp * error;
        self.integral += error * self.dt;
        let i_out = self.ki * self.integral;
        let derivative = (error - self.pre_error) / self.dt;
        let d_out = self.kd * derivative;

        let mut output = p_out + i_out + d_out;

        if output > self.max {
            output = self.max;
            // Stop wind-up at the upper rail — back out the
            // contribution we just added.
            self.integral -= error * self.dt;
        } else if output < self.min {
            output = self.min;
            self.integral -= error * self.dt;
        }

        self.pre_error = error;
        output
    }
}

impl Default for PidController {
    /// Matches the C++ default: `botPID(1, 50, -50, 0, 0, 0)`.
    fn default() -> Self {
        Self::new(1.0, 50.0, -50.0, 0.0, 0.0, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_gains_return_zero() {
        let mut pid = PidController::default();
        assert_eq!(pid.calculate(100.0, 50.0), 0.0);
        assert_eq!(pid.calculate(100.0, 50.0), 0.0);
    }

    #[test]
    fn proportional_only() {
        let mut pid = PidController::new(1.0, 50.0, -50.0, 0.5, 0.0, 0.0);
        // error = 100 - 60 = 40, output = 0.5 * 40 = 20.
        let out = pid.calculate(100.0, 60.0);
        assert!((out - 20.0).abs() < 1e-9);
    }

    #[test]
    fn output_clips_to_max() {
        let mut pid = PidController::new(1.0, 5.0, -5.0, 10.0, 0.0, 0.0);
        // Raw output 100 → clipped to 5.
        let out = pid.calculate(110.0, 100.0);
        assert_eq!(out, 5.0);
    }

    #[test]
    fn output_clips_to_min() {
        let mut pid = PidController::new(1.0, 5.0, -5.0, 10.0, 0.0, 0.0);
        let out = pid.calculate(50.0, 100.0);
        assert_eq!(out, -5.0);
    }

    #[test]
    fn integral_builds_up() {
        let mut pid = PidController::new(1.0, 100.0, -100.0, 0.0, 1.0, 0.0);
        // Each iteration adds the error (×dt=1) to the integral.
        assert_eq!(pid.calculate(10.0, 0.0), 10.0);
        assert_eq!(pid.calculate(10.0, 0.0), 20.0);
        assert_eq!(pid.calculate(10.0, 0.0), 30.0);
    }

    #[test]
    fn integral_windup_guard_at_max() {
        let mut pid = PidController::new(1.0, 5.0, -5.0, 0.0, 1.0, 0.0);
        // First call: integral = 10, output capped at 5 → integral reverts to 0.
        assert_eq!(pid.calculate(10.0, 0.0), 5.0);
        // Second call: same thing — integral should stay at 0, not 20.
        assert_eq!(pid.calculate(10.0, 0.0), 5.0);
        // If the setpoint drops we should be able to come off the rail.
        let out = pid.calculate(0.0, 0.0);
        assert_eq!(out, 0.0);
    }

    #[test]
    fn legacy_c_parameters_match() {
        // Same gains the RPBM constructor installs:
        //   pid.adjust(0.05, 0.001, 0.05);
        // with the default dt=1, max=50, min=-50.
        let mut pid = PidController::default();
        pid.adjust(0.05, 0.001, 0.05);

        // Empty server: avg diff = 80ms, wanted = 100ms → positive
        // correction (activity should rise).
        let out = pid.calculate(100.0, 80.0);
        assert!(out > 0.0);

        // With a real player: wanted diff 150ms, avg 170ms → negative.
        let out2 = pid.calculate(150.0, 170.0);
        assert!(out2 < 0.0);
    }

    #[test]
    fn reset_clears_integral_only() {
        let mut pid = PidController::new(1.0, 100.0, -100.0, 1.0, 1.0, 0.0);
        let _ = pid.calculate(10.0, 0.0);
        let _ = pid.calculate(10.0, 0.0);
        pid.reset();
        // Post-reset: integral = 0, but pre_error = 10 (preserved)
        // So first call: error = 10, p = 10, i = 10*1*1 = 10, d = 0 → 20.
        let out = pid.calculate(10.0, 0.0);
        assert!((out - 20.0).abs() < 1e-9);
    }
}
