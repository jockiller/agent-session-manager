use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::models::AgentKind;

pub struct AgentDetector {
    pub environment: HashMap<String, String>,
    pub home: PathBuf,
    pub application_roots: Vec<PathBuf>,
    pub extra_binary_directories: Vec<PathBuf>,
}

struct Spec {
    binaries: &'static [&'static str],
    bin_env_keys: &'static [&'static str],
    home_env_keys: &'static [&'static str],
    home_directories: &'static [&'static str],
    app_names: &'static [&'static str],
}

impl AgentDetector {
    pub fn new(
        environment: HashMap<String, String>,
        home: PathBuf,
        application_roots: Option<Vec<PathBuf>>,
        extra_binary_directories: Option<Vec<PathBuf>>,
    ) -> Self {
        let application_roots =
            application_roots.unwrap_or_else(|| default_application_roots(&home));
        let extra_binary_directories = extra_binary_directories.unwrap_or_else(|| {
            vec![
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
                home.join(".local/bin"),
                home.join("bin"),
            ]
        });
        Self {
            environment,
            home,
            application_roots,
            extra_binary_directories,
        }
    }

    pub fn live() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        Self::new(std::env::vars().collect(), home, None, None)
    }

    pub fn is_installed(&self, kind: AgentKind) -> bool {
        let spec = spec(kind);
        self.command_exists(spec.binaries, spec.bin_env_keys)
            || self.home_exists(spec.home_env_keys)
            || self.directories_exist(spec.home_directories)
            || self.app_exists(spec.app_names)
    }

    pub fn ordered(&self) -> Vec<AgentKind> {
        let kinds = AgentKind::ALL;
        kinds
            .into_iter()
            .filter(|kind| self.is_installed(*kind))
            .chain(kinds.into_iter().filter(|kind| !self.is_installed(*kind)))
            .collect()
    }

    fn command_exists(&self, names: &[&str], env_keys: &[&str]) -> bool {
        for key in env_keys {
            if let Some(path) = self.environment.get(*key) {
                if !path.is_empty() && is_executable_file(Path::new(path)) {
                    return true;
                }
            }
        }
        let mut directories = self.extra_binary_directories.clone();
        if let Some(path) = self.environment.get("PATH") {
            directories.extend(path.split(':').map(PathBuf::from));
        }
        let mut seen = HashSet::new();
        for directory in directories {
            if !seen.insert(directory.clone()) {
                continue;
            }
            for name in names {
                let candidate = directory.join(name);
                if is_executable_file(&candidate) {
                    return true;
                }
            }
        }
        false
    }

    fn home_exists(&self, keys: &[&str]) -> bool {
        for key in keys {
            let Some(path) = self.environment.get(*key) else {
                continue;
            };
            if path.is_empty() {
                continue;
            }
            if Path::new(path).exists() {
                return true;
            }
        }
        false
    }

    fn directories_exist(&self, paths: &[&str]) -> bool {
        for path in paths {
            if self.home.join(path).is_dir() {
                return true;
            }
        }
        false
    }

    fn app_exists(&self, names: &[&str]) -> bool {
        let wanted: HashSet<String> = names.iter().map(|name| name.to_lowercase()).collect();
        if wanted.is_empty() {
            return false;
        }
        for root in &self.application_roots {
            let Ok(entries) = std::fs::read_dir(root) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let is_app = path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case("app"))
                    .unwrap_or(false);
                if !is_app {
                    continue;
                }
                let name = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if wanted.contains(&name) {
                    return true;
                }
            }
        }
        false
    }
}

fn spec(kind: AgentKind) -> Spec {
    match kind {
        AgentKind::Opencode => Spec {
            binaries: &["opencode"],
            bin_env_keys: &["OPENCODE_BIN"],
            home_env_keys: &["OPENCODE_INSTALL_DIR"],
            home_directories: &[],
            app_names: &["OpenCode"],
        },
        AgentKind::Dsh => Spec {
            binaries: &["dsh"],
            bin_env_keys: &["DSH_BIN"],
            home_env_keys: &["DSH_HOME"],
            home_directories: &[],
            app_names: &["DSH"],
        },
        AgentKind::Zcode => Spec {
            binaries: &["zcode"],
            bin_env_keys: &["ZCODE_BIN"],
            home_env_keys: &["ZCODE_HOME"],
            home_directories: &[],
            app_names: &["ZCode"],
        },
        AgentKind::Grok => Spec {
            binaries: &["grok"],
            bin_env_keys: &["GROK_BIN"],
            home_env_keys: &["GROK_HOME"],
            home_directories: &[],
            app_names: &["Grok"],
        },
        AgentKind::Pi => Spec {
            binaries: &["pi"],
            bin_env_keys: &["PI_BIN"],
            home_env_keys: &["PI_CODING_AGENT_DIR"],
            home_directories: &[".pi/agent"],
            app_names: &[],
        },
        AgentKind::Omp => Spec {
            binaries: &["omp"],
            bin_env_keys: &["OMP_BIN"],
            home_env_keys: &["OMP_AGENT_DIR"],
            home_directories: &[".omp/agent"],
            app_names: &[],
        },
    }
}

