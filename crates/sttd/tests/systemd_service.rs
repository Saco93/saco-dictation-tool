#![allow(unused_crate_dependencies)]

use std::{fs, path::PathBuf};

fn load_sttd_service_file() -> String {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let service_path = repo_root.join("config").join("sttd.service");
    fs::read_to_string(&service_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", service_path.display()))
}

#[test]
fn sttd_service_contains_required_startup_contract() {
    let unit = load_sttd_service_file();

    for required in [
        "[Unit]",
        "[Service]",
        "[Install]",
        "After=graphical-session.target",
        "Wants=graphical-session.target",
        "EnvironmentFile=%h/.config/sttd/sttd.env",
        "ExecStart=/usr/bin/env sttd --config %h/.config/sttd/sttd.toml",
        "Restart=on-failure",
        "WantedBy=default.target",
    ] {
        assert!(
            unit.contains(required),
            "missing required service directive: {required}"
        );
    }
}
