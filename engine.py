"""Builds and executes the external-tool commands that do the actual conversion.

any2any never implements conversion logic itself — this module's only job is
to turn a (tool, input path, output path) triple into an argv list and run it.

The one exception is PANDOC + raw_text routes (plain text, source code,
configs, logs, ...): pandoc's default markdown reader reflows unindented
lines as prose and reinterprets characters like #, *, _, and - as markdown
syntax, which silently mangles the structure of anything that isn't actual
markdown prose. For those inputs we wrap the original content, byte-for-byte,
in a fenced code block first — pandoc treats fenced code as opaque literal
text, so indentation, brackets, and symbols survive untouched.
"""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path

from config import PANDOC_WRITERS, ConversionRoute, Tool


class BinaryNotFoundError(RuntimeError):
    """Raised when none of a route's candidate binaries exist on PATH."""


@dataclass(frozen=True)
class ConversionResult:
    """Outcome of a conversion attempt."""

    success: bool
    output_path: Path
    command: tuple[str, ...]
    stdout: str
    stderr: str
    returncode: int


def resolve_binary(route: ConversionRoute) -> str:
    """Find the first available binary from a route's candidates on PATH.

    Raises:
        BinaryNotFoundError: if none of the candidates are installed.
    """
    for candidate in route.binary_candidates:
        found = shutil.which(candidate)
        if found is not None:
            return candidate
    raise BinaryNotFoundError(
        f"None of the required binaries ({', '.join(route.binary_candidates)}) "
        "were found on PATH. Please install the corresponding system package."
    )


def _read_text(input_path: Path) -> str:
    """Read a file as text, tolerating encodings that aren't strict UTF-8."""
    raw = input_path.read_bytes()
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError:
        return raw.decode("utf-8", errors="replace")


def _fence_for(text: str) -> str:
    """Pick a backtick fence longer than any backtick run already in text.

    Prevents content that itself contains ``` (e.g. a file with embedded
    markdown examples) from prematurely closing our wrapper fence.
    """
    longest_run = 0
    current_run = 0
    for ch in text:
        if ch == "`":
            current_run += 1
            longest_run = max(longest_run, current_run)
        else:
            current_run = 0
    return "`" * max(longest_run + 1, 3)


def _wrap_as_code_fence(text: str, language_hint: str) -> str:
    """Wrap raw text in a markdown fenced code block, preserving it verbatim."""
    fence = _fence_for(text)
    # Normalize trailing newline so the closing fence always starts on its
    # own line, without adding a spurious blank line when one already exists.
    body = text if text.endswith("\n") else text + "\n"
    return f"{fence}{language_hint}\n{body}{fence}\n"


def build_command(
    tool: Tool,
    binary: str,
    input_path: Path,
    output_path: Path,
) -> list[str]:
    """Construct the argv list for the given tool.

    Always returns a list (never a shell string) so paths containing spaces
    or shell metacharacters are passed safely to subprocess.run().
    """
    if tool is Tool.IMAGEMAGICK:
        # `magick input output` (IM7) and `convert input output` (IM6) share
        # this exact argument order.
        return [binary, str(input_path), str(output_path)]

    if tool is Tool.FFMPEG:
        # -y: overwrite output without an interactive prompt (we own the
        # confirmation step in the CLI layer, not ffmpeg).
        return [binary, "-y", "-i", str(input_path), str(output_path)]

    raise ValueError(f"build_command doesn't handle {tool!r} directly; see run_conversion")


def _run_subprocess(command: list[str], output_path: Path) -> ConversionResult:
    proc = subprocess.run(command, capture_output=True, text=True, check=False)
    return ConversionResult(
        success=proc.returncode == 0,
        output_path=output_path,
        command=tuple(command),
        stdout=proc.stdout,
        stderr=proc.stderr,
        returncode=proc.returncode,
    )


def _target_ext(output_path: Path) -> str:
    return output_path.suffix.lstrip(".").lower()


# Preference order for pandoc's --pdf-engine. wkhtmltopdf is tried first
# because it has no LaTeX package dependencies to go missing (a common gap
# on minimal/declarative installs like NixOS or slim containers) — the
# LaTeX engines are still supported as a fallback for users who have a full
# TeX Live install and want its typographic quality.
_PDF_ENGINE_CANDIDATES = (
    "wkhtmltopdf",
    "weasyprint",
    "tectonic",
    "xelatex",
    "lualatex",
    "pdflatex",
    "context",
)


def resolve_pdf_engine() -> str | None:
    """Find the first available pandoc PDF engine on PATH, or None if none exist."""
    for candidate in _PDF_ENGINE_CANDIDATES:
        if shutil.which(candidate) is not None:
            return candidate
    return None


