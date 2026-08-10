"""any2any CLI entry point.

`any2any` detects a file's real type (via libmagic, ignoring extensions),
shows which formats it can be converted to, and routes the job to the
appropriate external tool (ImageMagick / ffmpeg / pandoc) via subprocess.
any2any never performs conversion logic itself.
"""

from __future__ import annotations

from pathlib import Path

import typer
from rich.console import Console
from rich.prompt import Prompt
from rich.table import Table

from config import ROUTES, resolve_route
from detector import detect_mime_type
from engine import BinaryNotFoundError, resolve_binary, run_conversion

app = typer.Typer(
    name="any2any",
    help="Universal file converter — detects a file's type and routes it to the right tool.",
    add_completion=False,
)
console = Console()


@app.command()
def convert(
    filepath: Path = typer.Argument(
        ...,
        exists=True,
        file_okay=True,
        dir_okay=False,
        readable=True,
        help="Path to the file you want to convert.",
    ),
    to: str = typer.Option(
        None,
        "--to",
        help="Target format (skips the interactive prompt), e.g. --to webp",
    ),
    output: Path = typer.Option(
        None,
        "--output",
        "-o",
        help="Explicit output path. Defaults to the input path with the new extension.",
    ),
) -> None:
    """Detect FILEPATH's type, ask for (or accept) a target format, and convert it."""
    mime_type = detect_mime_type(filepath)
    route, treated_as_markdown = resolve_route(mime_type, filepath)

    if treated_as_markdown:
        console.print(
            f"[bold cyan]{filepath.name}[/bold cyan] → [green]{mime_type}[/green] "
            f"[dim](.{filepath.suffix.lstrip('.')} extension → parsed as markdown prose)[/dim]"
        )
    else:
        console.print(f"[bold cyan]{filepath.name}[/bold cyan] → [green]{mime_type}[/green]")

    if route is None:
        console.print(f"[bold red]No conversion route configured for '{mime_type}'.[/bold red]")
        raise typer.Exit(code=1)

    # Fail fast if the required binary isn't installed, before we bother
    # asking the user to pick a format.
    try:
        resolve_binary(route)
    except BinaryNotFoundError as exc:
        console.print(f"[bold red]Missing dependency:[/bold red] {exc}")
        raise typer.Exit(code=1) from None

    if to is not None:
        target = to.lower().lstrip(".")
        if target not in route.targets:
            console.print(
                f"[bold red]'{target}' isn't a valid target for {mime_type}.[/bold red] "
                f"Choices: {', '.join(route.targets)}"
            )
            raise typer.Exit(code=1)
    else:
        table = Table(title=f"Available conversions for {filepath.name}")
        table.add_column("Target format", style="green")
        table.add_column("Tool", style="magenta")
        for fmt in route.targets:
            table.add_row(fmt, route.tool.value)
        console.print(table)

        target = Prompt.ask(
            "[bold]Convert to[/bold]",
            choices=list(route.targets),
            show_choices=True,
        )

    output_path = output if output is not None else filepath.with_suffix(f".{target}")

    if output_path.exists():
        overwrite = Prompt.ask(
            f"[yellow]{output_path.name} already exists. Overwrite?[/yellow]",
            choices=["y", "n"],
            default="n",
        )
        if overwrite != "y":
            console.print("[yellow]Aborted.[/yellow]")
            raise typer.Exit(code=0)

    with console.status(f"[bold cyan]Converting {filepath.name}...[/bold cyan]", spinner="dots"):
        result = run_conversion(route, filepath, output_path)

    if result.success:
        console.print(
            f"[bold green]✓ Done.[/bold green] Saved to [bold]{result.output_path}[/bold]"
        )
    else:
        console.print(f"[bold red]✗ Conversion failed[/bold red] (exit code {result.returncode})")
        console.print(f"[dim]$ {' '.join(result.command)}[/dim]")
        if result.stderr.strip():
            console.print("[bold red]stderr:[/bold red]")
            console.print(result.stderr.strip(), style="red")
        raise typer.Exit(code=1)


@app.command(name="formats")
def list_formats() -> None:
    """List every MIME type any2any currently knows how to route."""
    table = Table(title="any2any — supported input formats")
    table.add_column("MIME type", style="cyan")
    table.add_column("Tool", style="magenta")
    table.add_column("Targets", style="green")
    for mime_type, route in sorted(ROUTES.items()):
        table.add_row(mime_type, route.tool.value, ", ".join(route.targets))
    console.print(table)
    console.print(
        "[dim]Plus: any other text/* file (scripts, configs, logs, unusual "
        "source) is also supported via a generic text fallback — its "
        "structure is preserved verbatim rather than parsed as markdown. "
        "Files ending in .md/.markdown/.mdown/.mkd are parsed as real "
        "markdown prose instead of that fallback, even when detected as "
        "text/plain.[/dim]"
    )


if __name__ == "__main__":
    app()
