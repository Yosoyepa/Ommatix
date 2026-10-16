"""REL-01: the install and rollback guide, and the pending-sessions document, name nothing that
the repository does not have. Pure text checks: nothing is built or run.

    python -m unittest discover -s tools/repro -p "test_*.py"

Every repository path, every `ommatix` option, every build feature, every option of the Python
tools, every test name and every SHA-256 these two documents give must exist where the
repository defines it, so that a renamed flag or a moved file fails here instead of in the
owner's supervised session.
"""
import glob
import re
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
DOCS = [REPO / "docs" / "thesis" / "S4_INSTALACION_Y_REVERSION.md",
        REPO / "docs" / "thesis" / "S4_SESIONES_PENDIENTES.md"]
PYTHON_TOOLS = {
    "tools/perf/soak.py": REPO / "tools" / "perf" / "soak.py",
    "tools/perf/dpi_probe.py": REPO / "tools" / "perf" / "dpi_probe.py",
    "tools/perf/measure_latency.py": REPO / "tools" / "perf" / "measure_latency.py",
    "tools/perf/freshness_probe.py": REPO / "tools" / "perf" / "freshness_probe.py",
    "tools/s4/e2e_driver.py": REPO / "tools" / "s4" / "e2e_driver.py",
    "tools/s4/e2e_dossier.py": REPO / "tools" / "s4" / "e2e_dossier.py",
    "tools/repro/environment.py": REPO / "tools" / "repro" / "environment.py",
    "tools/s0/export_omniparser_v3.py": REPO / "tools" / "s0" / "export_omniparser_v3.py",
}
# Options of the programs the guides use that this repository does not define.
CARGO = {"--locked", "--release", "--features", "--workspace", "--all-targets", "--check",
         "--all", "--ignored", "--nocapture"}
OTHER = {"--all", "--tags", "--porcelain", "--animate"}  # git; the test application's hidden flag
FLAG = re.compile(r"(?<![\w-])(--[a-z][a-z0-9-]*)")
PATH = re.compile(r"(?<![\w%$.])((?:tools|docs|src)[\\/][\w.*\-\\/]*[\w*])")
HEX64 = re.compile(r"\b[0-9a-f]{64}\b")


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def tool_flags(path: Path) -> set[str]:
    return set(re.findall(r"""add_argument\(\s*["'](--[a-z0-9-]+)["']""", read(path)))


def cli_flags() -> set[str]:
    """`--long` options of `ommatix`: the field names of the clap structs, with `-` for `_`."""
    text = read(REPO / "src" / "app" / "src" / "cli.rs")
    fields = re.findall(r"^\s*pub (\w+): ", text, flags=re.M)
    return {"--" + f.replace("_", "-") for f in fields if f not in ("run", "command")}


def build_features() -> set[str]:
    text = read(REPO / "src" / "app" / "Cargo.toml")
    section = text.split("[features]", 1)[1].split("[dependencies]", 1)[0]
    return set(re.findall(r"^([a-z][a-z0-9-]*)\s*=", section, flags=re.M))


def commands(text: str) -> list[str]:
    """Logical command lines: join cmd/PowerShell continuations; keep inline spans outside fences."""
    found = []
    in_fence = False
    pending = ""
    prose = []
    for line in text.splitlines():
        if line.strip().startswith("```"):
            in_fence = not in_fence
            pending = ""
            continue
        if in_fence:
            joined = pending + line.strip()
            if joined.endswith(("^", "`")):
                pending = joined[:-1].rstrip() + " "
            else:
                pending = ""
                found.append(joined)
        else:
            prose.append(line)
    for span in re.findall(r"`([^`\n]+)`", "\n".join(prose)):
        found.append(span)
    return found


