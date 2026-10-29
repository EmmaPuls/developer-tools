use crate::utils::{claude_settings, ui};
use anyhow::{bail, Context, Result};
use console::style;
use dialoguer::Select;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "lowercase")]
pub enum Source {
    Global,
    Synced,
    Project(String),
    Plugin(String),
}

impl Source {
    /// Only skills in the user's own Claude setup can be toggled; repo and plugin skills are owned elsewhere.
    pub fn toggleable(&self) -> bool {
        matches!(self, Source::Global | Source::Synced)
    }

    fn label(&self) -> String {
        match self {
            Source::Global => "global".to_string(),
            Source::Synced => "synced".to_string(),
            Source::Project(repo) => format!("project:{}", repo),
            Source::Plugin(p) => format!("plugin:{}", p),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub source: Source,
    pub state: String,
    pub toggleable: bool,
    /// False when the SKILL.md sets `disable-model-invocation: true`, so only the user can invoke it.
    pub model_invocable: bool,
}

// ---------------------------------------------------------------------------
// Frontmatter
// ---------------------------------------------------------------------------

/// Parse `name` and `description` from SKILL.md YAML frontmatter.
/// Handles plain/quoted scalars and `>` / `|` block scalars; anything else is ignored.
pub fn parse_frontmatter(text: &str) -> (Option<String>, Option<String>) {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (None, None);
    }
    let fm: Vec<&str> = lines.take_while(|l| l.trim() != "---").collect();

    let mut name = None;
    let mut description = None;
    let mut i = 0;
    while i < fm.len() {
        let line = fm[i];
        i += 1;
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        let parsed = if matches!(value, ">" | ">-" | "|" | "|-" | ">+" | "|+") {
            let mut parts = Vec::new();
            while i < fm.len()
                && (fm[i].starts_with(' ') || fm[i].starts_with('\t') || fm[i].trim().is_empty())
            {
                parts.push(fm[i].trim());
                i += 1;
            }
            parts.join(" ").trim().to_string()
        } else {
            value.trim_matches(|c| c == '"' || c == '\'').to_string()
        };
        match key.trim() {
            "name" if !parsed.is_empty() => name = Some(parsed),
            "description" if !parsed.is_empty() => description = Some(parsed),
            _ => {}
        }
    }
    (name, description)
}

/// A skill is model-addressable unless its frontmatter sets `disable-model-invocation: true`.
pub fn parse_model_invocable(text: &str) -> bool {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return true;
    }
    for line in lines.take_while(|l| l.trim() != "---") {
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            if key.trim() == "disable-model-invocation" {
                let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
                return !value.eq_ignore_ascii_case("true");
            }
        }
    }
    true
}

fn read_skill(
    dir: &Path,
    source: Source,
    overrides: &serde_json::Map<String, Value>,
) -> Option<Skill> {
    let text = fs::read_to_string(dir.join("SKILL.md")).ok()?;
    let (fm_name, description) = parse_frontmatter(&text);
    let name = fm_name.or_else(|| dir.file_name().map(|n| n.to_string_lossy().to_string()))?;
    let state = overrides
        .get(&name)
        .and_then(Value::as_str)
        .unwrap_or("on")
        .to_string();
    Some(Skill {
        toggleable: source.toggleable(),
        model_invocable: parse_model_invocable(&text),
        name,
        description: description.unwrap_or_default(),
        path: dir.to_path_buf(),
        source,
        state,
    })
}

/// Immediate subdirectories (following symlinks) that aren't dot-dirs, sorted by name.
fn child_dirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter(|p| {
            p.file_name()
                .map(|n| !n.to_string_lossy().starts_with('.'))
                .unwrap_or(false)
        })
        .collect();
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

fn discover_global(overrides: &serde_json::Map<String, Value>) -> Result<Vec<Skill>> {
    let root = claude_settings::claude_dir()?.join("skills");
    let mut skills = Vec::new();
    for dir in child_dirs(&root) {
        if dir.file_name().map(|n| n == "synced").unwrap_or(false) && !dir.join("SKILL.md").exists()
        {
            // Account-synced skills: synced/<uuid>/<skill>/SKILL.md
            for bundle in child_dirs(&dir) {
                for skill_dir in child_dirs(&bundle) {
                    skills.extend(read_skill(&skill_dir, Source::Synced, overrides));
                }
            }
        } else {
            skills.extend(read_skill(&dir, Source::Global, overrides));
        }
    }
    Ok(skills)
}

