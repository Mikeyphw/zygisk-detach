//! Conservative diagnostics: only observed current Play Store PID can confirm hook activity.
//! No synthetic transactions and no unsupported claims about update prevention.
use std::path::Path;
use std::process::Command;
use std::io::{self, Write};
use std::fs::OpenOptions;

fn command(program: &str, args: &[&str]) -> Option<String> {
    let result = Command::new(program).args(args).output().ok()?;
    if !result.status.success() { return None; }
    Some(String::from_utf8_lossy(&result.stdout).trim().to_string())
}
fn provider() -> &'static str {
    for (folder,name) in [
        ("/data/adb/modules/rezygisk", "ReZygisk installed"),
        ("/data/adb/modules/zygisk_next", "Zygisk Next installed"),
        ("/data/adb/modules/zygisksu", "ZygiskSU installed")
    ] {
        if Path::new(folder).is_dir() && !Path::new(folder).join("disable").exists() {
            return name;
        }
    }
    "unknown (could be built-in Magisk Zygisk)"
}
fn manager() -> &'static str {
    if Path::new("/data/adb/ksu").exists() || Path::new("/data/adb/ksu/bin/ksud").exists() {
        "KernelSU-family detected"
    } else if Path::new("/data/adb/magisk").exists() { "Magisk detected" }
    else if Path::new("/data/adb/ap").exists() { "APatch-family detected" }
    else { "unknown" }
}
fn active_pid() -> Option<u32> {
    command("pidof", &["-s", "com.android.vending"])?.split_whitespace().next()?.parse().ok()
}
pub fn log_state(log: &str, pid: Option<u32>) -> &'static str {
    let Some(pid) = pid else { return "play_store_not_running"; };
    let mut loaded = false;
    for line in log.lines() {
        // logcat -v threadtime: MM-DD HH:MM:SS.mmm PID TID D TAG: message
        let mut fields = line.split_whitespace();
        let _date = fields.next();
        let _time = fields.next();
        let observed_pid = fields.next().and_then(|s| s.parse::<u32>().ok());
        if observed_pid != Some(pid) || !line.contains("zygisk-detach:") { continue; }
        if line.contains("HOOK_OBSERVED") { return "observed_in_current_store_process"; }
        if line.contains("HOOK_INSTALLED") { loaded = true; }
    }
    if loaded { "installed_no_matching_query_observed" }
    else { "unverified" }
}

pub struct Diagnostics {
    pub module_installed: bool,
    pub configured_packages: usize,
    pub android_sdk: u32,
    pub play_store_pid: Option<u32>,
    pub root_manager: &'static str,
    pub zygisk_provider: &'static str,
    pub hook_state: &'static str,
    pub log_access: bool,
}
pub fn collect(configured_packages: usize) -> Diagnostics {
    let pid = active_pid();
    let logs = command("logcat", &["-d", "-t", "200", "-v", "threadtime", "-s", "zygisk-detach:D"]);
    let hook_state = logs.as_deref().map(|s| log_state(s, pid)).unwrap_or("unverified");
    let android_sdk = command("getprop", &["ro.build.version.sdk"])
        .and_then(|v| v.parse().ok()).unwrap_or(0);
    Diagnostics {
        module_installed: Path::new("/data/adb/modules/zygisk-detach").is_dir(),
        configured_packages, android_sdk, play_store_pid: pid,
        root_manager: manager(), zygisk_provider: provider(), hook_state,
        log_access: logs.is_some(),
    }
}
pub fn to_json(d: &Diagnostics) -> String {
    // Strict internal enums and numeric counters only: no paths, package IDs or raw logs.
    format!("{{\"module_installed\":{},\"configured_packages\":{},\"android_sdk\":{},\"play_store_pid\":{},\"root_manager\":\"{}\",\"zygisk_provider\":\"{}\",\"hook_state\":\"{}\",\"log_access\":{}}}",
        d.module_installed, d.configured_packages, d.android_sdk, d.play_store_pid.unwrap_or(0),
        d.root_manager, d.zygisk_provider, d.hook_state, d.log_access)
}

pub fn write_bundle(d: &Diagnostics, path: &Path) -> io::Result<()> {
    // No full logs, device IDs, installed-app list or clipboard operations.
    // Refuses existing output: avoids overwriting user files and symlink targets.
    let mut options=OpenOptions::new();options.write(true).create_new(true);
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(to_json(d).as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()
}

pub fn print(d: &Diagnostics, json: bool) {
    let pid = d.play_store_pid.unwrap_or(0);
    if json {
        println!("{}",to_json(d));
    } else {
        println!("Module installed: {}",d.module_installed);
        println!("Configured packages: {}",d.configured_packages);
        println!("Android SDK: {}",d.android_sdk);
        println!("Root manager: {}",d.root_manager);
        println!("Zygisk provider: {}",d.zygisk_provider);
        println!("Play Store PID: {}",pid);
        println!("Hook observation: {}",d.hook_state);
        println!("Logcat accessible: {}",d.log_access);
        println!("Note: interception observation is not proof that future updates are blocked.");
        if !d.log_access { println!("Grant logcat access or run through su for fuller diagnostics."); }
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn no_synthetic_hook_claims() {
        assert_eq!(log_state("",Some(99)),"unverified");
        assert_eq!(log_state("",None),"play_store_not_running");
        assert_eq!(log_state("11-16 12:00:00.001  98  101 D zygisk-detach: HOOK_OBSERVED",Some(99)),"unverified");
    }
    #[test] fn actual_current_pid_required() {
        let log="11-16 12:00:00.001  123  124 D zygisk-detach: HOOK_INSTALLED\n11-16 12:00:02.001 123 124 D zygisk-detach: HOOK_OBSERVED";
        assert_eq!(log_state(log, Some(123)),"observed_in_current_store_process");
        assert_eq!(log_state(log, Some(100)),"unverified");
    }
}
