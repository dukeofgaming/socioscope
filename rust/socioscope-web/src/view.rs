//! HTML for the shell. Class names stay whole so the stylesheet can see them.

use crate::okf::{Concept, Summary};
use crate::record::{Live, Phase};
use crate::srt::{self, Cue};

pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub struct Shell<'a> {
    pub document_title: &'a str,
    pub tab: &'a str,
    pub package_href: &'a str,
    pub library: String,
    pub stage: String,
    pub package: String,
    pub library_mobile: bool,
    pub stage_mobile: bool,
    pub package_mobile: bool,
}

pub fn document(shell: &Shell) -> String {
    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<meta name="color-scheme" content="light dark">
<meta name="theme-color" content="#f4efe6">
<title>{title}</title>
<link rel="manifest" href="/manifest.webmanifest">
<link rel="icon" href="/icon.svg" type="image/svg+xml">
<link rel="stylesheet" href="/app.css">
<script>
window.socioscopeTheme = function (mode, persist) {{
  var dark = mode === "dark";
  document.documentElement.classList.toggle("dark", dark);
  if (persist) {{
    try {{ localStorage.setItem("socioscope-theme", mode); }} catch (e) {{}}
  }}
  var meta = document.querySelector('meta[name="theme-color"]');
  if (meta) meta.setAttribute("content", dark ? "#12110e" : "#f4efe6");
  var btn = document.getElementById("theme");
  if (!btn) return;
  btn.setAttribute("aria-pressed", dark ? "true" : "false");
  btn.setAttribute("aria-label", dark ? "Switch to light mode" : "Switch to dark mode");
  var label = document.getElementById("theme-label");
  if (label) label.textContent = dark ? "Light" : "Dark";
}};
(function () {{
  var saved = null;
  try {{ saved = localStorage.getItem("socioscope-theme"); }} catch (e) {{}}
  var dark = saved === "dark" || (saved !== "light" && matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.classList.toggle("dark", dark);
  var meta = document.querySelector('meta[name="theme-color"]');
  if (meta) meta.setAttribute("content", dark ? "#12110e" : "#f4efe6");
}})();
</script>
<script src="/htmx.min.js"></script>
<script src="/app.js" defer></script>
</head>
<body class="bg-paper text-ink antialiased selection:bg-copper/20">
<div class="app-shell has-package mx-auto w-full max-w-6xl">
<header class="app-header sticky top-0 z-20 flex items-center gap-3 border-b border-line bg-paper/90 px-3 py-2 backdrop-blur sm:px-5">
<a href="/" class="flex min-w-0 items-baseline gap-2">
<span class="font-serif text-xl tracking-tight sm:text-2xl">socioscope</span>
<span class="hidden text-[11px] uppercase tracking-[0.18em] text-muted min-[420px]:inline">field notes</span>
</a>
<div class="ml-auto flex items-center gap-2">
<a href="/record" class="hidden rounded-full bg-copper px-3 py-1.5 text-sm font-medium text-on-copper hover:opacity-90 sm:inline">Record</a>
<button type="button" id="theme" class="inline-flex items-center gap-1.5 rounded-full border border-line px-2.5 py-1.5 text-sm text-muted" aria-pressed="false" aria-label="Switch to dark mode">
<span class="theme-moon" aria-hidden="true"><svg class="theme-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M20 14.5A7.5 7.5 0 0 1 9.5 4 6.5 6.5 0 1 0 20 14.5z"/></svg></span>
<span class="theme-sun" aria-hidden="true"><svg class="theme-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><circle cx="12" cy="12" r="3.5"/><path d="M12 3v2M12 19v2M3 12h2M19 12h2M5.6 5.6l1.4 1.4M17 17l1.4 1.4M18.4 5.6 17 7M7 17l-1.4 1.4"/></svg></span>
<span id="theme-label" class="hidden min-[420px]:inline">Dark</span>
</button>
</div>
</header>
<aside id="library" class="{library_class} border-line lg:border-r">{library}</aside>
<main id="stage" class="{stage_class} min-w-0 px-3 py-4 pb-24 sm:px-6 lg:pb-8">{stage}</main>
<aside id="package" class="{package_class} border-line lg:border-l">{package}</aside>
</div>
<nav class="fixed inset-x-0 bottom-0 z-30 border-t border-line bg-paper/95 backdrop-blur lg:hidden" style="padding-bottom: env(safe-area-inset-bottom)">
<ul class="mx-auto grid max-w-lg grid-cols-3">{tabs}</ul>
</nav>
</body>
</html>"##,
        title = escape(shell.document_title),
        library_class = mobile(shell.library_mobile),
        stage_class = mobile(shell.stage_mobile),
        package_class = mobile(shell.package_mobile),
        library = shell.library,
        stage = shell.stage,
        package = shell.package,
        tabs = tabs(shell.tab, shell.package_href),
    )
}

