# any2any

A universal CLI converter that doesn't convert anything itself. It detects a
file's real type by content (via `libmagic`, ignoring the extension), shows
which formats it can become, and routes the job to the right external tool —
`ffmpeg`, ImageMagick, or `pandoc` — via `subprocess`.

## Install

```bash
pip install -r requirements.txt
```

System dependencies (must be on PATH):

```bash
# Debian/Ubuntu
sudo apt install ffmpeg imagemagick pandoc libmagic1

# Fedora
sudo dnf install ffmpeg ImageMagick pandoc file-libs

# Arch
sudo pacman -S ffmpeg imagemagick pandoc file

# NixOS (shell.nix / flake devShell, or system packages)
nix-shell -p ffmpeg imagemagick pandoc file wkhtmltopdf
```

`wkhtmltopdf` is optional but recommended: it's used as the default PDF
engine because it has no LaTeX dependencies. Without it (or another PDF
engine — `weasyprint`, `tectonic`, or a LaTeX install with `lmodern`),
`--to pdf` will fail with a clear "no PDF engine found" message instead of
converting.

## Usage

```bash
# Interactive — shows a table of possible targets, then prompts you to pick one
any2any convert photo.png

# Non-interactive — skip the prompt
any2any convert photo.png --to webp

# Explicit output path
any2any convert photo.png --to webp -o /tmp/out.webp

# List every MIME type any2any knows how to route
any2any formats
```

If `any2any` isn't installed as a script yet, run it directly:

```bash
python3 main.py convert photo.png --to webp
```

## Plain text and source code

`libmagic` can't distinguish hand-written markdown prose from source code,
config files, JSON, logs, or anything else — both come back as
`text/plain`. Feeding arbitrary code straight into pandoc's markdown reader
mangles it: unindented lines get reflowed as paragraphs, and `#`, `*`, `_`,
`-` get reinterpreted as markdown syntax instead of code.

any2any handles this by treating the two cases differently:

- **Files ending in `.md`, `.markdown`, `.mdown`, `.mkd`** are parsed as real
  markdown — headers become headers, `**bold**` becomes bold, lists render
  as lists. This is the one place any2any looks at the extension rather than
  content, because content-only detection genuinely can't tell prose from
  code here.
- **Everything else detected as text** (`.qml`, `.py`, `.json`, config
  files, logs, unrecognized source, ...) is preserved verbatim: wrapped in a
  fenced code block before conversion, so indentation, brackets, and symbols
  come through unchanged in the output document. `--to txt` and `--to md`
  skip pandoc entirely (direct copy / direct wrap) so there's zero risk of
  reformatting; `--to html/pdf/docx` renders the fenced content as a
  syntax-highlighted code block inside a proper document.

Any MIME type starting with `text/` (or common code-like `application/*`
types like `application/json`) is accepted even if it's not explicitly
listed in `config.py` — see the fallback route in `get_route()`.

## Architecture

| File | Responsibility |
|---|---|
| `detector.py` | MIME type detection via `python-magic` (content-based, never trusts the extension) |
| `config.py` | The routing table — MIME type → tool + allowed output formats. The only file you touch to add new format support. |
| `engine.py` | Resolves the binary (`shutil.which`), builds the `argv` list, runs `subprocess.run()`, captures stdout/stderr/exit code |
| `main.py` | `typer` CLI — wires detection → routing table → prompt → engine → `rich` output |

Commands are always passed to `subprocess.run()` as argument lists, never as
shell strings, so filenames with spaces or shell metacharacters are handled
safely.

## Adding a new format

Add an entry to `ROUTES` in `config.py`:

```python
"image/avif": ConversionRoute(
    tool=Tool.IMAGEMAGICK,
    binary_candidates=("magick", "convert"),
    targets=("png", "jpg", "webp"),
),
```

Nothing else needs to change — `main.py` and `engine.py` read the table
generically.
