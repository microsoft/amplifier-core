"""Type stubs for the Rust extension module (_engine).

These stubs describe the PyO3 bridge classes exposed by the compiled
Rust crate. Python consumers import them as::

    from amplifier_core._engine import RustSession, RustHookRegistry, ...
"""

from typing import Any, Optional

__version__: str
RUST_AVAILABLE: bool

class RustSession:
    """Rust-backed session lifecycle manager.

    Wraps ``amplifier_core::Session`` via PyO3.
    """

    def __init__(self, config: dict[str, Any]) -> None: ...
    @property
    def session_id(self) -> str: ...
    @property
    def parent_id(self) -> Optional[str]: ...
    @property
    def initialized(self) -> bool: ...
    async def initialize(self) -> None: ...
    async def execute(self, prompt: str) -> str: ...
    async def cleanup(self) -> None: ...

class RustHookRegistry:
    """Rust-backed hook dispatch pipeline.

    Wraps ``amplifier_core::HookRegistry`` via PyO3.
    """

    def __init__(self) -> None: ...
    def register(
        self,
        event: str,
        name: str,
        handler: Any,
        priority: int = 100,
    ) -> None: ...
    async def emit(self, event: str, data: dict[str, Any]) -> str: ...
    def unregister(self, name: str) -> None: ...

class RustCancellationToken:
    """Rust-backed cooperative cancellation token.

    Wraps ``amplifier_core::CancellationToken`` via PyO3.
    """

    def __init__(self) -> None: ...
    def request_cancellation(self) -> None: ...
    def is_cancelled(self) -> bool: ...
    @property
    def state(self) -> str: ...

class RustCoordinator:
    """Rust-backed module coordination hub.

    Wraps ``amplifier_core::Coordinator`` via PyO3.
    """

    def __init__(self) -> None: ...
    @property
    def hooks(self) -> RustHookRegistry: ...
    @property
    def cancellation(self) -> RustCancellationToken: ...
    @property
    def config(self) -> dict[str, Any]: ...
