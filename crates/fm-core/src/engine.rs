use crate::types::{FileContext, PipelineStep};
use image::DynamicImage;
use image::imageops::FilterType;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;

/// Interpolate `{key}` placeholders in a template string using the FileContext map.
pub fn format_str(template: &str, ctx: &FileContext) -> String {
    let mut res = template.to_string();
    for (key, val) in ctx.to_map() {
        res = res.replace(&format!("{{{}}}", key), &val);
    }
    res
}

/// Interpolate placeholders using both the FileContext *and* an extra UI state map.
pub fn format_str_with_state(
    template: &str,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> String {
    let mut res = format_str(template, ctx);
    for (key, val) in state {
        res = res.replace(&format!("{{{}}}", key), val);
    }
    res
}

/// Evaluate a step `condition` against a FileContext.
///
/// See [`crate::condition`] for the supported syntax. A condition that fails to
/// parse counts as false here; use [`evaluate_condition_with_state`] to get the
/// error message.
pub fn evaluate_condition(cond: &str, ctx: &FileContext) -> bool {
    evaluate_condition_with_state(cond, ctx, &HashMap::new()).unwrap_or(false)
}

/// Like [`evaluate_condition`], but placeholders may also refer to UI state and
/// syntax errors are returned instead of being treated as false.
pub fn evaluate_condition_with_state(
    cond: &str,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<bool, String> {
    crate::condition::evaluate(cond, &|s| format_str_with_state(s, ctx, state))
}

// ---------------------------------------------------------------------------
//  New TOML Pipeline execution
// ---------------------------------------------------------------------------

/// Execute a full named pipeline (a Vec of PipelineSteps) against a FileContext.
/// `state` carries widget values from the UI (e.g. slider positions, text inputs).
pub fn execute_pipeline(
    steps: &[PipelineStep],
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<(), String> {
    let mut sorted_steps = steps.to_vec();
    sorted_steps.sort_by_key(|s| s.step);

    for step in &sorted_steps {
        // Check condition
        if let Some(cond) = &step.condition {
            let run = evaluate_condition_with_state(cond, ctx, state)
                .map_err(|e| format!("step {}: invalid condition: {}", step.step, e))?;
            if !run {
                continue;
            }
        }

        execute_step(step, ctx, state)?;
    }
    Ok(())
}

/// Run `steps` on a file held in memory, for frontends that have bytes rather
/// than paths (the mobile app). `work` is wiped and reused as scratch space.
/// The pipeline must write its result to `{output.path}`; those bytes come back.
pub fn run_on_bytes(
    steps: &[PipelineStep],
    input_name: &str,
    input: &[u8],
    output_name: &str,
    state: &HashMap<String, String>,
    work: &Path,
) -> Result<Vec<u8>, String> {
    let _ = fs::remove_dir_all(work);
    let (in_dir, out_dir) = (work.join("in"), work.join("out"));
    for dir in [&in_dir, &out_dir] {
        fs::create_dir_all(dir).map_err(|e| format!("Cannot create '{}': {}", dir.display(), e))?;
    }
    let input_path = in_dir.join(safe_file_name(input_name));
    let output_path = out_dir.join(safe_file_name(output_name));
    fs::write(&input_path, input).map_err(|e| format!("Cannot stage the input: {}", e))?;

    let mime = crate::detector::detect_mime_type(&input_path);
    let ctx = FileContext::from_paths(&input_path, &output_path, &mime);
    execute_pipeline(steps, &ctx, state)?;
    fs::read(&output_path).map_err(|_| {
        "The plugin finished without writing a file to {output.path}, so there is nothing to save."
            .to_string()
    })
}

/// Just the final component of `name`, so a file name from outside cannot
/// point anywhere else (`../`, absolute paths).
pub fn safe_file_name(name: &str) -> String {
    Path::new(name)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "file".to_string())
}

/// Execute a single PipelineStep.
fn execute_step(
    step: &PipelineStep,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<(), String> {
    let action = step.action.as_str();
    match action {
        // --- Image pipeline actions ---
        "crop_image_visually" => action_crop_image(step, ctx, state),
        "resize" => action_resize(step, ctx, state),
        "rotate" => action_rotate(step, ctx, state),
        "adjust_brightness" | "adjust_contrast" | "adjust" => action_adjust_image(step, ctx, state),
        "convert_format" => action_convert_format(step, ctx, state),
        "export_image" => action_export_image(step, ctx, state),
        "create_directories" => action_create_directories(step, ctx, state),

        // --- Legacy file actions ported to PipelineStep ---
        "copy" => action_copy(step, ctx, state),
        "regex_copy" => action_regex_copy(step, ctx, state),
        "delete" => action_delete(step, ctx, state),
        "echo" => action_echo(step, ctx, state),
        "checksum" => action_checksum(step, ctx, state),
        "split_bytes" => action_split_bytes(step, ctx, state),
        "split_csv" => action_split_csv(step, ctx, state),
        "write_text" => action_write_text(step, ctx, state),
        "join_bytes" => action_join_bytes(step, ctx, state),
        "join_csv" => action_join_csv(step, ctx, state),
        "exec" => action_exec(step, ctx, state),
        "ffmpeg" | "magick" | "pandoc" | "wkhtmltopdf" | "chdman" | "par2" => {
            action_external_tool(action, step, ctx, state)
        }

        _ => Err(format!("Unknown pipeline action: {}", action)),
    }
}

// ===== Image Actions =====

fn resolve(tmpl: Option<&String>, ctx: &FileContext, state: &HashMap<String, String>) -> String {
    match tmpl {
        Some(t) => format_str_with_state(t, ctx, state),
        None => String::new(),
    }
}

fn load_image(path: &str) -> Result<DynamicImage, String> {
    image::open(path).map_err(|e| format!("Failed to open image '{}': {}", path, e))
}

fn save_image(img: &DynamicImage, path: &str, format: Option<&str>) -> Result<(), String> {
    if let Some(parent) = Path::new(path).parent() {
        let _ = fs::create_dir_all(parent);
    }
    match format {
        Some("jpeg") | Some("jpg") => {
            let rgb = img.to_rgb8();
            rgb.save(path)
                .map_err(|e| format!("Failed to save JPEG '{}': {}", path, e))
        }
        Some("png") => img
            .save(path)
            .map_err(|e| format!("Failed to save PNG '{}': {}", path, e)),
        Some("webp") => img
            .save(path)
            .map_err(|e| format!("Failed to save WEBP '{}': {}", path, e)),
        _ => img
            .save(path)
            .map_err(|e| format!("Failed to save image '{}': {}", path, e)),
    }
}

/// Crop an image using coordinates that may come from the UI state.
/// Coordinates are expected in the format "x,y,width,height" in the `crop_data` field
/// (or referencing a UI widget via `{crop_canvas_id}`).
fn action_crop_image(
    step: &PipelineStep,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<(), String> {
    let input = resolve(step.target_file.as_ref(), ctx, state);
    let output = resolve(step.output_file.as_ref(), ctx, state);
    let crop_str = resolve(step.crop_data.as_ref(), ctx, state);

    if input.is_empty() || output.is_empty() || crop_str.is_empty() {
        return Err("crop_image_visually: missing target_file, output_file, or crop_data".into());
    }

    let (w, h, x, y) = parse_crop(&crop_str)?;
    let img = load_image(&input)?;
    let cropped = img.crop_imm(x, y, w, h);
    save_image(&cropped, &output, step.format.as_deref())
}

/// Parse crop data in `WxH+X+Y` form (e.g. `100x200+10+20`) into (w, h, x, y).
fn parse_crop(crop_str: &str) -> Result<(u32, u32, u32, u32), String> {
    let parts: Vec<&str> = crop_str.split(&['x', '+'][..]).collect();
    if parts.len() != 4 {
        return Err(format!("crop_data must be 'WxH+X+Y', got: '{}'", crop_str));
    }
    let num = |i: usize, what: &str| -> Result<u32, String> {
        parts[i]
            .trim()
            .parse()
            .map_err(|_| format!("Invalid crop {}: '{}'", what, parts[i]))
    };
    Ok((
        num(0, "width")?,
        num(1, "height")?,
        num(2, "x")?,
        num(3, "y")?,
    ))
}

/// Crop, fit and encode an image in one pass, so a JPEG is only compressed
/// once. Every part is optional: `crop_data` (`WxH+X+Y`), `width`/`height`
/// with `fit` (`contain`, `cover` or `stretch`), and `max_kb`, which picks the
/// highest JPEG quality that fits (it has no effect on lossless formats).
fn action_export_image(
    step: &PipelineStep,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<(), String> {
    let input = resolve(step.target_file.as_ref(), ctx, state);
    let output = resolve(step.output_file.as_ref(), ctx, state);
    if input.is_empty() || output.is_empty() {
        return Err("export_image: missing target_file or output_file".into());
    }
    let mut img = load_image(&input)?;

    let crop = resolve(step.crop_data.as_ref(), ctx, state);
    if !crop.trim().is_empty() {
        let (w, h, x, y) = parse_crop(&crop)?;
        // 0x0 is what a canvas reports before it has an image: keep it all.
        if w > 0 && h > 0 {
            img = img.crop_imm(x, y, w, h);
        }
    }

    let width = resolve(step.width.as_ref(), ctx, state);
    let height = resolve(step.height.as_ref(), ctx, state);
    if !width.trim().is_empty() || !height.trim().is_empty() {
        let dim = |v: &str, what: &str| -> Result<u32, String> {
            match v.trim().parse::<f64>() {
                Ok(n) if n >= 1.0 => Ok(n.round() as u32),
                _ => Err(format!("export_image: invalid {}: '{}'", what, v)),
            }
        };
        let (w, h) = (dim(&width, "width")?, dim(&height, "height")?);
        let filter = FilterType::Lanczos3;
        img = match resolve(step.fit.as_ref(), ctx, state)
            .to_lowercase()
            .as_str()
        {
            "" | "contain" => img.resize(w, h, filter),
            "cover" => img.resize_to_fill(w, h, filter),
            "stretch" => img.resize_exact(w, h, filter),
            other => {
                return Err(format!(
                    "export_image: unknown fit '{}' (use contain, cover or stretch)",
                    other
                ));
            }
        };
    }

    let ext = Path::new(&output)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase);
    let format = step.format.as_deref().or(ext.as_deref());
    let max_kb = resolve(step.max_kb.as_ref(), ctx, state);
    if max_kb.trim().is_empty() || !matches!(format, Some("jpg") | Some("jpeg")) {
        return save_image(&img, &output, format);
    }

    let max_bytes = max_kb
        .trim()
        .parse::<f64>()
        .map(|kb| (kb * 1024.0) as usize)
        .map_err(|_| format!("export_image: invalid max_kb: '{}'", max_kb))?;
    let bytes = jpeg_within(&img.to_rgb8(), max_bytes)?;
    if let Some(parent) = Path::new(&output).parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&output, bytes).map_err(|e| format!("Failed to save JPEG '{}': {}", output, e))
}

/// The highest-quality JPEG of `img` that fits in `max_bytes` (quality 1 when
/// none does), found by binary search over the quality setting.
fn jpeg_within(img: &image::RgbImage, max_bytes: usize) -> Result<Vec<u8>, String> {
    let encode = |quality: u8| -> Result<Vec<u8>, String> {
        let mut buf = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality)
            .encode_image(img)
            .map_err(|e| format!("Failed to encode JPEG: {}", e))?;
        Ok(buf)
    };
    let mut best = encode(1)?;
    let (mut lo, mut hi) = (2u8, 100u8);
    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        let candidate = encode(mid)?;
        if candidate.len() <= max_bytes {
            best = candidate;
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    Ok(best)
}

/// Resize an image to width x height using a configurable filter type.
fn action_resize(
    step: &PipelineStep,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<(), String> {
    let input = resolve(step.target_file.as_ref(), ctx, state);
    let output = resolve(step.output_file.as_ref(), ctx, state);
    let width_str = resolve(step.width.as_ref(), ctx, state);
    let height_str = resolve(step.height.as_ref(), ctx, state);

    let width: u32 = width_str
        .parse()
        .map_err(|_| format!("Invalid resize width: '{}'", width_str))?;
    let height: u32 = height_str
        .parse()
        .map_err(|_| format!("Invalid resize height: '{}'", height_str))?;

    let filter = match step.filter_type.as_deref() {
        Some("lanczos3") | Some("Lanczos3") => FilterType::Lanczos3,
        Some("catmullrom") | Some("CatmullRom") => FilterType::CatmullRom,
        Some("nearest") | Some("Nearest") => FilterType::Nearest,
        Some("triangle") | Some("Triangle") => FilterType::Triangle,
        Some("gaussian") | Some("Gaussian") => FilterType::Gaussian,
        _ => FilterType::Lanczos3,
    };

    let img = load_image(&input)?;
    let resized = img.resize_exact(width, height, filter);
    save_image(&resized, &output, step.format.as_deref())
}

/// Rotate an image by a specified number of degrees (90, 180, 270).
fn action_rotate(
    step: &PipelineStep,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<(), String> {
    let input = resolve(step.target_file.as_ref(), ctx, state);
    let output = resolve(step.output_file.as_ref(), ctx, state);
    let degrees_str = resolve(step.degrees.as_ref(), ctx, state);

    let img = load_image(&input)?;
    let rotated = match degrees_str.as_str() {
        "90" => img.rotate90(),
        "180" => img.rotate180(),
        "270" => img.rotate270(),
        _ => return Err(format!("Unsupported rotation degrees: {}", degrees_str)),
    };

    save_image(&rotated, &output, step.format.as_deref())
}

/// Adjust brightness and/or contrast of an image.
fn action_adjust_image(
    step: &PipelineStep,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<(), String> {
    let input = resolve(step.target_file.as_ref(), ctx, state);
    let output = resolve(step.output_file.as_ref(), ctx, state);

    let mut img = load_image(&input)?;

    // Apply brightness adjustment if present
    if let Some(b_str) = &step.brightness {
        let b_resolved = format_str_with_state(b_str, ctx, state);
        let brightness: i32 = b_resolved
            .parse()
            .map_err(|_| format!("Invalid brightness: '{}'", b_resolved))?;
        img = DynamicImage::ImageRgba8(image::imageops::brighten(&img, brightness));
    }

    // Apply contrast adjustment if present
    if let Some(c_str) = &step.contrast {
        let c_resolved = format_str_with_state(c_str, ctx, state);
        let contrast: f32 = c_resolved
            .parse()
            .map_err(|_| format!("Invalid contrast: '{}'", c_resolved))?;
        img = DynamicImage::ImageRgba8(image::imageops::contrast(&img, contrast));
    }

    save_image(&img, &output, step.format.as_deref())
}

/// Convert an image to a different format (determined by output extension or explicit format field).
fn action_convert_format(
    step: &PipelineStep,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<(), String> {
    let input = resolve(step.target_file.as_ref(), ctx, state);
    let output = resolve(step.output_file.as_ref(), ctx, state);

    let img = load_image(&input)?;
    // Infer format from extension if not specified
    let fmt = step
        .format
        .as_deref()
        .or_else(|| Path::new(&output).extension().and_then(|e| e.to_str()));
    save_image(&img, &output, fmt)
}

/// Create one or more directories.
fn action_create_directories(
    step: &PipelineStep,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<(), String> {
    if let Some(paths) = &step.paths {
        for p in paths {
            let resolved = format_str_with_state(p, ctx, state);
            fs::create_dir_all(&resolved)
                .map_err(|e| format!("Failed to create dir '{}': {}", resolved, e))?;
        }
    }
    if let Some(target_dir) = &step.target_dir {
        let resolved = format_str_with_state(target_dir, ctx, state);
        fs::create_dir_all(&resolved)
            .map_err(|e| format!("Failed to create dir '{}': {}", resolved, e))?;
    }
    Ok(())
}

// ===== Legacy file actions (ported to PipelineStep) =====

fn action_copy(
    step: &PipelineStep,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<(), String> {
    let src = resolve(step.target_file.as_ref(), ctx, state);
    let mut dest = resolve(step.output_file.as_ref(), ctx, state);
    if dest.is_empty() {
        dest = resolve(step.destination.as_ref(), ctx, state);
    }
    let src = if src.is_empty() {
        ctx.file_path.clone()
    } else {
        src
    };
    let dest = if dest.is_empty() {
        ctx.output_path.clone()
    } else {
        dest
    };

    // Copying a file onto itself truncates it to nothing before reading it.
    if let (Ok(a), Ok(b)) = (fs::canonicalize(&src), fs::canonicalize(&dest))
        && a == b
    {
        return Err(format!(
            "Copy failed: '{}' is the input file itself; choose another folder or name.",
            dest
        ));
    }
    if let Some(parent) = Path::new(&dest).parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::copy(&src, &dest).map_err(|e| format!("Copy failed: {}", e))?;
    Ok(())
}

fn action_regex_copy(
    step: &PipelineStep,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<(), String> {
    let mut filename = ctx.file_name.clone();

    if let Some(rules) = &step.rules {
        for rule in rules {
            let re = regex::Regex::new(&rule.pattern)
                .map_err(|e| format!("Invalid regex pattern '{}': {}", rule.pattern, e))?;
            let rep = format_str_with_state(&rule.replacement, ctx, state);
            filename = re.replace_all(&filename, rep.as_str()).to_string();
        }
    }

    let src = ctx.file_path.clone();
    let dest = Path::new(&ctx.output_dir).join(&filename);

    if let Some(parent) = dest.parent() {
        let _ = fs::create_dir_all(parent);
    }

    fs::copy(&src, &dest).map_err(|e| format!("Copy failed: {}", e))?;
    Ok(())
}

fn action_delete(
    step: &PipelineStep,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<(), String> {
    let target = resolve(step.target_file.as_ref(), ctx, state);
    let target = if target.is_empty() {
        ctx.file_path.clone()
    } else {
        target
    };
    let path = Path::new(&target);
    if path.exists() {
        if path.is_dir() {
            fs::remove_dir_all(path).map_err(|e| format!("Delete dir failed: {}", e))?;
        } else {
            fs::remove_file(path).map_err(|e| format!("Delete file failed: {}", e))?;
        }
    }
    Ok(())
}

fn action_echo(
    step: &PipelineStep,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<(), String> {
    let msg = resolve(step.value.as_ref(), ctx, state);
    println!("{}", msg);
    Ok(())
}

fn action_checksum(
    step: &PipelineStep,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<(), String> {
    let input = resolve(step.target_file.as_ref(), ctx, state);
    let input = if input.is_empty() {
        ctx.file_path.clone()
    } else {
        input
    };
    let mut output = resolve(step.output_file.as_ref(), ctx, state);
    if output.is_empty() {
        output = resolve(step.destination.as_ref(), ctx, state);
    }
    if output.is_empty() {
        output = format!("{}.sha256", input);
    }
    let algorithm = resolve(step.algorithm.as_ref(), ctx, state).to_lowercase();
    if !matches!(algorithm.as_str(), "" | "sha256" | "sha-256") {
        return Err(format!(
            "checksum: algorithm '{}' is not supported (use sha256)",
            algorithm
        ));
    }

    let mut file =
        File::open(&input).map_err(|e| format!("Cannot open '{}' for checksum: {}", input, e))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|e| format!("Read error: {}", e))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let hash = hex::encode(hasher.finalize());
    let fname = Path::new(&input)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let content = format!("{}  {}\n", hash, fname);

    if let Some(parent) = Path::new(&output).parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&output, content).map_err(|e| format!("Write checksum failed: {}", e))
}

fn action_split_bytes(
    step: &PipelineStep,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<(), String> {
    let mut input = resolve(step.target_file.as_ref(), ctx, state);
    if input.is_empty() {
        input = ctx.file_path.clone();
    }
    let chunk_size = resolve(step.chunk_size.as_ref(), ctx, state);
    let output_pattern = resolve(step.output_pattern.as_ref(), ctx, state);

    if chunk_size.is_empty() {
        return Err("split_bytes requires chunk_size".to_string());
    }
    if output_pattern.is_empty() {
        return Err("split_bytes requires output_pattern".to_string());
    }
    let chunk =
        crate::exec::parse_size(&chunk_size).map_err(|e| format!("split_bytes: {}", e))? as usize;

    if let Some(parent) = Path::new(&output_pattern).parent() {
        let _ = fs::create_dir_all(parent);
    }

    let mut reader = BufReader::new(
        File::open(&input).map_err(|e| format!("split_bytes: cannot open '{}': {}", input, e))?,
    );
    let mut buffer = vec![0u8; 1024 * 1024];
    let mut part = 0usize;
    let mut written_in_part = 0usize;
    let mut out: Option<File> = None;

    loop {
        let want = buffer.len().min(chunk - written_in_part);
        let count = reader
            .read(&mut buffer[..want])
            .map_err(|e| format!("split_bytes: read error: {}", e))?;
        if count == 0 {
            break;
        }
        if out.is_none() {
            let dest = format!("{}{:02}", output_pattern, part);
            out = Some(
                File::create(&dest)
                    .map_err(|e| format!("split_bytes: cannot create '{}': {}", dest, e))?,
            );
        }
        out.as_mut()
            .unwrap()
            .write_all(&buffer[..count])
            .map_err(|e| format!("split_bytes: write error: {}", e))?;
        written_in_part += count;
        if written_in_part >= chunk {
            out = None;
            written_in_part = 0;
            part += 1;
        }
    }

    if part == 0 && out.is_none() {
        return Err(format!("split_bytes: '{}' is empty", input));
    }
    Ok(())
}

fn action_write_text(
    step: &PipelineStep,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<(), String> {
    let target = resolve(step.target_path.as_ref(), ctx, state);
    if target.is_empty() {
        return Err("write_text requires target_path".to_string());
    }

    let text_content = step.text.as_ref().or(step.text_content.as_ref());
    let content = resolve(text_content, ctx, state);

    if let Some(parent) = Path::new(&target).parent() {
        let _ = fs::create_dir_all(parent);
    }

    let content = content.replace("\\n", "\n");
    fs::write(&target, content)
        .map_err(|e| format!("Failed to write text to {}: {}", target, e))?;

    if step.executable.unwrap_or(false) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(mut perms) = fs::metadata(&target).map(|m| m.permissions()) {
                perms.set_mode(perms.mode() | 0o111);
                let _ = fs::set_permissions(&target, perms);
            }
        }
    }

    Ok(())
}

fn action_join_bytes(
    step: &PipelineStep,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<(), String> {
    let mut input = resolve(step.target_file.as_ref(), ctx, state);
    if input.is_empty() {
        input = ctx.file_path.clone();
    }

    // "movie.mkv.part03" -> base "movie.mkv", so every sibling part is found.
    let base_name = match input.find(".part") {
        Some(idx) => input[..idx].to_string(),
        None => input.clone(),
    };

    let mut dest = resolve(step.output_file.as_ref(), ctx, state);
    if dest.is_empty() {
        dest = ctx.output_path.clone();
    }
    if dest.is_empty() {
        dest = base_name.clone();
    }

    let base_path = Path::new(&base_name);
    let dir = base_path.parent().unwrap_or(Path::new("."));
    let prefix = format!(
        "{}.part",
        base_path
            .file_name()
            .ok_or_else(|| format!("join_bytes: cannot determine part name from '{}'", input))?
            .to_string_lossy()
    );

    let mut parts: Vec<std::path::PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("join_bytes: cannot read '{}': {}", dir.display(), e))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .map(|n| n.to_string_lossy().starts_with(&prefix))
                    .unwrap_or(false)
        })
        .collect();

    if parts.is_empty() {
        return Err(format!(
            "join_bytes: no '{}*' files found next to '{}'",
            prefix, input
        ));
    }

    // Natural order, so .part2 comes before .part10 regardless of padding.
    parts.sort_by(|a, b| {
        natord::compare(
            &a.file_name().unwrap().to_string_lossy(),
            &b.file_name().unwrap().to_string_lossy(),
        )
    });

    if let Some(parent) = Path::new(&dest).parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut out =
        File::create(&dest).map_err(|e| format!("join_bytes: cannot create '{}': {}", dest, e))?;
    for part in &parts {
        let mut f = File::open(part)
            .map_err(|e| format!("join_bytes: cannot open '{}': {}", part.display(), e))?;
        std::io::copy(&mut f, &mut out).map_err(|e| format!("join_bytes: write error: {}", e))?;
    }
    Ok(())
}

fn action_split_csv(
    step: &PipelineStep,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<(), String> {
    let mut input = resolve(step.target_file.as_ref(), ctx, state);
    if input.is_empty() {
        input = ctx.file_path.clone();
    }

    let chunk_size = resolve(step.chunk_size.as_ref(), ctx, state);
    let chunk_size: usize = chunk_size
        .parse()
        .map_err(|_| "Invalid chunk_size".to_string())?;

    let output_pattern = resolve(step.output_pattern.as_ref(), ctx, state);
    if output_pattern.is_empty() {
        return Err("split_csv requires output_pattern".to_string());
    }

    if let Some(parent) = Path::new(&output_pattern).parent() {
        let _ = fs::create_dir_all(parent);
    }

    let file = File::open(&input).map_err(|e| format!("Failed to open {}: {}", input, e))?;
    let mut reader = BufReader::new(file);

    let mut header = String::new();
    if reader.read_line(&mut header).is_err() || header.is_empty() {
        return Ok(());
    }

    let mut part = 0;
    let mut lines_in_chunk = 0;
    let mut current_out: Option<File> = None;

    for line in reader.lines() {
        let line = line.map_err(|e| format!("Read error: {}", e))?;
        if lines_in_chunk == 0 || current_out.is_none() {
            let dest = format!("{}{:03}.csv", output_pattern, part);
            let mut f =
                File::create(&dest).map_err(|e| format!("Failed to create {}: {}", dest, e))?;
            f.write_all(header.as_bytes())
                .map_err(|e| format!("Write error: {}", e))?;
            current_out = Some(f);
            part += 1;
        }

        if let Some(ref mut f) = current_out {
            writeln!(f, "{}", line).map_err(|e| format!("Write error: {}", e))?;
        }

        lines_in_chunk += 1;
        if lines_in_chunk >= chunk_size {
            lines_in_chunk = 0;
        }
    }

    Ok(())
}

fn action_join_csv(
    step: &PipelineStep,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<(), String> {
    let mut input = resolve(step.target_file.as_ref(), ctx, state);
    if input.is_empty() {
        input = ctx.file_path.clone();
    }

    let prefix = if input.contains("_part_") {
        input.split("_part_").next().unwrap().to_string()
    } else {
        input.clone()
    };

    let mut dest = resolve(step.output_file.as_ref(), ctx, state);
    if dest.is_empty() {
        dest = ctx.output_path.clone();
    }
    if dest.is_empty() {
        dest = format!("{}_joined.csv", prefix);
    }

    if let Some(parent) = Path::new(&dest).parent() {
        let _ = fs::create_dir_all(parent);
    }

    let mut out_f = File::create(&dest).map_err(|e| format!("Create failed: {}", e))?;
    let mut header_written = false;

    let mut part = 0;
    loop {
        let part_file = format!("{}_part_{:03}.csv", prefix, part);
        if !Path::new(&part_file).exists() {
            break;
        }

        let file = File::open(&part_file).map_err(|e| format!("Open failed: {}", e))?;
        let mut reader = BufReader::new(file);

        let mut first_line = String::new();
        if reader.read_line(&mut first_line).is_ok() && !first_line.is_empty() && !header_written {
            out_f.write_all(first_line.as_bytes()).unwrap();
            header_written = true;
        }

        std::io::copy(&mut reader, &mut out_f).map_err(|e| format!("Copy failed: {}", e))?;
        part += 1;
    }
    Ok(())
}

fn action_external_tool(
    tool_name: &str,
    step: &PipelineStep,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<(), String> {
    let args_val = step.args.as_ref().or(step.value.as_ref());
    let template = match args_val {
        Some(t) if !t.trim().is_empty() => t,
        _ => {
            return Err(format!(
                "{}: no arguments (value or args) provided",
                tool_name
            ));
        }
    };

    let argv = crate::exec::build_argv(template, ctx, state)
        .map_err(|e| format!("{}: {}", tool_name, e))?;
    crate::exec::run_program(tool_name, &argv)
}

/// Run any program with an explicit argument list. No shell, no quoting rules.
fn action_exec(
    step: &PipelineStep,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<(), String> {
    let program = resolve(step.program.as_ref(), ctx, state);
    if program.is_empty() {
        return Err("exec requires a 'program' (e.g. program = \"ffmpeg\")".to_string());
    }

    let argv: Vec<String> = match (&step.argv, &step.args) {
        (Some(list), _) => list
            .iter()
            .map(|a| format_str_with_state(a, ctx, state))
            .collect(),
        // Convenience: allow the string form too, tokenized the same safe way.
        (None, Some(template)) => {
            crate::exec::build_argv(template, ctx, state).map_err(|e| format!("exec: {}", e))?
        }
        (None, None) => Vec::new(),
    };

    crate::exec::run_program(&program, &argv)
}

// ---------------------------------------------------------------------------
//  Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_ctx() -> FileContext {
        FileContext {
            file_path: "/tmp/test.png".to_string(),
            file_name: "test.png".to_string(),
            file_stem: "test".to_string(),
            file_ext: "png".to_string(),
            file_dir: "/tmp".to_string(),
            file_mime: "image/png".to_string(),
            file_size: "1024".to_string(),
            output_dir: "/tmp/out".to_string(),
            output_path: "/tmp/out/test.png".to_string(),
            variables: HashMap::new(),
        }
    }

    #[test]
    fn test_format_str() {
        let ctx = dummy_ctx();
        let result = format_str("{file.stem}.{file.ext}", &ctx);
        assert_eq!(result, "test.png");
    }

    #[test]
    fn test_evaluate_condition_empty() {
        let ctx = dummy_ctx();
        assert!(evaluate_condition("", &ctx));
    }

    #[test]
    fn test_evaluate_condition_numeric_and_strings() {
        let ctx = dummy_ctx();
        assert!(evaluate_condition("{file.size} > 100", &ctx));
        assert!(!evaluate_condition("{file.size} > 40000", &ctx));
        assert!(evaluate_condition(
            "'{file.mime}'.startswith('image/')",
            &ctx
        ));
        assert!(evaluate_condition(
            "'{file.ext}' in ['png', 'jpg'] and {file.size} >= 1024",
            &ctx
        ));
    }

    #[test]
    fn test_condition_can_use_ui_state() {
        let ctx = dummy_ctx();
        let mut state = HashMap::new();
        state.insert("quality".to_string(), "85".to_string());
        assert_eq!(
            evaluate_condition_with_state("{quality} >= 80", &ctx, &state),
            Ok(true)
        );
    }

    #[test]
    fn test_invalid_condition_fails_pipeline_with_step_number() {
        let ctx = dummy_ctx();
        let step: PipelineStep = toml::from_str(
            "step = 4\naction = \"echo\"\nvalue = \"hi\"\ncondition = \"{file.size} >\"\n",
        )
        .unwrap();
        let err = execute_pipeline(&[step], &ctx, &HashMap::new()).unwrap_err();
        assert!(err.starts_with("step 4: invalid condition"), "{err}");
    }

    #[test]
    fn test_format_str_with_state() {
        let ctx = dummy_ctx();
        let mut state = HashMap::new();
        state.insert("quality_slider".to_string(), "85".to_string());
        let result = format_str_with_state("quality={quality_slider}", &ctx, &state);
        assert_eq!(result, "quality=85");
    }

    #[test]
    fn export_image_crops_covers_and_fits_the_size_budget() {
        let dir = std::env::temp_dir().join("fm_export_image");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let input = dir.join("in.png");
        // Noise compresses badly, so the budget actually has to bite.
        let noisy = image::RgbImage::from_fn(400, 300, |x, y| {
            let v = (x.wrapping_mul(2_654_435_761) ^ y.wrapping_mul(40_503)) as u8;
            image::Rgb([v, v.wrapping_mul(7), v.wrapping_mul(13)])
        });
        noisy.save(&input).unwrap();
        let output = dir.join("out.jpg");
        let ctx = FileContext::from_paths(&input, &output, "image/png");

        let step: PipelineStep = toml::from_str(
            r#"
            step = 1
            action = "export_image"
            target_file = "{file.path}"
            output_file = "{output.path}"
            crop_data = "{crop}"
            width = "{w}"
            height = "{h}"
            fit = "{fit}"
            max_kb = "{kb}"
            "#,
        )
        .unwrap();
        let state: HashMap<String, String> = [
            ("crop", "200x200+50+50"),
            ("w", "120"),
            ("h", "80"),
            ("fit", "Cover"),
            ("kb", "6"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        execute_pipeline(&[step], &ctx, &state).unwrap();

        let size = fs::metadata(&output).unwrap().len();
        assert!(size <= 6 * 1024, "{size} bytes is over the 6 KiB budget");
        let out = image::open(&output).unwrap();
        assert_eq!((out.width(), out.height()), (120, 80));
    }

    #[test]
    fn copy_refuses_to_overwrite_its_own_input() {
        let dir = std::env::temp_dir().join("fm_copy_self");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("keep.txt");
        fs::write(&file, "precious").unwrap();
        let ctx = FileContext::from_paths(&file, &file, "text/plain");
        let step = PipelineStep {
            step: 1,
            action: "copy".into(),
            destination: Some("{output.path}".into()),
            ..Default::default()
        };
        assert!(execute_pipeline(&[step], &ctx, &HashMap::new()).is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "precious");
    }

    #[test]
    fn run_on_bytes_returns_the_output_and_keeps_names_inside_work() {
        let work = std::env::temp_dir().join("fm_run_on_bytes");
        let mut png = Vec::new();
        DynamicImage::ImageRgb8(image::RgbImage::from_pixel(8, 6, image::Rgb([200, 30, 30])))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let step = PipelineStep {
            step: 1,
            action: "export_image".into(),
            target_file: Some("{file.path}".into()),
            output_file: Some("{output.path}".into()),
            ..Default::default()
        };
        let out = run_on_bytes(
            &[step],
            "../../evil.png",
            &png,
            "/etc/result.jpg",
            &HashMap::new(),
            &work,
        )
        .unwrap();
        assert_eq!(image::guess_format(&out).unwrap(), image::ImageFormat::Jpeg);
        assert!(work.join("in/evil.png").exists());
        assert!(work.join("out/result.jpg").exists());
        assert_eq!(safe_file_name(""), "file");
    }
}
