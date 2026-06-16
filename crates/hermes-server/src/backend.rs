use std::path::Path;
use std::time::Duration;

use hermes_core::{AgentError, CommandOutput, Skill, SkillMeta, SkillProvider, TerminalBackend};

/// Local terminal backend using tokio::process::Command.
#[derive(Clone)]
pub struct LocalTerminalBackend;

#[async_trait::async_trait]
impl TerminalBackend for LocalTerminalBackend {
    async fn execute_command(
        &self,
        command: &str,
        timeout: Option<u64>,
        workdir: Option<&str>,
        _background: bool,
        _pty: bool,
    ) -> Result<CommandOutput, AgentError> {
        let timeout_dur = timeout
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(120));

        let shell = if cfg!(target_os = "windows") { "powershell.exe" } else { "sh" };
        let arg = if cfg!(target_os = "windows") { "-Command" } else { "-c" };

        let mut cmd = tokio::process::Command::new(shell);
        cmd.arg(arg).arg(command);
        if let Some(dir) = workdir { cmd.current_dir(dir); }

        let output = tokio::time::timeout(timeout_dur, cmd.output())
            .await
            .map_err(|_| AgentError::ToolExecution(format!("Command timed out after {}s", timeout_dur.as_secs())))?
            .map_err(|e| AgentError::ToolExecution(format!("Failed to execute: {}", e)))?;

        Ok(CommandOutput {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    async fn execute_command_with_stdin(
        &self,
        command: &str,
        timeout: Option<u64>,
        workdir: Option<&str>,
        _background: bool,
        _pty: bool,
        stdin_data: Option<&str>,
    ) -> Result<CommandOutput, AgentError> {
        let timeout_dur = timeout
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(120));

        let shell = if cfg!(target_os = "windows") { "powershell.exe" } else { "sh" };
        let arg = if cfg!(target_os = "windows") { "-Command" } else { "-c" };

        let mut cmd = tokio::process::Command::new(shell);
        cmd.arg(arg).arg(command);
        if let Some(dir) = workdir { cmd.current_dir(dir); }
        if stdin_data.is_some() { cmd.stdin(std::process::Stdio::piped()); }

        let mut child = cmd.spawn()
            .map_err(|e| AgentError::ToolExecution(format!("Spawn failed: {}", e)))?;

        if let Some(data) = stdin_data {
            if let Some(mut stdin) = child.stdin.take() {
                use tokio::io::AsyncWriteExt;
                stdin.write_all(data.as_bytes()).await.ok();
                stdin.flush().await.ok();
            }
        }

        let output = tokio::time::timeout(timeout_dur, child.wait_with_output())
            .await
            .map_err(|_| AgentError::ToolExecution(format!("Command timed out after {}s", timeout_dur.as_secs())))?
            .map_err(|e| AgentError::ToolExecution(format!("Command failed: {}", e)))?;

        Ok(CommandOutput {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    async fn read_file(&self, path: &str, offset: Option<u64>, limit: Option<u64>) -> Result<String, AgentError> {
        let content = tokio::fs::read_to_string(path).await
            .map_err(|e| AgentError::ToolExecution(format!("Read failed: {}", e)))?;
        let lines: Vec<&str> = content.lines().collect();
        let start = offset.unwrap_or(0) as usize;
        if start >= lines.len() { return Ok(String::new()); }
        let end = limit.map(|l| start + l as usize).unwrap_or(lines.len());
        Ok(lines[start..end.min(lines.len())].join("\n"))
    }

    async fn write_file(&self, path: &str, content: &str) -> Result<(), AgentError> {
        if let Some(parent) = Path::new(path).parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }
        tokio::fs::write(path, content).await
            .map_err(|e| AgentError::ToolExecution(format!("Write failed: {}", e)))
    }

    async fn file_exists(&self, path: &str) -> Result<bool, AgentError> {
        Ok(Path::new(path).exists())
    }
}

/// Disk-based skill provider — reads/writes SKILL.md files from the skills directory.
#[derive(Clone)]
pub struct DiskSkillProvider {
    skills_dir: std::path::PathBuf,
}

impl DiskSkillProvider {
    pub fn new(skills_dir: std::path::PathBuf) -> Self {
        Self { skills_dir }
    }

    /// Parse YAML frontmatter from a markdown file.
    fn parse_frontmatter(content: &str) -> Option<serde_yaml::Value> {
        if !content.starts_with("---") { return None; }
        let end = content.find("\n---\n")?;
        let yaml_str = &content[3..end];
        serde_yaml::from_str(yaml_str).ok()
    }

    /// Find a SKILL.md file by name (searches recursively).
    fn find_skill(&self, name: &str) -> Option<std::path::PathBuf> {
        self.find_skill_recursive(&self.skills_dir, name)
    }

    fn find_skill_recursive(&self, dir: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
        let entries = std::fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = self.find_skill_recursive(&path, name) {
                    return Some(found);
                }
            } else if path.file_name().and_then(|f| f.to_str()) == Some("SKILL.md") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Some(fm) = Self::parse_frontmatter(&content) {
                        if fm.get("name").and_then(|v| v.as_str()) == Some(name) {
                            return Some(path);
                        }
                    }
                }
            }
        }
        None
    }
}

