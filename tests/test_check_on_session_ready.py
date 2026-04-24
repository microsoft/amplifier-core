"""
Tests for check_on_session_ready() helper and its integration into validators.
"""

import types
import pytest


class TestCheckOnSessionReadyFunction:
    """Tests for the check_on_session_ready() helper function."""

    def test_importable_from_structural(self):
        """check_on_session_ready must be importable from amplifier_core.validation.structural."""
        from amplifier_core.validation.structural import check_on_session_ready  # noqa: F401

    def test_in_all_list(self):
        """check_on_session_ready must be exported in __all__."""
        import amplifier_core.validation.structural as m

        assert "check_on_session_ready" in m.__all__

    def test_returns_none_when_on_session_ready_absent(self):
        """Returns None when module has no on_session_ready attribute."""
        from amplifier_core.validation.structural import check_on_session_ready

        module = types.ModuleType("fake_module")
        result = check_on_session_ready(module)
        assert result is None

    def test_returns_error_check_when_on_session_ready_not_async(self):
        """Returns failing ValidationCheck when on_session_ready exists but is not async."""
        from amplifier_core.validation.structural import check_on_session_ready

        module = types.ModuleType("fake_module")

        def on_session_ready():
            pass

        module.on_session_ready = on_session_ready  # type: ignore[attr-defined]

        result = check_on_session_ready(module)
        assert result is not None
        assert result.name == "on_session_ready_async"
        assert result.passed is False
        assert "async" in result.message.lower()
        assert result.severity == "error"

    def test_returns_none_when_on_session_ready_is_async_with_coordinator(self):
        """Returns None when on_session_ready is present, async, and accepts coordinator."""
        from amplifier_core.validation.structural import check_on_session_ready

        module = types.ModuleType("fake_module")

        async def on_session_ready(coordinator):
            pass

        module.on_session_ready = on_session_ready  # type: ignore[attr-defined]

        result = check_on_session_ready(module)
        assert result is None

    def test_check_name_is_on_session_ready_async(self):
        """When returning a check, name must be 'on_session_ready_async'."""
        from amplifier_core.validation.structural import check_on_session_ready

        module = types.ModuleType("fake_module")

        def on_session_ready():
            pass

        module.on_session_ready = on_session_ready  # type: ignore[attr-defined]

        result = check_on_session_ready(module)
        assert result is not None
        assert result.name == "on_session_ready_async"

    def test_check_severity_is_error(self):
        """When returning a check, severity must be 'error'."""
        from amplifier_core.validation.structural import check_on_session_ready

        module = types.ModuleType("fake_module")

        def on_session_ready():
            pass

        module.on_session_ready = on_session_ready  # type: ignore[attr-defined]

        result = check_on_session_ready(module)
        assert result is not None
        assert result.severity == "error"

    def test_b5_returns_error_when_no_coordinator_arg(self):
        """B5: Returns failing check when on_session_ready is async but takes no args."""
        from amplifier_core.validation.structural import check_on_session_ready

        module = types.ModuleType("fake_module")

        async def on_session_ready():  # missing coordinator param
            pass

        module.on_session_ready = on_session_ready  # type: ignore[attr-defined]

        result = check_on_session_ready(module)
        assert result is not None
        assert result.passed is False
        assert result.name == "on_session_ready_async"
        assert "coordinator" in result.message.lower()

    def test_b5_accepts_coordinator_with_default(self):
        """B5: coordinator with a default value does NOT count (must be required)."""
        from amplifier_core.validation.structural import check_on_session_ready

        module = types.ModuleType("fake_module")

        async def on_session_ready(coordinator=None):  # optional coordinator
            pass

        module.on_session_ready = on_session_ready  # type: ignore[attr-defined]

        # coordinator has a default — not required positional; should fail B5 check
        result = check_on_session_ready(module)
        assert result is not None
        assert result.passed is False

    def test_b5_passes_with_extra_kwargs(self):
        """B5: on_session_ready(coordinator, **kwargs) passes arity check."""
        from amplifier_core.validation.structural import check_on_session_ready

        module = types.ModuleType("fake_module")

        async def on_session_ready(coordinator, **kwargs):
            pass

        module.on_session_ready = on_session_ready  # type: ignore[attr-defined]

        result = check_on_session_ready(module)
        assert result is None


