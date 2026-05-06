use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("lxmd-cli-{label}-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path).expect("create temp test directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn lxmd(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lxmd-rs"))
        .args(args)
        .output()
        .expect("lxmd-rs subprocess should run")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn combined_output(output: &Output) -> String {
    format!("{}{}", stdout(output), stderr(output))
}

fn write_rust_identity(path: &Path) {
    rns_identity::identity::Identity::new()
        .to_file(path)
        .expect("write Rust identity");
}

#[test]
fn help_lists_cli_surface_without_starting_runtime() {
    let output = lxmd(&["--help"]);

    assert!(
        output.status.success(),
        "expected --help to succeed, got:\n{}",
        combined_output(&output)
    );
    let text = stdout(&output);
    assert!(text.contains("LXMF Propagation Daemon"));
    assert!(text.contains("Usage: lxmd-rs [OPTIONS]"));
    assert!(text.contains("--exampleconfig"));
    assert!(text.contains("--status"));
    assert!(text.contains("--peers"));
    assert!(text.contains("--sync <PEER_HASH>"));
    assert!(text.contains("-s, --service"));
    assert!(text.contains("--send <DEST_HASH> <CONTENT>"));
    assert!(text.contains("[possible values: opportunistic, direct, propagated]"));
}

#[test]
fn version_reports_binary_name_and_package_version() {
    let output = lxmd(&["--version"]);

    assert!(
        output.status.success(),
        "expected --version to succeed, got:\n{}",
        combined_output(&output)
    );
    assert_eq!(
        stdout(&output).trim(),
        format!("lxmd-rs {}", env!("CARGO_PKG_VERSION"))
    );
    assert!(stderr(&output).is_empty());
}

#[test]
fn rust_binary_runs_cli() {
    let output = lxmd(&["--version"]);

    assert!(
        output.status.success(),
        "expected lxmd-rs --version to succeed, got:\n{}",
        combined_output(&output)
    );
    assert_eq!(
        stdout(&output).trim(),
        format!("lxmd-rs {}", env!("CARGO_PKG_VERSION"))
    );
    assert!(stderr(&output).is_empty());
}

#[test]
fn example_config_exits_before_runtime_initialisation() {
    let output = lxmd(&["--exampleconfig"]);

    assert!(
        output.status.success(),
        "expected --exampleconfig to succeed, got:\n{}",
        combined_output(&output)
    );
    let text = stdout(&output);
    assert!(text.contains("[lxmf]"));
    assert!(text.contains("display_name = Anonymous Peer"));
    assert!(text.contains("announce_at_start = no"));
    assert!(text.contains("delivery_transfer_max_accepted_size = 1000"));
    assert!(text.contains("[propagation]"));
    assert!(text.contains("enable_node = no"));
    assert!(text.contains("announce_interval = 360"));
    assert!(text.contains("announce_at_start = yes"));
    assert!(text.contains("[logging]"));
    assert!(text.contains("loglevel = 4"));
    assert!(!text.contains("[control]"));
    assert!(
        stderr(&output).is_empty(),
        "--exampleconfig should return before logging/runtime startup"
    );
}

#[test]
fn send_method_values_parse_without_network_runtime() {
    for mode in ["opportunistic", "direct", "propagated"] {
        let output = lxmd(&["--exampleconfig", "--send-method", mode]);

        assert!(
            output.status.success(),
            "expected send method {mode:?} to parse, got:\n{}",
            combined_output(&output)
        );
        assert!(stdout(&output).contains("[lxmf]"));
        assert!(stderr(&output).is_empty());
    }
}

#[test]
fn clap_rejects_invalid_send_method() {
    let output = lxmd(&["--send-method", "bogus"]);

    assert_eq!(
        output.status.code(),
        Some(2),
        "expected Clap usage failure, got:\n{}",
        combined_output(&output)
    );
    let text = stderr(&output);
    assert!(text.contains("invalid value 'bogus' for '--send-method <SEND_METHOD>'"));
    assert!(text.contains("[possible values: opportunistic, direct, propagated]"));
}

#[test]
fn clap_requires_send_argument() {
    let output = lxmd(&["--send"]);

    assert_eq!(
        output.status.code(),
        Some(2),
        "expected Clap usage failure, got:\n{}",
        combined_output(&output)
    );
    assert!(stderr(&output).contains("a value is required for '--send <DEST_HASH> <CONTENT>'"));
}

#[test]
fn status_and_peers_query_control_and_timeout_without_daemon() {
    let lxmf_dir = TestDir::new("lxmf");
    let rns_dir = TestDir::new("rns");
    write_rust_identity(&lxmf_dir.path().join("identity"));
    fs::write(
        rns_dir.path().join("config"),
        "\
[reticulum]
share_instance = No
enable_transport = No
respond_to_probes = No
panic_on_interface_error = No
discover_interfaces = No

[interfaces]
",
    )
    .expect("write no-interface Reticulum config");

    let output = Command::new(env!("CARGO_BIN_EXE_lxmd-rs"))
        .arg("--config")
        .arg(lxmf_dir.path())
        .arg("--rnsconfig")
        .arg(rns_dir.path())
        .arg("--status")
        .arg("--peers")
        .arg("--timeout")
        .arg("0")
        .output()
        .expect("lxmd-rs subprocess should run");

    assert_eq!(
        output.status.code(),
        Some(200),
        "expected Python-compatible control timeout, got:\n{}",
        combined_output(&output)
    );
    assert!(
        combined_output(&output).contains("Getting lxmd statistics timed out, exiting now"),
        "expected Python-compatible control timeout text, got:\n{}",
        combined_output(&output)
    );
    assert!(
        lxmf_dir.path().join("identity").is_file(),
        "--config should use the configured lxmd identity path"
    );
    assert!(
        !lxmf_dir.path().join("storage").exists(),
        "--status/--peers should not start local daemon state"
    );

    let logs = combined_output(&output);
    assert!(
        logs.contains("Using default configuration"),
        "missing LXMF config should fall back to defaults, got logs:\n{logs}"
    );
    assert!(
        logs.contains("interfaces=0"),
        "test config should avoid live Reticulum interfaces, got logs:\n{logs}"
    );
}

#[test]
fn control_status_rejects_missing_identity_before_runtime() {
    let lxmf_dir = TestDir::new("lxmf-missing-identity");
    let rns_dir = TestDir::new("rns-missing-identity");
    fs::write(
        rns_dir.path().join("config"),
        "\
[reticulum]
share_instance = No

[interfaces]
",
    )
    .expect("write no-interface Reticulum config");

    let output = Command::new(env!("CARGO_BIN_EXE_lxmd-rs"))
        .arg("--config")
        .arg(lxmf_dir.path())
        .arg("--rnsconfig")
        .arg(rns_dir.path())
        .arg("--status")
        .output()
        .expect("lxmd-rs subprocess should run");

    assert_eq!(
        output.status.code(),
        Some(202),
        "expected Python-compatible missing identity exit, got:\n{}",
        combined_output(&output)
    );
    assert!(
        combined_output(&output)
            .contains("Identity file not found in specified configuration directory"),
        "missing identity error should be user-visible:\n{}",
        combined_output(&output)
    );
    assert!(
        !lxmf_dir.path().join("storage").exists(),
        "control preflight should fail before daemon state is created"
    );
}

#[test]
fn control_preflight_invalid_hashes_exit_203() {
    let lxmf_dir = TestDir::new("lxmf-invalid-control");
    let rns_dir = TestDir::new("rns-invalid-control");
    write_rust_identity(&lxmf_dir.path().join("identity"));
    fs::write(
        rns_dir.path().join("config"),
        "\
[reticulum]
share_instance = No

[interfaces]
",
    )
    .expect("write no-interface Reticulum config");

    for args in [
        vec!["--sync", "zz"],
        vec!["--sync", "00"],
        vec!["--break", "zz"],
        vec!["--status", "--remote", "zz"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_lxmd-rs"))
            .args(&args)
            .arg("--config")
            .arg(lxmf_dir.path())
            .arg("--rnsconfig")
            .arg(rns_dir.path())
            .output()
            .expect("lxmd-rs subprocess should run");

        assert_eq!(
            output.status.code(),
            Some(203),
            "expected invalid control hash exit for {args:?}, got:\n{}",
            combined_output(&output)
        );
        assert!(
            combined_output(&output).contains("Invalid"),
            "invalid hash error should be user-visible for {args:?}:\n{}",
            combined_output(&output)
        );
    }
}
