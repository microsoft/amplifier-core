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
        name: str,
        handler: Any,
        priority: int = 100,
    ) -> None: ...
    def on(
        self,
        event: str,
        name: str,
        handler: Any,
        priority: int = 100,
    ) -> None:
        """Alias for register()."""
        ...
    async def emit(self, event: str, data: dict[str, Any]) -> Any: ...
    async def emit_and_collect(
        self, event: str, data: dict[str, Any], timeout: Optional[float] = None
    ) -> list[Any]: ...
    def unregister(self, name: str) -> None: ...
    def set_default_fields(self, **kwargs: Any) -> None: ...
    def list_handlers(self, event: Optional[str] = None) -> list[dict[str, Any]]: ...

# ---------------------------------------------------------------------------
# RustCancellationToken — wraps amplifier_core::CancellationToken
# ---------------------------------------------------------------------------

class RustCancellationToken:
    """Rust-backed cooperative cancellation token.

    Wraps ``amplifier_core::CancellationToken`` via PyO3.
    Drop-in replacement for ``amplifier_core.cancellation.CancellationToken``.
    """

    def __init__(self) -> None: ...
    def request_graceful(self) -> bool: ...
    def request_immediate(self) -> bool: ...
    def is_cancelled(self) -> bool: ...
    def is_graceful(self) -> bool: ...
    def is_immediate(self) -> bool: ...
    @property
    def state(self) -> str: ...
    @property
    def running_tools(self) -> set[str]: ...
    @property
    def running_tool_names(self) -> list[str]: ...
    def track_tool(self, tool_id: str, name: str) -> None: ...
    def complete_tool(self, tool_id: str) -> None: ...
    def register_callback(self, callback: Callable[[], Awaitable[None]]) -> None: ...
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

    # --- Cancellation / turn ---
    async def request_cancel(self, immediate: bool = False) -> None: ...
    def reset_turn(self) -> None: ...