fn discover_plugins(overrides: &serde_json::Map<String, Value>) -> Result<Vec<Skill>> {
    let manifest = claude_settings::claude_dir()?
        .join("plugins")
        .join("installed_plugins.json");
    let Ok(text) = fs::read_to_string(&manifest) else {
        return Ok(Vec::new());
    };
    let Ok(json) = serde_json::from_str::<Value>(&text) else {
        return Ok(Vec::new());
    };
    let mut skills = Vec::new();
    if let Some(plugins) = json.get("plugins").and_then(Value::as_object) {
        for (key, installs) in plugins {
            let plugin_name = key.split('@').next().unwrap_or(key).to_string();
            for install in installs.as_array().into_iter().flatten() {
                let Some(path) = install.get("installPath").and_then(Value::as_str) else {
                    continue;
                };
                for dir in child_dirs(&Path::new(path).join("skills")) {
                    skills.extend(read_skill(
                        &dir,
                        Source::Plugin(plugin_name.clone()),
                        overrides,
                    ));
                }
            }
        }
    }
    Ok(skills)
}

/// Find `.claude/skills` dirs under `root` (bounded depth, skipping heavy dirs).
fn find_project_skill_dirs(root: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Don't follow symlinks while walking: avoids cycles.
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name == ".claude" {
            let skills = path.join("skills");
            if skills.is_dir() {
                out.push(skills);
            }
        } else if depth > 0
            && !name.starts_with('.')
            && !matches!(name.as_str(), "node_modules" | "target")
        {
            find_project_skill_dirs(&path, depth - 1, out);
        }
    }
}

fn discover_project(start: &Path, overrides: &serde_json::Map<String, Value>) -> Vec<Skill> {
    let Ok(repo) = git2::Repository::discover(start) else {
        return Vec::new();
    };
    let Some(root) = repo.workdir().map(Path::to_path_buf) else {
        return Vec::new();
    };
    let repo_name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".to_string());
    let global = claude_settings::claude_dir()
        .ok()
        .and_then(|d| d.join("skills").canonicalize().ok());

    let mut dirs = Vec::new();
    find_project_skill_dirs(&root, 4, &mut dirs);
    let mut skills = Vec::new();
    for skills_dir in dirs {
        // A repo rooted at $HOME would otherwise list the global skills as "project" skills.
        if skills_dir.canonicalize().ok() == global {
            continue;
        }
        for dir in child_dirs(&skills_dir) {
            skills.extend(read_skill(
                &dir,
                Source::Project(repo_name.clone()),
                overrides,
            ));
        }
    }
    skills
}

pub fn discover(repos: &[PathBuf]) -> Result<Vec<Skill>> {
    let settings = claude_settings::read_settings()?;
    let overrides = claude_settings::skill_overrides(&settings);

    let mut skills = discover_global(&overrides)?;
    skills.extend(discover_plugins(&overrides)?);

    let mut starts: Vec<PathBuf> = repos.to_vec();
    if starts.is_empty() {
        starts.push(std::env::current_dir()?);
    }
    let mut seen_roots = Vec::new();
    for start in starts {
        for skill in discover_project(&start, &overrides) {
            if !seen_roots.contains(&skill.path) {
                seen_roots.push(skill.path.clone());
                skills.push(skill);
            }
        }
    }
    Ok(skills)
}

// ---------------------------------------------------------------------------
// list
// ---------------------------------------------------------------------------

fn tilde(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rest) = path.strip_prefix(&home) {
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", cut)
    }
}