pub fn stage_swap(stage: &str, items: &str, package: &str) -> String {
    format!(
        r#"{stage}<div id="library-list" hx-swap-oob="innerHTML">{items}</div><div id="package" class="max-lg:hidden border-line lg:border-l" hx-swap-oob="true">{package}</div>"#
    )
}

pub fn library(items: &[Summary], query: &str) -> String {
    format!(
        r##"<div class="px-3 py-3">
<label class="block text-[11px] uppercase tracking-[0.16em] text-muted" for="q">Packages</label>
<input id="q" name="q" value="{query}" placeholder="Search titles" autocomplete="off" hx-get="/library" hx-trigger="keyup changed delay:200ms" hx-target="#library-list" hx-swap="innerHTML" class="mt-2 w-full rounded-lg border border-line bg-panel px-3 py-2 text-sm outline-none focus:border-copper">
</div>
<div id="library-list">{list}</div>"##,
        query = escape(query),
        list = library_items(items),
    )
}

pub fn library_items(items: &[Summary]) -> String {
    if items.is_empty() {
        return r#"<p class="px-3 py-6 text-sm text-muted">No packages yet. A recording or an opened file becomes a bundle you can read back.</p>"#.to_string();
    }
    items.iter().map(library_item).collect()
}

fn library_item(item: &Summary) -> String {
    let when = if item.recorded_at.is_empty() {
        String::new()
    } else {
        crate::timeutil::format_chicago(&item.recorded_at)
    };
    let kind = if item.kind == "emulated" {
        "Emulated tape"
    } else {
        "Opened file"
    };
    format!(
        r#"<a href="/bundles/{id}" class="block border-t border-line px-3 py-3 hover:bg-panel">
<span class="flex items-baseline justify-between gap-2">
<span class="truncate font-serif text-lg leading-tight">{title}</span>
<span class="shrink-0 text-[10px] uppercase tracking-wider text-muted">{status}</span>
</span>
<span class="mt-1 line-clamp-2 block text-sm text-muted">{description}</span>
<span class="mt-1 block text-xs text-muted">{when} · {kind}</span>
</a>"#,
        id = escape(&item.id),
        title = escape(&item.title),
        status = escape(&item.status),
        description = escape(&item.description),
        when = escape(&when),
    )
}

pub fn home_stage(full_tape: bool) -> String {
    format!(
        r#"<section class="mx-auto flex max-w-xl flex-col gap-6">
<p class="text-[11px] uppercase tracking-[0.18em] text-copper">Desk</p>
<h1 class="font-serif text-4xl leading-none tracking-tight sm:text-5xl">Keep the take.</h1>
<p class="max-w-md text-sm leading-relaxed text-muted sm:text-base">Record from the debate tape, or open an audio file. Each session is stored as an OKF package: the original audio, the transcript, and the notes around them.</p>
<div class="flex flex-wrap items-center gap-3">
<a href="/record" class="rounded-full bg-copper px-4 py-2 text-sm font-medium text-on-copper">Record</a>
{open}
</div>
{path}
<p class="text-xs text-muted">{tape}</p>
</section>"#,
        open = open_file_form(),
        path = path_form(),
        tape = if full_tape {
            "Full debate tape is on this machine."
        } else {
            "Opening tape is built in. Add the archive file to unlock the 16 minute debate."
        }
    )
}

