"""
Helper for collect_contributions that handles both sync and async callbacks.

This module exists because the Rust PyO3 bridge cannot easily await Python
coroutines from within Python::try_attach. Instead, the Rust code delegates
to this pure-Python async function which handles both sync and async callbacks
naturally within the Python event loop.
"""

import asyncio
import inspect
import logging

logger = logging.getLogger(__name__)


async def collect_contributions(channels: dict, channel: str) -> list:
    """Collect contributions from a channel, handling sync and async callbacks.

    Matches Python ModuleCoordinator.collect_contributions behavior:
    - Errors in individual contributors are logged, not propagated
    - None returns are filtered out
    - Both sync and async callbacks are supported
    """
    contributions = []
    contributors = channels.get(channel)
    if not contributors:
        return contributions

    for contributor in contributors:
        try:
            callback = contributor["callback"]
            # Handle both sync and async callables
            if inspect.iscoroutinefunction(callback):
                result = await callback()
            else:
                result = callback()
                # If the result is a coroutine, await it
                if inspect.iscoroutine(result):
                    result = await result

            if result is not None:
                contributions.append(result)
        except asyncio.CancelledError:
            logger.warning(
                f"Collection cancelled during contributor "
                f"'{contributor['name']}' on channel '{channel}'"
            )
            break
        except Exception as e:
            logger.warning(
                f"Contributor '{contributor['name']}' on channel '{channel}' failed: {e}"
            )

    return contributions
