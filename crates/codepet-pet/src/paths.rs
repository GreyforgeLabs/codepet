//! Where the pet reads and talks: the same files the QML overlay used.

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Paths {
    pub home: PathBuf,
    /// $XDG_STATE_HOME/codepet (bar-state.json, work-state.json, the off flag).
    pub state: PathBuf,
    /// The installed runtime's assets (pets/, icons/).
    pub assets: PathBuf,
    /// The installed runtime's work window entry (opened on a click).
    pub work_qml: PathBuf,
    /// ~/.config/omarchy (shell.json lives here; Omarchy ignores XDG_CONFIG_HOME).
    pub omarchy_config: PathBuf,
    pub socket: PathBuf,
    pub codepet_bin: String,
    pub work_launcher: String,
    pub omarchy_path: String,
}

fn abs_env(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).map(PathBuf::from).filter(|p| p.is_absolute())
}

impl Paths {
    pub fn from_env() -> Option<Paths> {
        let home = abs_env("HOME")?;
        let state_home = abs_env("XDG_STATE_HOME").unwrap_or_else(|| home.join(".local/state"));
        let data_home = abs_env("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"));
        let runtime = abs_env("CODEPET_PET_RUNTIME").unwrap_or_else(|| data_home.join("codepet/current"));
        let assets = abs_env("CODEPET_PET_ASSETS").unwrap_or_else(|| runtime.join("qml/assets"));
        let run_dir = abs_env("XDG_RUNTIME_DIR").unwrap_or_else(|| std::env::temp_dir());
        let codepet_bin = std::env::var("CODEPET_BIN").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| {
            let b = home.join(".local/bin/codepet");
            if b.is_file() { b.to_string_lossy().into_owned() } else { "codepet".into() }
        });
        let work_launcher = std::env::var("CODEPET_WORK_LAUNCHER")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| match codepet_bin.rsplit_once('/') {
                Some((dir, _)) => format!("{dir}/codepet-work"),
                None => "codepet-work".into(),
            });
        Some(Paths {
            state: state_home.join("codepet"),
            work_qml: runtime.join("qml/work.qml"),
            assets,
            omarchy_config: home.join(".config/omarchy"),
            socket: abs_env("CODEPET_PET_SOCKET").unwrap_or_else(|| run_dir.join("codepet-pet.sock")),
            codepet_bin,
            work_launcher,
            omarchy_path: std::env::var("OMARCHY_PATH").unwrap_or_default(),
            home,
        })
    }

    pub fn bar_state(&self) -> PathBuf {
        self.state.join("bar-state.json")
    }
    pub fn work_state(&self) -> PathBuf {
        self.state.join("work-state.json")
    }
    pub fn settings(&self) -> PathBuf {
        self.omarchy_config.join("shell.json")
    }
    pub fn off_flag(&self) -> PathBuf {
        self.state.join("off")
    }
}