pub fn record_stage(full_tape: bool) -> String {
    let full = if full_tape {
        r#"<button name="source" value="debate" class="rounded-2xl border border-line bg-panel p-4 text-left hover:border-copper">
<span class="block text-[11px] uppercase tracking-[0.16em] text-muted">Full debate · 16 min</span>
<span class="mt-1 block font-serif text-2xl leading-tight">Smith, Kennedy, Nixon</span>
<span class="mt-2 block text-sm text-muted">The archive.org tape. The prepared alignment covers the opening; the rest is kept for a later streaming pass.</span>
</button>"#
    } else {
        r#"<p class="rounded-2xl border border-dashed border-line p-4 text-sm text-muted">The 16 minute tape is not on this machine. The opening still is.</p>"#
    };
    format!(
        r##"<section class="mx-auto flex max-w-xl flex-col gap-5">
<p class="text-[11px] uppercase tracking-[0.18em] text-copper">Emulated microphone</p>
<h1 class="font-serif text-4xl leading-none tracking-tight">The debate, at 1×.</h1>
<p class="text-sm leading-relaxed text-muted">Nothing is read from a handset. The input stream is the first Kennedy–Nixon debate, and the samples that arrive are the original recording.</p>
<form method="post" action="/recordings" hx-post="/recordings" hx-target="#stage" hx-swap="innerHTML" class="grid gap-3">
<button name="source" value="opening" class="rounded-2xl border border-line bg-panel p-4 text-left hover:border-copper">
<span class="flex items-center justify-between gap-3">
<span>
<span class="block text-[11px] uppercase tracking-[0.16em] text-muted">Opening · 1:20</span>
<span class="mt-1 block font-serif text-2xl leading-tight">Smith, then Kennedy</span>
</span>
<span class="record-btn shrink-0" aria-hidden="true">Rec</span>
</span>
<span class="mt-2 block text-sm text-muted">Howard K. Smith introduces Senator Kennedy. Cues follow the tape.</span>
</button>
{full}
</form>
</section>"##
    )
}

pub fn recording_stage(live: &Live) -> String {
    format!(
        r#"<section class="mx-auto flex max-w-xl flex-col gap-5">
<div class="flex items-end justify-between gap-3">
<div>
<p class="text-[11px] uppercase tracking-[0.18em] text-copper">Recording</p>
<h1 class="font-serif text-3xl leading-tight">{title}</h1>
</div>
<p id="clock" class="font-serif text-3xl tabular-nums">{elapsed}<span class="text-base text-muted"> / {total}</span></p>
</div>
<p class="text-sm text-muted">{label} · input at 1×</p>
<form method="post" action="/recordings/{id}/stop">
<button class="record-btn" data-state="live" type="submit">Stop</button>
</form>
{status}
</section>"#,
        title = escape(&live.title),
        elapsed = srt::clock(live.elapsed_ms),
        total = srt::clock(live.duration_ms),
        label = escape(&live.source_label),
        id = escape(&live.id),
        status = live_status(live),
    )
}

pub fn live_poll(live: &Live) -> String {
    format!(
        r#"<p id="clock" hx-swap-oob="true" class="font-serif text-3xl tabular-nums">{elapsed}<span class="text-base text-muted"> / {total}</span></p>{status}"#,
        elapsed = srt::clock(live.elapsed_ms),
        total = srt::clock(live.duration_ms),
        status = live_status(live),
    )
}