#[async_trait::async_trait]
impl SkillProvider for DiskSkillProvider {
    async fn create_skill(&self, name: &str, content: &str, category: Option<&str>) -> Result<Skill, AgentError> {
        let skill_dir = match category {
            Some(cat) => self.skills_dir.join(cat).join(name),
            None => self.skills_dir.join(name),
        };
        std::fs::create_dir_all(&skill_dir)
            .map_err(|e| AgentError::ToolExecution(format!("Create dir: {}", e)))?;
        std::fs::write(skill_dir.join("SKILL.md"), content)
            .map_err(|e| AgentError::ToolExecution(format!("Write SKILL.md: {}", e)))?;
        Ok(Skill { name: name.to_string(), content: content.to_string(), category: category.map(String::from), description: None })
    }

    async fn get_skill(&self, name: &str) -> Result<Option<Skill>, AgentError> {
        let Some(path) = self.find_skill(name) else {
            return Ok(None);
        };
        let content = std::fs::read_to_string(&path)
            .map_err(|e| AgentError::ToolExecution(format!("Read: {}", e)))?;
        let category = path.parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
            .map(|f| f.to_string_lossy().to_string());
        Ok(Some(Skill { name: name.to_string(), content, category, description: None }))
    }

    async fn list_skills(&self) -> Result<Vec<SkillMeta>, AgentError> {
        let mut skills = Vec::new();
        self.list_skills_recursive(&self.skills_dir, &mut skills);
        Ok(skills)
    }

    async fn update_skill(&self, name: &str, content: &str) -> Result<Skill, AgentError> {
        let path = self.find_skill(name)
            .ok_or_else(|| AgentError::ToolExecution(format!("Skill '{}' not found", name)))?;
        std::fs::write(&path, content)
            .map_err(|e| AgentError::ToolExecution(format!("Write: {}", e)))?;
        Ok(Skill { name: name.to_string(), content: content.to_string(), category: None, description: None })
    }

    async fn delete_skill(&self, name: &str) -> Result<(), AgentError> {
        let path = self.find_skill(name)
            .ok_or_else(|| AgentError::ToolExecution(format!("Skill '{}' not found", name)))?;
        std::fs::remove_file(&path)
            .map_err(|e| AgentError::ToolExecution(format!("Delete: {}", e)))
    }
}

impl DiskSkillProvider {
    fn list_skills_recursive(&self, dir: &std::path::Path, skills: &mut Vec<SkillMeta>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                self.list_skills_recursive(&path, skills);
            } else if path.file_name().and_then(|f| f.to_str()) == Some("SKILL.md") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Some(fm) = Self::parse_frontmatter(&content) {
                        let name = fm.get("name").and_then(|v| v.as_str()).unwrap_or("")
                            .to_string();
                        let description = fm.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let category = path.parent()
                            .and_then(|p| p.parent())
                            .and_then(|p| p.file_name())
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_else(|| "general".to_string());
                        skills.push(SkillMeta { name, description: Some(description), category: Some(category) });
                    }
                }
            }
        }
    }
}
