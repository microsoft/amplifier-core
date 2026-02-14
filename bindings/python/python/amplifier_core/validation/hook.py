"""
Hook module validator.

Validates that a module correctly implements the HookHandler protocol.
Uses dynamic import to check protocol compliance via isinstance().
"""

import asyncio
import importlib
import importlib.util
import inspect
from pathlib import Path
from typing import Any

from ..interfaces import HookHandler
from .base import ValidationCheck
from .base import ValidationResult


class HookValidator:
    """Validates HookHandler module compliance."""

    async def validate(
        self,
        module_path: str | Path,
        entry_point: str | None = None,
        config: dict[str, Any] | None = None,
    ) -> ValidationResult:
        """
        Validate a hook module.

        Args:
            module_path: Path to module directory or Python module name
            entry_point: Optional entry point name (e.g., 'hooks-logging')
            config: Optional module configuration to use during validation

        Returns:
            ValidationResult with all checks
        """
        result = ValidationResult(module_type="hook", module_path=str(module_path))

        # Check 1: Module is importable
        module = self._check_importable(result, module_path)
        if module is None:
            return result

        # Check 2: mount() function exists
        mount_fn = self._check_mount_exists(result, module)
        if mount_fn is None:
            return result

        # Check 3: mount() signature is correct
        self._check_mount_signature(result, mount_fn)

        # Check 4: Protocol compliance (requires calling mount)
        await self._check_protocol_compliance(result, mount_fn, config=config)

        return result

    def _check_importable(
        self, result: ValidationResult, module_path: str | Path
    ) -> Any:
        """Check if module can be imported."""
        try:
            path = Path(module_path)
            if path.exists():
                # File path - find the Python module
                if path.is_dir():
                    init_file = path / "__init__.py"
                    if init_file.exists():
                        spec = importlib.util.spec_from_file_location(
                            path.name, init_file
                        )
                    else:
                        result.add(
                            ValidationCheck(
                                name="module_importable",
                                passed=False,
                                message=f"No __init__.py found in {path}",
                                severity="error",
                            )
                        )
                        return None
                else:
                    spec = importlib.util.spec_from_file_location(path.stem, path)

                if spec and spec.loader:
                    module = importlib.util.module_from_spec(spec)
                    spec.loader.exec_module(module)
                    result.add(
                        ValidationCheck(
                            name="module_importable",
                            passed=True,
                            message=f"Module loaded from {path}",
                            severity="info",
                        )
                    )
                    return module
            else:
                # Module name - import directly
                module = importlib.import_module(str(module_path))
                result.add(
                    ValidationCheck(
                        name="module_importable",
                        passed=True,
                        message=f"Module '{module_path}' imported successfully",
                        severity="info",
                    )
                )
                return module

        except ImportError as e:
            result.add(
                ValidationCheck(
                    name="module_importable",
                    passed=False,
                    message=f"Failed to import module: {e}",
                    severity="error",
                )
            )
            return None
        except Exception as e:
            result.add(
                ValidationCheck(
                    name="module_importable",
                    passed=False,
                    message=f"Error loading module: {e}",
                    severity="error",
                )
            )
            return None

    def _check_mount_exists(self, result: ValidationResult, module: Any) -> Any:
        """Check if mount() function exists."""
        mount_fn = getattr(module, "mount", None)
        if mount_fn is None:
            result.add(
                ValidationCheck(
                    name="mount_exists",
                    passed=False,
                    message="No mount() function found in module",
                    severity="error",
                )
            )
            return None

        if not callable(mount_fn):
            result.add(
                ValidationCheck(
                    name="mount_exists",
                    passed=False,
                    message="mount is not callable",
                    severity="error",
                )
            )
            return None

        result.add(
            ValidationCheck(
                name="mount_exists",
                passed=True,
                message="mount() function found",
                severity="info",
            )
        )
        return mount_fn

    def _check_mount_signature(self, result: ValidationResult, mount_fn: Any) -> None:
        """Check if mount() has correct signature."""
        sig = inspect.signature(mount_fn)
        params = list(sig.parameters.keys())

        # Should have at least coordinator and config
        if len(params) < 2:
            result.add(
                ValidationCheck(
                    name="mount_signature",
                    passed=False,
                    message=f"mount() should have at least 2 parameters (coordinator, config), found {len(params)}",
                    severity="error",
                )
            )
            return

        # Check if async
        if asyncio.iscoroutinefunction(mount_fn):
            result.add(
                ValidationCheck(
                    name="mount_signature",
                    passed=True,
                    message="mount() is async with correct signature",
                    severity="info",
                )
            )
        else:
            result.add(
                ValidationCheck(
                    name="mount_signature",
                    passed=False,
                    message="mount() should be async (async def mount(...))",
                    severity="error",
                )
            )

    async def _check_protocol_compliance(
        self,
        result: ValidationResult,
        mount_fn: Any,
        config: dict[str, Any] | None = None,
    ) -> None:
        """
        Check if mounted instance implements HookHandler protocol.

        Args:
            result: ValidationResult to update
            mount_fn: Module's mount function
            config: Optional module configuration (uses empty dict if not provided)
        """
        # Create coordinator and track mount_result outside try block so finally can access them
        from ..testing import TestCoordinator

        coordinator = TestCoordinator()
        mount_result = None  # Track returned cleanup function
        try:
            # Use provided config or empty dict as fallback
            actual_config = config if config is not None else {}

            # Call mount() and get the result (may be a cleanup function)
            mount_result = await mount_fn(coordinator, actual_config)

            # Check what was mounted - hooks mount point is a HookRegistry, not a dict
            hook_registry = coordinator.mount_points.get("hooks")
            # Check if any handlers were registered
            has_registered_hooks = (
                hook_registry is not None
                and hasattr(hook_registry, "_handlers")
                and any(hook_registry._handlers.values())
            )
            if not has_registered_hooks:
                # Module might return the instance directly
                if mount_result is not None and isinstance(mount_result, HookHandler):
                    result.add(
                        ValidationCheck(
                            name="protocol_compliance",
                            passed=True,
                            message="mount() returned a valid HookHandler instance",
                            severity="info",
                        )
                    )
                    self._check_hook_methods(result, mount_result)
                    return
                if callable(mount_result):
                    # Hooks often register via coordinator.hooks.register() instead of mount_points
                    # Check if hooks were registered via the hook registry
                    if hasattr(coordinator, "hooks") and coordinator.hooks:
                        result.add(
                            ValidationCheck(
                                name="protocol_compliance",
                                passed=True,
                                message="mount() registered hooks via coordinator.hooks",
                                severity="info",
                            )
                        )
                        return
                    result.add(
                        ValidationCheck(
                            name="protocol_compliance",
                            passed=True,
                            message="mount() returned a cleanup callable (hooks may be registered internally)",
                            severity="warning",
                        )
                    )
                    return
                # Check if hooks were registered via coordinator.hooks
                if hasattr(coordinator, "hooks") and coordinator.hooks:
                    result.add(
                        ValidationCheck(
                            name="protocol_compliance",
                            passed=True,
                            message="Hooks registered via coordinator.hooks",
                            severity="info",
                        )
                    )
                    return
                result.add(
                    ValidationCheck(
                        name="protocol_compliance",
                        passed=False,
                        message="No hook was mounted and mount() did not return a HookHandler instance",
                        severity="error",
                    )
                )
                return

            # Hooks were registered - check all registered handlers
            result.add(
                ValidationCheck(
                    name="protocol_compliance",
                    passed=True,
                    message="Hooks registered via coordinator.hooks.register()",
                    severity="info",
                )
            )

            # Optionally check each handler implements HookHandler protocol
            # At this point, hook_registry is guaranteed to be not None (checked above)
            assert hook_registry is not None
            for _event_name, handlers in hook_registry._handlers.items():
                for hook in handlers:
                    if isinstance(hook, HookHandler):
                        self._check_hook_methods(result, hook)
                    # Note: Hooks registered via lambdas/callables are also valid

        except Exception as e:
            result.add(
                ValidationCheck(
                    name="protocol_compliance",
                    passed=False,
                    message=f"Error during protocol compliance check: {e}",
                    severity="error",
                )
            )
        finally:
            # CRITICAL: Clean up any resources created during mount() to avoid
            # "Unclosed client session" warnings. Hook modules like hooks-notify-push
            # create aiohttp.ClientSession instances that must be properly closed.
            #
            # Cleanup can come from two sources:
            # 1. Returned from mount() - the cleanup function is returned directly
            # 2. Registered via coordinator.register_cleanup() - stored in _cleanup_functions
            #
            # We must handle BOTH patterns.

            # First, call any cleanup function returned from mount()
            if mount_result is not None and callable(mount_result):
                try:
                    await mount_result()
                except Exception:
                    pass  # Ignore cleanup errors during validation

            # Then, call any cleanup functions registered with the coordinator
            if hasattr(coordinator, "_cleanup_functions"):
                for cleanup_fn in coordinator._cleanup_functions:
                    try:
                        await cleanup_fn()
                    except Exception:
                        pass  # Ignore cleanup errors during validation

    def _check_hook_methods(self, result: ValidationResult, hook: HookHandler) -> None:
        """Check that hook has all required methods with correct signatures."""
        # Check __call__ method (the core hook interface)
        if not callable(hook):
            result.add(
                ValidationCheck(
                    name="hook_call",
                    passed=False,
                    message="HookHandler missing __call__() method",
                    severity="error",
                )
            )
            return

        call_method = hook.__call__
        if not asyncio.iscoroutinefunction(call_method):
            result.add(
                ValidationCheck(
                    name="hook_call",
                    passed=False,
                    message="HookHandler.__call__() should be async",
                    severity="error",
                )
            )
            return

        # Check signature: event, data
        sig = inspect.signature(call_method)
        params = [p for p in sig.parameters if p != "self"]
        if len(params) >= 2:
            result.add(
                ValidationCheck(
                    name="hook_call",
                    passed=True,
                    message="HookHandler.__call__() has correct async signature (event, data)",
                    severity="info",
                )
            )
        else:
            result.add(
                ValidationCheck(
                    name="hook_call",
                    passed=False,
                    message=f"HookHandler.__call__() should accept (event, data), found {len(params)} params",
                    severity="error",
                )
            )