pub fn live_status(live: &Live) -> String {
    let poll = if matches!(live.phase, Phase::Preparing | Phase::Recording) {
        format!(
            r#" hx-get="/recordings/{}" hx-trigger="every 400ms" hx-swap="outerHTML""#,
            escape(&live.id)
        )
    } else {
        String::new()
    };
    let body = match live.phase {
        Phase::Preparing => {
            r#"<p class="text-sm text-muted">Opening the tape…</p>"#.to_string()
        }
        Phase::Failed => format!(
            r#"<p class="text-sm text-copper" role="alert">{}</p>"#,
            escape(&live.error)
        ),
        Phase::Recording | Phase::Done => {
            let cues = srt::visible(&live.cues, live.elapsed_ms);
            let latest = cues.last().map(|cue| cue.text.as_str()).unwrap_or("");
            let waiting = if live.phase == Phase::Recording
                && cues.last().is_some_and(|cue| live.elapsed_ms > cue.end_ms + 1500)
            {
                r#"<p class="text-sm text-muted">The prepared alignment ends here. The rest of the take is still being stored.</p>"#
            } else if cues.is_empty() {
                r#"<p class="text-sm text-muted">Listening.</p>"#
            } else {
                ""
            };
            format!(
                r#"<div class="meter" aria-hidden="true">{bars}</div>
<p class="sr-only" aria-live="polite">{elapsed}. {latest}</p>
<div id="cues" class="max-h-[50dvh] space-y-4 overflow-y-auto pr-1">{turns}</div>
{waiting}"#,
                bars = meter_bars(&tail(&live.peaks, 42), false, 0),
                elapsed = escape(&srt::clock(live.elapsed_ms)),
                latest = escape(latest),
                turns = turns(&cues, Some(live.elapsed_ms)),
                waiting = waiting,
            )
        }
    };
    format!(r#"<div id="live-status"{poll}>{body}</div>"#)
}

pub fn session_stage(summary: &Summary, cues: &[Cue], peaks: &[f32], notice: &str) -> String {
    let banner = if summary.analysis == "queued" || summary.analysis == "running" {
        format!(
            r#"<div id="analysis" hx-get="/bundles/{}/analysis" hx-trigger="every 1s" hx-swap="outerHTML" class="rounded-xl border border-line bg-panel px-3 py-2 text-sm">Reading the take…</div>"#,
            escape(&summary.id)
        )
    } else if summary.analysis == "error" {
        format!(
            r#"<p id="analysis" class="text-sm text-copper" role="alert">{}</p>"#,
            escape(notice)
        )
    } else {
        String::new()
    };
    format!(
        r#"<section class="mx-auto flex max-w-2xl flex-col gap-4">
<p class="text-[11px] uppercase tracking-[0.18em] text-moss">{kind}</p>
<h1 class="font-serif text-3xl leading-tight sm:text-4xl">{title}</h1>
<p class="text-sm text-muted">{description}</p>
<audio id="take" class="w-full" controls preload="metadata" src="/bundles/{id}/media"></audio>
<div class="meter" aria-hidden="true">{bars}</div>
{banner}
<div id="cues" class="max-h-[60dvh] space-y-5 overflow-y-auto">{turns}</div>
</section>"#,
        kind = if summary.kind == "emulated" { "Emulated tape" } else { "Opened file" },
        title = escape(&summary.title),
        description = escape(&summary.description),
        id = escape(&summary.id),
        bars = meter_bars(&downsample(peaks, 64), true, summary_duration(cues, peaks)),
        turns = turns(cues, None),
    )
}

pub fn analysis_done(cues: &[Cue]) -> String {
    format!(
        r#"<p id="analysis" class="text-sm text-moss">Transcript is in the package.</p>
<div id="cues" hx-swap-oob="outerHTML" class="max-h-[60dvh] space-y-5 overflow-y-auto">{turns}</div>"#,
        turns = turns(cues, None),
    )
}

pub fn concept_stage(id: &str, concept: &Concept) -> String {
    let body = render_markdown(id, &concept.body);
    let matter = if concept.frontmatter.is_empty() {
        String::new()
    } else {
        format!(
            r#"<pre class="overflow-x-auto rounded-xl bg-panel p-3 text-xs leading-relaxed text-muted">{front}</pre>"#,
            front = escape(&concept.frontmatter)
        )
    };
    format!(
        r#"<article class="mx-auto flex max-w-2xl flex-col gap-4">
<p class="text-[11px] uppercase tracking-[0.18em] text-muted">{type_name}</p>
<h1 class="font-serif text-3xl leading-tight">{title}</h1>
<p class="text-xs text-muted">{path}</p>
<p class="text-sm text-muted">{description}</p>
{matter}
<div class="space-y-3 text-base leading-relaxed">{body}</div>
<details class="rounded-xl border border-line p-3 text-sm">
<summary class="cursor-pointer text-muted">OKF source</summary>
<pre class="mt-3 overflow-x-auto whitespace-pre-wrap text-xs leading-relaxed">{raw}</pre>
</details>
<p class="text-sm"><a class="text-copper underline decoration-copper/40 underline-offset-4" href="/bundles/{id}">Back to the take</a></p>
</article>"#,
        type_name = escape(&concept.type_name),
        path = escape(&concept.relative),
        title = escape(&concept.title),
        description = escape(&concept.description),
        raw = escape(&concept.raw),
        id = escape(id),
    )
}

pub fn package_panel(id: Option<&str>, title: &str, files: &[(String, String)]) -> String {
    if files.is_empty() {
        return r#"<div class="px-4 py-5">
<p class="text-[11px] uppercase tracking-[0.16em] text-muted">Open Knowledge Format</p>
<p class="mt-2 font-serif text-xl leading-tight">A package is a folder of notes.</p>
<p class="mt-2 text-sm leading-relaxed text-muted">v0.2: markdown, YAML frontmatter, and the original audio beside them. Open a session to walk the files.</p>
</div>"#
            .to_string();
    }
    let id = id.unwrap_or("");
    let links = files
        .iter()
        .map(|(path, type_name)| {
            format!(
                r#"<li><a class="flex items-baseline justify-between gap-3 py-2 text-sm hover:text-copper" href="/bundles/{id}/concepts/{path}"><span class="truncate">{path}</span><span class="shrink-0 text-[10px] uppercase tracking-wider text-muted">{type_name}</span></a></li>"#,
                id = escape(id),
                path = escape(path),
                type_name = escape(type_name),
            )
        })
        .collect::<String>();
    format!(
        r#"<div id="package-body" class="px-4 py-5">
<p class="text-[11px] uppercase tracking-[0.16em] text-muted">OKF package</p>
<h2 class="mt-1 font-serif text-xl leading-tight">{title}</h2>
<ol class="mt-3 divide-y divide-line">{links}</ol>
</div>"#,
        title = escape(title),
    )
}

pub fn alert(message: &str) -> String {
    format!(
        r#"<section class="mx-auto max-w-xl"><p class="rounded-xl border border-line bg-panel px-3 py-3 text-sm text-copper" role="alert">{}</p></section>"#,
        escape(message)
    )
}

fn open_file_form() -> &'static str {
    r##"<form method="post" action="/imports/file" enctype="multipart/form-data" hx-post="/imports/file" hx-target="#stage" hx-swap="innerHTML" hx-encoding="multipart/form-data" class="flex flex-wrap items-center gap-2">
<label class="rounded-full border border-line bg-panel px-4 py-2 text-sm">
<span class="sr-only">Audio file</span>
<input name="audio" type="file" accept=".wav,.mp3,.m4a,audio/*" required data-autosubmit class="max-w-[12rem] text-xs file:mr-2 file:rounded-full file:border-0 file:bg-ink file:px-2 file:py-1 file:text-xs file:text-paper">
</label>
<button class="rounded-full border border-line px-3 py-2 text-sm" type="submit">Open</button>
</form>"##
}

