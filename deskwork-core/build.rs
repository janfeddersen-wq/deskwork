use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set"));
    let repo_root = manifest_dir
        .parent()
        .expect("deskwork-core has workspace root parent")
        .to_path_buf();

    let plugins_root = repo_root.join("knowledge-work-plugins");

    // Per requirement: rerun when anything under knowledge-work-plugins changes.
    // (We also emit rerun-if-changed per discovered file for extra correctness.)
    println!("cargo:rerun-if-changed={}", plugins_root.display());

    let categories = discover_categories(&plugins_root)
        .unwrap_or_else(|err| panic!("Failed scanning {}: {err}", plugins_root.display()));

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is set"));
    let out_file = out_dir.join("bundled_categories.rs");

    let generated = generate_bundled_categories_rs(&categories);
    fs::write(&out_file, generated).expect("write generated bundled_categories.rs");
}

#[derive(Debug, Clone)]
struct CategoryAssets {
    id: String,
    readme: String,
    connectors_md: String,
    mcp_json: String,
    playbook_template: String,
    // (relative_path, content)
    skills: Vec<(String, String)>,
    commands: Vec<(String, String)>,
}

fn discover_categories(plugins_root: &Path) -> io::Result<Vec<CategoryAssets>> {
    let mut categories = Vec::new();

    let mut entries = plugins_root
        .read_dir()?
        .collect::<Result<Vec<_>, _>>()?;

    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let file_name = entry.file_name();
        let id = file_name.to_string_lossy().to_string();

        if is_hidden_name(&file_name) {
            continue;
        }

        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let readme_path = path.join("README.md");
        let connectors_path = path.join("CONNECTORS.md");
        let mcp_path = path.join(".mcp.json");
        let playbook_path = path.join("PLAYBOOK_TEMPLATE.md");

        // Emit explicit rerun triggers for the files we care about.
        // This avoids relying on directory mtimes.
        emit_rerun_if_changed_optional(&readme_path);
        emit_rerun_if_changed_optional(&connectors_path);
        emit_rerun_if_changed_optional(&mcp_path);
        emit_rerun_if_changed_optional(&playbook_path);

        let readme = read_optional_to_string(&readme_path)?;
        let connectors_md = read_optional_to_string(&connectors_path)?;
        let mcp_json = read_optional_to_string(&mcp_path)?;
        let playbook_template = read_optional_to_string(&playbook_path)?;

        let skills = discover_skills(&path)?;
        let commands = discover_commands(&path)?;

        // Skip categories with no skills AND no commands.
        if skills.is_empty() && commands.is_empty() {
            continue;
        }

        categories.push(CategoryAssets {
            id,
            readme,
            connectors_md,
            mcp_json,
            playbook_template,
            skills,
            commands,
        });
    }

    // Deterministic output.
    categories.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(categories)
}

fn discover_skills(category_root: &Path) -> io::Result<Vec<(String, String)>> {
    let skills_root = category_root.join("skills");
    if !skills_root.is_dir() {
        return Ok(Vec::new());
    }

    println!("cargo:rerun-if-changed={}", skills_root.display());

    let mut skill_dirs = skills_root.read_dir()?.collect::<Result<Vec<_>, _>>()?;
    skill_dirs.sort_by_key(|e| e.file_name());

    let mut skills = Vec::new();

    for entry in skill_dirs {
        let name = entry.file_name();
        if is_hidden_name(&name) {
            continue;
        }

        let skill_dir = entry.path();
        if !skill_dir.is_dir() {
            continue;
        }

        let skill_md = skill_dir.join("SKILL.md");
        if !skill_md.is_file() {
            continue;
        }

        println!("cargo:rerun-if-changed={}", skill_md.display());

        let content = fs::read_to_string(&skill_md)?;
        let rel = to_forward_slash_path(&skill_md.strip_prefix(category_root).unwrap_or(&skill_md));
        skills.push((rel, content));
    }

    skills.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(skills)
}

