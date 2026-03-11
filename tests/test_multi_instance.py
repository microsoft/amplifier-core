"""
Tests for multi-instance provider support (instance_id extraction + mount-name remapping).

Covers both session init paths:
  - amplifier_core.session.AmplifierSession (Python session)
  - amplifier_core._session_init.initialize_session (Rust bridge path)
"""

import pytest
from unittest.mock import AsyncMock

from amplifier_core.session import AmplifierSession as PyAmplifierSession
from amplifier_core._session_init import initialize_session
from amplifier_core.testing import MockCoordinator


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_provider_mount_fn(default_name: str, provider_instance=None):
    """Return an async mount function that registers the provider under default_name."""
    if provider_instance is None:
        provider_instance = object()  # unique sentinel

    async def mount_fn(coordinator):
        await coordinator.mount("providers", provider_instance, name=default_name)
        return None  # no cleanup

    return mount_fn


def _make_loader(module_to_mount_fn: dict):
    """Return a mock loader whose load() returns the configured mount function."""
    loader = AsyncMock()

    async def _load(module_id, config=None, source_hint=None, coordinator=None):
        return module_to_mount_fn[module_id]

    loader.load.side_effect = _load
    return loader


# ---------------------------------------------------------------------------
# Tests against PyAmplifierSession (session.py path)
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_single_instance_no_remapping():
    """Provider without instance_id stays mounted under its default name (backwards compat)."""
    config = {
        "session": {"orchestrator": "loop-basic", "context": "context-simple"},
        "providers": [
            {"module": "provider-openai"},  # no instance_id
        ],
    }

    # Mount fn simulates provider self-mounting as "openai"
    mount_fn = _make_provider_mount_fn("openai")

    # Mock orchestrator + context mount fns so session init completes
    orch_mount_fn = AsyncMock(return_value=None)
    ctx_mount_fn = AsyncMock(return_value=None)

    loader = _make_loader(
        {
            "loop-basic": orch_mount_fn,
            "context-simple": ctx_mount_fn,
            "provider-openai": mount_fn,
        }
    )

    coordinator = MockCoordinator()
    coordinator.loader = loader

    await initialize_session(config, coordinator, session_id="test", parent_id=None)

    providers = coordinator.get("providers") or {}

    # Default name should be present
    assert "openai" in providers, (
        f"Expected 'openai' in providers, got: {list(providers)}"
    )

    # No unmount should have happened
    assert coordinator.unmount_history == [], (
        f"Expected no unmounts, got: {coordinator.unmount_history}"
    )


@pytest.mark.asyncio
async def test_instance_id_remapping_removes_default_key():
    """Provider with instance_id is remapped: custom key present, default key absent."""
    config = {
        "session": {"orchestrator": "loop-basic", "context": "context-simple"},
        "providers": [
            {"module": "provider-openai", "instance_id": "my-custom"},
        ],
    }

    mount_fn = _make_provider_mount_fn("openai")

    orch_mount_fn = AsyncMock(return_value=None)
    ctx_mount_fn = AsyncMock(return_value=None)

    loader = _make_loader(
        {
            "loop-basic": orch_mount_fn,
            "context-simple": ctx_mount_fn,
            "provider-openai": mount_fn,
        }
    )

    coordinator = MockCoordinator()
    coordinator.loader = loader

    await initialize_session(config, coordinator, session_id="test", parent_id=None)

    providers = coordinator.get("providers") or {}

    # Custom key must exist
    assert "my-custom" in providers, (
        f"Expected 'my-custom' in providers, got: {list(providers)}"
    )
    # Default key must be gone
    assert "openai" not in providers, (
        f"Expected 'openai' to be removed from providers, got: {list(providers)}"
    )

    # Exactly one unmount should have happened (the default name)
    assert len(coordinator.unmount_history) == 1, (
        f"Expected 1 unmount, got: {coordinator.unmount_history}"
    )
    assert coordinator.unmount_history[0] == {
        "mount_point": "providers",
        "name": "openai",
    }, f"Wrong unmount entry: {coordinator.unmount_history[0]}"


