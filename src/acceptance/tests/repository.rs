//! Invariants of the repository as a whole, checked against the files themselves:
//!
//! - SEC-04 (nothing leaves by default): no production code opens a socket, writes a file,
//!   starts a process or prints where it should not, and no crate in the lockfile speaks
//!   the network.
//! - REL-01 (reproducibility): the lockfiles are committed and current, the toolchain is one
//!   pinned release, every digest an operator configuration pins is listed with its
//!   provenance, the hardware of the measurements is recorded, the Python checks are
//!   pinned, and every dependency has a licence.
//! - DOC-01 (traceability): every planned test marked implemented in Rust has a Rust test
//!   that carries its ID, every Rust test named after a planned test names one that exists,
//!   the scenarios the acceptance suite runs are those the matrix says it runs, and every
//!   status is from the vocabulary of the matrices. (The links between requirements, tests,
//!   evidence and ADRs are checked by `docs/thesis/validar_paquete.py`.)
//!
//! They need no screen, no model and no input.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(relative: &str) -> String {
    let path = root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// A CSV file as rows of `column -> value` (UTF-8 with or without a BOM, RFC 4180 quotes).
fn csv(relative: &str) -> Vec<BTreeMap<String, String>> {
    let text = read(relative);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut rows: Vec<Vec<String>> = Vec::new();
    let (mut row, mut field, mut quoted) = (Vec::new(), String::new(), false);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => row.push(std::mem::take(&mut field)),
            ('\r', false) => {}
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            (c, _) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    let header = rows.remove(0);
    rows.into_iter()
        .map(|values| {
            assert_eq!(values.len(), header.len(), "{relative}: {values:?}");
            header.iter().cloned().zip(values).collect()
        })
        .collect()
}

/// Every file under `dir` (relative to the repository) with one of `extensions`, skipping
/// build output and the directories named in `skip`.
fn files(dir: &str, extensions: &[&str], skip: &[&str]) -> Vec<(String, PathBuf)> {
    fn walk(
        base: &Path,
        dir: &Path,
        extensions: &[&str],
        skip: &[&str],
        out: &mut Vec<(String, PathBuf)>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.map(|e| e.unwrap().path()).collect();
        entries.sort();
        for path in entries {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if path.is_dir() {
                if name != "target" && !skip.contains(&name.as_str()) {
                    walk(base, &path, extensions, skip, out);
                }
            } else if path
                .extension()
                .is_some_and(|e| extensions.contains(&e.to_string_lossy().as_ref()))
            {
                let relative = path
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((relative, path));
            }
        }
    }
    let base = root().canonicalize().unwrap();
    let mut out = Vec::new();
    walk(&base, &base.join(dir), extensions, skip, &mut out);
    out
}

/// The Rust sources of the product, up to their unit-test module: no tests, no examples,
/// no benchmark, and nothing of the acceptance crate.
fn production_sources() -> Vec<(String, String)> {
    files(
        "src",
        &["rs"],
        &["tests", "examples", "benches", "acceptance"],
    )
    .into_iter()
    .map(|(relative, path)| {
        let text = std::fs::read_to_string(path).unwrap();
        let end = text.find("#[cfg(test)]").unwrap_or(text.len());
        (relative, text[..end].to_owned())
    })
    .collect()
}

/// Lines of `text` that are code (not a `//` comment) and contain `needle`.
fn code_lines_with<'a>(text: &'a str, needle: &str) -> Vec<&'a str> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("//") && l.contains(needle))
        .collect()
}

// ---- SEC-04 ----

