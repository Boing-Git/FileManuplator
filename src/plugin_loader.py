"""Dynamic YAML plugin loader for filemanuplator.

Plugins are YAML files that define a target, accepted mime_types, and a declarative pipeline of actions.
"""

import os
import sys
import shutil
import subprocess
from pathlib import Path
from typing import Callable

try:
    import yaml
except ImportError:
    yaml = None

# Map: mime_type -> dict[target, convert_callable]
PLUGIN_ROUTES: dict[str, dict[str, Callable[[str, str, dict], bool]]] = {}

def load_plugins() -> None:
    """Discover and load all YAML plugins."""
    if yaml is None:
        print("Warning: 'pyyaml' is not installed. YAML plugins will not be loaded.", file=sys.stderr)
        print("To enable plugins, run: pip install pyyaml", file=sys.stderr)
        return

    # Look for bundled plugins
    bundled_dir = Path(__file__).parent / "plugins"
    
    # Look for user plugins
    user_dir = Path.home() / ".local" / "share" / "filemanuplator" / "plugins"
    
    dirs_to_check = [bundled_dir, user_dir]
    
    for plugin_dir in dirs_to_check:
        if not plugin_dir.exists() or not plugin_dir.is_dir():
            continue
            
        for filepath in plugin_dir.glob("*.yaml"):
            _load_yaml_plugin(filepath)
        for filepath in plugin_dir.glob("*.yml"):
            _load_yaml_plugin(filepath)

def _parse_size(size_str: str) -> int:
    """Parse sizes like 3900M, 4G, 100K to bytes."""
    size_str = str(size_str).strip().upper()
    if size_str.endswith("G") or size_str.endswith("GB"):
        return int(float(size_str.replace("GB", "").replace("G", "")) * 1024**3)
    if size_str.endswith("M") or size_str.endswith("MB"):
        return int(float(size_str.replace("MB", "").replace("M", "")) * 1024**2)
    if size_str.endswith("K") or size_str.endswith("KB"):
        return int(float(size_str.replace("KB", "").replace("K", "")) * 1024)
    return int(size_str)

def _build_context(input_path: str, output_path: str, file_props: dict) -> dict:
    in_p = Path(input_path)
    out_p = Path(output_path)
    
    return {
        "file.path": str(in_p.absolute()),
        "file.name": file_props.get("name", in_p.name),
        "file.stem": file_props.get("stem", in_p.stem),
        "file.ext": file_props.get("suffix", in_p.suffix),
        "file.size": str(file_props.get("size_bytes", in_p.stat().st_size)),
        "file.mime": file_props.get("mime_type", "*/*"),
        "output.path": str(out_p.absolute()),
        "output.dir": str(out_p.parent.absolute()),
    }

def _format_str(val: str, ctx: dict) -> str:
    if not isinstance(val, str):
        return val
    for k, v in ctx.items():
        val = val.replace(f"{{{k}}}", str(v))
    return val

def _evaluate_condition(cond: str, ctx: dict) -> bool:
    if not cond:
        return True
    expr = _format_str(cond, ctx)
    try:
        # Evaluate simple math/logic conditions safely
        result = eval(expr, {"__builtins__": {}})
        return bool(result)
    except Exception as e:
        print(f"Plugin condition evaluation error on '{expr}': {e}", file=sys.stderr)
        return False

def _execute_action(action: dict, ctx: dict) -> bool:
    a_type = action.get("type")
    
    if a_type == "copy":
        dest = _format_str(action.get("destination", "{output.path}"), ctx)
        os.makedirs(Path(dest).parent, exist_ok=True)
        shutil.copy2(ctx["file.path"], dest)
        
    elif a_type == "split_bytes":
        chunk_size_str = action.get("chunk_size", "100M")
        chunk_size = _parse_size(chunk_size_str)
        out_pattern = _format_str(action.get("output_pattern", "{output.dir}/{file.name}.part"), ctx)
        
        in_path = ctx["file.path"]
        out_dir = Path(out_pattern).parent
        os.makedirs(out_dir, exist_ok=True)
        
        with open(in_path, 'rb') as f_in:
            part_num = 1
            while True:
                chunk = f_in.read(chunk_size)
                if not chunk:
                    break
                part_path = out_pattern + str(part_num)
                with open(part_path, 'wb') as f_out:
                    f_out.write(chunk)
                part_num += 1
                
    elif a_type == "shell":
        cmd = _format_str(action.get("command", ""), ctx)
        proc = subprocess.run(cmd, shell=True)
        if proc.returncode != 0:
            return False
            
    elif a_type == "ffmpeg":
        args = _format_str(action.get("args", ""), ctx)
        cmd = f"ffmpeg {args}"
        proc = subprocess.run(cmd, shell=True)
        if proc.returncode != 0:
            return False
            
    elif a_type == "magick":
        args = _format_str(action.get("args", ""), ctx)
        cmd = f"magick {args}"
        proc = subprocess.run(cmd, shell=True)
        if proc.returncode != 0:
            return False
            
    elif a_type == "delete":
        target = _format_str(action.get("target_path", ""), ctx)
        if os.path.exists(target):
            if os.path.isdir(target):
                shutil.rmtree(target)
            else:
                os.remove(target)
                
    elif a_type == "echo":
        msg = _format_str(action.get("message", ""), ctx)
        print(msg)
        
    else:
        print(f"Unknown action type: {a_type}", file=sys.stderr)
        return False
        
    return True

def _load_yaml_plugin(filepath: Path) -> None:
    try:
        with open(filepath, 'r', encoding='utf-8') as f:
            data = yaml.safe_load(f)
    except Exception as e:
        print(f"Failed to load YAML plugin {filepath.name}: {e}", file=sys.stderr)
        return
        
    if not isinstance(data, dict):
        return
        
    target = data.get("target")
    mime_types = data.get("mime_types", ["*/*"])
    actions = data.get("actions", [])
    
    if not target or not actions:
        return
        
    def _plugin_convert(input_path: str, output_path: str, file_props: dict) -> bool:
        ctx = _build_context(input_path, output_path, file_props)
        for action in actions:
            cond = action.get("condition", "")
            if not _evaluate_condition(cond, ctx):
                continue
                
            success = _execute_action(action, ctx)
            if not success:
                return False
        return True
        
    # Provide a meaningful name for the function matching the plugin for main.py logs
    _plugin_convert.__module__ = filepath.stem
    
    for mime in mime_types:
        if mime not in PLUGIN_ROUTES:
            PLUGIN_ROUTES[mime] = {}
        PLUGIN_ROUTES[mime][target] = _plugin_convert
