"""Stub validation tests — verify .pyi stubs match the compiled _engine module.

These tests ensure the type stubs declared in _engine.pyi accurately reflect
the actual exports and signatures of the compiled Rust extension module.
"""


def test_engine_exports_match_stubs():
    """Verify the Rust module exports match what the stubs declare."""
    import amplifier_core._engine as engine

    # Module-level attributes
    assert hasattr(engine, "__version__")
    assert hasattr(engine, "RUST_AVAILABLE")

    # All four PyO3 classes
    assert hasattr(engine, "RustSession")
    assert hasattr(engine, "RustHookRegistry")
    assert hasattr(engine, "RustCancellationToken")
    assert hasattr(engine, "RustCoordinator")


def test_rust_session_has_stub_members():
    """Verify RustSession exposes every member declared in the stub."""
    from amplifier_core._engine import RustSession

    # __init__ takes a config dict
    assert callable(RustSession)

    # Minimal valid config for Rust SessionConfig::from_value
    config = {"session": {"orchestrator": "loop-basic", "context": "context-simple"}}
    session = RustSession(config)
    assert hasattr(session, "session_id")
    assert hasattr(session, "parent_id")
    assert hasattr(session, "initialized")

    # Methods declared in stubs
    assert hasattr(session, "initialize")
    assert hasattr(session, "execute")
    assert hasattr(session, "cleanup")
    assert callable(session.initialize)
    assert callable(session.execute)
    assert callable(session.cleanup)


def test_rust_hook_registry_has_stub_members():
    """Verify RustHookRegistry exposes every member declared in the stub."""
    from amplifier_core._engine import RustHookRegistry

    registry = RustHookRegistry()

    assert hasattr(registry, "register")
    assert hasattr(registry, "emit")
    assert hasattr(registry, "unregister")
    assert callable(registry.register)
    assert callable(registry.emit)
    assert callable(registry.unregister)


def test_rust_cancellation_token_has_stub_members():
    """Verify RustCancellationToken exposes every member declared in the stub."""
    from amplifier_core._engine import RustCancellationToken

    token = RustCancellationToken()

    assert hasattr(token, "request_cancellation")
    assert hasattr(token, "is_cancelled")
    assert hasattr(token, "state")
    assert callable(token.request_cancellation)
    assert callable(token.is_cancelled)


def test_rust_coordinator_has_stub_members():
    """Verify RustCoordinator exposes every member declared in the stub."""
    from amplifier_core._engine import RustCoordinator

    class _FakeSession:
        session_id = "test-123"
        parent_id = None
        config = {"session": {"orchestrator": "loop-basic"}}

    coordinator = RustCoordinator(_FakeSession())

    # Properties declared in stubs
    assert hasattr(coordinator, "hooks")
    assert hasattr(coordinator, "cancellation")
    assert hasattr(coordinator, "config")


def test_version_and_flag_values():
    """Verify module-level constants have the expected types and values."""
    import amplifier_core._engine as engine

    assert isinstance(engine.__version__, str)
    assert isinstance(engine.RUST_AVAILABLE, bool)
    assert engine.RUST_AVAILABLE is True