/// SEC-04: the product's code has no path that could carry a capture or a secret out: it
/// opens no socket, writes no file, starts no process, and prints only where the rules of
/// AGENTS.md allow (stdout belongs to MCP; diagnostics go to stderr from a short list of
/// places that print numbers, counts and its own sentences, never screen content).
#[test]
fn sec_04_no_production_code_opens_a_socket_writes_a_file_or_starts_a_process() {
    let forbidden = [
        "std::net",
        "TcpStream",
        "TcpListener",
        "UdpSocket",
        "tokio::net",
        "reqwest",
        "hyper::",
        "File::create",
        "fs::write",
        "OpenOptions",
        "create_dir",
        "fs::copy",
        "fs::rename",
        "fs::remove",
        ".save(",
        "Command::new",
        "dbg!",
    ];
    let sources = production_sources();
    assert!(sources.len() > 50, "{} sources scanned", sources.len());
    let mut found = Vec::new();
    for (path, text) in &sources {
        for needle in forbidden {
            for line in code_lines_with(text, needle) {
                found.push(format!("{path}: {line}"));
            }
        }
    }
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn sec_04_only_the_known_places_print_and_stdout_belongs_to_the_demo_and_the_protocol() {
    // `print!`/`println!` only in the demo printer (the workspace lint denies it elsewhere).
    // `eprint!`/`eprintln!` only in the composition root, the dry-run actuator and the stop
    // notice; the first two print the engine's own sentences and counts.
    let stderr_allowed = [
        "src/app/src/main.rs",
        "src/app/src/demo.rs",
        "src/app/src/policy.rs",
        "src/adapters/input/src/dry_run.rs",
    ];
    let mut stdout_users = BTreeSet::new();
    let mut stderr_users = BTreeSet::new();
    for (path, text) in production_sources() {
        let code: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.starts_with("//"))
            .collect();
        if code
            .iter()
            .any(|l| (l.contains("println!(") || l.contains("print!(")) && !l.contains("eprint"))
        {
            stdout_users.insert(path.clone());
        }
        if code.iter().any(|l| l.contains("eprint")) {
            stderr_users.insert(path);
        }
    }
    assert_eq!(
        stdout_users.iter().map(String::as_str).collect::<Vec<_>>(),
        ["src/app/src/demo.rs"]
    );
    let unexpected: Vec<_> = stderr_users
        .iter()
        .filter(|p| !stderr_allowed.contains(&p.as_str()))
        .collect();
    assert!(unexpected.is_empty(), "{unexpected:?}");
}

/// The package names of a Cargo.lock.
fn locked_packages(relative: &str) -> BTreeSet<String> {
    read(relative)
        .lines()
        .filter_map(|l| l.strip_prefix("name = \""))
        .map(|l| l.trim_end_matches('"').to_owned())
        .collect()
}

/// SEC-04: nothing in the dependency graph speaks HTTP, TLS or WebSockets, so nothing the
/// engine links could send a frame anywhere even by mistake. The operator's tools
/// (`tools/s4`) are separate crates with their own lockfiles and are checked too.
#[test]
fn sec_04_no_crate_in_any_lockfile_speaks_the_network() {
    let network = [
        "reqwest",
        "hyper",
        "ureq",
        "curl",
        "isahc",
        "surf",
        "attohttpc",
        "tungstenite",
        "tokio-tungstenite",
        "h2",
        "native-tls",
        "openssl",
        "rustls",
        "socket2",
        "mio",
        "tonic",
        "axum",
        "warp",
        "actix-web",
        "tiny_http",
        "rouille",
    ];
    for lock in [
        "Cargo.lock",
        "tools/s4/testapp/Cargo.lock",
        "tools/s4/testeditor/Cargo.lock",
    ] {
        let packages = locked_packages(lock);
        assert!(packages.len() > 10, "{lock}");
        let found: Vec<_> = network.iter().filter(|n| packages.contains(**n)).collect();
        assert!(found.is_empty(), "{lock}: {found:?}");
    }
}

/// The names under `[dependencies]` of a crate's manifest.
fn dependencies(relative: &str) -> BTreeSet<String> {
    let manifest: toml::Value = toml::from_str(&read(relative)).unwrap();
    manifest
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .map(|t| t.keys().cloned().collect())
        .unwrap_or_default()
}

