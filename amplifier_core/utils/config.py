"""Configuration utility functions."""

import os
import re
from typing import Any


def interpolate_env_vars(config: Any) -> Any:
    """
    Recursively interpolate environment variables in configuration.

    Replaces strings matching ${VAR_NAME} with the value of environment variable VAR_NAME.
    If the environment variable doesn't exist, the placeholder is kept as-is.

    Args:
        config: Configuration data structure (dict, list, or scalar)

    Returns:
        The configuration with environment variables interpolated
    """
    if isinstance(config, dict):
        # Recursively process dictionary values
        return {key: interpolate_env_vars(value) for key, value in config.items()}

    if isinstance(config, list):
        # Recursively process list items
        return [interpolate_env_vars(item) for item in config]

    if isinstance(config, str):
        # Pattern to match ${VAR_NAME} where VAR_NAME can contain letters, numbers, and underscores
        pattern = re.compile(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}")

        def replace_var(match):
            var_name = match.group(1)
            # Get environment variable value, keep placeholder if not found
            return os.environ.get(var_name, match.group(0))

        # Replace all environment variable references
        return pattern.sub(replace_var, config)

    # Return other types as-is (int, float, bool, None, etc.)
    return config
