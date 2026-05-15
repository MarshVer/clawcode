use serde::Serialize;

use claw_core::{limit_text_chars, ContextAssemblyBudget};

#[derive(Debug, Serialize)]
pub struct BudgetedReadFileOutput {
    #[serde(rename = "type")]
    kind: String,
    file: BudgetedTextFilePayload,
    truncated: bool,
}

#[derive(Debug, Serialize)]
struct BudgetedTextFilePayload {
    #[serde(rename = "filePath")]
    file_path: String,
    content: String,
    #[serde(rename = "numLines")]
    num_lines: usize,
    #[serde(rename = "startLine")]
    start_line: usize,
    #[serde(rename = "totalLines")]
    total_lines: usize,
    #[serde(rename = "contentBytes")]
    content_bytes: usize,
}

#[derive(Debug, Serialize)]
pub struct BudgetedWriteFileOutput {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "filePath")]
    file_path: String,
    #[serde(rename = "contentBytes")]
    content_bytes: usize,
    #[serde(rename = "originalBytes")]
    original_bytes: Option<usize>,
    #[serde(rename = "oldLines")]
    old_lines: usize,
    #[serde(rename = "newLines")]
    new_lines: usize,
    #[serde(rename = "structuredPatch")]
    structured_patch: Vec<BudgetedPatchHunk>,
    truncated: bool,
    note: String,
}

#[derive(Debug, Serialize)]
pub struct BudgetedEditFileOutput {
    #[serde(rename = "filePath")]
    file_path: String,
    #[serde(rename = "oldString")]
    old_string: String,
    #[serde(rename = "newString")]
    new_string: String,
    #[serde(rename = "oldStringBytes")]
    old_string_bytes: usize,
    #[serde(rename = "newStringBytes")]
    new_string_bytes: usize,
    #[serde(rename = "originalBytes")]
    original_bytes: usize,
    #[serde(rename = "oldLines")]
    old_lines: usize,
    #[serde(rename = "newLines")]
    new_lines: usize,
    #[serde(rename = "structuredPatch")]
    structured_patch: Vec<BudgetedPatchHunk>,
    #[serde(rename = "userModified")]
    user_modified: bool,
    #[serde(rename = "replaceAll")]
    replace_all: bool,
    truncated: bool,
    note: String,
}

#[derive(Debug, Serialize)]
struct BudgetedPatchHunk {
    #[serde(rename = "oldStart")]
    old_start: usize,
    #[serde(rename = "oldLines")]
    old_lines: usize,
    #[serde(rename = "newStart")]
    new_start: usize,
    #[serde(rename = "newLines")]
    new_lines: usize,
    lines: Vec<String>,
    truncated: bool,
}

pub fn budget_read_file_output(output: runtime::ReadFileOutput) -> BudgetedReadFileOutput {
    let content_bytes = output.file.content.len();
    let budgeted = limit_text_chars(
        &output.file.content,
        ContextAssemblyBudget::default().tool_text_preview_chars,
        "read_file output",
    );
    BudgetedReadFileOutput {
        kind: output.kind,
        file: BudgetedTextFilePayload {
            file_path: output.file.file_path,
            content: budgeted.text,
            num_lines: output.file.num_lines,
            start_line: output.file.start_line,
            total_lines: output.file.total_lines,
            content_bytes,
        },
        truncated: budgeted.truncated,
    }
}

pub fn budget_write_file_output(output: runtime::WriteFileOutput) -> BudgetedWriteFileOutput {
    let content_bytes = output.content.len();
    let original_bytes = output.original_file.as_ref().map(String::len);
    let patch = budget_patch(output.structured_patch);
    let patch_truncated = patch.iter().any(|hunk| hunk.truncated);
    BudgetedWriteFileOutput {
        kind: output.kind,
        file_path: output.file_path,
        content_bytes,
        original_bytes,
        old_lines: output.original_file.as_deref().map_or(0, |value| value.lines().count()),
        new_lines: output.content.lines().count(),
        truncated: patch_truncated,
        structured_patch: patch,
        note: String::from(
            "Full file contents are omitted from tool output to keep conversation context small. Use read_file with offset/limit to inspect specific ranges.",
        ),
    }
}

pub fn budget_edit_file_output(output: runtime::EditFileOutput) -> BudgetedEditFileOutput {
    let old_string_bytes = output.old_string.len();
    let new_string_bytes = output.new_string.len();
    let old_budgeted = limit_text_chars(
        &output.old_string,
        ContextAssemblyBudget::default().tool_string_preview_chars,
        "edit_file old_string",
    );
    let new_budgeted = limit_text_chars(
        &output.new_string,
        ContextAssemblyBudget::default().tool_string_preview_chars,
        "edit_file new_string",
    );
    let old_lines = output
        .structured_patch
        .first()
        .map_or_else(|| output.original_file.lines().count(), |hunk| hunk.old_lines);
    let new_lines = output
        .structured_patch
        .first()
        .map_or(old_lines, |hunk| hunk.new_lines);
    let patch = budget_patch(output.structured_patch);
    let patch_truncated = patch.iter().any(|hunk| hunk.truncated);
    let original_bytes = output.original_file.len();
    BudgetedEditFileOutput {
        file_path: output.file_path,
        old_string: old_budgeted.text,
        new_string: new_budgeted.text,
        old_string_bytes,
        new_string_bytes,
        original_bytes,
        old_lines,
        new_lines,
        structured_patch: patch,
        user_modified: output.user_modified,
        replace_all: output.replace_all,
        truncated: old_budgeted.truncated || new_budgeted.truncated || patch_truncated,
        note: String::from(
            "Original file content is omitted from tool output to keep conversation context small. Use read_file with offset/limit to inspect specific ranges.",
        ),
    }
}

fn budget_patch(hunks: Vec<runtime::StructuredPatchHunk>) -> Vec<BudgetedPatchHunk> {
    let max_patch_lines = ContextAssemblyBudget::default().tool_patch_preview_lines;
    hunks
        .into_iter()
        .map(|hunk| {
            let truncated = hunk.lines.len() > max_patch_lines;
            let mut lines = hunk
                .lines
                .into_iter()
                .take(max_patch_lines)
                .collect::<Vec<_>>();
            if truncated {
                lines.push(format!(
                    "[patch truncated: showing first {max_patch_lines} lines]"
                ));
            }
            BudgetedPatchHunk {
                old_start: hunk.old_start,
                old_lines: hunk.old_lines,
                new_start: hunk.new_start,
                new_lines: hunk.new_lines,
                lines,
                truncated,
            }
        })
        .collect()
}
