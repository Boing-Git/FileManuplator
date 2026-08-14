# filemanuplator Plugin System

filemanuplator supports simple, declarative YAML plugins. Anyone can create powerful custom conversion pipelines by simply explaining the steps the file needs to go through.

Drop any `.yaml` file into `~/.local/share/filemanuplator/plugins/` (or this `src/plugins/` folder) and it will instantly become available as a conversion target.

## Structure of a YAML Plugin

A basic plugin requires a `target` name, the `mime_types` it accepts, and a list of `actions`.

```yaml
target: my_plugin
mime_types: 
  - "*/*"  # Use "*/*" to accept all file types, or e.g., "video/mp4" for specifics
description: "An optional description of what the plugin does"

actions:
  - type: shell
    command: "echo 'Processing {file.name}...'"
```

## Available Variables
You can inject the following variables directly into your strings (like conditions, destinations, or commands) by wrapping them in `{}` brackets:

* `{file.path}` : The absolute path to the input file.
* `{file.name}` : The full file name (e.g. `movie.part1.mp4`).
* `{file.stem}` : The name without the extension (e.g. `movie.part1`).
* `{file.ext}` : The file extension (e.g. `.mp4`).
* `{file.size}` : The exact size of the input file in bytes.
* `{file.mime}` : The detected MIME type (e.g. `video/mp4`).
* `{output.path}`: The requested absolute output file path.
* `{output.dir}` : The requested output directory path.

## Supported Actions

Every action can optionally include a `condition` string. If the condition evaluates to `False`, the action is skipped. Conditions are evaluated using standard Python math logic (e.g., `size > 40000`).

### 1. `copy`
Copies the input file to a destination.
```yaml
- type: copy
  condition: "{file.size} <= 1000000"
  destination: "{output.path}"
```

### 2. `split_bytes`
Splits a massive file into smaller byte chunks. The `chunk_size` can be specified in bytes, or with suffixes (e.g. `4G`, `3900M`, `2K`).
```yaml
- type: split_bytes
  chunk_size: "3900M"
  output_pattern: "{output.dir}/{file.name}.part"
```

### 3. `shell`
Executes an arbitrary shell command.
```yaml
- type: shell
  command: "tar -czf '{output.path}' '{file.path}'"
```

### 4. `ffmpeg`
Runs an FFmpeg command. (Input and Output `-i` flags are automatically handled if you leave them out, but it's best to specify exact args).
```yaml
- type: ffmpeg
  args: "-i '{file.path}' -vcodec libx264 '{output.path}'"
```

### 5. `magick`
Runs an ImageMagick command.
```yaml
- type: magick
  args: "'{file.path}' -resize 50% '{output.path}'"
```

### 6. `delete`
Deletes a file (useful for cleaning up temporary files after a pipeline).
```yaml
- type: delete
  target_path: "{output.dir}/temp.txt"
```

### 7. `echo`
Prints a message to the console.
```yaml
- type: echo
  message: "Successfully processed {file.name}!"
```

## Full Example: FAT32 Splitter
This plugin intercepts any file converted to `fat32`. If the file is > 3.9GB, it splits it. Otherwise, it just copies it.

```yaml
target: fat32
mime_types: ["*/*"]
description: "Splits files over 3.9GB for FAT32 USB compatibility"

actions:
  - type: split_bytes
    condition: "{file.size} > 3900000000"
    chunk_size: "3900M"
    output_pattern: "{output.dir}/{file.name}.part"
    
  - type: copy
    condition: "{file.size} <= 3900000000"
    destination: "{output.path}"

```
