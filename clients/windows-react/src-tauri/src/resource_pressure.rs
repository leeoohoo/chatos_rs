use crate::{host_lifecycle::HostManager, machine_resource_governor::MachineResourceGovernor};
use chatos_local_agent_protocol::LocalAgentResourcePressureLevel;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

const SAMPLE_INTERVAL: Duration = Duration::from_secs(5);
const RECOVERY_DELAY: Duration = Duration::from_secs(30);
const MEBIBYTE: u64 = 1_024 * 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MachinePressureSample {
    memory_load_percent: u32,
    available_memory_bytes: u64,
    total_memory_bytes: u64,
    on_battery_power: bool,
    battery_percent: Option<u8>,
    energy_saver_enabled: bool,
}

impl MachinePressureSample {
    fn pressure_level(self) -> LocalAgentResourcePressureLevel {
        let critical_available = (self.total_memory_bytes / 20).min(512 * MEBIBYTE);
        if self.memory_load_percent >= 95 || self.available_memory_bytes <= critical_available {
            return LocalAgentResourcePressureLevel::Critical;
        }
        if self.memory_load_percent >= 85 || self.available_memory_bytes <= 1_024 * MEBIBYTE {
            return LocalAgentResourcePressureLevel::Serious;
        }
        if self.memory_load_percent >= 75
            || self.energy_saver_enabled
            || (self.on_battery_power && self.battery_percent.is_some_and(|level| level <= 20))
        {
            return LocalAgentResourcePressureLevel::Elevated;
        }
        LocalAgentResourcePressureLevel::Normal
    }
}

#[derive(Debug)]
struct PressureHysteresis {
    applied: Option<LocalAgentResourcePressureLevel>,
    pending_recovery: Option<(LocalAgentResourcePressureLevel, Instant)>,
    recovery_delay: Duration,
}

impl PressureHysteresis {
    fn new(recovery_delay: Duration) -> Self {
        Self {
            applied: None,
            pending_recovery: None,
            recovery_delay,
        }
    }

    fn observe(
        &mut self,
        observed: LocalAgentResourcePressureLevel,
        now: Instant,
    ) -> Option<LocalAgentResourcePressureLevel> {
        let Some(applied) = self.applied else {
            self.applied = Some(observed);
            return Some(observed);
        };
        match pressure_rank(observed).cmp(&pressure_rank(applied)) {
            std::cmp::Ordering::Greater => {
                self.applied = Some(observed);
                self.pending_recovery = None;
                Some(observed)
            }
            std::cmp::Ordering::Equal => {
                self.pending_recovery = None;
                None
            }
            std::cmp::Ordering::Less => {
                let started = match self.pending_recovery {
                    Some((pending, started)) if pending == observed => started,
                    _ => {
                        self.pending_recovery = Some((observed, now));
                        return None;
                    }
                };
                if now.saturating_duration_since(started) < self.recovery_delay {
                    return None;
                }
                self.applied = Some(observed);
                self.pending_recovery = None;
                Some(observed)
            }
        }
    }
}

fn pressure_rank(level: LocalAgentResourcePressureLevel) -> u8 {
    match level {
        LocalAgentResourcePressureLevel::Normal => 0,
        LocalAgentResourcePressureLevel::Elevated => 1,
        LocalAgentResourcePressureLevel::Serious => 2,
        LocalAgentResourcePressureLevel::Critical => 3,
    }
}

pub(crate) fn start_monitor(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut hysteresis = PressureHysteresis::new(RECOVERY_DELAY);
        loop {
            let sample = tauri::async_runtime::spawn_blocking(sample_machine_pressure).await;
            if let Ok(Ok(sample)) = sample {
                if let Some(level) = hysteresis.observe(sample.pressure_level(), Instant::now()) {
                    app.state::<MachineResourceGovernor>()
                        .update_pressure(level);
                    let _ = app
                        .state::<HostManager>()
                        .update_scheduler_pressure(level)
                        .await;
                }
            }
            tokio::time::sleep(SAMPLE_INTERVAL).await;
        }
    });
}