pub fn run_list(repos: Vec<PathBuf>, json: bool) -> Result<()> {
    let skills = discover(&repos)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&skills)?);
        return Ok(());
    }
    if skills.is_empty() {
        ui::info("No skills found");
        return Ok(());
    }

    let name_w = skills
        .iter()
        .map(|s| s.name.chars().count())
        .max()
        .unwrap_or(4)
        .max(4);
    let src_w = skills
        .iter()
        .map(|s| s.source.label().chars().count() + if s.toggleable { 0 } else { 9 })
        .max()
        .unwrap_or(6)
        .max(6);

    println!(
        "{}",
        style(format!(
            "{:<name_w$}  {:<5}  {:<5}  {:<src_w$}  {}",
            "NAME", "STATE", "MODEL", "SOURCE", "PATH"
        ))
        .bold()
    );
    for s in &skills {
        let source = if s.toggleable {
            s.source.label()
        } else {
            format!("{} [locked]", s.source.label())
        };
        let state = if s.state == "on" {
            style(format!("{:<5}", s.state)).green()
        } else {
            style(format!("{:<5}", s.state)).red()
        };
        let model = if s.model_invocable {
            style(format!("{:<5}", "yes")).green()
        } else {
            style(format!("{:<5}", "no")).red()
        };
        println!(
            "{:<name_w$}  {}  {}  {:<src_w$}  {}",
            s.name,
            state,
            model,
            source,
            tilde(&s.path)
        );
        if !s.description.is_empty() {
            println!(
                "{:<name_w$}  {}",
                "",
                style(truncate(&s.description, 90)).dim()
            );
        }
    }
    println!();
    println!(
        "{}",
        style("[locked] = owned by a repo or plugin; can't be toggled here").dim()
    );
    println!(
        "{}",
        style("MODEL = whether Claude can invoke the skill itself (no = disable-model-invocation)")
            .dim()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// enable / disable
// ---------------------------------------------------------------------------

pub fn run_toggle(names: Vec<String>, enable: bool, dry_run: bool) -> Result<()> {
    let skills = discover(&[])?;
    let mut settings = claude_settings::read_settings()?;
    let mut failed = false;

    for name in &names {
        let matches: Vec<&Skill> = skills.iter().filter(|s| &s.name == name).collect();
        if matches.is_empty() {
            ui::error(&format!("No skill named '{}'", name));
            failed = true;
            continue;
        }
        if !matches.iter().any(|s| s.toggleable) {
            let owner = matches[0].source.label();
            ui::error(&format!(
                "'{}' is a {} skill; it's owned by the repo/plugin and can't be toggled here",
                name, owner
            ));
            failed = true;
            continue;
        }
        if matches.iter().any(|s| !s.toggleable) {
            ui::info(&format!(
                "'{}' also exists as a repo/plugin skill; the override applies to every skill with that name",
                name
            ));
        }

        let (verb, state) = if enable {
            ("Enabled", None)
        } else {
            ("Disabled", Some("off"))
        };
        if dry_run {
            ui::dry_run(&format!("{} '{}'", verb, name));
        } else {
            claude_settings::set_override(&mut settings, name, state);
            ui::success(&format!("{} '{}'", verb, name));
        }
    }

    if !dry_run && !names.is_empty() {
        claude_settings::write_settings(&settings)?;
    }
    if failed {
        bail!("Some skills could not be updated");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// install
// ---------------------------------------------------------------------------

fn valid_skill_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        // metadata() follows symlinks, so linked dirs/files are copied by content
        if from.metadata()?.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            fs::copy(&from, &to).with_context(|| format!("Copying {}", from.display()))?;
        }
    }
    Ok(())
}

fn is_git_source(source: &str) -> bool {
    source.contains("://") || source.starts_with("git@") || source.ends_with(".git")
}

/// `owner/repo` shorthand → GitHub URL.
fn looks_like_shorthand(source: &str) -> bool {
    let parts: Vec<&str> = source.split('/').collect();
    parts.len() == 2
        && parts
            .iter()
            .all(|p| !p.is_empty() && !p.starts_with('.') && !p.starts_with('~'))
}

fn repo_basename(source: &str) -> String {
    source
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit(|c| c == '/' || c == ':')
        .next()
        .unwrap_or("skill")
        .to_string()
}

/// Candidate skill dirs under `base`: base itself, else base/*, else base/skills/*.
fn find_installable(base: &Path) -> Vec<PathBuf> {
    if base.join("SKILL.md").is_file() {
        return vec![base.to_path_buf()];
    }
    for parent in [base.to_path_buf(), base.join("skills")] {
        let found: Vec<PathBuf> = child_dirs(&parent)
            .into_iter()
            .filter(|d| d.join("SKILL.md").is_file())
            .collect();
        if !found.is_empty() {
            return found;
        }
    }
    Vec::new()
}

pub struct InstallOptions {
    pub source: String,
    pub path: Option<String>,
    pub name: Option<String>,
    pub on: bool,
    pub off: bool,
    pub force: bool,
    pub dry_run: bool,
}