@pytest.mark.asyncio
async def test_multi_instance_providers_both_mounted():
    """Two providers sharing a module but different instance_ids are both accessible."""
    provider_a = object()
    provider_b = object()

    async def mount_fn_a(coordinator):
        await coordinator.mount("providers", provider_a, name="openai")
        return None

    async def mount_fn_b(coordinator):
        await coordinator.mount("providers", provider_b, name="openai")
        return None

    call_count = {"n": 0}

    async def load_side_effect(module_id, config=None, source_hint=None, coordinator=None):
        if module_id == "loop-basic":
            return AsyncMock(return_value=None)
        if module_id == "context-simple":
            return AsyncMock(return_value=None)
        if module_id == "provider-openai":
            call_count["n"] += 1
            return mount_fn_a if call_count["n"] == 1 else mount_fn_b
        raise ValueError(f"Unexpected module: {module_id}")

    loader = AsyncMock()
    loader.load.side_effect = load_side_effect

    config = {
        "session": {"orchestrator": "loop-basic", "context": "context-simple"},
        "providers": [
            {"module": "provider-openai", "instance_id": "openai-gpt4"},
            {"module": "provider-openai", "instance_id": "openai-gpt35"},
        ],
    }

    coordinator = MockCoordinator()
    coordinator.loader = loader

    await initialize_session(config, coordinator, session_id="test", parent_id=None)

    providers = coordinator.get("providers") or {}

    assert "openai-gpt4" in providers, (
        f"Expected 'openai-gpt4' in providers, got: {list(providers)}"
    )
    assert "openai-gpt35" in providers, (
        f"Expected 'openai-gpt35' in providers, got: {list(providers)}"
    )
    assert "openai" not in providers, (
        f"Expected default 'openai' key to be removed, got: {list(providers)}"
    )


# ---------------------------------------------------------------------------
# Tests against PyAmplifierSession.initialize() (session.py path)
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_session_py_instance_id_remapping():
    """session.py path: instance_id triggers remapping in PyAmplifierSession.initialize()."""
    config = {
        "session": {"orchestrator": "loop-basic", "context": "context-simple"},
        "providers": [
            {"module": "provider-openai", "instance_id": "primary"},
        ],
    }

    mount_fn = _make_provider_mount_fn("openai")
    orch_mount_fn = AsyncMock(return_value=None)
    ctx_mount_fn = AsyncMock(return_value=None)

    loader = _make_loader(
        {
            "loop-basic": orch_mount_fn,
            "context-simple": ctx_mount_fn,
            "provider-openai": mount_fn,
        }
    )

    session = PyAmplifierSession(config, loader=loader)

    # Replace the session's coordinator with our tracking one so we can inspect
    # mount/unmount history after initialization.
    tracking_coordinator = MockCoordinator()
    tracking_coordinator.loader = loader
    session.coordinator = tracking_coordinator

    await session.initialize()

    providers = tracking_coordinator.get("providers") or {}

    assert "primary" in providers, (
        f"Expected 'primary' in providers, got: {list(providers)}"
    )
    assert "openai" not in providers, (
        f"Expected 'openai' to be removed, got: {list(providers)}"
    )


# ---------------------------------------------------------------------------
# Task 3: Tests for multi-instance validation (duplicate module + instance_id)
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_duplicate_module_without_instance_id_raises():
    """Two providers with the same module, neither has instance_id → ValueError."""
    config = {
        "session": {"orchestrator": "loop-basic", "context": "context-simple"},
        "providers": [
            {"module": "provider-mock"},
            {"module": "provider-mock"},  # duplicate, no instance_id
        ],
    }

    orch_mount_fn = AsyncMock(return_value=None)
    ctx_mount_fn = AsyncMock(return_value=None)

    loader = _make_loader(
        {
            "loop-basic": orch_mount_fn,
            "context-simple": ctx_mount_fn,
        }
    )

    coordinator = MockCoordinator()
    coordinator.loader = loader

    with pytest.raises(ValueError, match="instance_id"):
        await initialize_session(config, coordinator, session_id="test", parent_id=None)


