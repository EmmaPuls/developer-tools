use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn write_skill(dir: &Path, name: &str, desc: &str) {
    let d = dir.join(name);
    fs::create_dir_all(&d).unwrap();
    fs::write(
        d.join("SKILL.md"),
        format!("---\nname: {}\ndescription: {}\n---\nbody\n", name, desc),
    )
    .unwrap();
}

/// Hermetic env: HOME and CLAUDE_CONFIG_DIR in a tempdir, cwd is a git repo with a project skill.
struct Env {
    home: TempDir,
    repo: TempDir,
}

impl Env {
    fn new() -> Self {
        let home = TempDir::new().unwrap();
        let repo = TempDir::new().unwrap();
        git2::Repository::init(repo.path()).unwrap();
        write_skill(
            &home.path().join(".claude/skills"),
            "globalskill",
            "A global skill",
        );
        write_skill(
            &repo.path().join(".claude/skills"),
            "reposkill",
            "A repo skill",
        );
        fs::write(
            home.path().join(".claude/settings.json"),
            r#"{"model":"opus","enabledPlugins":{"x":true}}"#,
        )
        .unwrap();
        Env { home, repo }
    }

    fn dt(&self) -> Command {
        let mut c = Command::cargo_bin("dt").unwrap();
        c.env("HOME", self.home.path())
            .env("CLAUDE_CONFIG_DIR", self.home.path().join(".claude"))
            .current_dir(self.repo.path());
        c
    }

    fn settings(&self) -> serde_json::Value {
        serde_json::from_str(
            &fs::read_to_string(self.home.path().join(".claude/settings.json")).unwrap(),
        )
        .unwrap()
    }
}

#[test]
fn list_shows_global_and_project_with_lock() {
    let e = Env::new();
    e.dt()
        .args(["skills", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("globalskill"))
        .stdout(predicate::str::contains("project:"))
        .stdout(predicate::str::contains("[locked]"));
}

#[test]
fn disable_then_enable_round_trips_and_preserves_settings() {
    let e = Env::new();
    e.dt()
        .args(["skills", "disable", "globalskill"])
        .assert()
        .success();
    let s = e.settings();
    assert_eq!(s["skillOverrides"]["globalskill"], "off");
    assert_eq!(s["model"], "opus");
    assert_eq!(s["enabledPlugins"]["x"], true);

    e.dt()
        .args(["skills", "enable", "globalskill"])
        .assert()
        .success();
    let s = e.settings();
    assert!(s.get("skillOverrides").is_none());
    assert_eq!(s["model"], "opus");
}

#[test]
fn disabling_project_skill_fails() {
    let e = Env::new();
    e.dt()
        .args(["skills", "disable", "reposkill"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("can't be toggled"));
    assert!(e.settings().get("skillOverrides").is_none());
}

#[test]
fn disabling_unknown_skill_fails() {
    let e = Env::new();
    e.dt()
        .args(["skills", "disable", "nope"])
        .assert()
        .failure();
}

#[test]
fn malformed_settings_not_overwritten() {
    let e = Env::new();
    let p = e.home.path().join(".claude/settings.json");
    fs::write(&p, "{ not json").unwrap();
    e.dt()
        .args(["skills", "disable", "globalskill"])
        .assert()
        .failure();
    assert_eq!(fs::read_to_string(&p).unwrap(), "{ not json");
}

#[test]
fn dry_run_changes_nothing() {
    let e = Env::new();
    e.dt()
        .args(["skills", "disable", "globalskill", "--dry-run"])
        .assert()
        .success();
    assert!(e.settings().get("skillOverrides").is_none());
}

#[test]
fn install_local_off_then_collision() {
    let e = Env::new();
    let src = TempDir::new().unwrap();
    write_skill(src.path(), "newskill", "New");
    let skill_dir = src.path().join("newskill");

    e.dt()
        .args(["skills", "install", skill_dir.to_str().unwrap(), "--off"])
        .assert()
        .success();
    assert!(e
        .home
        .path()
        .join(".claude/skills/newskill/SKILL.md")
        .exists());
    assert_eq!(e.settings()["skillOverrides"]["newskill"], "off");

    e.dt()
        .args(["skills", "install", skill_dir.to_str().unwrap(), "--off"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--force"));
    e.dt()
        .args([
            "skills",
            "install",
            skill_dir.to_str().unwrap(),
            "--on",
            "--force",
        ])
        .assert()
        .success();
    assert!(e.settings().get("skillOverrides").is_none());
}

#[test]
fn install_from_git_repo_with_path() {
    let e = Env::new();
    let src = TempDir::new().unwrap();
    write_skill(&src.path().join("skills"), "gitskill", "From git");
    let repo = git2::Repository::init(src.path()).unwrap();
    let mut idx = repo.index().unwrap();
    idx.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    idx.write().unwrap();
    let tree = repo.find_tree(idx.write_tree().unwrap()).unwrap();
    let sig = git2::Signature::now("t", "t@example.com").unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
        .unwrap();

    let url = format!("file://{}", src.path().display());
    e.dt()
        .args(["skills", "install", &url, "--off"])
        .assert()
        .success();
    assert!(e
        .home
        .path()
        .join(".claude/skills/gitskill/SKILL.md")
        .exists());
    assert_eq!(e.settings()["skillOverrides"]["gitskill"], "off");
}

#[test]
fn json_output_is_valid() {
    let e = Env::new();
    let out = e.dt().args(["skills", "list", "--json"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["name"] == "globalskill" && s["toggleable"] == true));
}