pub fn run_install(opts: InstallOptions) -> Result<()> {
    if opts.on && opts.off {
        bail!("--on and --off are mutually exclusive");
    }

    // Resolve the source to a local directory (cloning if needed).
    let local = PathBuf::from(&opts.source);
    let (root, default_name, _scratch) = if local.is_dir() {
        let name = local
            .canonicalize()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "skill".to_string());
        (local, name, None)
    } else if is_git_source(&opts.source) || looks_like_shorthand(&opts.source) {
        let url = if is_git_source(&opts.source) {
            opts.source.clone()
        } else {
            format!("https://github.com/{}.git", opts.source)
        };
        let scratch = tempfile::tempdir().context("Creating temp dir")?;
        ui::info(&format!("Cloning {}", url));
        git2::build::RepoBuilder::new()
            .clone(&url, scratch.path())
            .with_context(|| format!("Cloning {}", url))?;
        (
            scratch.path().to_path_buf(),
            repo_basename(&url),
            Some(scratch),
        )
    } else {
        bail!(
            "'{}' is not a local directory or a git URL / owner/repo",
            opts.source
        );
    };

    let base = match &opts.path {
        Some(p) => root.join(p),
        None => root.clone(),
    };
    if !base.is_dir() {
        bail!("Path '{}' not found in source", base.display());
    }

    let candidates = find_installable(&base);
    if candidates.is_empty() {
        bail!(
            "No SKILL.md found in '{}' (try --path SUBDIR)",
            base.display()
        );
    }
    if opts.name.is_some() && candidates.len() > 1 {
        bail!(
            "--name only works when installing a single skill ({} found)",
            candidates.len()
        );
    }

    let skills_dir = claude_settings::claude_dir()?.join("skills");
    let mut settings = claude_settings::read_settings()?;
    let mut changed_settings = false;

    // Decide state once for the whole install.
    let off = if opts.off {
        true
    } else if opts.on {
        false
    } else if console::Term::stdout().is_term() && !opts.dry_run {
        let choice = Select::new()
            .with_prompt("Install as")
            .items(&["on", "off"])
            .default(0)
            .interact()?;
        choice == 1
    } else {
        false
    };

    for dir in candidates {
        let text = fs::read_to_string(dir.join("SKILL.md"))?;
        let (fm_name, _) = parse_frontmatter(&text);
        let name = opts.name.clone().or(fm_name).unwrap_or_else(|| {
            if dir == base && dir == root {
                default_name.clone()
            } else {
                dir.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| default_name.clone())
            }
        });
        if !valid_skill_name(&name) {
            bail!("Invalid skill name '{}'", name);
        }

        let dest = skills_dir.join(&name);
        let state = if off { "off" } else { "on" };
        if dest.exists() && !opts.force {
            bail!(
                "'{}' already exists at {} (use --force to overwrite)",
                name,
                dest.display()
            );
        }
        if opts.dry_run {
            ui::dry_run(&format!(
                "Install '{}' to {} ({})",
                name,
                tilde(&dest),
                state
            ));
            continue;
        }
        if dest.exists() {
            fs::remove_dir_all(&dest)?;
        }
        copy_dir(&dir, &dest)?;
        claude_settings::set_override(&mut settings, &name, if off { Some("off") } else { None });
        changed_settings = true;
        ui::success(&format!(
            "Installed '{}' ({}) to {}",
            name,
            state,
            tilde(&dest)
        ));
    }

    if changed_settings {
        claude_settings::write_settings(&settings)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_plain_and_quoted() {
        let (n, d) = parse_frontmatter("---\nname: foo\ndescription: \"Does things\"\n---\nbody");
        assert_eq!(n.as_deref(), Some("foo"));
        assert_eq!(d.as_deref(), Some("Does things"));
    }

    #[test]
    fn frontmatter_block_scalar() {
        let (_, d) = parse_frontmatter(
            "---\nname: foo\ndescription: >\n  line one\n  line two\nmodel: x\n---\n",
        );
        assert_eq!(d.as_deref(), Some("line one line two"));
    }

    #[test]
    fn frontmatter_missing() {
        assert_eq!(parse_frontmatter("# no frontmatter"), (None, None));
    }

    #[test]
    fn model_invocable_flag() {
        assert!(parse_model_invocable("---\nname: a\n---\n"));
        assert!(parse_model_invocable("# no frontmatter"));
        assert!(!parse_model_invocable(
            "---\nname: a\ndisable-model-invocation: true\n---\n"
        ));
        assert!(parse_model_invocable(
            "---\nname: a\ndisable-model-invocation: false\n---\n"
        ));
    }

    #[test]
    fn name_validation() {
        assert!(valid_skill_name("dev-worktree"));
        assert!(!valid_skill_name("../evil"));
        assert!(!valid_skill_name(".hidden"));
        assert!(!valid_skill_name(""));
    }

    #[test]
    fn source_detection() {
        assert!(is_git_source("https://github.com/a/b"));
        assert!(is_git_source("git@github.com:a/b.git"));
        assert!(looks_like_shorthand("owner/repo"));
        assert!(!looks_like_shorthand("./foo/bar"));
        assert_eq!(
            repo_basename("https://github.com/a/my-skill.git"),
            "my-skill"
        );
    }
}