@pytest.mark.asyncio
async def test_duplicate_module_with_instance_id_passes():
    """Two providers with same module but different instance_ids → no error."""
    provider_a = object()
    provider_b = object()
    call_count = {"n": 0}

    async def mount_fn_a(coord):
        await coord.mount("providers", provider_a, name="mock")
        return None

    async def mount_fn_b(coord):
        await coord.mount("providers", provider_b, name="mock")
        return None

    async def load_side_effect(module_id, config=None, source_hint=None, coordinator=None):
        if module_id == "loop-basic":
            return AsyncMock(return_value=None)
        if module_id == "context-simple":
            return AsyncMock(return_value=None)
        if module_id == "provider-mock":
            call_count["n"] += 1
            return mount_fn_a if call_count["n"] == 1 else mount_fn_b
        raise ValueError(f"Unexpected module: {module_id}")

    loader = AsyncMock()
    loader.load.side_effect = load_side_effect

    config = {
        "session": {"orchestrator": "loop-basic", "context": "context-simple"},
        "providers": [
            {"module": "provider-mock", "instance_id": "mock-a"},
            {"module": "provider-mock", "instance_id": "mock-b"},
        ],
    }

    coordinator = MockCoordinator()
    coordinator.loader = loader

    # Should not raise
    await initialize_session(config, coordinator, session_id="test", parent_id=None)


@pytest.mark.asyncio
async def test_single_module_no_instance_id_ok():
    """Single provider entry without instance_id → no error (backward compat)."""
    config = {
        "session": {"orchestrator": "loop-basic", "context": "context-simple"},
        "providers": [
            {"module": "provider-mock"},  # only one — no instance_id is fine
        ],
    }

    mount_fn = _make_provider_mount_fn("mock")
    orch_mount_fn = AsyncMock(return_value=None)
    ctx_mount_fn = AsyncMock(return_value=None)

    loader = _make_loader(
        {
            "loop-basic": orch_mount_fn,
            "context-simple": ctx_mount_fn,
            "provider-mock": mount_fn,
        }
    )

    coordinator = MockCoordinator()
    coordinator.loader = loader

    # Should not raise
    await initialize_session(config, coordinator, session_id="test", parent_id=None)


@pytest.mark.asyncio
async def test_duplicate_module_one_missing_instance_id_allowed():
    """One entry with instance_id + one without → allowed (the no-id entry is the default).

    Previously this raised ValueError with the strict "every entry needs instance_id" rule.
    Now it's allowed: the entry without instance_id is treated as the "default" instance
    that keeps the provider's default mount name. Only entries with explicit instance_id
    are remapped.
    """
    default_instance = object()
    named_instance = object()
    call_count = {"n": 0}

    async def mount_fn_default(coord):
        await coord.mount("providers", default_instance, name="mock")
        return None

    async def mount_fn_named(coord):
        await coord.mount("providers", named_instance, name="mock")
        return None

    async def load_side_effect(module_id, config=None, source_hint=None, coordinator=None):
        if module_id == "loop-basic":
            return AsyncMock(return_value=None)
        if module_id == "context-simple":
            return AsyncMock(return_value=None)
        if module_id == "provider-mock":
            call_count["n"] += 1
            return mount_fn_default if call_count["n"] == 1 else mount_fn_named
        raise ValueError(f"Unexpected module: {module_id}")

    loader = AsyncMock()
    loader.load.side_effect = load_side_effect

    config = {
        "session": {"orchestrator": "loop-basic", "context": "context-simple"},
        "providers": [
            {"module": "provider-mock"},  # no instance_id — default
            {"module": "provider-mock", "instance_id": "mock-a"},  # explicit instance
        ],
    }

    coordinator = MockCoordinator()
    coordinator.loader = loader

    # Should NOT raise — one default entry is allowed
    await initialize_session(config, coordinator, session_id="test", parent_id=None)

    providers = coordinator.get("providers") or {}
    assert "mock" in providers, f"Expected default 'mock' key, got: {list(providers)}"
    assert "mock-a" in providers, f"Expected 'mock-a' key, got: {list(providers)}"
    assert providers["mock"] is default_instance
    assert providers["mock-a"] is named_instance