/// AGENTS.md §3, as the dependency table: the domain has no I/O and no serialisation (it
/// cannot serialise a frame), the application sees only the domain, an adapter never sees
/// another adapter, and the acceptance crate is never linked into the binary.
#[test]
fn sec_04_and_doc_01_crate_dependencies_follow_the_hexagon() {
    let domain = dependencies("src/domain/Cargo.toml");
    assert!(
        domain.is_subset(&BTreeSet::from([
            "thiserror".to_owned(),
            "unicode-normalization".to_owned()
        ])),
        "{domain:?}"
    );
    let application = dependencies("src/application/Cargo.toml");
    assert!(
        application.is_subset(&BTreeSet::from([
            "ommatix-domain".to_owned(),
            "thiserror".to_owned()
        ])),
        "{application:?}"
    );
    let adapters = [
        ("accessibility", "ommatix-adapter-accessibility"),
        ("capture", "ommatix-adapter-capture"),
        ("vision", "ommatix-adapter-vision"),
        ("input", "ommatix-adapter-input"),
        ("mcp", "ommatix-adapter-mcp"),
        ("sim", "ommatix-adapter-sim"),
    ];
    for (dir, name) in adapters {
        let deps = dependencies(&format!("src/adapters/{dir}/Cargo.toml"));
        let other_adapters: Vec<_> = deps
            .iter()
            .filter(|d| d.starts_with("ommatix-adapter-") && d.as_str() != name)
            .collect();
        assert!(
            other_adapters.is_empty(),
            "{name} depends on {other_adapters:?}"
        );
        assert!(
            !deps.contains("ommatix"),
            "{name} depends on the composition root"
        );
    }
    assert!(dependencies("src/acceptance/Cargo.toml").is_empty());
    // The binary is the only place where adapters meet.
    let app = dependencies("src/app/Cargo.toml");
    assert!(app.contains("ommatix-adapter-mcp") && app.contains("ommatix-adapter-vision"));
}

// ---- REL-01 ----

