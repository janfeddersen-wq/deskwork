//! Skills context builder for system prompt injection.
//!
//! This module constructs the skills-related sections of the system prompt,
//! providing the agent with information about available skills, the Python
//! virtual environment, and installed external tools.

use std::path::PathBuf;

use tracing::{debug, info, warn};

use crate::external_tools::catalog::get_tool_definition;
use crate::external_tools::manifest::load_manifest;
use crate::external_tools::paths::{get_skills_dir, get_tools_dir, get_venvs_dir};
use crate::external_tools::types::{ExternalToolId, Platform};
use crate::python::{ensure_venv, get_venv_python};
use crate::skills::discover_skills;
use crate::skills::SkillMetadata;

/// Information about an installed external tool.
#[derive(Debug, Clone)]
pub struct InstalledTool {
    pub name: String,
    pub executable_path: PathBuf,
}

/// Context for skills that gets injected into the system prompt.
#[derive(Debug, Clone)]
pub struct SkillsContext {
    /// Discovered skills.
    pub skills: Vec<SkillMetadata>,
    /// Path to the skills venv.
    pub venv_path: PathBuf,
    /// Python interpreter path.
    pub python_path: PathBuf,
    /// Path to the skills directory.
    pub skills_dir: PathBuf,
    /// Installed external tools.
    pub installed_tools: Vec<InstalledTool>,
}

impl SkillsContext {
    /// Build the skills context by discovering skills and tools.
    ///
    /// This creates the venv if needed and discovers all available
    /// skills and external tools.
    pub fn build() -> Self {
        let skills_dir = get_skills_dir();
        let venv_path = get_venvs_dir().join("skills-venv");

        // Discover skills
        let skills = match discover_skills() {
            Ok(s) => {
                info!("Skills context: discovered {} skills", s.len());
                s
            }
            Err(e) => {
                warn!("Failed to discover skills: {}", e);
                Vec::new()
            }
        };

        // Ensure venv exists (best-effort)
        if let Err(e) = ensure_venv(&venv_path) {
            warn!("Failed to ensure skills venv: {}", e);
        }

        let python_path = get_venv_python(&venv_path);

        // Discover installed external tools
        let installed_tools = discover_installed_tools();
        info!(
            "Skills context: {} installed external tools",
            installed_tools.len()
        );

        Self {
            skills,
            venv_path,
            python_path,
            skills_dir,
            installed_tools,
        }
    }

    /// Build an empty context (no skills, no tools).
    pub fn empty() -> Self {
        Self {
            skills: Vec::new(),
            venv_path: PathBuf::new(),
            python_path: PathBuf::new(),
            skills_dir: PathBuf::new(),
            installed_tools: Vec::new(),
        }
    }

    /// Returns whether any skills are available.
    pub fn has_skills(&self) -> bool {
        !self.skills.is_empty()
    }

    /// Build the full skills prompt section for injection into the system prompt.
    pub fn to_prompt_section(&self, working_directory: Option<&str>) -> String {
        if self.skills.is_empty() && self.installed_tools.is_empty() {
            return String::new();
        }

        let mut out = String::new();

        out.push_str("## Skills & Tools\n\n");
        out.push_str("You have access to Python-based skills for specialized tasks. ");
        out.push_str("Use `run_shell_command` to execute them.\n\n");

        // Python environment
        if !self.venv_path.as_os_str().is_empty() {
            out.push_str("### Python Environment\n\n");
            out.push_str(&format!(
                "**CRITICAL:** ALL Python executions MUST use this interpreter:\n`{}`\n\n",
                self.python_path.display()
            ));
            out.push_str(&format!(
                "Example: `{} /path/to/script.py --arg value`\n\n",
                self.python_path.display()
            ));
        }

        // Skills
        if !self.skills.is_empty() {
            out.push_str("### Available Skills\n\n");
            out.push_str("Before using any skill, READ its SKILL.md file to understand usage, parameters, and examples.\n\n");

            for skill in &self.skills {
                out.push_str(&format!("#### {}\n", skill.name));
                out.push_str(&format!("- **Description:** {}\n", skill.description));
                out.push_str(&format!("- **Path:** `{}`\n", skill.path.display()));
                out.push_str(&format!(
                    "- **Documentation:** `{}` (READ BEFORE USING)\n",
                    skill.skill_md_path.display()
                ));
                out.push('\n');
            }
        }

        // External tools
        if !self.installed_tools.is_empty() {
            out.push_str("### External Tools\n\n");
            out.push_str("The following external tools are installed and available:\n\n");

            for tool in &self.installed_tools {
                out.push_str(&format!(
                    "- **{}**: `{}`\n",
                    tool.name,
                    tool.executable_path.display()
                ));
            }

            out.push_str("\nInvoke these tools directly using `run_shell_command`.\n");
        }

        // Workflow
        out.push_str("\n### Skill Execution Workflow\n\n");
        out.push_str("1. **Read Documentation**: Read the skill's SKILL.md file first\n");
        out.push_str("2. **Execute**: Use `run_shell_command` with the venv Python interpreter\n");
        out.push_str("3. **Validate**: Check output for success/failure\n");

        if let Some(wd) = working_directory {
            out.push_str(&format!(
                "4. **Move Output to Working Directory**: If the skill produces any output files \
                 (PDF, DOCX, images, spreadsheets, etc.), you MUST move them to the user's working \
                 directory at `{}` using `run_shell_command` with `mv <output_path> {}/` — \
                 always use this absolute path, never a relative path like `./`. \
                 Report the final file location to the user.\n",
                wd, wd
            ));
        } else {
            out.push_str(
                "4. **Move Output to Working Directory**: If the skill produces any output files \
                 (PDF, DOCX, images, spreadsheets, etc.), you MUST ask the user where to save them \
                 or move them to the current directory. Report the final file location to the user.\n",
            );
        }

        out
    }
}

