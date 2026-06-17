use std::collections::HashSet;
use std::path::{Path, PathBuf};

use hermes_core::AgentError;

fn layered_skill_roots() -> Vec<PathBuf> {
    hermes_skills::skill_search_roots()
}

fn collect_skills_from_roots(roots: &[PathBuf]) -> Vec<(String, String)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for root in roots {
        collect_skills_recursive(root, root, &mut seen, &mut out);
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn collect_skills_recursive(
    root: &Path,
    dir: &Path,
    seen: &mut HashSet<String>,
    out: &mut Vec<(String, String)>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_md = path.join("SKILL.md");
        if skill_md.is_file() {
            let name = read_skill_name(&skill_md).unwrap_or_else(|| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            });
            if seen.insert(name.clone()) {
                let desc = std::fs::read_to_string(&skill_md)
                    .ok()
                    .and_then(|c| {
                        c.lines()
                            .find(|l| l.starts_with('#'))
                            .map(|l| l.trim_start_matches('#').trim().to_string())
                    })
                    .unwrap_or_else(|| "(no description)".to_string());
                out.push((name, desc));
            }
            continue;
        }
        if dir != root
            || path
                .file_name()
                .is_some_and(|n| !n.to_string_lossy().starts_with('.'))
        {
            collect_skills_recursive(root, &path, seen, out);
        }
    }
}

fn read_skill_name(skill_md: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(skill_md).ok()?;
    if !raw.starts_with("---") {
        return None;
    }
    let rest = &raw[3..];
    let end = rest.find("\n---")?;
    let yaml = &rest[..end];
    for line in yaml.lines() {
        if let Some(value) = line.strip_prefix("name:") {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

pub(crate) fn run_list(_skills_dir: &Path) -> Result<(), AgentError> {
    let roots = layered_skill_roots();
    let skills = collect_skills_from_roots(&roots);
    let home = hermes_config::skills_dir();
    println!("Installed skills ({}):", home.display());
    if skills.is_empty() {
        println!("  (no skills installed)");
        return Ok(());
    }
    for (name, desc) in skills {
        println!("  • {} — {}", name, desc);
    }
    Ok(())
}

pub(crate) fn run_browse(_skills_dir: &Path) -> Result<(), AgentError> {
    let roots = layered_skill_roots();
    println!("Skills Browser");
    println!("==============\n");
    let mut categories: std::collections::HashMap<String, Vec<(String, String)>> =
        std::collections::HashMap::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            walk_category(&path, &path, &mut categories);
        }
    }
    for (category, skills) in &categories {
        println!("[{}]", category);
        for (name, desc) in skills {
            println!("  • {} — {}", name, desc);
        }
        println!();
    }
    if categories.is_empty() {
        println!("  (no skills installed)");
    }
    Ok(())
}

fn walk_category(
    category_dir: &Path,
    dir: &Path,
    categories: &mut std::collections::HashMap<String, Vec<(String, String)>>,
) {
    let skill_md = dir.join("SKILL.md");
    if skill_md.is_file() {
        let name = read_skill_name(&skill_md).unwrap_or_else(|| {
            dir.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
        let desc = std::fs::read_to_string(&skill_md)
            .ok()
            .and_then(|c| {
                c.lines()
                    .find(|l| l.starts_with('#'))
                    .map(|l| l.trim_start_matches('#').trim().to_string())
            })
            .unwrap_or_else(|| "(no description)".to_string());
        let category = category_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "general".to_string());
        let entry = categories.entry(category).or_default();
        if !entry.iter().any(|(n, _)| n == &name) {
            entry.push((name, desc));
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            walk_category(category_dir, &path, categories);
        }
    }
}
