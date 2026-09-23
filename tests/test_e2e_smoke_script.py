"""Exercise the release script with inert Docker and CLI process fixtures."""

from __future__ import annotations

import json
from pathlib import Path
import shutil
import subprocess
import sys

import pytest


SCRIPT = Path(__file__).resolve().parents[1] / "scripts/e2e-smoke-test.sh"
BASH = shutil.which("bash")
pytestmark = pytest.mark.skipif(BASH is None, reason="release script requires bash")


@pytest.fixture
def smoke(tmp_path):
    repo = tmp_path / "source"
    (repo / "scripts").mkdir(parents=True)
    shutil.copyfile(SCRIPT, repo / "scripts/e2e-smoke-test.sh")
    (repo / "dist").mkdir()
    (repo / "dist/amplifier_core-2.0.0-py3-none-any.whl").write_text("inert wheel")
    home = tmp_path / "home"
    (home / ".amplifier").mkdir(parents=True)
    commands = tmp_path / "bin"
    commands.mkdir()
    log = tmp_path / "calls.jsonl"
    state = tmp_path / "container.json"
    fixture = tmp_path / "fixture.py"
    fixture.write_text(
        """import json, os, pathlib, subprocess, sys
kind, *args = sys.argv[1:]
with open(os.environ['FIXTURE_LOG'], 'a') as f:
    f.write(json.dumps({'kind': kind, 'args': args,
                       'provider_env': {k:v for k,v in os.environ.items()
                                        if k.startswith(('ANTHROPIC_', 'OPENAI_', 'AZURE_'))}}) + '\\n')
if kind == 'cli':
    print(os.environ.get('FIXTURE_SMOKE_OUTPUT', 'inert CLI completed'))
    sys.exit(int(os.environ.get('FIXTURE_SMOKE_EXIT', '0')))
if kind == 'timeout':
    sys.exit(0)
if kind != 'docker':
    sys.exit(90)
if args[0] in ('rm', 'cp'):
    sys.exit(0)
if args[0] == 'run':
    if '-d' in args:
        selected = {}
        for i, arg in enumerate(args):
            if arg == '-e':
                key, sep, value = args[i+1].partition('=')
                if sep or key in os.environ:
                    selected[key] = value if sep else os.environ[key]
        pathlib.Path(os.environ['FIXTURE_STATE']).write_text(json.dumps(selected))
        print('inert-container')
    else:
        sys.exit(int(os.environ.get('FIXTURE_PREFLIGHT_EXIT', '0')))
    sys.exit(0)
if args[0] == 'exec' and args[2:4] == ['bash', '-c']:
    command = args[4]
    if 'amplifier run' in command:
        env = {k:v for k,v in os.environ.items()
               if not k.startswith(('ANTHROPIC_', 'OPENAI_', 'AZURE_'))}
        env.update(json.loads(pathlib.Path(os.environ['FIXTURE_STATE']).read_text()))
        result = subprocess.run([os.environ['FIXTURE_BASH'], '-c', *args[4:]],
                                env=env, timeout=5)
        sys.exit(result.returncode)
    if 'amplifier --version' in command:
        print('amplifier fixture (core 2.0.0)')
    elif 'uv pip install' in command:
        code = int(os.environ.get('FIXTURE_INSTALL_EXIT', '0'))
        print('Installed inert wheel' if code == 0 else 'install failed')
        sys.exit(code)
    elif 'apt-get' not in command and 'uv tool install' not in command:
        sys.exit(91)
    sys.exit(0)
sys.exit(92)
"""
    )
    docker = commands / "docker"
    docker.write_text(
        f"#!{sys.executable}\n"
        "import os, sys\n"
        "os.execv(sys.executable, [sys.executable, os.environ['FIXTURE_PROGRAM'],"
        " 'docker', *sys.argv[1:]])\n"
    )
    docker.chmod(0o700)
    # Shell functions take precedence over PATH, including /root/.local/bin
    # inserted by the actual smoke command. No installed CLI can be reached.
    shell_env = tmp_path / "shell-env"
    shell_env.write_text(
        'amplifier() { "$FIXTURE_PYTHON" "$FIXTURE_PROGRAM" cli "$@"; }\n'
        'timeout() { "$FIXTURE_PYTHON" "$FIXTURE_PROGRAM" timeout "$1"; '
        'shift; "$@"; }\n'
    )
    env = {
        "PATH": f"{commands}:/usr/bin:/bin",
        "HOME": str(home),
        "BASH_ENV": str(shell_env),
        "FIXTURE_PROGRAM": str(fixture),
        "FIXTURE_PYTHON": sys.executable,
        "FIXTURE_BASH": BASH,
        "FIXTURE_LOG": str(log),
        "FIXTURE_STATE": str(state),
    }

    def run(extra=None, keys=None):
        if keys is not None:
            (home / ".amplifier/keys.env").write_text(keys)
        result = subprocess.run(
            [BASH, str(repo / "scripts/e2e-smoke-test.sh"), "--skip-build"],
            env=env | (extra or {}),
            capture_output=True,
            text=True,
            timeout=15,
        )
        calls = [json.loads(line) for line in log.read_text().splitlines()]
        return result, calls

    return run