class Guides(unittest.TestCase):
    def test_powershell_continuations_keep_python_flags_with_their_own_program(self):
        text = ('```powershell\npython tools/perf/freshness_probe.py --exe ommatix.exe `\n'
                '  --pairs 30 `\n  --target-x 1\n```\nUse `--config` here.\n')
        self.assertEqual(commands(text), [
            'python tools/perf/freshness_probe.py --exe ommatix.exe --pairs 30 --target-x 1',
            '--config'])

    def test_both_documents_exist_and_are_not_empty(self):
        for doc in DOCS:
            self.assertGreater(len(read(doc)), 2000, doc.name)

    def test_every_repository_path_they_name_exists(self):
        checked = 0
        for doc in DOCS:
            for raw in PATH.findall(read(doc)):
                path = raw.rstrip(".").replace("\\", "/")
                if "/target/" in path or "<" in path:
                    continue  # build output, a placeholder
                hits = glob.glob(str(REPO / path)) if "*" in path else [REPO / path]
                self.assertTrue(hits and all(Path(h).exists() for h in hits),
                                f"{doc.name}: {raw} does not exist")
                checked += 1
        self.assertGreater(checked, 20)

    def test_every_option_they_use_is_defined_by_the_program_it_belongs_to(self):
        ommatix = cli_flags()
        self.assertTrue({"--source", "--config", "--observations", "--timings",
                         "--session-id", "--legacy", "--input"} <= ommatix)
        known = ommatix | CARGO | OTHER
        for tool in PYTHON_TOOLS.values():
            known |= tool_flags(tool)
        for doc in DOCS:
            for command in commands(read(doc)):
                flags = FLAG.findall(command)
                if not flags:
                    continue
                owner = next((name for name in PYTHON_TOOLS if name in command.replace("\\", "/")),
                             None)
                if owner:
                    allowed = tool_flags(PYTHON_TOOLS[owner])
                elif "ommatix.exe" in command or "cargo run" in command and "--" in command:
                    allowed = ommatix | CARGO
                else:
                    allowed = known
                for flag in flags:
                    self.assertIn(flag, allowed, f"{doc.name}: {flag} in `{command}`")

    def test_every_build_feature_they_name_exists(self):
        features = build_features()
        self.assertTrue({"dxgi", "onnx", "directml", "winrt-ocr", "uia", "sendinput"} <= features)
        named = 0
        for doc in DOCS:
            text = read(doc)
            for listing in re.findall(r"--features ([a-z0-9,-]+)", text):
                for feature in listing.split(","):
                    self.assertIn(feature, features, f"{doc.name}: {feature}")
                    named += 1
            for span in re.findall(r"`([a-z0-9,-]+)`", text):
                if "," in span:  # a bare list of features, as in `dxgi,directml,winrt-ocr,uia`
                    for feature in span.split(","):
                        self.assertIn(feature, features, f"{doc.name}: {feature}")
        self.assertGreater(named, 3)

    def test_every_crate_they_build_is_in_the_workspace(self):
        members = read(REPO / "Cargo.toml")
        for doc in DOCS:
            for crate in re.findall(r"-p ([a-z][a-z0-9-]+)", read(doc)):
                self.assertTrue(crate == "ommatix" or f"ommatix-{crate.removeprefix('ommatix-')}"
                                in {f"ommatix-{n}" for n in re.findall(r"ommatix-([a-z-]+)", members)},
                                f"{doc.name}: {crate}")

    def test_every_digest_they_give_is_in_the_provenance_table_of_the_s0_report(self):
        report = read(REPO / "docs" / "thesis" / "S0_RESULTADOS.md")
        digests = {d for doc in DOCS for d in HEX64.findall(read(doc))}
        self.assertGreaterEqual(len(digests), 8)
        for digest in sorted(digests):
            self.assertIn(digest, report, f"{digest} is not in S0_RESULTADOS.md")

    def test_every_pinned_digest_of_the_operator_configurations_is_in_the_guide(self):
        guide = read(DOCS[0])
        for config in sorted((REPO / "tools" / "s0").glob("*.toml")):
            for digest in re.findall(r'_sha256 = "([0-9a-f]{64})"', read(config)):
                self.assertIn(digest, guide, f"{config.name}: {digest}")

    def test_every_test_they_cite_exists(self):
        sources = "\n".join(read(p) for p in (REPO / "src").rglob("*.rs"))
        cited = {t for doc in DOCS
                 for t in re.findall(r"`((?:rel_01|sec_0\d|dxgi)_[a-z0-9_]+)`", read(doc))}
        self.assertIn("rel_01_the_install_guide_points_at_files_that_exist", cited)
        for name in sorted(cited):
            self.assertTrue(f"fn {name}" in sources, name)

    def test_the_probe_reads_the_line_the_manual_capture_test_prints(self):
        import sys
        sys.path.insert(0, str(REPO / "tools" / "perf"))
        import dpi_probe
        source = read(REPO / "src" / "adapters" / "capture" / "src" / "dxgi" / "win.rs")
        self.assertIn(f"fn {dpi_probe.CAPTURE_TEST}", source)
        self.assertIn('"dxgi output 0: {}x{} stride {} epoch {} mapping {mapping:?} {:?}"', source)
        domain = read(REPO / "src" / "domain" / "src" / "display.rs")
        for field in ("origin_x", "origin_y", "frame_w", "frame_h", "os_w", "os_h"):
            self.assertIn(f"{field}: ", domain)


if __name__ == "__main__":
    unittest.main()
