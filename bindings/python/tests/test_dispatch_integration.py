"""Test that _session_init.py can route through loader_dispatch."""

import asyncio


def test_dispatch_functions_importable():
    """The dispatch functions are importable from the right locations."""
    from amplifier_core.loader_dispatch import _detect_transport
    from amplifier_core.loader_dispatch import load_module

    assert callable(load_module)
    assert callable(_detect_transport)


def test_session_init_still_works():
    """_session_init.initialize_session is still importable and async."""
    from amplifier_core._session_init import initialize_session

    assert asyncio.iscoroutinefunction(initialize_session)
