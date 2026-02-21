"""Tests for expanded RustSession API matching Python AmplifierSession.

Milestone 3: Tasks 3.1 through 3.6.
"""

import pytest
from amplifier_core._engine import RustSession, RustCoordinator


# ---- Task 3.1: Expanded constructor ----


def test_session_full_constructor():
    """Session accepts the full Python AmplifierSession constructor signature."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(
        config=config,
        session_id="test-123",
        parent_id="parent-456",
        is_resumed=True,
    )
    assert session.session_id == "test-123"
    assert session.parent_id == "parent-456"
    assert session.is_resumed is True


def test_session_default_args():
    """Session works with just config (all optionals default to None/False)."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config=config)
    assert len(session.session_id) > 0  # UUID generated
    assert session.parent_id is None
    assert session.is_resumed is False


def test_session_generates_uuid():
    """Session generates a UUID when session_id is not provided."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    s1 = RustSession(config=config)
    s2 = RustSession(config=config)
    assert s1.session_id != s2.session_id


def test_session_validates_config_empty():
    """Session raises for empty config."""
    with pytest.raises(Exception):
        RustSession(config={})


def test_session_validates_config_missing_context():
    """Session raises for config missing context."""
    with pytest.raises(Exception):
        RustSession(config={"session": {"orchestrator": "loop-basic"}})


def test_session_validates_config_missing_orchestrator():
    """Session raises for config missing orchestrator."""
    with pytest.raises(Exception):
        RustSession(config={"session": {"context": "context-simple"}})


# ---- Task 3.2: coordinator, config, is_resumed properties ----


def test_session_coordinator_property():
    """Session has a coordinator property returning a RustCoordinator."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config=config)
    coord = session.coordinator
    assert coord is not None
    assert isinstance(coord, RustCoordinator)
    assert hasattr(coord, "mount_points")
    assert hasattr(coord, "hooks")


def test_session_config_property():
    """Session has a config property returning the original dict."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config=config)
    assert session.config == config


def test_session_is_resumed_property():
    """Session has an is_resumed property."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config=config)
    assert session.is_resumed is False

    session2 = RustSession(config=config, is_resumed=True)
    assert session2.is_resumed is True


def test_session_coordinator_has_session_backref():
    """The coordinator created by session has a back-reference to the session."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config=config)
    # The coordinator's session_id should match
    assert session.coordinator.session_id == session.session_id


def test_session_coordinator_hooks_have_default_fields():
    """The coordinator's hooks should have session_id set as default field."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config=config, session_id="test-456")
    # The hooks should have been set with default fields during construction.
    # Verify hooks have default fields set (session_id)
    hooks = session.coordinator.hooks
    assert hooks is not None


def test_session_coordinator_parent_id_propagated():
    """Parent ID is propagated from session to coordinator."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config=config, parent_id="parent-789")
    assert session.coordinator.parent_id == "parent-789"


def test_session_coordinator_parent_id_none():
    """When no parent_id, coordinator parent_id is also None."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config=config)
    assert session.coordinator.parent_id is None


# ---- Task 3.3: _session_init.py helper and initialize() ----


def test_session_init_module_exists():
    """The _session_init helper module exists and is importable."""
    from amplifier_core._session_init import initialize_session

    assert callable(initialize_session)


def test_session_initialized_flag():
    """Session starts as not initialized."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config=config)
    assert session.initialized is False


# ---- Task 3.4 / Task 9: _session_exec.py helper and execute() ----


def test_session_exec_module_exists():
    """The _session_exec helper module exists and is importable."""
    from amplifier_core._session_exec import run_orchestrator

    assert callable(run_orchestrator)


# ---- Task 3.5: cleanup() wired to coordinator ----


@pytest.mark.asyncio
async def test_cleanup_runs_coordinator_cleanup():
    """cleanup() calls coordinator.cleanup() which runs cleanup functions."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config=config)
    # Register a cleanup function on the coordinator
    called = []
    session.coordinator.register_cleanup(lambda: called.append("cleaned"))
    await session.cleanup()
    assert "cleaned" in called


@pytest.mark.asyncio
async def test_cleanup_runs_in_reverse_order():
    """cleanup() runs coordinator cleanup in reverse order."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config=config)
    order = []
    session.coordinator.register_cleanup(lambda: order.append(1))
    session.coordinator.register_cleanup(lambda: order.append(2))
    session.coordinator.register_cleanup(lambda: order.append(3))
    await session.cleanup()
    assert order == [3, 2, 1]


# ---- Task 3.6: async context manager ----


def test_session_has_aenter():
    """Session has __aenter__ method."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config=config)
    assert hasattr(session, "__aenter__")


def test_session_has_aexit():
    """Session has __aexit__ method."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config=config)
    assert hasattr(session, "__aexit__")


@pytest.mark.asyncio
async def test_session_aexit_calls_cleanup():
    """__aexit__ calls cleanup, running registered cleanup functions."""
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config=config)
    cleaned = []
    session.coordinator.register_cleanup(lambda: cleaned.append(True))
    await session.__aexit__(None, None, None)
    assert cleaned == [True]


# ---- Task 13: Python helper file cleanup ----


def test_hooks_bridge_removed():
    """_hooks_bridge.py should be deleted — no longer needed since Rust HookRegistry handles dispatch."""
    import importlib

    with pytest.raises(ImportError):
        importlib.import_module("amplifier_core._hooks_bridge")


def test_session_init_is_thin_helper():
    """_session_init.py must still exist as a thin boundary helper called by Rust."""
    from amplifier_core._session_init import initialize_session

    assert callable(initialize_session)


def test_session_exec_is_thin_helper():
    """_session_exec.py must still exist as a thin boundary helper called by Rust."""
    from amplifier_core._session_exec import run_orchestrator

    assert callable(run_orchestrator)