/// Discovers installed external tools and returns their paths.
fn discover_installed_tools() -> Vec<InstalledTool> {
    let platform = match Platform::detect() {
        Some(p) => p,
        None => return Vec::new(),
    };

    let tools_dir = get_tools_dir();

    let manifest = match load_manifest() {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };

    let mut installed = Vec::new();

    for tool_id in ExternalToolId::all() {
        if !manifest.is_installed(*tool_id) {
            continue;
        }

        let def = get_tool_definition(*tool_id);
        let exec_relpath = def.get_executable_path(platform);
        let tool_dir = tools_dir.join(tool_id.as_str());
        let exec_path = tool_dir.join(exec_relpath);

        if exec_path.exists() {
            installed.push(InstalledTool {
                name: def.display_name.to_string(),
                executable_path: exec_path,
            });
            debug!(
                "Found installed tool: {} at {:?}",
                def.display_name, exec_relpath
            );
        }
    }

    installed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_context() {
        let ctx = SkillsContext::empty();
        assert!(!ctx.has_skills());
        assert!(ctx.skills.is_empty());
        assert!(ctx.installed_tools.is_empty());
    }

    #[test]
    fn test_empty_context_no_prompt() {
        let ctx = SkillsContext::empty();
        let section = ctx.to_prompt_section(None);
        assert!(
            section.is_empty(),
            "Empty context should produce empty prompt section"
        );
    }

    #[test]
    fn test_context_with_skills() {
        let ctx = SkillsContext {
            skills: vec![SkillMetadata {
                name: "docx".to_string(),
                description: "Document creation".to_string(),
                license: "MIT".to_string(),
                path: PathBuf::from("/skills/docx"),
                skill_md_path: PathBuf::from("/skills/docx/SKILL.md"),
            }],
            venv_path: PathBuf::from("/venvs/skills-venv"),
            python_path: PathBuf::from("/venvs/skills-venv/bin/python"),
            skills_dir: PathBuf::from("/skills"),
            installed_tools: Vec::new(),
        };

        assert!(ctx.has_skills());
        let section = ctx.to_prompt_section(Some("/home/user/projects"));
        assert!(section.contains("#### docx"));
        assert!(section.contains("Document creation"));
        assert!(section.contains("READ BEFORE USING"));
        assert!(section.contains("/venvs/skills-venv/bin/python"));
        assert!(section.contains("/home/user/projects"));
        assert!(section.contains("Move Output to Working Directory"));
    }

    #[test]
    fn test_context_with_tools() {
        let ctx = SkillsContext {
            skills: Vec::new(),
            venv_path: PathBuf::new(),
            python_path: PathBuf::new(),
            skills_dir: PathBuf::new(),
            installed_tools: vec![InstalledTool {
                name: "Pandoc".to_string(),
                executable_path: PathBuf::from("/tools/pandoc/bin/pandoc"),
            }],
        };

        let section = ctx.to_prompt_section(None);
        assert!(section.contains("**Pandoc**"));
        assert!(section.contains("/tools/pandoc/bin/pandoc"));
    }

    #[test]
    fn test_discover_installed_tools_doesnt_panic() {
        let tools = discover_installed_tools();
        // Should not panic regardless of tool state
        let _ = tools.len();
    }
}