#[cfg(target_os = "windows")]
fn sample_machine_pressure() -> Result<MachinePressureSample, String> {
    use windows_sys::Win32::System::{
        Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS},
        SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
    };

    let mut memory = unsafe { std::mem::zeroed::<MEMORYSTATUSEX>() };
    memory.dwLength = u32::try_from(std::mem::size_of::<MEMORYSTATUSEX>())
        .map_err(|_| "Windows memory status structure is too large".to_string())?;
    if unsafe { GlobalMemoryStatusEx(&mut memory) } == 0 {
        return Err("Windows memory pressure is unavailable".to_string());
    }

    let mut power = unsafe { std::mem::zeroed::<SYSTEM_POWER_STATUS>() };
    let power_available = unsafe { GetSystemPowerStatus(&mut power) } != 0;
    let on_battery_power = power_available && power.ACLineStatus == 0;
    let battery_percent =
        (power_available && power.BatteryLifePercent <= 100).then_some(power.BatteryLifePercent);
    Ok(MachinePressureSample {
        memory_load_percent: memory.dwMemoryLoad,
        available_memory_bytes: memory.ullAvailPhys,
        total_memory_bytes: memory.ullTotalPhys,
        on_battery_power,
        battery_percent,
        energy_saver_enabled: power_available && power.SystemStatusFlag == 1,
    })
}

#[cfg(not(target_os = "windows"))]
fn sample_machine_pressure() -> Result<MachinePressureSample, String> {
    Err("Windows machine pressure sampling is unavailable on this platform".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(
        load: u32,
        available_mebibytes: u64,
        on_battery: bool,
        battery_percent: Option<u8>,
        energy_saver: bool,
    ) -> MachinePressureSample {
        MachinePressureSample {
            memory_load_percent: load,
            available_memory_bytes: available_mebibytes * MEBIBYTE,
            total_memory_bytes: 16 * 1_024 * MEBIBYTE,
            on_battery_power: on_battery,
            battery_percent,
            energy_saver_enabled: energy_saver,
        }
    }

    #[test]
    fn windows_pressure_policy_preserves_an_interactive_host_worker() {
        assert_eq!(
            sample(50, 8_192, false, None, false).pressure_level(),
            LocalAgentResourcePressureLevel::Normal
        );
        assert_eq!(
            sample(76, 4_096, false, None, false).pressure_level(),
            LocalAgentResourcePressureLevel::Elevated
        );
        assert_eq!(
            sample(50, 8_192, true, Some(15), false).pressure_level(),
            LocalAgentResourcePressureLevel::Elevated
        );
        assert_eq!(
            sample(86, 900, false, None, false).pressure_level(),
            LocalAgentResourcePressureLevel::Serious
        );
        assert_eq!(
            sample(96, 400, false, None, false).pressure_level(),
            LocalAgentResourcePressureLevel::Critical
        );
    }

    #[test]
    fn escalation_is_immediate_and_recovery_requires_a_stable_delay() {
        let delay = Duration::from_secs(30);
        let started = Instant::now();
        let mut state = PressureHysteresis::new(delay);
        assert_eq!(
            state.observe(LocalAgentResourcePressureLevel::Normal, started),
            Some(LocalAgentResourcePressureLevel::Normal)
        );
        assert_eq!(
            state.observe(
                LocalAgentResourcePressureLevel::Serious,
                started + Duration::from_secs(1)
            ),
            Some(LocalAgentResourcePressureLevel::Serious)
        );
        assert_eq!(
            state.observe(
                LocalAgentResourcePressureLevel::Normal,
                started + Duration::from_secs(2)
            ),
            None
        );
        assert_eq!(
            state.observe(
                LocalAgentResourcePressureLevel::Normal,
                started + Duration::from_secs(31)
            ),
            None
        );
        assert_eq!(
            state.observe(
                LocalAgentResourcePressureLevel::Normal,
                started + Duration::from_secs(32)
            ),
            Some(LocalAgentResourcePressureLevel::Normal)
        );
    }

    #[test]
    fn renewed_pressure_cancels_pending_recovery() {
        let started = Instant::now();
        let mut state = PressureHysteresis::new(Duration::from_secs(30));
        state.observe(LocalAgentResourcePressureLevel::Serious, started);
        state.observe(
            LocalAgentResourcePressureLevel::Normal,
            started + Duration::from_secs(1),
        );
        assert_eq!(
            state.observe(
                LocalAgentResourcePressureLevel::Critical,
                started + Duration::from_secs(2)
            ),
            Some(LocalAgentResourcePressureLevel::Critical)
        );
        assert_eq!(state.pending_recovery, None);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_pressure_sampler_returns_bounded_system_values() {
        let sample = sample_machine_pressure().expect("Windows pressure sample");
        assert!(sample.memory_load_percent <= 100);
        assert!(sample.total_memory_bytes > 0);
        assert!(sample.available_memory_bytes <= sample.total_memory_bytes);
        assert!(sample.battery_percent.is_none_or(|level| level <= 100));
    }
}