fn path_form() -> &'static str {
    r##"<details class="text-sm">
<summary class="cursor-pointer text-muted">Path on this machine</summary>
<form method="post" action="/imports/path" hx-post="/imports/path" hx-target="#stage" hx-swap="innerHTML" class="mt-2 flex gap-2">
<label class="sr-only" for="path">Path</label>
<input id="path" name="path" placeholder="tests/data/jfk.wav" autocomplete="off" spellcheck="false" class="min-w-0 flex-1 rounded-lg border border-line bg-panel px-3 py-2 text-sm outline-none focus:border-copper">
<button class="rounded-full border border-line px-3 py-2" type="submit">Open</button>
</form>
</details>"##
}

fn tabs(active: &str, package_href: &str) -> String {
    [
        ("library", "/", "Library", "M4 6h16M4 12h16M4 18h10"),
        ("record", "/record", "Record", "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z"),
        ("package", package_href, "Package", "M4 7h16v12H4zM4 7l8-3 8 3"),
    ]
    .into_iter()
    .map(|(id, href, label, path)| {
        let class = if id == active {
            "flex flex-col items-center gap-1 px-2 py-2 text-[11px] font-medium text-copper"
        } else {
            "flex flex-col items-center gap-1 px-2 py-2 text-[11px] font-medium text-muted"
        };
        format!(
            r#"<li><a href="{href}" class="{class}"><svg viewBox="0 0 24 24" class="h-5 w-5" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><path d="{path}"/></svg>{label}</a></li>"#
        )
    })
    .collect()
}