fn cargo_metadata() -> Value {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = std::process::Command::new(cargo)
        .args(["metadata", "--locked", "--offline", "--format-version", "1"])
        .current_dir(root())
        .output()
        .expect("cargo runs");
    assert!(
        output.status.success(),
        "cargo metadata --locked: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("cargo metadata prints JSON")
}

/// REL-01: Cargo.lock is committed (not ignored) and current: the workspace resolves from
/// it without changing it, offline. The two test programs of S4 carry their own.
#[test]
fn rel_01_the_lockfiles_are_committed_and_current() {
    for lock in [
        "Cargo.lock",
        "tools/s4/testapp/Cargo.lock",
        "tools/s4/testeditor/Cargo.lock",
    ] {
        assert!(root().join(lock).is_file(), "{lock} is missing");
    }
    let ignored: Vec<String> = read(".gitignore")
        .lines()
        .map(|l| l.trim().trim_start_matches('/').to_owned())
        .filter(|l| l == "Cargo.lock" || l == "**/Cargo.lock" || l == "*.lock")
        .collect();
    assert!(ignored.is_empty(), "the lockfile is ignored: {ignored:?}");
    let before = read("Cargo.lock");
    let metadata = cargo_metadata();
    assert_eq!(before, read("Cargo.lock"), "resolving changed Cargo.lock");
    let packages = metadata["packages"].as_array().unwrap();
    assert!(packages.len() > 100, "{} packages", packages.len());
}

fn version_of(text: &str) -> (u32, u32, u32) {
    let mut parts = text.split('.').map(|p| p.parse::<u32>().expect("a number"));
    (
        parts.next().unwrap(),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

/// REL-01: the toolchain is one release, with the components the checks need, and not
/// older than the minimum the workspace declares.
#[test]
fn rel_01_the_toolchain_is_pinned_to_one_release() {
    let toolchain: toml::Value = toml::from_str(&read("rust-toolchain.toml")).unwrap();
    let channel = toolchain["toolchain"]["channel"].as_str().unwrap();
    assert!(
        channel.split('.').count() == 3 && channel.split('.').all(|p| p.parse::<u32>().is_ok()),
        "the channel must be a release like 1.99.0, not {channel:?}"
    );
    let components: BTreeSet<&str> = toolchain["toolchain"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(toml::Value::as_str)
        .collect();
    assert!(components.contains("rustfmt") && components.contains("clippy"));
    let workspace: toml::Value = toml::from_str(&read("Cargo.toml")).unwrap();
    let minimum = workspace["workspace"]["package"]["rust-version"]
        .as_str()
        .unwrap();
    assert!(
        version_of(channel) >= version_of(minimum),
        "{channel} < {minimum}"
    );
    // The CI installs the very release the file pins, in every job that builds.
    let ci = read(".github/workflows/ci.yml");
    let installed: Vec<&str> = ci
        .lines()
        .filter_map(|l| l.trim().strip_prefix("toolchain:"))
        .map(str::trim)
        .collect();
    assert!(!installed.is_empty(), "no CI job installs a toolchain");
    assert!(
        installed.iter().all(|v| *v == channel),
        "CI installs {installed:?}, the file pins {channel}"
    );
    assert!(
        !ci.contains("rust-toolchain@stable"),
        "a CI job builds with whatever stable is"
    );
}

/// REL-01: the Python of the checks is pinned to exact versions.
#[test]
fn rel_01_the_python_checks_are_pinned() {
    let requirements = read("docs/thesis/requirements-validacion.txt");
    let pins: Vec<&str> = requirements
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    assert_ne!(pins.len(), 0);
    for pin in pins {
        assert!(pin.contains("=="), "{pin} is not an exact pin");
    }
}

/// Every `key = "<64 hex>"` of a configuration whose key ends in `_sha256`.
fn pinned_digests(config: &str) -> Vec<(String, String)> {
    config
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let (key, value) = l.split_once('=')?;
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            (key.ends_with("_sha256") && value.len() == 64)
                .then(|| (key.to_owned(), value.to_owned()))
        })
        .collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut hex, b| {
            let _ = write!(hex, "{b:02x}");
            hex
        })
}

/// REL-01: every digest the operator configurations of the spike pin (the manifests, the
/// ONNX Runtime and DirectML libraries) is in the S0 report with its origin and licence,
/// and every pinned manifest is the manifest this repository ships, byte for byte.
#[test]
fn rel_01_every_pinned_digest_is_listed_with_its_provenance() {
    let report = read("docs/thesis/S0_RESULTADOS.md");
    let manifests: BTreeMap<String, String> = files("docs/thesis/plantillas", &["json"], &[])
        .into_iter()
        .filter(|(name, _)| name.contains("model_manifest"))
        .map(|(name, path)| (sha256_hex(&std::fs::read(path).unwrap()), name))
        .collect();
    let configs = files("tools/s0", &["toml"], &[]);
    assert!(configs.len() >= 3, "{} configurations", configs.len());
    let mut pinned = 0;
    for (name, path) in configs {
        for (key, digest) in pinned_digests(&std::fs::read_to_string(path).unwrap()) {
            pinned += 1;
            assert!(
                report.contains(&digest),
                "{name}: {key} = {digest} is not in S0_RESULTADOS.md"
            );
            if key == "detector_manifest_sha256" {
                assert!(
                    manifests.contains_key(&digest),
                    "{name}: {digest} is not the digest of a shipped manifest ({:?})",
                    manifests.values().collect::<Vec<_>>()
                );
            }
        }
    }
    assert!(pinned >= 8, "{pinned} pins checked");
    // The weights the shipped manifests name are the digests the report lists.
    for name in manifests.values() {
        let manifest: Value = serde_json::from_str(&read(name)).unwrap();
        if manifest["status"] != "SELECCIONADO" {
            continue;
        }
        for artifact in manifest["artifacts"].as_array().unwrap() {
            if artifact["role"] == "detector" {
                let digest = artifact["sha256"].as_str().unwrap();
                assert!(
                    report.contains(digest),
                    "{name}: weights {digest} are not in the report"
                );
            }
        }
    }
}

/// REL-01: the hardware and software the measurements ran on are recorded in the form the
/// dossier asks (chapter 03 §3.6): nothing left null.
#[test]
fn rel_01_the_hardware_of_the_measurements_is_recorded() {
    let profile: Value =
        serde_json::from_str(&read("docs/thesis/plantillas/hardware_profile.s0.json")).unwrap();
    assert_eq!(profile["status"], "MEDIDO_S0");
    for (key, value) in profile.as_object().unwrap() {
        assert!(!value.is_null(), "{key} is not recorded");
    }
    for key in [
        "os_build",
        "cpu_model",
        "ram_gib",
        "gpu_model",
        "vram_gib",
        "driver",
        "power_plan",
    ] {
        assert!(profile.get(key).is_some(), "{key}");
    }
    // The template stays a template: nothing measured is invented in it.
    let template: Value =
        serde_json::from_str(&read("docs/thesis/plantillas/hardware_profile.json")).unwrap();
    assert_eq!(template["status"], "PENDIENTE_S0");
}

/// REL-01: no dependency is without a licence and none is under a licence that the MIT
/// licence of the weights and the project's own terms would not carry (GPL family without
/// the library exception). The distinct licence expressions are listed in
/// `docs/thesis/S4_REPRODUCIBILIDAD.md`.
#[test]
fn rel_01_every_dependency_has_a_licence_and_none_is_copyleft() {
    let metadata = cargo_metadata();
    let mut expressions = BTreeSet::new();
    let mut without = Vec::new();
    for package in metadata["packages"].as_array().unwrap() {
        let name = package["name"].as_str().unwrap();
        if name.starts_with("ommatix") {
            continue;
        }
        match package["license"].as_str() {
            Some(licence) => {
                expressions.insert(licence.to_owned());
            }
            None if package["license_file"].is_string() => {}
            None => without.push(name.to_owned()),
        }
    }
    assert!(without.is_empty(), "no licence: {without:?}");
    let copyleft: Vec<_> = expressions
        .iter()
        .filter(|e| e.contains("GPL") && !e.contains("LGPL"))
        .collect();
    assert!(copyleft.is_empty(), "{copyleft:?}");
    let listed = read("docs/thesis/S4_REPRODUCIBILIDAD.md");
    let unlisted: Vec<_> = expressions
        .iter()
        .filter(|e| !listed.contains(e.as_str()))
        .collect();
    assert!(
        unlisted.is_empty(),
        "licences missing from S4_REPRODUCIBILIDAD.md: {unlisted:#?}"
    );
}

/// Asserts that every repository path a document names in code spans exists, and returns how
/// many it checked. A span is a path if it has a `/`, no space, `<` or `%`, and a file
/// extension the repository uses.
fn assert_paths_exist(guide: &str) -> usize {
    let mut paths = 0;
    for part in guide.split('`').skip(1).step_by(2) {
        let looks_like_a_path = part.contains('/')
            && !part.contains(' ')
            && !part.contains('<')
            && !part.contains('%')
            && !part.starts_with("http")
            && part.rsplit_once('.').is_some_and(|(_, ext)| {
                [
                    "md", "rs", "py", "json", "toml", "csv", "lock", "yml", "txt", "feature",
                ]
                .contains(&ext)
            });
        if looks_like_a_path {
            paths += 1;
            let relative = part.trim_start_matches("./");
            assert!(
                root().join(relative).exists()
                    || root().join("docs/thesis").join(relative).exists(),
                "{part} does not exist"
            );
        }
    }
    paths
}

/// REL-01: the reproduction guide names files that exist, and the commands it gives are
/// the ones the repository defines.
#[test]
fn rel_01_the_reproduction_guide_points_at_files_that_exist() {
    let guide = read("docs/thesis/S4_REPRODUCIBILIDAD.md");
    let paths = assert_paths_exist(&guide);
    assert!(paths >= 10, "{paths} paths checked");
    for command in [
        "cargo test --workspace",
        "cargo clippy --workspace --all-targets -- -D warnings",
        "python referencia_ejecutable/run_tests.py",
        "python validar_paquete.py",
        "core_latency",
        "tools/repro/environment.py",
    ] {
        assert!(
            guide.contains(command),
            "the guide does not give {command:?}"
        );
    }
}

/// REL-01: the install and rollback guide and the document of the pending supervised
/// sessions name only repository paths that exist, and give the steps the Definition of
/// Done asks for: build, verify, connect a client, roll back and check that nothing is
/// left. The options, features and digests they cite are checked by
/// `tools/repro/test_install_guide.py`.
#[test]
fn rel_01_the_install_guide_points_at_files_that_exist() {
    let install = read("docs/thesis/S4_INSTALACION_Y_REVERSION.md");
    let sessions = read("docs/thesis/S4_SESIONES_PENDIENTES.md");
    for (name, text, at_least) in [("install", &install, 5), ("sessions", &sessions, 1)] {
        let paths = assert_paths_exist(text);
        assert!(paths >= at_least, "{name}: {paths} paths checked");
    }
    for step in [
        "cargo build --release --locked",
        "tools/repro/environment.py",
        "--operator-dir",
        "git worktree add",
        "Remove-Item -Recurse",
        "## 8. Reversión",
        "mcpServers",
        "rel_01_the_install_guide_points_at_files_that_exist",
    ] {
        assert!(install.contains(step), "the install guide lacks {step:?}");
    }
    for tool in ["tools/perf/soak.py", "tools/perf/dpi_probe.py"] {
        assert!(
            sessions.contains(tool),
            "the sessions document lacks {tool}"
        );
        assert!(root().join(tool).is_file(), "{tool} is missing");
    }
}

// ---- DOC-01 ----

/// The statuses the matrices use. A new one is a deliberate edit of this list and of the
/// validator, with its entry in the changelog.
const PLAN_STATUSES: &[&str] = &[
    "IMPLEMENTADO_EN_RUST_S1",
    "IMPLEMENTADO_EN_RUST_S2",
    "IMPLEMENTADO_EN_RUST_S4",
    "EJECUTADO_SUPERVISADO",
    "EJECUTADO_SUPERVISADO_PARCIAL",
    "EJECUTADO_PARCIAL_SPIKE_S0",
    "PENDIENTE_PRODUCTO",
];
const SCENARIO_STATUSES: &[&str] = &[
    "EJECUTADO_SOBRE_NUCLEO_S1",
    "EJECUTADO_SOBRE_NUCLEO_S2",
    "EJECUTADO_SOBRE_CONTRATO_S2",
    "EJECUTADO_SOBRE_CONTRATO_S3_DOBLE",
];

fn rust_sources() -> String {
    files("src", &["rs"], &[])
        .into_iter()
        .map(|(_, path)| std::fs::read_to_string(path).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

/// DOC-01: a Rust test accredits a planned test only when it carries its identifier
/// (AGENTS.md §4). Every planned test the matrix marks implemented in Rust has one.
#[test]
fn doc_01_every_test_marked_implemented_in_rust_has_a_rust_test_that_carries_its_id() {
    let sources = rust_sources();
    let mut missing = Vec::new();
    let mut implemented = 0;
    for row in csv("docs/thesis/matrices/plan_pruebas.csv") {
        if !row["estado_aceptacion_producto"].starts_with("IMPLEMENTADO_EN_RUST") {
            continue;
        }
        implemented += 1;
        let prefix = format!("fn {}_", row["id"].to_lowercase().replace('-', "_"));
        if !sources.contains(&prefix) {
            missing.push(row["id"].clone());
        }
    }
    assert!(implemented >= 50, "{implemented} rows");
    assert!(
        missing.is_empty(),
        "no Rust test carries the ID of {missing:?}"
    );
}

/// DOC-01: the other way round: a test function named like a planned test (`ut_07_…`,
/// `sec_03_…`) names one the matrix has.
#[test]
fn doc_01_every_rust_test_named_after_a_planned_test_names_one_that_exists() {
    let planned: BTreeSet<String> = csv("docs/thesis/matrices/plan_pruebas.csv")
        .into_iter()
        .map(|r| r["id"].clone())
        .collect();
    let mut unknown = BTreeSet::new();
    let mut named = 0;
    for token in rust_sources().split("fn ").skip(1) {
        let name: String = token
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        let mut parts = name.split('_');
        let (Some(prefix), Some(number)) = (parts.next(), parts.next()) else {
            continue;
        };
        let known_prefix = ["ut", "it", "ct", "sec", "perf", "rel", "doc", "ev"].contains(&prefix);
        if known_prefix && number.len() == 2 && number.chars().all(|c| c.is_ascii_digit()) {
            named += 1;
            let id = format!("{}-{number}", prefix.to_uppercase());
            if !planned.contains(&id) {
                unknown.insert(id);
            }
        }
    }
    assert!(named > 100, "{named} tests named after a planned test");
    assert!(
        unknown.is_empty(),
        "tests named after planned tests the matrix lacks: {unknown:?}"
    );
}

/// DOC-01: the scenarios the acceptance suite has step definitions for are the scenarios
/// the matrix says were executed, and the matrix lists every scenario of the feature files.
#[test]
fn doc_01_the_scenario_matrix_and_the_acceptance_suite_agree() {
    let suite = read("src/acceptance/tests/thesis.rs");
    let start = suite
        .find("const IMPLEMENTED")
        .expect("the list of implemented scenarios");
    let block = &suite[start..start + suite[start..].find("];").unwrap()];
    let implemented: BTreeSet<String> = block
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect();
    let rows = csv("docs/thesis/matrices/gherkin_trazabilidad.csv");
    let executed: BTreeSet<String> = rows
        .iter()
        .filter(|r| r["status"].starts_with("EJECUTADO"))
        .map(|r| r["scenario"].clone())
        .collect();
    assert_eq!(implemented, executed);
    for row in &rows {
        assert!(
            SCENARIO_STATUSES.contains(&row["status"].as_str()),
            "{row:?}"
        );
    }
    let mut in_features = BTreeSet::new();
    for (_, path) in files("docs/thesis/features", &["feature"], &[]) {
        for line in std::fs::read_to_string(path).unwrap().lines() {
            if let Some(rest) = line.trim().strip_prefix("Escenario: ") {
                in_features.insert(rest.split_whitespace().next().unwrap().to_owned());
            }
        }
    }
    let in_matrix: BTreeSet<String> = rows.iter().map(|r| r["scenario"].clone()).collect();
    assert_eq!(in_features, in_matrix);
}

/// DOC-01: every status in the plan is from the vocabulary of the matrices; a status the
/// vocabulary lacks is a decision, not a typo.
#[test]
fn doc_01_every_status_is_from_the_vocabulary_of_the_matrices() {
    for row in csv("docs/thesis/matrices/plan_pruebas.csv") {
        assert!(
            PLAN_STATUSES.contains(&row["estado_aceptacion_producto"].as_str()),
            "{}: {}",
            row["id"],
            row["estado_aceptacion_producto"]
        );
    }
    for row in csv("docs/thesis/matrices/requisitos_trazabilidad.csv") {
        assert_eq!(
            row["estado_producto"], "PENDIENTE_IMPLEMENTACION_O_VALIDACION",
            "{}",
            row["id"]
        );
    }
}