@pytest.mark.asyncio
async def test_mixed_instance_id_preserves_default_entry():
    """One entry without instance_id + one with instance_id → BOTH mounted correctly.

    Reproduces the real-world bug:
      settings.yaml has:
        - module: provider-anthropic          (no id — original entry)
        - module: provider-anthropic
          id: anthropic-sonnet               (newly added instance)

    After _map_id_to_instance_id the second entry has instance_id="anthropic-sonnet".
    The first entry has no instance_id (it's the default instance).

    Expected: both "anthropic" and "anthropic-sonnet" are accessible after init.
    """
    first_instance = object()  # sentinel for original anthropic entry
    second_instance = object()  # sentinel for anthropic-sonnet entry

    async def mount_fn_first(coordinator):
        await coordinator.mount("providers", first_instance, name="anthropic")
        return None

    async def mount_fn_second(coordinator):
        await coordinator.mount("providers", second_instance, name="anthropic")
        return None

    call_count = {"n": 0}

    async def load_side_effect(module_id, config=None, source_hint=None, coordinator=None):
        if module_id == "loop-basic":
            return AsyncMock(return_value=None)
        if module_id == "context-simple":
            return AsyncMock(return_value=None)
        if module_id == "provider-anthropic":
            call_count["n"] += 1
            return mount_fn_first if call_count["n"] == 1 else mount_fn_second
        raise ValueError(f"Unexpected module: {module_id}")

    loader = AsyncMock()
    loader.load.side_effect = load_side_effect

    config = {
        "session": {"orchestrator": "loop-basic", "context": "context-simple"},
        "providers": [
            {"module": "provider-anthropic"},  # no instance_id
            {
                "module": "provider-anthropic",
                "instance_id": "anthropic-sonnet",
            },  # explicit
        ],
    }

    coordinator = MockCoordinator()
    coordinator.loader = loader

    await initialize_session(config, coordinator, session_id="test", parent_id=None)

    providers = coordinator.get("providers") or {}

    assert "anthropic" in providers, (
        f"Expected default 'anthropic' entry to be preserved, got: {list(providers)}"
    )
    assert "anthropic-sonnet" in providers, (
        f"Expected 'anthropic-sonnet' to be mounted, got: {list(providers)}"
    )
    assert providers["anthropic"] is first_instance, (
        "Expected 'anthropic' to be the first (original) instance"
    )
    assert providers["anthropic-sonnet"] is second_instance, (
        "Expected 'anthropic-sonnet' to be the second instance"
    )


@pytest.mark.asyncio
async def test_session_py_no_instance_id_no_remap():
    """session.py path: no instance_id → provider stays under default name."""
    config = {
        "session": {"orchestrator": "loop-basic", "context": "context-simple"},
        "providers": [
            {"module": "provider-openai"},
        ],
    }

    mount_fn = _make_provider_mount_fn("openai")
    orch_mount_fn = AsyncMock(return_value=None)
    ctx_mount_fn = AsyncMock(return_value=None)

    loader = _make_loader(
        {
            "loop-basic": orch_mount_fn,
            "context-simple": ctx_mount_fn,
            "provider-openai": mount_fn,
        }
    )

    session = PyAmplifierSession(config, loader=loader)

    tracking_coordinator = MockCoordinator()
    tracking_coordinator.loader = loader
    session.coordinator = tracking_coordinator

    await session.initialize()

    providers = tracking_coordinator.get("providers") or {}

    assert "openai" in providers, (
        f"Expected 'openai' in providers, got: {list(providers)}"
    )
    assert tracking_coordinator.unmount_history == [], (
        f"Expected no unmounts, got: {tracking_coordinator.unmount_history}"
    )
