"""Dev toolchain helpers for SigmaOS-dev-tools (inspired by mint-dev-tools).

Pure planning over an injected toolchain inventory: doctor reports missing
or outdated tools, fmt-plan/lint-plan map detected languages to formatter
and linter commands (dry-run), and bootstrap composes a package install
plan for a project stack. Nothing is installed or executed here.
"""
import argparse, json, sys

ENGINE = {
    "name": "sigma-devtools",
    "summary": "Toolchain doctor, format/lint plans, stack bootstrap",
    "language": "python",
    "verbs": ["doctor", "fmt-plan", "lint-plan", "bootstrap"],
    "principles": ["Plan Before Apply", "Pure Functions", "Fail-Fast",
                   "KISS", "Deterministic Testing"],
}

cli_doc = """\
$ sigma-devtools doctor --tools tools.json
$ sigma-devtools fmt-plan --languages python,shell,markdown
$ sigma-devtools lint-plan --languages python --strict
$ sigma-devtools bootstrap --stack python-web --package-manager apt
"""

# min supported versions per tool (major.minor tuples)
REQUIRED_TOOLS = {
    "git": (2, 34),
    "python3": (3, 10),
    "node": (18, 0),
    "npm": (9, 0),
    "shellcheck": (0, 8),
    "black": (23, 0),
    "ruff": (0, 1),
}

FORMATTERS = {
    "python": "black {files}",
    "shell": "shfmt -w {files}",
    "markdown": "prettier --write {files}",
    "json": "prettier --write {files}",
    "yaml": "prettier --write {files}",
    "toml": "taplo format {files}",
    "go": "gofmt -w {files}",
    "rust": "cargo fmt",
}

LINTERS = {
    "python": "ruff check {files}",
    "shell": "shellcheck {files}",
    "markdown": "markdownlint {files}",
    "json": "prettier --check {files}",
    "yaml": "yamllint {files}",
    "go": "golangci-lint run",
    "rust": "cargo clippy -- -D warnings",
}

FILE_EXTENSIONS = {
    ".py": "python", ".sh": "shell", ".bash": "shell",
    ".md": "markdown", ".json": "json", ".yaml": "yaml", ".yml": "yaml",
    ".toml": "toml", ".go": "go", ".rs": "rust",
}

STACKS = {
    "python-web": {"languages": ["python", "markdown"],
                   "packages": ["python3", "python3-venv", "python3-pip",
                                "black", "ruff"],
                   "setup": ["python3 -m venv .venv",
                             ".venv/bin/pip install -r requirements.txt"]},
    "node-web": {"languages": ["javascript", "json", "yaml"],
                 "packages": ["nodejs", "npm"],
                 "setup": ["npm install", "npm run lint --if-present"]},
    "go-cli": {"languages": ["go"],
               "packages": ["golang-go", "golangci-lint"],
               "setup": ["go mod download", "go build ./..."]},
    "rust-cli": {"languages": ["rust"],
                 "packages": ["cargo", "rustc"],
                 "setup": ["cargo fetch", "cargo build"]},
    "shell": {"languages": ["shell"],
              "packages": ["shellcheck", "shfmt"],
              "setup": ["shellcheck *.sh"]},
}


class DevToolsError(Exception):
    pass


def parse_version(text):
    """Pure: 'v2.43.0' / '18.19.1' -> (18, 19) style tuple (major, minor)."""
    nums = []
    for chunk in str(text).strip().lstrip("v").split("."):
        digits = "".join(c for c in chunk if c.isdigit())
        if not digits:
            break
        nums.append(int(digits))
        if len(nums) == 2:
            break
    if len(nums) < 2:
        raise DevToolsError(f"unparseable version: {text!r}")
    return tuple(nums)


def detect_languages(paths):
    """Pure: map file paths -> detected language set via extensions."""
    langs = set()
    for path in paths:
        for ext, lang in FILE_EXTENSIONS.items():
            if path.endswith(ext):
                langs.add(lang)
                break
    return sorted(langs)


