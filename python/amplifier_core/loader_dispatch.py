"""Polyglot module loader dispatch.

Routes module loading to the appropriate loader based on amplifier.toml.
If no amplifier.toml exists, falls back to the existing Python loader
for 100% backward compatibility.

Integration point: _session_init.py calls load_module() instead of
directly calling loader.load().
"""

import logging
import os
from typing import Any

logger = logging.getLogger(__name__)


def _read_module_meta(source_path: str) -> dict[str, Any]:
    """Read amplifier.toml from a module's source directory.

    Returns:
        Parsed TOML as a dict, or empty dict if file doesn't exist.
    """
    toml_path = os.path.join(source_path, "amplifier.toml")
    if not os.path.exists(toml_path):
        return {}

    try:
        import tomli
    except ImportError:
        try:
            import tomllib as tomli  # Python 3.11+
        except ImportError:
            logger.warning(
                "Neither tomli nor tomllib available, cannot read amplifier.toml"
            )
            return {}

    with open(toml_path, "rb") as f:
        return tomli.load(f)


def _detect_transport(source_path: str) -> str:
    """Detect the transport type from amplifier.toml.

    Returns:
        Transport string: "python" (default), "grpc", "native", or "wasm".
    """
    meta = _read_module_meta(source_path)
    if not meta:
        return "python"
    return meta.get("module", {}).get("transport", "python")


async def load_module(
    module_id: str,
    config: dict[str, Any] | None,
    source_path: str,
    coordinator: Any,
) -> Any:
    """Load a module from a resolved source path.

    Uses the Rust module resolver to auto-detect transport type.
    Falls back to Python loader for backward compatibility.

    Args:
        module_id: Module identifier (e.g., "tool-database")
        config: Optional module configuration dict
        source_path: Resolved filesystem path to the module
        coordinator: The coordinator instance (RustCoordinator or ModuleCoordinator)

    Returns:
        Mount function for the module

    Raises:
        NotImplementedError: For transport types not yet supported
        ValueError: If module cannot be loaded
    """
    try:
        from amplifier_core._engine import resolve_module as rust_resolve

        manifest = rust_resolve(source_path)
        transport = manifest.get("transport", "python")
    except ImportError:
        logger.debug("Rust engine not available, using Python-only transport detection")
        transport = _detect_transport(source_path)
    except Exception as e:
        logger.debug(
            f"Rust resolver failed for '{module_id}': {e}, falling back to Python detection"
        )
        transport = _detect_transport(source_path)

    if transport == "grpc":
        from .loader_grpc import load_grpc_module

        meta = _read_module_meta(source_path)
        return await load_grpc_module(module_id, config, meta, coordinator)

    if transport == "wasm":
        try:
            from amplifier_core._engine import load_wasm_from_path

            result = load_wasm_from_path(source_path)
            logger.info(
                f"[module:mount] {module_id} loaded via WASM resolver: {result}"
            )

            async def _noop_mount(coord: Any) -> None:
                pass

            return _noop_mount
        except ImportError:
            raise NotImplementedError(
                f"WASM module loading for '{module_id}' requires the Rust engine. "
                "Install amplifier-core with Rust extensions enabled."
            )
        except Exception as e:
            raise ValueError(
                f"Failed to load WASM module '{module_id}' from {source_path}: {e}"
            ) from e

    if transport == "native":
        raise NotImplementedError(
            f"Native Rust module loading not yet implemented for '{module_id}'. "
            "Use transport = 'grpc' to load Rust modules as gRPC services."
        )

    # Default: existing Python loader (backward compatible)
    from .loader import ModuleLoader

    loader = coordinator.loader or ModuleLoader(coordinator=coordinator)
    return await loader.load(module_id, config, source_hint=source_path)
