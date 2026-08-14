"""filemanuplator CLI entry point.

`filemanuplator` detects a file's real type (via libmagic, ignoring extensions),
shows which formats it can be converted to, and routes the job to the
appropriate external tool (ImageMagick / ffmpeg / pandoc) via subprocess.
filemanuplator never performs conversion logic itself.
"""

from __future__ import annotations

import os
from pathlib import Path
from typing import List, Optional

import typer
from rich.console import Console
from rich.prompt import Prompt
from rich.table import Table
from rich.panel import Panel
from rich.text import Text
from rich.align import Align
from rich import box

from config import ROUTES, resolve_route
from detector import detect_mime_type
from engine import BinaryNotFoundError, resolve_binary, run_conversion
from plugin_loader import load_plugins, PLUGIN_ROUTES

app = typer.Typer(
    name="filemanuplator",
    help="Universal file converter — detects a file's type and routes it to the right tool.",
    add_completion=False,
)
console = Console()

def _print_header():
    title = Text(" ♻️ filemanuplator ", style="bold white on blue", justify="center")
    console.print(Panel(title, box=box.DOUBLE, border_style="blue", expand=False))

def _process_file(filepath: Path, target: str, output_path: Optional[Path] = None) -> bool:
    """Process a single file."""
    mime_type = detect_mime_type(filepath)
    route, treated_as_markdown = resolve_route(mime_type, filepath)

    load_plugins()
    plugin_targets = PLUGIN_ROUTES.get(mime_type, {})
    wildcard_targets = PLUGIN_ROUTES.get("*/*", {})
    plugin_targets.update(wildcard_targets)
    
    all_targets = (list(route.targets) if route else []) + list(plugin_targets.keys())

    if target not in all_targets:
        console.print(f"[yellow]⚠ Skipping[/yellow] [bold]{filepath.name}[/bold]: '{target}' is not a valid target for {mime_type}.")
        return False

    out_p = output_path if output_path is not None else filepath.with_suffix(f".{target}")

    with console.status(f"[bold cyan]Converting[/bold cyan] [white]{filepath.name}[/white]...", spinner="point"):
        if target in plugin_targets:
            convert_fn = plugin_targets[target]
            try:
                file_props = {
                    "mime_type": mime_type,
                    "size_bytes": filepath.stat().st_size,
                    "name": filepath.name,
                    "stem": filepath.stem,
                    "suffix": filepath.suffix
                }
                success = convert_fn(str(filepath), str(out_p), file_props)
                if success:
                    console.print(f"[bold green]✓ Success:[/bold green] [dim]{filepath.name} → {out_p.name}[/dim]")
                    return True
                else:
                    console.print(f"[bold red]✗ Failed:[/bold red] [dim]{filepath.name} (Plugin error)[/dim]")
                    return False
            except Exception as e:
                console.print(f"[bold red]✗ Failed:[/bold red] [dim]{filepath.name} ({e})[/dim]")
                return False
        else:
            result = run_conversion(route, filepath, out_p)
            if result.success:
                console.print(f"[bold green]✓ Success:[/bold green] [dim]{filepath.name} → {out_p.name}[/dim]")
                return True
            else:
                console.print(f"[bold red]✗ Failed:[/bold red] [dim]{filepath.name} (Exit code {result.returncode})[/dim]")
                if result.stderr.strip():
                    console.print(Panel(result.stderr.strip(), title="Error Output", border_style="red"))
                return False