def _run_pandoc(
    binary: str,
    pandoc_from: str,
    input_path: Path,
    output_path: Path,
) -> ConversionResult:
    """Invoke pandoc with explicit -f/-t flags (never guessed from extensions)."""
    target = _target_ext(output_path)
    writer = PANDOC_WRITERS.get(target)
    if writer is None:
        return ConversionResult(
            success=False,
            output_path=output_path,
            command=(binary,),
            stdout="",
            stderr=f"No pandoc writer configured for target extension '.{target}'.",
            returncode=-1,
        )

    command = [binary, str(input_path), "-f", pandoc_from, "-t", writer, "-o", str(output_path)]
    # Full documents (not plain markdown/text) need a real document wrapper
    # — title, headers, body — rather than a bare content fragment.
    if writer in ("html", "docx", "odt", "epub3", "pdf"):
        command.append("--standalone")
        command.append(f"--metadata=title:{input_path.stem}")

    if writer == "pdf":
        pdf_engine = resolve_pdf_engine()
        if pdf_engine is None:
            return ConversionResult(
                success=False,
                output_path=output_path,
                command=tuple(command),
                stdout="",
                stderr=(
                    "No PDF engine found. Install one of: wkhtmltopdf, weasyprint, "
                    "tectonic, or a LaTeX distribution (texlive-xetex / texlive-latex-base "
                    "+ lmodern)."
                ),
                returncode=-1,
            )
        command.append(f"--pdf-engine={pdf_engine}")

    return _run_subprocess(command, output_path)


def run_conversion(
    route: ConversionRoute,
    input_path: Path,
    output_path: Path,
) -> ConversionResult:
    """Resolve the binary, build the command, and execute it.

    Raises:
        BinaryNotFoundError: if the required tool isn't installed.
    """
    binary = resolve_binary(route)

    if route.tool in (Tool.IMAGEMAGICK, Tool.FFMPEG):
        command = build_command(route.tool, binary, input_path, output_path)
        return _run_subprocess(command, output_path)

    # route.tool is Tool.PANDOC from here on.
    target = _target_ext(output_path)

    if not route.raw_text:
        # Genuine prose (markdown/html/docx/rtf/epub source) — let pandoc
        # parse it normally with its reader for that format.
        return _run_pandoc(binary, route.pandoc_from, input_path, output_path)

    # raw_text route: plain text, source code, config, logs, etc. Preserve
    # structure by treating the content as opaque, never as parseable
    # markdown.
    try:
        original_text = _read_text(input_path)
    except OSError as exc:
        return ConversionResult(
            success=False,
            output_path=output_path,
            command=(),
            stdout="",
            stderr=f"Could not read '{input_path}': {exc}",
            returncode=-1,
        )

    if target == "txt":
        # Already flat text — copy verbatim, byte for byte. No reason to
        # round-trip this through pandoc at all.
        try:
            shutil.copy2(input_path, output_path)
        except OSError as exc:
            return ConversionResult(
                success=False,
                output_path=output_path,
                command=(),
                stdout="",
                stderr=f"Could not write '{output_path}': {exc}",
                returncode=-1,
            )
        return ConversionResult(
            success=True,
            output_path=output_path,
            command=("<copy>", str(input_path), str(output_path)),
            stdout="",
            stderr="",
            returncode=0,
        )

    language_hint = input_path.suffix.lstrip(".").lower()
    wrapped_markdown = _wrap_as_code_fence(original_text, language_hint)

    if target in ("md", "markdown"):
        # Target is markdown itself — write the wrapped fence directly, no
        # need to invoke pandoc just to convert markdown to markdown.
        try:
            output_path.write_text(wrapped_markdown, encoding="utf-8")
        except OSError as exc:
            return ConversionResult(
                success=False,
                output_path=output_path,
                command=(),
                stdout="",
                stderr=f"Could not write '{output_path}': {exc}",
                returncode=-1,
            )
        return ConversionResult(
            success=True,
            output_path=output_path,
            command=("<wrap-as-code-fence>", str(input_path), str(output_path)),
            stdout="",
            stderr="",
            returncode=0,
        )

    # Target is html/pdf/docx/odt/etc: write the wrapped markdown to a temp
    # file and let pandoc render *that* — since it's now valid markdown
    # containing a fenced code block, pandoc reproduces the original content
    # verbatim inside a nicely formatted (and syntax-highlighted) document.
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".md", delete=False, encoding="utf-8"
    ) as tmp:
        tmp.write(wrapped_markdown)
        tmp_path = Path(tmp.name)

    try:
        result = _run_pandoc(binary, "markdown", tmp_path, output_path)
    finally:
        tmp_path.unlink(missing_ok=True)

    # Report the real input/output paths in the result, not the temp file,
    # so error messages make sense to the user.
    return ConversionResult(
        success=result.success,
        output_path=output_path,
        command=(binary, str(input_path), "-f", "markdown", "-t", target, "-o", str(output_path)),
        stdout=result.stdout,
        stderr=result.stderr,
        returncode=result.returncode,
    )
