// SPDX-License-Identifier: GPL-3.0-only
//! Daemon service installer — installs the OS-level service for the daemon.

use std::path::PathBuf;

#[allow(dead_code)]
pub enum InstallResult {
    Installed,
    AlreadyInstalled,
    Updated,
}

#[derive(Debug)]
pub struct ServiceInstaller {
    pub binary_path: PathBuf,
    pub service_name: String,
    pub daemon_addr: String,
}

impl ServiceInstaller {
    pub fn new(binary_path: PathBuf) -> Self {
        Self {
            binary_path,
            service_name: "com.skill.daemon".to_string(),
            daemon_addr: "127.0.0.1:18444".to_string(),
        }
    }

    pub fn install(&self) -> Result<InstallResult, String> {
        #[cfg(target_os = "macos")]
        return self.install_launchagent();

        #[cfg(target_os = "linux")]
        return self.install_systemd_user();

        #[cfg(target_os = "windows")]
        return self.install_windows_service();

        #[allow(unreachable_code)]
        Err("unsupported platform".to_string())
    }

    pub fn uninstall(&self) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        return self.uninstall_launchagent();

        #[cfg(target_os = "linux")]
        return self.uninstall_systemd_user();

        #[cfg(target_os = "windows")]
        return self.uninstall_windows_service();

        #[allow(unreachable_code)]
        Err("unsupported platform".to_string())
    }

    pub fn status(&self) -> ServiceStatus {
        #[cfg(target_os = "macos")]
        return self.status_launchagent();

        #[cfg(target_os = "linux")]
        return self.status_systemd_user();

        #[cfg(target_os = "windows")]
        return self.status_windows_service();

        #[allow(unreachable_code)]
        ServiceStatus::Unknown
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStatus {
    Running,
    Stopped,
    NotInstalled,
    Unknown,
}

// ── macOS LaunchAgent ─────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
impl ServiceInstaller {
    fn plist_path(&self) -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("Library/LaunchAgents")
            .join(format!("{}.plist", self.service_name))
    }

    fn plist_content(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{binary}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>/tmp/skill-daemon.out.log</string>
  <key>StandardErrorPath</key>
  <string>/tmp/skill-daemon.err.log</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>SKILL_DAEMON_ADDR</key>
    <string>{addr}</string>
  </dict>
</dict>
</plist>
"#,
            label = self.service_name,
            binary = self.binary_path.display(),
            addr = self.daemon_addr,
        )
    }

    fn install_launchagent(&self) -> Result<InstallResult, String> {
        let plist = self.plist_path();
        let already = plist.exists();

        if let Some(parent) = plist.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&plist, self.plist_content()).map_err(|e| e.to_string())?;

        std::process::Command::new("launchctl")
            .args(["load", "-w", plist.to_str().unwrap_or("")])
            .output()
            .map_err(|e| e.to_string())?;

        Ok(if already {
            InstallResult::Updated
        } else {
            InstallResult::Installed
        })
    }

    fn uninstall_launchagent(&self) -> Result<(), String> {
        let plist = self.plist_path();
        if plist.exists() {
            std::process::Command::new("launchctl")
                .args(["unload", "-w", plist.to_str().unwrap_or("")])
                .output()
                .map_err(|e| e.to_string())?;
            std::fs::remove_file(&plist).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn status_launchagent(&self) -> ServiceStatus {
        let out = std::process::Command::new("launchctl")
            .args(["list", &self.service_name])
            .output();
        match out {
            Ok(o) if o.status.success() => ServiceStatus::Running,
            Ok(_) => {
                if self.plist_path().exists() {
                    ServiceStatus::Stopped
                } else {
                    ServiceStatus::NotInstalled
                }
            }
            Err(_) => ServiceStatus::Unknown,
        }
    }
}

// ── Linux systemd --user ──────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
impl ServiceInstaller {
    fn unit_path(&self) -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("systemd/user")
            .join(format!("{}.service", self.service_name))
    }

    fn unit_content(&self) -> String {
        format!(
            "[Unit]\nDescription=Skill Daemon\nAfter=network.target\n\n\
             [Service]\nType=simple\nExecStart={binary}\nRestart=on-failure\nRestartSec=2\n\
             Environment=SKILL_DAEMON_ADDR={addr}\n\n[Install]\nWantedBy=default.target\n",
            binary = self.binary_path.display(),
            addr = self.daemon_addr,
        )
    }

    fn install_systemd_user(&self) -> Result<InstallResult, String> {
        let unit = self.unit_path();
        let already = unit.exists();
        if let Some(p) = unit.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        std::fs::write(&unit, self.unit_content()).map_err(|e| e.to_string())?;
        std::process::Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .output()
            .map_err(|e| e.to_string())?;
        std::process::Command::new("systemctl")
            .args(["--user", "enable", "--now", &self.service_name])
            .output()
            .map_err(|e| e.to_string())?;
        Ok(if already {
            InstallResult::Updated
        } else {
            InstallResult::Installed
        })
    }

    fn uninstall_systemd_user(&self) -> Result<(), String> {
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "disable", "--now", &self.service_name])
            .output();
        let _ = std::fs::remove_file(self.unit_path());
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .output();
        Ok(())
    }

    fn status_systemd_user(&self) -> ServiceStatus {
        let out = std::process::Command::new("systemctl")
            .args(["--user", "is-active", &self.service_name])
            .output();
        match out {
            Ok(o) if o.stdout.starts_with(b"active") => ServiceStatus::Running,
            Ok(_) => {
                if self.unit_path().exists() {
                    ServiceStatus::Stopped
                } else {
                    ServiceStatus::NotInstalled
                }
            }
            Err(_) => ServiceStatus::Unknown,
        }
    }
}