@app.command()
def convert(
    filepath: Path = typer.Argument(
        ...,
        exists=True,
        file_okay=True,
        dir_okay=True,
        readable=True,
        help="Path to the file or directory you want to convert.",
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
        help="Explicit output path or directory. Defaults to the input path with the new extension.",
    ),
    recursive: bool = typer.Option(
        False,
        "--recursive",
        "-r",
        help="Scan directories recursively for bulk conversion.",
    )
) -> None:
    """Detect FILEPATH's type, ask for (or accept) a target format, and convert it."""
    _print_header()

    if filepath.is_dir():
        if to is None:
            to = Prompt.ask("[bold magenta]Target format for directory[/bold magenta]")
            if not to:
                console.print("[red]A target format is required for bulk directory processing.[/red]")
                raise typer.Exit(1)
                
        target = to.lower().lstrip(".")
        console.print(f"[bold cyan]Scanning directory:[/bold cyan] {filepath.absolute()}")
        
        files_to_process = []
        if recursive:
            files_to_process = [p for p in filepath.rglob("*") if p.is_file()]
        else:
            files_to_process = [p for p in filepath.iterdir() if p.is_file()]
            
        console.print(f"Found [bold]{len(files_to_process)}[/bold] files. Starting bulk conversion to [bold green].{target}[/bold green]\n")
        
        success_count = 0
        for f in files_to_process:
            # If output is provided for a directory, it must be a directory
            out_p = None
            if output is not None:
                os.makedirs(output, exist_ok=True)
                out_p = output / f.with_suffix(f".{target}").name
            if _process_file(f, target, out_p):
                success_count += 1
                
        console.print(Panel(f"Bulk processing complete! [bold green]{success_count}/{len(files_to_process)}[/bold green] successful.", border_style="green"))
        return

    # Single file processing
    mime_type = detect_mime_type(filepath)
    route, treated_as_markdown = resolve_route(mime_type, filepath)

    console.print(f"File: [bold cyan]{filepath.name}[/bold cyan]")
    console.print(f"Type: [bold magenta]{mime_type}[/bold magenta]")
    if treated_as_markdown:
        console.print("[dim](Parsed as markdown prose based on extension)[/dim]")
    print("")

    load_plugins()
    plugin_targets = PLUGIN_ROUTES.get(mime_type, {})
    wildcard_targets = PLUGIN_ROUTES.get("*/*", {})
    plugin_targets.update(wildcard_targets)
    
    all_targets = (list(route.targets) if route else []) + list(plugin_targets.keys())

    if not all_targets:
        console.print(f"[bold red]No conversion route configured for '{mime_type}'.[/bold red]")
        raise typer.Exit(code=1)

    if route:
        try:
            resolve_binary(route)
        except BinaryNotFoundError as exc:
            console.print(f"[bold red]Missing dependency:[/bold red] {exc}")
            raise typer.Exit(code=1) from None

    if to is not None:
        target = to.lower().lstrip(".")
        if target not in all_targets:
            console.print(
                f"[bold red]'{target}' isn't a valid target for {mime_type}.[/bold red] "
                f"Choices: {', '.join(all_targets)}"
            )
            raise typer.Exit(code=1)
    else:
        table = Table(title=f"Available Conversions", box=box.ROUNDED, border_style="cyan")
        table.add_column("Target format", style="green", justify="center")
        table.add_column("Engine", style="magenta", justify="center")
        if route:
            for fmt in route.targets:
                table.add_row(fmt, route.tool.value)
        for fmt, convert_fn in plugin_targets.items():
            table.add_row(fmt, f"plugin ({convert_fn.__module__.split('.')[-1]})")
            
        console.print(table)

        target = Prompt.ask(
            "\n[bold]Convert to[/bold]",
            choices=all_targets,
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

    _process_file(filepath, target, output_path)

@app.command(name="formats")
def list_formats() -> None:
    """List every MIME type filemanuplator currently knows how to route."""
    _print_header()
    load_plugins()
    table = Table(title="Supported Input Formats", box=box.SIMPLE_HEAVY, border_style="blue")
    table.add_column("MIME type", style="cyan")
    table.add_column("Engine", style="magenta")
    table.add_column("Targets", style="green")
    
    all_mimes = set(ROUTES.keys()) | set(PLUGIN_ROUTES.keys())
    for mime_type in sorted(all_mimes):
        tools = []
        targets = []
        
        route = ROUTES.get(mime_type)
        if route:
            tools.append(route.tool.value)
            targets.extend(route.targets)
            
        plugin_targets = PLUGIN_ROUTES.get(mime_type, {})
        if plugin_targets:
            tools.append("plugin")
            targets.extend(plugin_targets.keys())
            
        table.add_row(mime_type, ", ".join(tools), ", ".join(targets))
        
    console.print(table)
    console.print(
        Panel(
            "Any other text/* file (scripts, configs, logs) is supported via a generic text fallback, "
            "preserving its structure verbatim rather than parsing it as markdown.",
            border_style="dim"
        )
    )

if __name__ == "__main__":
    app()