fn mobile(show: bool) -> &'static str {
    if show {
        ""
    } else {
        "max-lg:hidden"
    }
}

fn turns(cues: &[Cue], elapsed: Option<u64>) -> String {
    if cues.is_empty() {
        return String::new();
    }
    let mut html = String::new();
    let mut index = 0usize;
    let mut current = String::new();
    let mut buffer: Vec<&Cue> = Vec::new();
    let flush = |html: &mut String, index: usize, speaker: &str, buffer: &[&Cue], elapsed: Option<u64>| {
        if buffer.is_empty() {
            return;
        }
        let name = if speaker.is_empty() { "Voice" } else { speaker };
        let cues_html = buffer
            .iter()
            .map(|cue| {
                let active = elapsed.is_some_and(|elapsed| cue.start_ms <= elapsed && elapsed < cue.end_ms);
                let class = if active {
                    "cue is-active rounded-md px-2 py-1 font-serif text-lg leading-snug sm:text-xl"
                } else {
                    "cue rounded-md px-2 py-1 font-serif text-lg leading-snug sm:text-xl"
                };
                format!(
                    r#"<p data-start="{start}" data-end="{end}" class="{class}">{text}</p>"#,
                    start = cue.start_ms,
                    end = cue.end_ms,
                    text = escape(&cue.text),
                )
            })
            .collect::<String>();
        html.push_str(&format!(
            r#"<article class="spk spk-{index}">
<header class="flex items-baseline justify-between gap-3">
<h2 class="text-[11px] font-semibold uppercase tracking-[0.16em]" style="color:var(--spk)">{name}</h2>
<time class="text-xs tabular-nums text-muted">{clock}</time>
</header>
<div class="mt-1 space-y-1">{cues_html}</div>
</article>"#,
            index = index % 6,
            name = escape(name),
            clock = srt::clock(buffer[0].start_ms),
        ));
    };
    for cue in cues {
        let speaker = cue.speakers.join(", ");
        if speaker != current && !buffer.is_empty() {
            flush(&mut html, index, &current, &buffer, elapsed);
            index += 1;
            buffer.clear();
        }
        current = speaker;
        buffer.push(cue);
    }
    flush(&mut html, index, &current, &buffer, elapsed);
    html
}