def doctor(tools):
    """Pure: compare an injected {tool: version} inventory to requirements."""
    report = []
    for tool in sorted(REQUIRED_TOOLS):
        min_ver = REQUIRED_TOOLS[tool]
        have = tools.get(tool)
        if have is None:
            report.append({"tool": tool, "state": "missing",
                           "have": None, "min": "%d.%d" % min_ver,
                           "hint": f"install {tool}"})
            continue
        try:
            ver = parse_version(have)
        except DevToolsError:
            report.append({"tool": tool, "state": "unknown", "have": have,
                           "min": "%d.%d" % min_ver,
                           "hint": f"could not parse version {have!r}"})
            continue
        if ver < min_ver:
            report.append({"tool": tool, "state": "outdated", "have": have,
                           "min": "%d.%d" % min_ver,
                           "hint": f"upgrade {tool} to >= {'.'.join(map(str, min_ver))}"})
        else:
            report.append({"tool": tool, "state": "ok", "have": have,
                           "min": "%d.%d" % min_ver, "hint": None})
    return report


def plan_fmt(languages, files=("{files}",)):
    """Pure: formatter commands for the given languages."""
    unknown = [l for l in languages if l not in FORMATTERS]
    if unknown:
        raise DevToolsError("no formatter for: " + ", ".join(unknown)
                            + "; known: " + ", ".join(sorted(FORMATTERS)))
    joined = " ".join(files)
    return [{"language": l, "cmd": FORMATTERS[l].format(files=joined)}
            for l in sorted(set(languages))]


def plan_lint(languages, files=("{files}",), strict=False):
    """Pure: linter commands; strict appends --strict-like flags where real."""
    unknown = [l for l in languages if l not in LINTERS]
    if unknown:
        raise DevToolsError("no linter for: " + ", ".join(unknown)
                            + "; known: " + ", ".join(sorted(LINTERS)))
    joined = " ".join(files)
    plans = []
    for l in sorted(set(languages)):
        cmd = LINTERS[l].format(files=joined)
        if strict and l == "markdown":
            cmd += " --disable MD013"
        plans.append({"language": l, "cmd": cmd, "strict": strict})
    return plans


def plan_bootstrap(stack, package_manager="apt"):
    """Pure: package + setup plan for a named stack."""
    if stack not in STACKS:
        raise DevToolsError(f"unknown stack {stack!r}; choose from "
                            + ", ".join(sorted(STACKS)))
    if package_manager not in ("apt", "dnf", "pacman"):
        raise DevToolsError(f"unknown package manager {package_manager!r}")
    info = STACKS[stack]
    install = {"apt": "apt-get install -y",
               "dnf": "dnf install -y",
               "pacman": "pacman -S --noconfirm"}[package_manager]
    return {"stack": stack,
            "languages": sorted(info["languages"]),
            "commands": [f"{install} " + " ".join(info["packages"])]
                        + list(info["setup"]),
            "packages": list(info["packages"])}


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog=ENGINE["name"], description=ENGINE["summary"])
    sub = p.add_subparsers(dest="cmd", required=True)
    dr = sub.add_parser("doctor", help="check a tool inventory against requirements")
    dr.add_argument("--tools", required=True, help="JSON {tool: version}")
    fp = sub.add_parser("fmt-plan", help="formatter commands (dry-run)")
    fp.add_argument("--languages", required=True, help="comma-separated")
    fp.add_argument("--files", default="{files}", help="file list placeholder")
    lp = sub.add_parser("lint-plan", help="linter commands (dry-run)")
    lp.add_argument("--languages", required=True)
    lp.add_argument("--files", default="{files}")
    lp.add_argument("--strict", action="store_true")
    bs = sub.add_parser("bootstrap", help="install+setup plan for a stack")
    bs.add_argument("--stack", required=True, choices=sorted(STACKS))
    bs.add_argument("--package-manager", default="apt",
                    choices=["apt", "dnf", "pacman"])
    return p


