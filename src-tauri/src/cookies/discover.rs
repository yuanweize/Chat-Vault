use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CookieFileEntry {
    pub browser_name: String,
    pub cookie_file: String,
    pub profile_name: String,
}

struct BrowserSpec {
    name: &'static str,
    base_dirs: Vec<PathBuf>,
}

/// Discover all Chrome/Chromium-based browser cookie files on this machine.
/// Returns (entries, permission_issues).
pub fn discover_chrome_cookie_files() -> (Vec<CookieFileEntry>, Vec<String>) {
    let specs = browser_specs();
    let mut results = Vec::new();
    let mut permission_issues = Vec::new();

    for spec in &specs {
        for base_dir in &spec.base_dirs {
            if !base_dir.is_dir() {
                // Check if parent exists but child is hidden (TCC privacy)
                if base_dir.parent().is_some_and(|p| p.is_dir()) {
                    if let Ok(siblings) = std::fs::read_dir(base_dir.parent().unwrap()) {
                        let names: Vec<String> = siblings
                            .filter_map(|e| e.ok())
                            .map(|e| e.file_name().to_string_lossy().to_string())
                            .collect();
                        let dir_name = base_dir
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        if !names.contains(&dir_name)
                            && (dir_name == "Chrome" || dir_name == "Google Chrome")
                        {
                            permission_issues.push(format!(
                                "[{}] directory {} is hidden, possibly blocked by system privacy \
                                 (grant Full Disk Access in System Settings)",
                                spec.name,
                                base_dir.display()
                            ));
                        }
                    }
                }
                continue;
            }

            // Collect profile directories
            let mut profile_dirs = Vec::new();
            for name in &["Default", "Guest Profile", "System Profile"] {
                let p = base_dir.join(name);
                if p.is_dir() {
                    profile_dirs.push((p, name.to_string()));
                }
            }
            // Add "Profile *" directories
            if let Ok(entries) = std::fs::read_dir(base_dir) {
                let mut extra: Vec<(PathBuf, String)> = entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        let n = e.file_name().to_string_lossy().to_string();
                        n.starts_with("Profile ") && e.path().is_dir()
                    })
                    .map(|e| {
                        let n = e.file_name().to_string_lossy().to_string();
                        (e.path(), n)
                    })
                    .collect();
                extra.sort_by(|a, b| a.1.cmp(&b.1));
                profile_dirs.extend(extra);
            }

            if profile_dirs.is_empty() {
                // If Local State exists but no profiles visible -> privacy block
                if base_dir.join("Local State").exists() {
                    match std::fs::read_dir(base_dir) {
                        Ok(entries) => {
                            let names: Vec<String> = entries
                                .filter_map(|e| e.ok())
                                .map(|e| e.file_name().to_string_lossy().to_string())
                                .collect();
                            if !names.contains(&"Default".to_string()) {
                                permission_issues.push(format!(
                                    "[{}] Profile directories under {} are hidden, \
                                     possibly blocked by system privacy (grant Full Disk Access)",
                                    spec.name,
                                    base_dir.display()
                                ));
                            }
                        }
                        Err(_) => {
                            permission_issues.push(format!(
                                "[{}] Cannot list {} (grant Full Disk Access)",
                                spec.name,
                                base_dir.display()
                            ));
                        }
                    }
                }
                continue;
            }

            for (pdir, profile_name) in &profile_dirs {
                // Prefer Network/Cookies, fallback to Cookies
                for rel in &["Network/Cookies", "Cookies"] {
                    let f = pdir.join(rel);
                    if f.is_file() {
                        match check_file_readable(&f) {
                            Ok(()) => {
                                results.push(CookieFileEntry {
                                    browser_name: spec.name.to_string(),
                                    cookie_file: f.to_string_lossy().to_string(),
                                    profile_name: profile_name.clone(),
                                });
                            }
                            Err(detail) => {
                                permission_issues
                                    .push(format!("[{}/{}] {}", spec.name, profile_name, detail));
                            }
                        }
                        break; // Network/Cookies found, skip plain Cookies
                    }
                }
            }
        }
    }

    (results, permission_issues)
}

fn check_file_readable(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("File does not exist: {}", path.display()));
    }
    match std::fs::File::open(path) {
        Ok(mut f) => {
            use std::io::Read;
            let mut buf = [0u8; 16];
            if f.read(&mut buf).is_err() {
                Err(format!(
                    "Cannot read: {} (grant Full Disk Access in System Settings)",
                    path.display()
                ))
            } else {
                Ok(())
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Err(format!(
            "Permission denied: {} (grant Full Disk Access in System Settings)",
            path.display()
        )),
        Err(e) => Err(format!("Cannot read {}: {}", path.display(), e)),
    }
}

#[cfg(target_os = "macos")]
fn browser_specs() -> Vec<BrowserSpec> {
    let base = dirs::home_dir()
        .unwrap_or_default()
        .join("Library/Application Support");
    vec![
        BrowserSpec {
            name: "Chrome",
            base_dirs: ["Chrome", "Chrome Beta", "Chrome Dev", "Chrome Canary"]
                .iter()
                .map(|ch| base.join("Google").join(ch))
                .collect(),
        },
        BrowserSpec {
            name: "Chromium",
            base_dirs: vec![base.join("Chromium")],
        },
        BrowserSpec {
            name: "Brave",
            base_dirs: vec![base.join("BraveSoftware/Brave-Browser")],
        },
        BrowserSpec {
            name: "Edge",
            base_dirs: vec![base.join("Microsoft Edge")],
        },
    ]
}

#[cfg(target_os = "windows")]
fn browser_specs() -> Vec<BrowserSpec> {
    let local = PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default());
    vec![
        BrowserSpec {
            name: "Chrome",
            base_dirs: ["Chrome", "Chrome Beta", "Chrome Dev", "Chrome SxS"]
                .iter()
                .map(|ch| local.join("Google").join(ch).join("User Data"))
                .collect(),
        },
        BrowserSpec {
            name: "Chromium",
            base_dirs: vec![local.join("Chromium/User Data")],
        },
        BrowserSpec {
            name: "Brave",
            base_dirs: vec![local.join("BraveSoftware/Brave-Browser/User Data")],
        },
        BrowserSpec {
            name: "Edge",
            base_dirs: vec![local.join("Microsoft/Edge/User Data")],
        },
    ]
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn browser_specs() -> Vec<BrowserSpec> {
    let home = dirs::home_dir().unwrap_or_default();
    let suffixes = ["", "-beta", "-unstable"];
    vec![
        BrowserSpec {
            name: "Chrome",
            base_dirs: suffixes
                .iter()
                .map(|s| home.join(format!(".config/google-chrome{}", s)))
                .collect(),
        },
        BrowserSpec {
            name: "Chromium",
            base_dirs: vec![home.join(".config/chromium")],
        },
        BrowserSpec {
            name: "Brave",
            base_dirs: vec![home.join(".config/BraveSoftware/Brave-Browser")],
        },
        BrowserSpec {
            name: "Edge",
            base_dirs: vec![home.join(".config/microsoft-edge")],
        },
    ]
}