class TestCheckOnSessionReadyInValidators:
    """Tests that all 5 validators call check_on_session_ready and add the result."""

    @pytest.mark.asyncio
    async def test_hook_validator_catches_sync_on_session_ready(self, tmp_path):
        """HookValidator includes on_session_ready_async check when on_session_ready is sync."""
        module_file = tmp_path / "test_hook_sync_on_session_ready.py"
        module_file.write_text("""
async def mount(coordinator, config):
    return None

def on_session_ready(coordinator):
    pass  # sync, should be async
""")
        from amplifier_core.validation import HookValidator

        validator = HookValidator()
        result = await validator.validate(str(module_file))
        check_names = [c.name for c in result.checks]
        assert "on_session_ready_async" in check_names
        on_sr_check = next(
            c for c in result.checks if c.name == "on_session_ready_async"
        )
        assert on_sr_check.passed is False

    @pytest.mark.asyncio
    async def test_tool_validator_catches_sync_on_session_ready(self, tmp_path):
        """ToolValidator includes on_session_ready_async check when on_session_ready is sync."""
        module_file = tmp_path / "test_tool_sync_on_session_ready.py"
        module_file.write_text("""
async def mount(coordinator, config):
    return None

def on_session_ready(coordinator):
    pass  # sync, should be async
""")
        from amplifier_core.validation import ToolValidator

        validator = ToolValidator()
        result = await validator.validate(str(module_file))
        check_names = [c.name for c in result.checks]
        assert "on_session_ready_async" in check_names
        on_sr_check = next(
            c for c in result.checks if c.name == "on_session_ready_async"
        )
        assert on_sr_check.passed is False

    @pytest.mark.asyncio
    async def test_orchestrator_validator_catches_sync_on_session_ready(self, tmp_path):
        """OrchestratorValidator includes on_session_ready_async check."""
        module_file = tmp_path / "test_orch_sync_on_session_ready.py"
        module_file.write_text("""
async def mount(coordinator, config):
    return None

def on_session_ready(coordinator):
    pass  # sync, should be async
""")
        from amplifier_core.validation import OrchestratorValidator

        validator = OrchestratorValidator()
        result = await validator.validate(str(module_file))
        check_names = [c.name for c in result.checks]
        assert "on_session_ready_async" in check_names
        on_sr_check = next(
            c for c in result.checks if c.name == "on_session_ready_async"
        )
        assert on_sr_check.passed is False

    @pytest.mark.asyncio
    async def test_provider_validator_catches_sync_on_session_ready(self, tmp_path):
        """ProviderValidator includes on_session_ready_async check."""
        module_file = tmp_path / "test_provider_sync_on_session_ready.py"
        module_file.write_text("""
async def mount(coordinator, config):
    return None

def on_session_ready(coordinator):
    pass  # sync, should be async
""")
        from amplifier_core.validation import ProviderValidator

        validator = ProviderValidator()
        result = await validator.validate(str(module_file))
        check_names = [c.name for c in result.checks]
        assert "on_session_ready_async" in check_names
        on_sr_check = next(
            c for c in result.checks if c.name == "on_session_ready_async"
        )
        assert on_sr_check.passed is False

    @pytest.mark.asyncio
    async def test_context_validator_catches_sync_on_session_ready(self, tmp_path):
        """ContextValidator includes on_session_ready_async check."""
        module_file = tmp_path / "test_context_sync_on_session_ready.py"
        module_file.write_text("""
async def mount(coordinator, config):
    return None

def on_session_ready(coordinator):
    pass  # sync, should be async
""")
        from amplifier_core.validation import ContextValidator

        validator = ContextValidator()
        result = await validator.validate(str(module_file))
        check_names = [c.name for c in result.checks]
        assert "on_session_ready_async" in check_names
        on_sr_check = next(
            c for c in result.checks if c.name == "on_session_ready_async"
        )
        assert on_sr_check.passed is False

    @pytest.mark.asyncio
    async def test_hook_validator_no_check_when_on_session_ready_absent(self, tmp_path):
        """HookValidator does not add check when on_session_ready is absent."""
        module_file = tmp_path / "test_hook_no_on_session_ready.py"
        module_file.write_text("""
async def mount(coordinator, config):
    return None
""")
        from amplifier_core.validation import HookValidator

        validator = HookValidator()
        result = await validator.validate(str(module_file))
        check_names = [c.name for c in result.checks]
        assert "on_session_ready_async" not in check_names

    @pytest.mark.asyncio
    async def test_hook_validator_no_check_when_on_session_ready_async(self, tmp_path):
        """HookValidator does not add check when on_session_ready is properly async."""
        module_file = tmp_path / "test_hook_async_on_session_ready.py"
        module_file.write_text("""
async def mount(coordinator, config):
    return None

async def on_session_ready(coordinator):
    pass  # properly async with coordinator arg
""")
        from amplifier_core.validation import HookValidator

        validator = HookValidator()
        result = await validator.validate(str(module_file))
        check_names = [c.name for c in result.checks]
        assert "on_session_ready_async" not in check_names

    @pytest.mark.asyncio
    async def test_check_on_session_ready_appears_after_mount_signature(self, tmp_path):
        """on_session_ready_async check appears after mount_signature check."""
        module_file = tmp_path / "test_check_order.py"
        module_file.write_text("""
async def mount(coordinator, config):
    return None

def on_session_ready(coordinator):
    pass  # sync, should be async
""")
        from amplifier_core.validation import HookValidator

        validator = HookValidator()
        result = await validator.validate(str(module_file))

        check_names = [c.name for c in result.checks]
        # mount_signature should appear before on_session_ready_async
        assert "mount_signature" in check_names
        assert "on_session_ready_async" in check_names
        sig_idx = check_names.index("mount_signature")
        osr_idx = check_names.index("on_session_ready_async")
        assert sig_idx < osr_idx
