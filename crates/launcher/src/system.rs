//! System scan (M0 subset: OS, CPU, memory, disk). GPU, driver and display
//! detection with wgpu/winit and the automatic preset arrive in M2.

use sysinfo::{Disks, System};

#[derive(Debug, Clone, Default)]
pub struct SystemInfo {
    pub os: String,
    pub cpu: String,
    pub cores: usize,
    pub ram_gib: f64,
    pub free_disk_gib: f64,
}

pub fn scan() -> SystemInfo {
    let Ok(mut sys) = System::new() else {
        return SystemInfo::default();
    };
    sys.refresh_memory();
    sys.refresh_cpu_all();
    let cpu = sys
        .cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .unwrap_or_default();
    let disks = Disks::new_with_refreshed_list().ok();
    let exe_dir = std::env::current_exe().ok();
    // Free space on the disk that holds the game (longest matching mount).
    let free = disks
        .iter()
        .flat_map(|d| d.into_iter())
        .filter(|d| {
            exe_dir
                .as_ref()
                .is_some_and(|p| p.starts_with(d.mount_point()))
        })
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
        .unwrap_or(0);
    SystemInfo {
        os: format!(
            "{} {}",
            System::name().unwrap_or_default(),
            System::os_version().unwrap_or_default()
        ),
        cpu,
        cores: sys.cpus().len(),
        ram_gib: sys.total_memory() as f64 / (1u64 << 30) as f64,
        free_disk_gib: free as f64 / (1u64 << 30) as f64,
    }
}