fn meter_bars(peaks: &[f32], seek: bool, duration_ms: u64) -> String {
    if peaks.is_empty() {
        return r#"<i style="height:12%"></i>"#.repeat(12);
    }
    peaks
        .iter()
        .enumerate()
        .map(|(index, peak)| {
            let height = (12.0 + peak.clamp(0.0, 1.0) * 88.0).round() as u32;
            if seek && duration_ms > 0 {
                let at = duration_ms * index as u64 / peaks.len().max(1) as u64;
                format!(
                    r#"<button type="button" data-seek="{at}" style="height:{height}%" aria-label="Seek to {clock}"></button>"#,
                    clock = srt::clock(at),
                )
            } else {
                format!(r#"<i style="height:{height}%"></i>"#)
            }
        })
        .collect()
}

fn tail(peaks: &[f32], n: usize) -> Vec<f32> {
    if peaks.len() <= n {
        peaks.to_vec()
    } else {
        peaks[peaks.len() - n..].to_vec()
    }
}

fn downsample(peaks: &[f32], n: usize) -> Vec<f32> {
    if peaks.is_empty() || peaks.len() <= n {
        return peaks.to_vec();
    }
    (0..n)
        .map(|index| {
            let start = index * peaks.len() / n;
            let end = ((index + 1) * peaks.len() / n).max(start + 1);
            peaks[start..end].iter().copied().fold(0.0_f32, f32::max)
        })
        .collect()
}

fn summary_duration(cues: &[Cue], peaks: &[f32]) -> u64 {
    let from_cues = cues.iter().map(|cue| cue.end_ms).max().unwrap_or(0);
    let from_peaks = peaks.len() as u64 * 100;
    from_cues.max(from_peaks)
}

fn render_markdown(bundle_id: &str, body: &str) -> String {
    let mut html = String::new();
    let mut list: Vec<String> = Vec::new();
    let flush_list = |html: &mut String, list: &mut Vec<String>| {
        if list.is_empty() {
            return;
        }
        html.push_str("<ul class=\"list-disc space-y-1 pl-5\">");
        for item in list.drain(..) {
            html.push_str("<li>");
            html.push_str(&item);
            html.push_str("</li>");
        }
        html.push_str("</ul>");
    };
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush_list(&mut html, &mut list);
            continue;
        }
        if let Some(item) = trimmed.strip_prefix("* ").or_else(|| trimmed.strip_prefix("- ")) {
            list.push(inline(bundle_id, item));
            continue;
        }
        flush_list(&mut html, &mut list);
        if let Some(rest) = trimmed.strip_prefix("# ") {
            html.push_str(&format!(
                "<h2 class=\"font-serif text-2xl\">{}</h2>",
                inline(bundle_id, rest)
            ));
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            html.push_str(&format!(
                "<h3 class=\"font-serif text-xl\">{}</h3>",
                inline(bundle_id, rest)
            ));
        } else if trimmed.starts_with('|') {
            html.push_str(&format!(
                "<p class=\"font-mono text-xs text-muted\">{}</p>",
                escape(trimmed)
            ));
        } else {
            html.push_str(&format!("<p>{}</p>", inline(bundle_id, trimmed)));
        }
    }
    flush_list(&mut html, &mut list);
    html
}

fn inline(bundle_id: &str, text: &str) -> String {
    let mut html = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        html.push_str(&escape(&rest[..start]));
        let Some(label_end) = rest[start + 1..].find("](") else {
            html.push_str(&escape(&rest[start..]));
            return html;
        };
        let label = &rest[start + 1..start + 1 + label_end];
        let url_start = start + 1 + label_end + 2;
        let Some(url_end) = rest[url_start..].find(')') else {
            html.push_str(&escape(&rest[start..]));
            return html;
        };
        let url = &rest[url_start..url_start + url_end];
        let href = if url.starts_with("http://") || url.starts_with("https://") {
            url.to_string()
        } else if let Some(path) = url.strip_prefix('/') {
            format!("/bundles/{bundle_id}/concepts/{path}")
        } else {
            format!("/bundles/{bundle_id}/concepts/{url}")
        };
        html.push_str(&format!(
            r#"<a class="text-copper underline decoration-copper/40 underline-offset-4" href="{}">{}</a>"#,
            escape(&href),
            escape(label)
        ));
        rest = &rest[url_start + url_end + 1..];
    }
    html.push_str(&escape(rest));
    let mut strong = String::new();
    let mut leftover = html.as_str();
    while let Some(start) = leftover.find("**") {
        strong.push_str(&leftover[..start]);
        let after = &leftover[start + 2..];
        if let Some(end) = after.find("**") {
            strong.push_str("<strong>");
            strong.push_str(&after[..end]);
            strong.push_str("</strong>");
            leftover = &after[end + 2..];
        } else {
            strong.push_str(&leftover[start..]);
            leftover = "";
            break;
        }
    }
    strong.push_str(leftover);
    strong
}
