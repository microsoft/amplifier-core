"""
CLI for amplifier-core module validation.

Provides the `amplifier-core validate` command for module developers
to check their modules implement required protocols correctly.
"""

import asyncio
import sys

import click

from .validation import ContextValidator
from .validation import HookValidator
from .validation import OrchestratorValidator
from .validation import ProviderValidator
from .validation import ToolValidator
from .validation import ValidationResult

VALIDATORS = {
    "provider": ProviderValidator,
    "tool": ToolValidator,
    "hook": HookValidator,
    "orchestrator": OrchestratorValidator,
    "context": ContextValidator,
}


def print_result(result: ValidationResult) -> None:
    """Print validation result with colored output."""
    # Summary line
    if result.passed:
        click.secho(result.summary(), fg="green", bold=True)
    else:
        click.secho(result.summary(), fg="red", bold=True)

    click.echo()

    # Individual checks
    for check in result.checks:
        if check.passed:
            symbol = click.style("✓", fg="green")
        else:
            symbol = click.style("✗", fg="red")

        severity_colors = {"error": "red", "warning": "yellow", "info": "blue"}
        severity = click.style(
            f"[{check.severity}]",
            fg=severity_colors.get(check.severity, "white"),
        )

        click.echo(f"  {symbol} {severity:20} {check.name}: {check.message}")


@click.group()
@click.version_option(version="1.0.0", prog_name="amplifier-core")
def cli() -> None:
    """Amplifier Core - Module validation tools."""
    pass


@cli.command()
@click.argument("module_type", type=click.Choice(list(VALIDATORS.keys())))
@click.argument("module_path", type=click.Path(exists=True))
@click.option(
    "--entry-point",
    "-e",
    help="Entry point name (e.g., 'provider-anthropic')",
)
@click.option(
    "--quiet",
    "-q",
    is_flag=True,
    help="Only show summary, not individual checks",
)
def validate(
    module_type: str,
    module_path: str,
    entry_point: str | None,
    quiet: bool,
) -> None:
    """Validate a module implements its required protocol.

    MODULE_TYPE is one of: provider, tool, hook, orchestrator, context

    MODULE_PATH is the path to the module directory or Python file

    Examples:

        amplifier-core validate provider ./my-provider/

        amplifier-core validate tool ./tools/my_tool.py

        amplifier-core validate hook ./hooks/logging/
    """
    validator_class = VALIDATORS[module_type]
    validator = validator_class()

    click.echo(f"Validating {module_type} module: {module_path}")
    click.echo()

    result = asyncio.run(validator.validate(module_path, entry_point))

    if quiet:
        click.echo(result.summary())
    else:
        print_result(result)

    sys.exit(0 if result.passed else 1)


@cli.command(name="list-types")
def list_types() -> None:
    """List available module types that can be validated."""
    click.echo("Available module types:")
    click.echo()

    descriptions = {
        "provider": "LLM backends (Anthropic, OpenAI, Azure, etc.)",
        "tool": "Agent capabilities (filesystem, bash, web, etc.)",
        "hook": "Observability and control (logging, approval, etc.)",
        "orchestrator": "Execution strategies (basic, streaming, events)",
        "context": "Memory management (simple, persistent)",
    }

    for name, desc in descriptions.items():
        click.echo(f"  {click.style(name, fg='cyan', bold=True):20} {desc}")


def main() -> None:
    """Entry point for the CLI."""
    cli()


if __name__ == "__main__":
    main()