def main(argv=None) -> int:
    args = build_parser().parse_args(argv)
    try:
        if args.cmd == "doctor":
            with open(args.tools) as fh:
                tools = json.load(fh)
            report = doctor(tools)
            rc = 0
            for r in report:
                if r["state"] == "ok":
                    print(f"OK       {r['tool']:10} {r['have']}")
                else:
                    print(f"{r['state'].upper():8} {r['tool']:10} "
                          f"have={r['have']} min={r['min']} -- {r['hint']}")
                    rc = 1
            return rc
        if args.cmd == "fmt-plan":
            languages = [l.strip() for l in args.languages.split(",") if l.strip()]
            for step in plan_fmt(languages, (args.files,)):
                print(f"[{step['language']}] {step['cmd']}")
        elif args.cmd == "lint-plan":
            languages = [l.strip() for l in args.languages.split(",") if l.strip()]
            for step in plan_lint(languages, (args.files,), args.strict):
                print(f"[{step['language']}] {step['cmd']}")
        elif args.cmd == "bootstrap":
            plan = plan_bootstrap(args.stack, args.package_manager)
            for cmd in plan["commands"]:
                print(cmd)
        return 0
    except (DevToolsError, OSError, json.JSONDecodeError) as e:
        print(f"error: {e}", file=sys.stderr)
        return 1


def self_test() -> None:
    # version parsing
    assert parse_version("v2.43.0") == (2, 43)
    assert parse_version("18.19.1") == (18, 19)
    assert parse_version("3.12") == (3, 12)
    for bad in ("abc", "1", ""):
        try:
            parse_version(bad)
        except DevToolsError:
            pass
        else:
            raise AssertionError(f"version not caught: {bad!r}")

    # doctor against a realistic inventory
    tools = {"git": "2.43.0", "python3": "3.11.2", "node": "16.20.1",
             "npm": "9.6.7", "shellcheck": "0.9.0", "black": "24.3.0",
             "ruff": "0.1.9"}
    report = doctor(tools)
    by = {r["tool"]: r for r in report}
    assert by["git"]["state"] == "ok"
    assert by["python3"]["state"] == "ok"
    assert by["node"]["state"] == "outdated"      # 16 < 18
    assert by["node"]["hint"].startswith("upgrade node")
    assert all(by[t]["state"] == "ok" for t in ("npm", "shellcheck", "black", "ruff"))
    missing = doctor({})
    assert all(r["state"] == "missing" for r in missing)
    assert len(missing) == len(REQUIRED_TOOLS)
    weird = doctor({"git": "not-a-version"})
    assert [r for r in weird if r["tool"] == "git"][0]["state"] == "unknown"

    # language detection from file names
    langs = detect_languages(["main.py", "setup.sh", "README.md", "conf.yaml",
                              "Cargo.toml", "Makefile", "src/lib.go",
                              "src/main.rs"])
    assert langs == ["go", "markdown", "python", "rust", "shell", "toml", "yaml"]

    # format + lint plans
    fmt = plan_fmt(["python", "shell"])
    assert fmt == [{"language": "python", "cmd": "black {files}"},
                   {"language": "shell", "cmd": "shfmt -w {files}"}]
    fmt_files = plan_fmt(["python"], ("a.py", "b.py"))
    assert fmt_files[0]["cmd"] == "black a.py b.py"
    try:
        plan_fmt(["python", "brainfuck"])
    except DevToolsError as e:
        assert "no formatter for: brainfuck" in str(e)
    else:
        raise AssertionError("unknown language not caught")
    lint = plan_lint(["python", "markdown"], strict=True)
    assert lint[0]["cmd"] == "markdownlint {files} --disable MD013"  # sorted: markdown first
    assert lint[1]["cmd"] == "ruff check {files}"
    assert all(s["strict"] for s in lint)
    assert plan_lint(["json"])[0]["cmd"] == "prettier --check {files}"

    # bootstrap plans
    boot = plan_bootstrap("python-web")
    assert boot["commands"][0].startswith("apt-get install -y ")
    assert "python3-venv" in boot["commands"][0]
    assert boot["commands"][1] == "python3 -m venv .venv"
    assert boot["languages"] == ["markdown", "python"]
    dnf = plan_bootstrap("node-web", package_manager="dnf")
    assert dnf["commands"][0].startswith("dnf install -y ")
    assert plan_bootstrap("rust-cli", "pacman")["commands"][0].startswith(
        "pacman -S --noconfirm ")
    try:
        plan_bootstrap("cobol-mainframe")
    except DevToolsError as e:
        assert "unknown stack" in str(e)
    else:
        raise AssertionError("unknown stack not caught")
    try:
        plan_bootstrap("python-web", package_manager="portage")
    except DevToolsError:
        pass
    else:
        raise AssertionError("unknown pm not caught")

    # determinism
    assert plan_bootstrap("python-web") == plan_bootstrap("python-web")
    assert doctor(tools) == doctor(tools)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