def cli_call(calls):
    found = [call for call in calls if call["kind"] == "cli"]
    assert len(found) == 1
    return found[0]


def test_default_anthropic_keeps_optional_model_and_endpoint(smoke):
    result, calls = smoke({"ANTHROPIC_API_KEY": "fixture-anthropic"})
    assert result.returncode == 0, result.stdout + result.stderr
    call = cli_call(calls)
    assert call["args"] == [
        "run", "--provider", "anthropic", "--",
        "Ask recipe author to run one of its example recipes",
    ]
    assert call["provider_env"] == {"ANTHROPIC_API_KEY": "fixture-anthropic"}


@pytest.mark.parametrize("provider", ["anthropic", "openai"])
def test_selected_family_only_with_endpoint_model_and_literal_arguments(smoke, tmp_path, provider):
    prefix = provider.upper()
    marker = tmp_path / "must-not-exist"
    prompt = f"literal'; touch {marker}; # $(touch {marker})"
    model = f"model' $(touch {marker})"
    result, calls = smoke({
        "SMOKE_PROVIDER": provider, "SMOKE_MODEL": model, "SMOKE_PROMPT": prompt,
        "ANTHROPIC_API_KEY": "fixture-anthropic", "OPENAI_API_KEY": "fixture-openai",
        "AZURE_OPENAI_API_KEY": "fixture-azure",
        "ANTHROPIC_BASE_URL": "https://anthropic.invalid/", "OPENAI_BASE_URL": "https://openai.invalid/v1",
    }, keys="exit 87\n")
    assert result.returncode == 0, result.stdout + result.stderr
    call = cli_call(calls)
    assert call["args"] == ["run", "--provider", provider, "--model", model, "--", prompt]
    assert call["provider_env"] == {
        f"{prefix}_API_KEY": f"fixture-{provider}",
        f"{prefix}_BASE_URL": f"https://{provider}.invalid/" + ("v1" if provider == "openai" else ""),
    }
    assert not marker.exists()
    assert "Loading API keys" not in result.stdout


def test_leading_dash_prompt_cannot_become_cli_help(smoke):
    result, calls = smoke({"ANTHROPIC_API_KEY": "fixture-key", "SMOKE_PROMPT": "--help"})
    assert result.returncode == 0, result.stdout + result.stderr
    assert cli_call(calls)["args"] == ["run", "--provider", "anthropic", "--", "--help"]


def test_optional_bundle_uses_supported_cli_argument(smoke):
    result, calls = smoke({
        "OPENAI_API_KEY": "fixture-key", "SMOKE_PROVIDER": "openai",
        "SMOKE_MODEL": "gpt-5.6-terra", "SMOKE_BUNDLE": "foundation",
        "SMOKE_PROMPT": "fixture recipe",
    })
    assert result.returncode == 0, result.stdout + result.stderr
    assert cli_call(calls)["args"] == [
        "run", "--provider", "openai", "--model", "gpt-5.6-terra",
        "--bundle", "foundation", "--", "fixture recipe",
    ]


@pytest.mark.parametrize("provider", ["anthropic", "openai"])
def test_selected_key_fallback_is_preserved(smoke, provider):
    prefix = provider.upper()
    result, calls = smoke(
        {"SMOKE_PROVIDER": provider},
        keys=f'{prefix}_API_KEY="fixture-key"\n{prefix}_BASE_URL="https://fixture.invalid/"\n',
    )
    assert result.returncode == 0, result.stdout + result.stderr
    assert "Loading API keys" in result.stdout
    assert cli_call(calls)["provider_env"] == {
        f"{prefix}_API_KEY": "fixture-key", f"{prefix}_BASE_URL": "https://fixture.invalid/",
    }


@pytest.mark.parametrize("extra", [{"SMOKE_PROVIDER": "unsupported"}, {"SMOKE_PROVIDER": "openai"}])
def test_invalid_selection_or_missing_selected_key_stops_before_container(smoke, extra):
    result, calls = smoke({"ANTHROPIC_API_KEY": "fixture-unselected"} | extra)
    assert result.returncode != 0
    assert all(call["kind"] == "docker" and call["args"][0] == "rm" for call in calls)


@pytest.mark.parametrize(
    ("extra", "message"),
    [
        ({"FIXTURE_SMOKE_EXIT": "7"}, "amplifier exited with status 7"),
        ({"FIXTURE_SMOKE_EXIT": "124"}, "TIMED OUT"),
        ({"FIXTURE_SMOKE_OUTPUT": "Tool fixture failed: synthetic"}, "tool failure"),
        ({"FIXTURE_PREFLIGHT_EXIT": "9"}, "Pristine-import preflight failed"),
        ({"FIXTURE_INSTALL_EXIT": "8"}, "Wheel override failed"),
    ],
)
def test_real_script_propagates_fixture_failures_and_cleans_container(smoke, extra, message):
    result, calls = smoke({"ANTHROPIC_API_KEY": "fixture-key"} | extra)
    assert result.returncode != 0
    assert message in result.stdout
    assert "SMOKE TEST PASSED" not in result.stdout
    assert calls[-1]["kind"] == "docker" and calls[-1]["args"][:2] == ["rm", "-f"]