fn default_application_roots(home: &Path) -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        vec![PathBuf::from("/Applications"), home.join("Applications")]
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = home;
        Vec::new()
    }
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        Ok(meta) => meta.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "asm_test_{}_{}",
                name,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let _ = std::fs::create_dir_all(&path);
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn make_executable(path: &Path) {
        std::fs::write(path, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    fn kinds(list: &[AgentKind]) -> Vec<&str> {
        list.iter().map(|kind| kind.raw()).collect()
    }

    #[test]
    fn detector_finds_cli_on_path_and_orders_installed_first() {
        let home = TestDir::new("detector_path");
        let bin = home.path().join("bin");
        let apps = home.path().join("Applications");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&apps).unwrap();
        make_executable(&bin.join("opencode"));

        let detector = AgentDetector::new(
            HashMap::from([("PATH".to_string(), bin.to_string_lossy().into_owned())]),
            home.path().to_path_buf(),
            Some(vec![apps]),
            Some(Vec::new()),
        );
        assert!(detector.is_installed(AgentKind::Opencode));
        assert!(!detector.is_installed(AgentKind::Dsh));
        assert!(!detector.is_installed(AgentKind::Zcode));
        assert!(!detector.is_installed(AgentKind::Grok));
        assert_eq!(
            kinds(&detector.ordered()),
            ["opencode", "dsh", "zcode", "grok", "pi", "omp"]
        );
    }

    #[test]
    fn detector_finds_desktop_app() {
        let home = TestDir::new("detector_app");
        let apps = home.path().join("Applications");
        std::fs::create_dir_all(apps.join("ZCode.app")).unwrap();

        let detector = AgentDetector::new(
            HashMap::from([("PATH".to_string(), String::new())]),
            home.path().to_path_buf(),
            Some(vec![apps]),
            Some(Vec::new()),
        );
        assert!(detector.is_installed(AgentKind::Zcode));
        assert!(!detector.is_installed(AgentKind::Opencode));
        assert_eq!(detector.ordered().first(), Some(&AgentKind::Zcode));
    }

    #[test]
    fn detector_honors_bin_and_home_env() {
        let home = TestDir::new("detector_env");
        std::fs::create_dir_all(home.path()).unwrap();
        let bin = home.path().join("grok-cli");
        let dsh_home = home.path().join("dsh-home");
        std::fs::create_dir_all(&dsh_home).unwrap();
        make_executable(&bin);

        let detector = AgentDetector::new(
            HashMap::from([
                ("PATH".to_string(), String::new()),
                ("GROK_BIN".to_string(), bin.to_string_lossy().into_owned()),
                (
                    "DSH_HOME".to_string(),
                    dsh_home.to_string_lossy().into_owned(),
                ),
            ]),
            home.path().to_path_buf(),
            Some(vec![home.path().join("Applications")]),
            Some(Vec::new()),
        );
        assert!(detector.is_installed(AgentKind::Grok));
        assert!(detector.is_installed(AgentKind::Dsh));
        assert!(!detector.is_installed(AgentKind::Opencode));
        assert_eq!(
            kinds(&detector.ordered()),
            ["dsh", "grok", "opencode", "zcode", "pi", "omp"]
        );
    }

    #[test]
    fn detector_ignores_config_home_without_env_or_app() {
        let home = TestDir::new("detector_ignore");
        std::fs::create_dir_all(home.path().join(".dsh")).unwrap();
        std::fs::create_dir_all(home.path().join(".grok")).unwrap();

        let detector = AgentDetector::new(
            HashMap::from([("PATH".to_string(), String::new())]),
            home.path().to_path_buf(),
            Some(vec![home.path().join("Applications")]),
            Some(Vec::new()),
        );
        assert!(!detector.is_installed(AgentKind::Dsh));
        assert!(!detector.is_installed(AgentKind::Grok));
        assert_eq!(
            kinds(&detector.ordered()),
            AgentKind::ALL.iter().map(|kind| kind.raw()).collect::<Vec<_>>()
        );
    }
}
