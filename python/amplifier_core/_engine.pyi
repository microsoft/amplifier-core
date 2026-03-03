"""Type stubs for the Rust extension module (_engine).

These stubs describe the PyO3 bridge classes exposed by the compiled
Rust crate. Python consumers import them as::

    from amplifier_core._engine import RustSession, RustHookRegistry, ...

After the Milestone 4 switchover, top-level imports alias these types::

    from amplifier_core import AmplifierSession  # -> RustSession
    from amplifier_core import HookRegistry       # -> RustHookRegistry
    from amplifier_core import CancellationToken   # -> RustCancellationToken
"""

from collections.abc import Awaitable, Callable
from typing import Any, Optional

__version__: str
RUST_AVAILABLE: bool

# ---------------------------------------------------------------------------
# RustSession — wraps amplifier_core::Session
# ---------------------------------------------------------------------------

class RustSession:
    """Rust-backed session lifecycle manager.

    Wraps ``amplifier_core::Session`` via PyO3.
    Drop-in replacement for ``amplifier_core.session.AmplifierSession``.
    """

    def __init__(
        self,
        config: dict[str, Any],
        loader: Any = None,
        session_id: Optional[str] = None,
        parent_id: Optional[str] = None,
        approval_system: Any = None,
        display_system: Any = None,
        is_resumed: bool = False,
    ) -> None: ...
    @property
    def session_id(self) -> str: ...
    @property
    def parent_id(self) -> Optional[str]: ...
    @property
    def coordinator(self) -> "RustCoordinator": ...
    @property
    def config(self) -> dict[str, Any]: ...
    @property
    def is_resumed(self) -> bool: ...
    @property
    def initialized(self) -> bool: ...
    async def initialize(self) -> None: ...
    async def execute(self, prompt: str) -> str: ...
    async def cleanup(self) -> None: ...
    async def __aenter__(self) -> "RustSession": ...
    async def __aexit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> None: ...

# ---------------------------------------------------------------------------
# RustHookRegistry — wraps amplifier_core::HookRegistry
# ---------------------------------------------------------------------------

class RustHookRegistry:
    """Rust-backed hook dispatch pipeline.

    Wraps ``amplifier_core::HookRegistry`` via PyO3.
    Drop-in replacement for ``amplifier_core.hooks.HookRegistry``.
    """

    # Event constants
    SESSION_START: str
    SESSION_END: str
    SESSION_ERROR: str
    SESSION_RESUME: str
    SESSION_FORK: str
    TURN_START: str
    TURN_END: str
    TURN_ERROR: str
    PROVIDER_REQUEST: str
    PROVIDER_RESPONSE: str
    PROVIDER_ERROR: str
    TOOL_CALL: str
    TOOL_RESULT: str
    TOOL_ERROR: str
    CANCEL_REQUESTED: str
    CANCEL_COMPLETED: str

    def __init__(self) -> None: ...
    def register(
        self,
        event: str,
        handler: Any,
        priority: int = 0,
        name: Optional[str] = None,
    ) -> Any: ...  # Returns a callable unregister function (RustUnregisterFn)
    def on(
        self,
        event: str,
        handler: Any,
        priority: int = 0,
        name: Optional[str] = None,
    ) -> Any:
        """Alias for register()."""
        ...
    async def emit(self, event: str, data: dict[str, Any]) -> Any: ...
    async def emit_and_collect(
        self, event: str, data: dict[str, Any], timeout: Optional[float] = None
    ) -> list[Any]: ...
    def unregister(self, name: str) -> None: ...
    def set_default_fields(self, **kwargs: Any) -> None: ...
    def list_handlers(self, event: Optional[str] = None) -> dict[str, list[str]]: ...

# ---------------------------------------------------------------------------
# RustCancellationToken — wraps amplifier_core::CancellationToken
# ---------------------------------------------------------------------------

class RustCancellationToken:
    """Rust-backed cooperative cancellation token.

    Wraps ``amplifier_core::CancellationToken`` via PyO3.
    Drop-in replacement for ``amplifier_core.cancellation.CancellationToken``.
    """

    def __init__(self) -> None: ...

    # --- Properties ---
    @property
    def is_cancelled(self) -> bool: ...
    @property
    def is_graceful(self) -> bool: ...
    @property
    def is_immediate(self) -> bool: ...
    @property
    def state(self) -> str: ...
    @property
    def running_tools(self) -> set[str]: ...
    @property
    def running_tool_names(self) -> list[str]: ...

    # --- Cancellation requests ---
    def request_cancellation(self) -> None: ...
    def request_graceful(self) -> bool: ...
    def request_immediate(self) -> bool: ...
    def reset(self) -> None: ...

    # --- Tool tracking ---
    def register_tool_start(self, tool_call_id: str, tool_name: str) -> None: ...
    def register_tool_complete(self, tool_call_id: str) -> None: ...

    # --- Child token propagation ---
    def register_child(self, child: "RustCancellationToken") -> None: ...
    def unregister_child(self, child: "RustCancellationToken") -> None: ...

    # --- Callbacks ---
    def on_cancel(self, callback: Callable[[], Awaitable[None]]) -> None: ...
    async def trigger_callbacks(self) -> None: ...

# ---------------------------------------------------------------------------
# RustCoordinator — wraps amplifier_core::Coordinator
# ---------------------------------------------------------------------------

