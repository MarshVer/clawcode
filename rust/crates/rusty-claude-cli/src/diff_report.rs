use std::path::Path;
use std::process::Command;

use claw_core::{limit_text_chars, ContextAssemblyBudget};
use serde_json::json;

pub fn render_diff_report_for(cwd: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let in_git_repo = is_inside_git_repo(cwd);
    if !in_git_repo {
        return Ok(format!(
            "Diff\n  Result           no git repository\n  Detail           {} is not inside a git project",
            cwd.display()
        ));
    }
    let staged = run_git_diff_command_in(cwd, &["diff", "--cached", "--stat=100,60,200"])?;
    let unstaged = run_git_diff_command_in(cwd, &["diff", "--stat=100,60,200"])?;
    if staged.trim().is_empty() && unstaged.trim().is_empty() {
        return Ok(
            "Diff\n  Result           clean working tree\n  Detail           no current changes"
                .to_string(),
        );
    }

    let mut sections = Vec::new();
    if !staged.trim().is_empty() {
        sections.push(format!(
            "Staged changes:\n{}",
            limit_text_chars(
                staged.trim_end(),
                ContextAssemblyBudget::default().diff_report_chars_per_section,
                "/diff staged output",
            )
            .text
        ));
    }
    if !unstaged.trim().is_empty() {
        sections.push(format!(
            "Unstaged changes:\n{}",
            limit_text_chars(
                unstaged.trim_end(),
                ContextAssemblyBudget::default().diff_report_chars_per_section,
                "/diff unstaged output",
            )
            .text
        ));
    }

    Ok(format!(
        "Diff\n  Mode             summary\n  Detail           showing git diff stats; run git diff directly for the full patch\n\n{}",
        sections.join("\n\n")
    ))
}

pub fn render_diff_json_for(cwd: &Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    if !is_inside_git_repo(cwd) {
        return Ok(json!({
            "kind": "diff",
            "result": "no_git_repo",
            "detail": format!("{} is not inside a git project", cwd.display()),
        }));
    }
    let staged = run_git_diff_command_in(cwd, &["diff", "--cached", "--stat=100,60,200"])?;
    let unstaged = run_git_diff_command_in(cwd, &["diff", "--stat=100,60,200"])?;
    let staged_budgeted = limit_text_chars(
        staged.trim(),
        ContextAssemblyBudget::default().diff_json_chars_per_section,
        "/diff staged JSON",
    );
    let unstaged_budgeted = limit_text_chars(
        unstaged.trim(),
        ContextAssemblyBudget::default().diff_json_chars_per_section,
        "/diff unstaged JSON",
    );
    Ok(json!({
        "kind": "diff",
        "mode": "summary",
        "result": if staged.trim().is_empty() && unstaged.trim().is_empty() { "clean" } else { "changes" },
        "staged": staged_budgeted.text,
        "unstaged": unstaged_budgeted.text,
        "truncated": staged_budgeted.truncated || unstaged_budgeted.truncated,
        "fullCommand": "git diff && git diff --cached"
    }))
}

fn is_inside_git_repo(cwd: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(cwd)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn run_git_diff_command_in(
    cwd: &Path,
    args: &[&str],
) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("git").args(args).current_dir(cwd).output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git {} failed: {stderr}", args.join(" ")).into());
    }
    Ok(String::from_utf8(output.stdout)?)
}