// ── Windows Service ───────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
impl ServiceInstaller {
    fn install_windows_service(&self) -> Result<InstallResult, String> {
        let bin = self.binary_path.to_str().ok_or("invalid binary path")?;
        let status = self.status_windows_service();
        let already = !matches!(status, ServiceStatus::NotInstalled);

        if already {
            let _ = std::process::Command::new("sc.exe")
                .args(["stop", &self.service_name])
                .output();
            let _ = std::process::Command::new("sc.exe")
                .args(["delete", &self.service_name])
                .output();
        }

        std::process::Command::new("sc.exe")
            .args(["create", &self.service_name, &format!("binPath= {bin}"), "start= auto"])
            .output()
            .map_err(|e| e.to_string())?;

        std::process::Command::new("sc.exe")
            .args([
                "failure",
                &self.service_name,
                "reset= 86400",
                "actions= restart/5000/restart/5000/restart/5000",
            ])
            .output()
            .map_err(|e| e.to_string())?;

        std::process::Command::new("sc.exe")
            .args(["start", &self.service_name])
            .output()
            .map_err(|e| e.to_string())?;

        Ok(if already {
            InstallResult::Updated
        } else {
            InstallResult::Installed
        })
    }

    fn uninstall_windows_service(&self) -> Result<(), String> {
        let _ = std::process::Command::new("sc.exe")
            .args(["stop", &self.service_name])
            .output();
        std::process::Command::new("sc.exe")
            .args(["delete", &self.service_name])
            .output()
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn status_windows_service(&self) -> ServiceStatus {
        let out = std::process::Command::new("sc.exe")
            .args(["query", &self.service_name])
            .output();
        match out {
            Ok(o) => {
                let s = String::from_utf8_lossy(&o.stdout);
                if s.contains("RUNNING") {
                    ServiceStatus::Running
                } else if s.contains("STOPPED") || s.contains("START_PENDING") {
                    ServiceStatus::Stopped
                } else if !o.status.success() {
                    ServiceStatus::NotInstalled
                } else {
                    ServiceStatus::Unknown
                }
            }
            Err(_) => ServiceStatus::Unknown,
        }
    }
}