fn discover_commands(category_root: &Path) -> io::Result<Vec<(String, String)>> {
    let commands_root = category_root.join("commands");
    if !commands_root.is_dir() {
        return Ok(Vec::new());
    }

    println!("cargo:rerun-if-changed={}", commands_root.display());

    let mut entries = commands_root.read_dir()?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|e| e.file_name());

    let mut commands = Vec::new();

    for entry in entries {
        let name = entry.file_name();
        if is_hidden_name(&name) {
            continue;
        }

        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        if path.extension() != Some(OsStr::new("md")) {
            continue;
        }

        println!("cargo:rerun-if-changed={}", path.display());

        let content = fs::read_to_string(&path)?;
        let rel = to_forward_slash_path(&path.strip_prefix(category_root).unwrap_or(&path));
        commands.push((rel, content));
    }

    commands.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(commands)
}

fn generate_bundled_categories_rs(categories: &[CategoryAssets]) -> String {
    let mut out = String::new();

    out.push_str("// @generated by deskwork-core/build.rs\n");
    out.push_str("// This file is auto-generated. Do not edit by hand.\n\n");

    out.push_str("#[derive(Debug, Clone, Copy)]\n");
    out.push_str("pub struct BundledCategory {\n");
    out.push_str("    pub id: &'static str,\n");
    out.push_str("    pub readme: &'static str,\n");
    out.push_str("    pub connectors_md: &'static str,\n");
    out.push_str("    pub mcp_json: &'static str,\n");
    out.push_str("    pub playbook_template: &'static str,\n");
    out.push_str(
        "    pub skills: &'static [(&'static str, &'static str)],\n",
    );
    out.push_str(
        "    pub commands: &'static [(&'static str, &'static str)],\n",
    );
    out.push_str("}\n\n");

    out.push_str("pub static BUNDLED_CATEGORIES: &[BundledCategory] = &[\n");

    for category in categories {
        out.push_str("    BundledCategory {\n");
        out.push_str(&format!("        id: {},\n", to_rust_string_literal(&category.id)));
        out.push_str(&format!(
            "        readme: {},\n",
            to_rust_string_literal(&category.readme)
        ));
        out.push_str(&format!(
            "        connectors_md: {},\n",
            to_rust_string_literal(&category.connectors_md)
        ));
        out.push_str(&format!(
            "        mcp_json: {},\n",
            to_rust_string_literal(&category.mcp_json)
        ));
        out.push_str(&format!(
            "        playbook_template: {},\n",
            to_rust_string_literal(&category.playbook_template)
        ));

        out.push_str("        skills: &[\n");
        for (rel, content) in &category.skills {
            out.push_str(&format!(
                "            ({}, {}),\n",
                to_rust_string_literal(rel),
                to_rust_string_literal(content)
            ));
        }
        out.push_str("        ],\n");

        out.push_str("        commands: &[\n");
        for (rel, content) in &category.commands {
            out.push_str(&format!(
                "            ({}, {}),\n",
                to_rust_string_literal(rel),
                to_rust_string_literal(content)
            ));
        }
        out.push_str("        ],\n");

        out.push_str("    },\n");
    }

    out.push_str("];
");

    out
}

fn read_optional_to_string(path: &Path) -> io::Result<String> {
    if !path.is_file() {
        return Ok(String::new());
    }
    fs::read_to_string(path)
}

fn emit_rerun_if_changed_optional(path: &Path) {
    // If the file doesn't exist, we still want changes (like creation) to rerun.
    // Directory-level rerun-if-changed covers this, but we keep this helper for consistency.
    println!("cargo:rerun-if-changed={}", path.display());
}

fn is_hidden_name(name: &OsStr) -> bool {
    name.to_string_lossy().starts_with('.')
}

fn to_forward_slash_path(path: &Path) -> String {
    // Build scripts run on multiple platforms. Internally we want stable, forward-slash paths.
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn to_rust_string_literal(value: &str) -> String {
    // Debug formatting yields a valid Rust string literal with proper escaping.
    format!("{value:?}")
}