class RustCoordinator:
    """Rust-backed module coordination hub.

    Wraps ``amplifier_core::Coordinator`` via PyO3.
    Subclassable — use ``#[pyclass(subclass)]``.
    The top-level ``ModuleCoordinator`` is a Python subclass that adds
    ``process_hook_result``.
    """

    def __init__(
        self,
        session: Any = None,
        approval_system: Any = None,
        display_system: Any = None,
    ) -> None: ...

    # --- Properties ---
    @property
    def mount_points(self) -> dict[str, Any]: ...
    @property
    def session_id(self) -> str: ...
    @property
    def parent_id(self) -> Optional[str]: ...
    @property
    def session(self) -> Any: ...
    @property
    def hooks(self) -> RustHookRegistry: ...
    @property
    def cancellation(self) -> RustCancellationToken: ...
    @property
    def config(self) -> dict[str, Any]: ...
    @property
    def channels(self) -> dict[str, list[dict[str, Any]]]: ...
    @property
    def injection_budget_per_turn(self) -> Optional[int]: ...
    @property
    def injection_size_limit(self) -> Optional[int]: ...
    @property
    def loader(self) -> Any: ...
    @loader.setter
    def loader(self, value: Any) -> None: ...
    @property
    def approval_system(self) -> Any: ...
    @approval_system.setter
    def approval_system(self, value: Any) -> None: ...
    @property
    def display_system(self) -> Any: ...
    @display_system.setter
    def display_system(self, value: Any) -> None: ...
    @property
    def _current_turn_injections(self) -> int: ...
    @_current_turn_injections.setter
    def _current_turn_injections(self, value: int) -> None: ...

    # --- Mount/unmount/get ---
    async def mount(
        self, mount_point: str, module: Any, name: Optional[str] = None
    ) -> None: ...
    async def unmount(self, mount_point: str, name: Optional[str] = None) -> None: ...
    def get(self, mount_point: str, name: Optional[str] = None) -> Any: ...

    # --- Capabilities ---
    def register_capability(self, name: str, value: Any) -> None: ...
    def get_capability(self, name: str) -> Any: ...

    # --- Cleanup ---
    def register_cleanup(self, cleanup_fn: Callable[[], Any]) -> None: ...
    async def cleanup(self) -> None: ...

    # --- Contributions ---
    def register_contributor(
        self, channel: str, name: str, callback: Callable[[], Any]
    ) -> None: ...
    async def collect_contributions(self, channel: str) -> list[Any]: ...

    # --- Introspection ---
    def to_dict(self) -> dict[str, Any]: ...

    # --- Cancellation / turn ---
    async def request_cancel(self, immediate: bool = False) -> None: ...
    def reset_turn(self) -> None: ...

# ---------------------------------------------------------------------------
# ProviderError — structured error from a provider (PyO3 bridge)
# ---------------------------------------------------------------------------

class ProviderError:
    """Structured provider error exposed via PyO3.

    Can be constructed directly from Python (for testing) or created
    from a Rust ``ProviderError`` when errors cross the PyO3 boundary.
    """

    def __init__(
        self,
        message: str,
        *,
        provider: Optional[str] = None,
        model: Optional[str] = None,
        retry_after: Optional[float] = None,
        delay_multiplier: Optional[float] = None,
        retryable: bool = False,
        error_type: str = "Other",
    ) -> None: ...
    @property
    def message(self) -> str: ...
    @property
    def provider(self) -> Optional[str]: ...
    @property
    def model(self) -> Optional[str]: ...
    @property
    def retry_after(self) -> Optional[float]: ...
    @property
    def delay_multiplier(self) -> Optional[float]: ...
    @property
    def retryable(self) -> bool: ...
    @property
    def error_type(self) -> str: ...
    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...

# ---------------------------------------------------------------------------
# RetryConfig — retry configuration (PyO3 bridge)
# ---------------------------------------------------------------------------

class RetryConfig:
    """Retry configuration wrapping the Rust ``RetryConfig`` struct.

    All constructor arguments have defaults matching the Rust ``Default`` impl.
    ``jitter`` accepts a bool, a float (deprecated), or None (defaults to True).
    ``min_delay`` and ``backoff_multiplier`` are deprecated aliases.
    """

    def __init__(
        self,
        max_retries: int = 3,
        initial_delay: float = 1.0,
        max_delay: float = 60.0,
        backoff_factor: float = 2.0,
        jitter: "bool | float | None" = None,
        honor_retry_after: bool = True,
        min_delay: Optional[float] = None,
        backoff_multiplier: Optional[float] = None,
    ) -> None: ...
    @property
    def max_retries(self) -> int: ...
    @property
    def initial_delay(self) -> float: ...
    @property
    def max_delay(self) -> float: ...
    @property
    def backoff_factor(self) -> float: ...
    @property
    def jitter(self) -> float:
        """Returns 0.2 if jitter is enabled, 0.0 if disabled."""
        ...
    @property
    def honor_retry_after(self) -> bool: ...
    @property
    def min_delay(self) -> float:
        """Deprecated: use ``initial_delay`` instead."""
        ...
    @property
    def backoff_multiplier(self) -> float:
        """Deprecated: use ``backoff_factor`` instead."""
        ...

# ---------------------------------------------------------------------------
# Retry utility functions (PyO3 bridge)
# ---------------------------------------------------------------------------

def classify_error_message(message: str) -> str:
    """Classify an error message string into an error category.

    Returns one of: ``"rate_limit"``, ``"timeout"``, ``"authentication"``,
    ``"context_length"``, ``"content_filter"``, ``"not_found"``,
    ``"provider_unavailable"``, or ``"unknown"``.
    """
    ...

def compute_delay(
    config: RetryConfig,
    attempt: int,
    retry_after: Optional[float] = None,
    delay_multiplier: Optional[float] = None,
) -> float:
    """Compute the delay for a given retry attempt.

    Pure function (deterministic when ``config.jitter`` is False).
    The caller is responsible for sleeping.

    Non-finite or non-positive ``delay_multiplier`` values are ignored.
    """
    ...
