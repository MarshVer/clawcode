use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

const DEFAULT_INPUT_COST_PER_MILLION: f64 = 15.0;
const DEFAULT_OUTPUT_COST_PER_MILLION: f64 = 75.0;
const DEFAULT_CACHE_CREATION_COST_PER_MILLION: f64 = 18.75;
const DEFAULT_CACHE_READ_COST_PER_MILLION: f64 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextBudget {
    pub max_chars: usize,
    pub label: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetedText {
    pub text: String,
    pub original_chars: usize,
    pub omitted_chars: usize,
    pub truncated: bool,
}

impl TextBudget {
    #[must_use]
    pub const fn new(max_chars: usize, label: &'static str) -> Self {
        Self { max_chars, label }
    }

    #[must_use]
    pub fn apply(self, value: &str) -> BudgetedText {
        limit_text_chars(value, self.max_chars, self.label)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineBudget {
    pub max_lines: usize,
    pub label: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetedLines {
    pub text: String,
    pub original_lines: usize,
    pub omitted_lines: usize,
    pub truncated: bool,
}

impl LineBudget {
    #[must_use]
    pub const fn new(max_lines: usize, label: &'static str) -> Self {
        Self { max_lines, label }
    }

    #[must_use]
    pub fn apply(self, value: &str) -> BudgetedLines {
        limit_text_lines(value, self.max_lines, self.label)
    }
}

#[must_use]
pub fn limit_text_chars(value: &str, max_chars: usize, label: &str) -> BudgetedText {
    let original_chars = value.chars().count();
    if original_chars <= max_chars {
        return BudgetedText {
            text: value.to_string(),
            original_chars,
            omitted_chars: 0,
            truncated: false,
        };
    }

    let mut text = value.chars().take(max_chars).collect::<String>();
    let omitted_chars = original_chars.saturating_sub(max_chars);
    text.push_str(&format!(
        "\n... truncated {omitted_chars} additional character(s) from {label}."
    ));
    BudgetedText {
        text,
        original_chars,
        omitted_chars,
        truncated: true,
    }
}

#[must_use]
pub fn limit_text_lines(value: &str, max_lines: usize, label: &str) -> BudgetedLines {
    let lines = value.lines().collect::<Vec<_>>();
    let original_lines = lines.len();
    if original_lines <= max_lines {
        return BudgetedLines {
            text: value.to_string(),
            original_lines,
            omitted_lines: 0,
            truncated: false,
        };
    }

    let omitted_lines = original_lines.saturating_sub(max_lines);
    let mut text = lines
        .into_iter()
        .take(max_lines)
        .collect::<Vec<_>>()
        .join("\n");
    text.push_str(&format!(
        "\n... omitted {omitted_lines} additional line(s) from {label}."
    ));
    BudgetedLines {
        text,
        original_lines,
        omitted_lines,
        truncated: true,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextAssemblyBudget {
    pub instruction_file_chars: usize,
    pub total_instruction_chars: usize,
    pub git_status_lines: usize,
    pub git_diff_chars_per_section: usize,
    pub staged_files: usize,
    pub tool_text_preview_chars: usize,
    pub tool_string_preview_chars: usize,
    pub tool_patch_preview_lines: usize,
    pub hook_message_chars: usize,
    pub mcp_result_chars: usize,
    pub diff_report_chars_per_section: usize,
    pub diff_json_chars_per_section: usize,
}

impl Default for ContextAssemblyBudget {
    fn default() -> Self {
        Self {
            instruction_file_chars: 4_000,
            total_instruction_chars: 12_000,
            git_status_lines: 200,
            git_diff_chars_per_section: 16_000,
            staged_files: 200,
            tool_text_preview_chars: 8_000,
            tool_string_preview_chars: 2_000,
            tool_patch_preview_lines: 80,
            hook_message_chars: 4_000,
            mcp_result_chars: 16_000,
            diff_report_chars_per_section: 24_000,
            diff_json_chars_per_section: 24_000,
        }
    }
}

impl ContextAssemblyBudget {
    #[must_use]
    pub const fn instruction_file(self) -> TextBudget {
        TextBudget::new(self.instruction_file_chars, "instruction file")
    }

    #[must_use]
    pub const fn git_status(self) -> LineBudget {
        LineBudget::new(self.git_status_lines, "git status")
    }

    #[must_use]
    pub const fn git_diff(self) -> TextBudget {
        TextBudget::new(self.git_diff_chars_per_section, "git diff")
    }

    #[must_use]
    pub const fn hook_message(self) -> TextBudget {
        TextBudget::new(self.hook_message_chars, "hook output")
    }

    #[must_use]
    pub const fn mcp_result(self) -> TextBudget {
        TextBudget::new(self.mcp_result_chars, "MCP tool result")
    }

    #[must_use]
    pub const fn diff_report(self) -> TextBudget {
        TextBudget::new(self.diff_report_chars_per_section, "diff report")
    }

    #[must_use]
    pub const fn diff_json(self) -> TextBudget {
        TextBudget::new(self.diff_json_chars_per_section, "diff JSON")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelCapability {
    pub max_output_tokens: u32,
    pub context_window_tokens: u32,
    pub max_prompt_tokens: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPricing {
    pub input_cost_per_million: f64,
    pub output_cost_per_million: f64,
    pub cache_creation_cost_per_million: f64,
    pub cache_read_cost_per_million: f64,
}

impl ModelPricing {
    #[must_use]
    pub const fn default_sonnet_tier() -> Self {
        Self {
            input_cost_per_million: DEFAULT_INPUT_COST_PER_MILLION,
            output_cost_per_million: DEFAULT_OUTPUT_COST_PER_MILLION,
            cache_creation_cost_per_million: DEFAULT_CACHE_CREATION_COST_PER_MILLION,
            cache_read_cost_per_million: DEFAULT_CACHE_READ_COST_PER_MILLION,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_creation_input_tokens: u32,
    pub cache_read_input_tokens: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UsageCostEstimate {
    pub input_cost_usd: f64,
    pub output_cost_usd: f64,
    pub cache_creation_cost_usd: f64,
    pub cache_read_cost_usd: f64,
}

impl UsageCostEstimate {
    #[must_use]
    pub fn total_cost_usd(self) -> f64 {
        self.input_cost_usd
            + self.output_cost_usd
            + self.cache_creation_cost_usd
            + self.cache_read_cost_usd
    }
}

#[must_use]
pub fn pricing_for_model(model: &str) -> Option<ModelPricing> {
    let normalized = model.to_ascii_lowercase();
    if normalized.contains("haiku") {
        return Some(ModelPricing {
            input_cost_per_million: 1.0,
            output_cost_per_million: 5.0,
            cache_creation_cost_per_million: 1.25,
            cache_read_cost_per_million: 0.1,
        });
    }
    if normalized.contains("opus") {
        return Some(ModelPricing {
            input_cost_per_million: 15.0,
            output_cost_per_million: 75.0,
            cache_creation_cost_per_million: 18.75,
            cache_read_cost_per_million: 1.5,
        });
    }
    if normalized.contains("sonnet") {
        return Some(ModelPricing::default_sonnet_tier());
    }
    None
}

impl TokenUsage {
    #[must_use]
    pub fn total_tokens(self) -> u32 {
        self.input_tokens
            + self.output_tokens
            + self.cache_creation_input_tokens
            + self.cache_read_input_tokens
    }

    #[must_use]
    pub fn estimate_cost_usd(self) -> UsageCostEstimate {
        self.estimate_cost_usd_with_pricing(ModelPricing::default_sonnet_tier())
    }

    #[must_use]
    pub fn estimate_cost_usd_with_pricing(self, pricing: ModelPricing) -> UsageCostEstimate {
        UsageCostEstimate {
            input_cost_usd: cost_for_tokens(self.input_tokens, pricing.input_cost_per_million),
            output_cost_usd: cost_for_tokens(self.output_tokens, pricing.output_cost_per_million),
            cache_creation_cost_usd: cost_for_tokens(
                self.cache_creation_input_tokens,
                pricing.cache_creation_cost_per_million,
            ),
            cache_read_cost_usd: cost_for_tokens(
                self.cache_read_input_tokens,
                pricing.cache_read_cost_per_million,
            ),
        }
    }

    #[must_use]
    pub fn summary_lines(self, label: &str) -> Vec<String> {
        self.summary_lines_for_model(label, None)
    }

    #[must_use]
    pub fn summary_lines_for_model(self, label: &str, model: Option<&str>) -> Vec<String> {
        let pricing = model.and_then(pricing_for_model);
        let cost = pricing.map_or_else(
            || self.estimate_cost_usd(),
            |pricing| self.estimate_cost_usd_with_pricing(pricing),
        );
        let model_suffix =
            model.map_or_else(String::new, |model_name| format!(" model={model_name}"));
        let pricing_suffix = if pricing.is_some() {
            ""
        } else if model.is_some() {
            " pricing=estimated-default"
        } else {
            ""
        };
        vec![
            format!(
                "{label}: total_tokens={} input={} output={} cache_write={} cache_read={} estimated_cost={}{}{}",
                self.total_tokens(),
                self.input_tokens,
                self.output_tokens,
                self.cache_creation_input_tokens,
                self.cache_read_input_tokens,
                format_usd(cost.total_cost_usd()),
                model_suffix,
                pricing_suffix,
            ),
            format!(
                "  cost breakdown: input={} output={} cache_write={} cache_read={}",
                format_usd(cost.input_cost_usd),
                format_usd(cost.output_cost_usd),
                format_usd(cost.cache_creation_cost_usd),
                format_usd(cost.cache_read_cost_usd),
            ),
        ]
    }
}

fn cost_for_tokens(tokens: u32, usd_per_million_tokens: f64) -> f64 {
    f64::from(tokens) / 1_000_000.0 * usd_per_million_tokens
}

#[must_use]
pub fn format_usd(amount: f64) -> String {
    format!("${amount:.4}")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthTokenSet {
    #[serde(rename = "accessToken", alias = "access_token")]
    pub access_token: String,
    #[serde(rename = "refreshToken", alias = "refresh_token")]
    pub refresh_token: Option<String>,
    #[serde(rename = "expiresAt", alias = "expires_at")]
    pub expires_at: Option<u64>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SharedContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: String,
    },
    ToolResult {
        tool_use_id: String,
        tool_name: String,
        output: String,
        is_error: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedMessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedConversationMessage {
    pub role: SharedMessageRole,
    pub blocks: Vec<SharedContentBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPairingIssue {
    pub message_index: usize,
    pub tool_use_id: String,
    pub reason: String,
}

#[must_use]
pub fn validate_tool_result_pairing(messages: &[SharedConversationMessage]) -> Vec<ToolPairingIssue> {
    let mut issues = Vec::new();
    let mut pending_tool_use_ids = BTreeSet::new();

    for (index, message) in messages.iter().enumerate() {
        for block in &message.blocks {
            match block {
                SharedContentBlock::ToolUse { id, .. } if message.role == SharedMessageRole::Assistant => {
                    pending_tool_use_ids.insert(id.clone());
                }
                SharedContentBlock::ToolResult { tool_use_id, .. } => {
                    if !pending_tool_use_ids.remove(tool_use_id) {
                        issues.push(ToolPairingIssue {
                            message_index: index,
                            tool_use_id: tool_use_id.clone(),
                            reason: "tool_result has no preceding assistant tool_use".to_string(),
                        });
                    }
                }
                _ => {}
            }
        }

        if !message
            .blocks
            .iter()
            .any(|block| matches!(block, SharedContentBlock::ToolUse { .. }))
            && !message
                .blocks
                .iter()
                .any(|block| matches!(block, SharedContentBlock::ToolResult { .. }))
        {
            pending_tool_use_ids.clear();
        }
    }

    issues
}

#[must_use]
pub fn sanitize_tool_result_pairing(
    messages: &[SharedConversationMessage],
) -> Vec<SharedConversationMessage> {
    let mut pending_tool_use_ids = BTreeSet::new();
    let mut sanitized = Vec::with_capacity(messages.len());

    for message in messages {
        let mut blocks = Vec::with_capacity(message.blocks.len());
        for block in &message.blocks {
            match block {
                SharedContentBlock::ToolUse { id, .. }
                    if message.role == SharedMessageRole::Assistant =>
                {
                    pending_tool_use_ids.insert(id.clone());
                    blocks.push(block.clone());
                }
                SharedContentBlock::ToolResult { tool_use_id, .. } => {
                    if pending_tool_use_ids.remove(tool_use_id) {
                        blocks.push(block.clone());
                    }
                }
                _ => blocks.push(block.clone()),
            }
        }

        if !blocks
            .iter()
            .any(|block| matches!(block, SharedContentBlock::ToolUse { .. }))
            && !blocks
                .iter()
                .any(|block| matches!(block, SharedContentBlock::ToolResult { .. }))
        {
            pending_tool_use_ids.clear();
        }

        if !blocks.is_empty() {
            sanitized.push(SharedConversationMessage {
                role: message.role,
                blocks,
            });
        }
    }

    sanitized
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthConfig {
    pub client_id: String,
    pub authorize_url: String,
    pub token_url: String,
    pub callback_port: Option<u16>,
    pub manual_redirect_url: Option<String>,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkceCodePair {
    pub verifier: String,
    pub challenge: String,
    pub challenge_method: PkceChallengeMethod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PkceChallengeMethod {
    S256,
}

impl PkceChallengeMethod {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::S256 => "S256",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthAuthorizationRequest {
    pub authorize_url: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    pub state: String,
    pub code_challenge: String,
    pub code_challenge_method: PkceChallengeMethod,
    pub extra_params: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthTokenExchangeRequest {
    pub grant_type: &'static str,
    pub code: String,
    pub redirect_uri: String,
    pub client_id: String,
    pub code_verifier: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthRefreshRequest {
    pub grant_type: &'static str,
    pub refresh_token: String,
    pub client_id: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthCallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

impl OAuthAuthorizationRequest {
    #[must_use]
    pub fn from_config(
        config: &OAuthConfig,
        redirect_uri: impl Into<String>,
        state: impl Into<String>,
        pkce: &PkceCodePair,
    ) -> Self {
        Self {
            authorize_url: config.authorize_url.clone(),
            client_id: config.client_id.clone(),
            redirect_uri: redirect_uri.into(),
            scopes: config.scopes.clone(),
            state: state.into(),
            code_challenge: pkce.challenge.clone(),
            code_challenge_method: pkce.challenge_method,
            extra_params: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_extra_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_params.insert(key.into(), value.into());
        self
    }

    #[must_use]
    pub fn build_url(&self) -> String {
        let mut params = vec![
            ("response_type", "code".to_string()),
            ("client_id", self.client_id.clone()),
            ("redirect_uri", self.redirect_uri.clone()),
            ("scope", self.scopes.join(" ")),
            ("state", self.state.clone()),
            ("code_challenge", self.code_challenge.clone()),
            (
                "code_challenge_method",
                self.code_challenge_method.as_str().to_string(),
            ),
        ];
        params.extend(
            self.extra_params
                .iter()
                .map(|(key, value)| (key.as_str(), value.clone())),
        );
        let query = params
            .into_iter()
            .map(|(key, value)| format!("{}={}", percent_encode(key), percent_encode(&value)))
            .collect::<Vec<_>>()
            .join("&");
        format!(
            "{}{}{}",
            self.authorize_url,
            if self.authorize_url.contains('?') {
                '&'
            } else {
                '?'
            },
            query
        )
    }
}

impl OAuthTokenExchangeRequest {
    #[must_use]
    pub fn from_config(
        config: &OAuthConfig,
        code: impl Into<String>,
        state: impl Into<String>,
        verifier: impl Into<String>,
        redirect_uri: impl Into<String>,
    ) -> Self {
        Self {
            grant_type: "authorization_code",
            code: code.into(),
            redirect_uri: redirect_uri.into(),
            client_id: config.client_id.clone(),
            code_verifier: verifier.into(),
            state: state.into(),
        }
    }

    #[must_use]
    pub fn form_params(&self) -> BTreeMap<&str, String> {
        BTreeMap::from([
            ("grant_type", self.grant_type.to_string()),
            ("code", self.code.clone()),
            ("redirect_uri", self.redirect_uri.clone()),
            ("client_id", self.client_id.clone()),
            ("code_verifier", self.code_verifier.clone()),
            ("state", self.state.clone()),
        ])
    }
}

impl OAuthRefreshRequest {
    #[must_use]
    pub fn from_config(
        config: &OAuthConfig,
        refresh_token: impl Into<String>,
        scopes: Option<Vec<String>>,
    ) -> Self {
        Self {
            grant_type: "refresh_token",
            refresh_token: refresh_token.into(),
            client_id: config.client_id.clone(),
            scopes: scopes.unwrap_or_else(|| config.scopes.clone()),
        }
    }

    #[must_use]
    pub fn form_params(&self) -> BTreeMap<&str, String> {
        BTreeMap::from([
            ("grant_type", self.grant_type.to_string()),
            ("refresh_token", self.refresh_token.clone()),
            ("client_id", self.client_id.clone()),
            ("scope", self.scopes.join(" ")),
        ])
    }
}

pub fn generate_pkce_pair() -> io::Result<PkceCodePair> {
    let verifier = generate_random_token(32)?;
    Ok(PkceCodePair {
        challenge: code_challenge_s256(&verifier),
        verifier,
        challenge_method: PkceChallengeMethod::S256,
    })
}

pub fn generate_state() -> io::Result<String> {
    generate_random_token(32)
}

#[must_use]
pub fn code_challenge_s256(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64url_encode(&digest)
}

#[must_use]
pub fn loopback_redirect_uri(port: u16) -> String {
    format!("http://localhost:{port}/callback")
}

pub fn credentials_path() -> io::Result<PathBuf> {
    Ok(credentials_home_dir()?.join("credentials.json"))
}

pub fn load_oauth_credentials() -> io::Result<Option<OAuthTokenSet>> {
    let path = credentials_path()?;
    let root = read_credentials_root(&path)?;
    let Some(oauth) = root.get("oauth") else {
        return Ok(None);
    };
    if oauth.is_null() {
        return Ok(None);
    }
    serde_json::from_value::<OAuthTokenSet>(oauth.clone())
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

pub fn save_oauth_credentials(token_set: &OAuthTokenSet) -> io::Result<()> {
    let path = credentials_path()?;
    let mut root = read_credentials_root(&path)?;
    root.insert(
        "oauth".to_string(),
        serde_json::to_value(token_set)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
    );
    write_credentials_root(&path, &root)
}

pub fn clear_oauth_credentials() -> io::Result<()> {
    let path = credentials_path()?;
    let mut root = read_credentials_root(&path)?;
    root.remove("oauth");
    write_credentials_root(&path, &root)
}

pub fn parse_oauth_callback_request_target(target: &str) -> Result<OAuthCallbackParams, String> {
    let (path, query) = target
        .split_once('?')
        .map_or((target, ""), |(path, query)| (path, query));
    if path != "/callback" {
        return Err(format!("unexpected callback path: {path}"));
    }
    parse_oauth_callback_query(query)
}

pub fn parse_oauth_callback_query(query: &str) -> Result<OAuthCallbackParams, String> {
    let mut params = BTreeMap::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair
            .split_once('=')
            .map_or((pair, ""), |(key, value)| (key, value));
        params.insert(percent_decode(key)?, percent_decode(value)?);
    }
    Ok(OAuthCallbackParams {
        code: params.get("code").cloned(),
        state: params.get("state").cloned(),
        error: params.get("error").cloned(),
        error_description: params.get("error_description").cloned(),
    })
}

fn generate_random_token(bytes: usize) -> io::Result<String> {
    let mut buffer = vec![0_u8; bytes];
    File::open("/dev/urandom")?.read_exact(&mut buffer)?;
    Ok(base64url_encode(&buffer))
}

fn credentials_home_dir() -> io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("CLAW_CONFIG_HOME") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "HOME is not set (on Windows, set USERPROFILE or HOME, \
                 or use CLAW_CONFIG_HOME to point directly at the config directory)",
            )
        })?;
    Ok(PathBuf::from(home).join(".claw"))
}

fn read_credentials_root(path: &PathBuf) -> io::Result<Map<String, Value>> {
    match fs::read_to_string(path) {
        Ok(contents) => {
            if contents.trim().is_empty() {
                return Ok(Map::new());
            }
            serde_json::from_str::<Value>(&contents)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
                .as_object()
                .cloned()
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "credentials file must contain a JSON object",
                    )
                })
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Map::new()),
        Err(error) => Err(error),
    }
}

fn write_credentials_root(path: &PathBuf, root: &Map<String, Value>) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let rendered = serde_json::to_string_pretty(&Value::Object(root.clone()))
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, format!("{rendered}\n"))?;
    fs::rename(temp_path, path)
}

fn base64url_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::new();
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let block = (u32::from(bytes[index]) << 16)
            | (u32::from(bytes[index + 1]) << 8)
            | u32::from(bytes[index + 2]);
        output.push(TABLE[((block >> 18) & 0x3F) as usize] as char);
        output.push(TABLE[((block >> 12) & 0x3F) as usize] as char);
        output.push(TABLE[((block >> 6) & 0x3F) as usize] as char);
        output.push(TABLE[(block & 0x3F) as usize] as char);
        index += 3;
    }
    match bytes.len().saturating_sub(index) {
        1 => {
            let block = u32::from(bytes[index]) << 16;
            output.push(TABLE[((block >> 18) & 0x3F) as usize] as char);
            output.push(TABLE[((block >> 12) & 0x3F) as usize] as char);
        }
        2 => {
            let block = (u32::from(bytes[index]) << 16) | (u32::from(bytes[index + 1]) << 8);
            output.push(TABLE[((block >> 18) & 0x3F) as usize] as char);
            output.push(TABLE[((block >> 12) & 0x3F) as usize] as char);
            output.push(TABLE[((block >> 6) & 0x3F) as usize] as char);
        }
        _ => {}
    }
    output
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(&mut encoded, "%{byte:02X}");
            }
        }
    }
    encoded
}

fn percent_decode(value: &str) -> Result<String, String> {
    let mut decoded = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hi = decode_hex(bytes[index + 1])?;
                let lo = decode_hex(bytes[index + 2])?;
                decoded.push((hi << 4) | lo);
                index += 3;
            }
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).map_err(|error| error.to_string())
}

fn decode_hex(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(format!("invalid percent byte: {byte}")),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextBudget {
    pub estimated_input_tokens: u32,
    pub requested_output_tokens: u32,
    pub max_output_tokens: u32,
    pub context_window_tokens: Option<u32>,
    pub safety_margin_tokens: u32,
    pub available_output_tokens: u32,
}

impl ContextBudget {
    #[must_use]
    pub fn bounded_output_tokens(
        estimated_input_tokens: u32,
        requested_output_tokens: u32,
        model_max_output_tokens: u32,
        context_window_tokens: Option<u32>,
    ) -> Self {
        let safety_margin_tokens = context_window_tokens.map_or(0, context_window_safety_margin);
        let available_output_tokens = context_window_tokens.map_or(requested_output_tokens, |window| {
            window
                .saturating_sub(estimated_input_tokens)
                .saturating_sub(safety_margin_tokens)
        });
        let max_output_tokens = requested_output_tokens
            .min(model_max_output_tokens)
            .min(available_output_tokens)
            .max(1);
        Self {
            estimated_input_tokens,
            requested_output_tokens,
            max_output_tokens,
            context_window_tokens,
            safety_margin_tokens,
            available_output_tokens,
        }
    }

    #[must_use]
    pub fn should_compact_before_request(self) -> bool {
        self.context_window_tokens.is_some_and(|window| {
            let preflight_total = self
                .estimated_input_tokens
                .saturating_add(self.requested_output_tokens)
                .saturating_add(self.safety_margin_tokens);
            preflight_total > window
                || self.estimated_input_tokens
                    > window.saturating_sub(self.safety_margin_tokens).saturating_mul(3) / 4
        })
    }
}

#[must_use]
pub fn context_window_safety_margin(context_window_tokens: u32) -> u32 {
    (context_window_tokens / 50).clamp(1_000, 8_000)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellSegment<'a> {
    raw: &'a str,
}

impl<'a> ShellSegment<'a> {
    #[must_use]
    pub fn as_str(self) -> &'a str {
        self.raw
    }
}

#[must_use]
pub fn split_shell_segments(command: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut chars = command.chars().peekable();

    while let Some(ch) = chars.next() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            current.push(ch);
            escaped = true;
            continue;
        }
        if matches!(quote, Some(q) if q == ch) {
            quote = None;
            current.push(ch);
            continue;
        }
        if quote.is_none() && (ch == '\'' || ch == '"') {
            quote = Some(ch);
            current.push(ch);
            continue;
        }
        if quote.is_none() && matches!(ch, ';' | '|' | '&') {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                segments.push(trimmed.to_string());
            }
            current.clear();
            if chars.peek() == Some(&ch) {
                chars.next();
            }
            continue;
        }
        current.push(ch);
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        segments.push(trimmed.to_string());
    }
    segments
}

#[must_use]
pub fn has_write_redirection(command: &str) -> bool {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut chars = command.chars().peekable();

    while let Some(ch) = chars.next() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if matches!(quote, Some(q) if q == ch) {
            quote = None;
            continue;
        }
        if quote.is_none() && (ch == '\'' || ch == '"') {
            quote = Some(ch);
            continue;
        }
        if quote.is_some() {
            continue;
        }
        if ch == '>' {
            return true;
        }
        if ch.is_ascii_digit() && chars.peek() == Some(&'>') {
            return true;
        }
    }
    false
}

#[must_use]
pub fn first_shell_token(segment: &str) -> &str {
    segment
        .split_whitespace()
        .next()
        .unwrap_or("")
        .rsplit('/')
        .next()
        .unwrap_or("")
}

#[must_use]
pub fn is_read_only_shell_command(command: &str) -> bool {
    if command.trim().is_empty() || has_write_redirection(command) || command.contains("--in-place") {
        return false;
    }

    split_shell_segments(command).into_iter().all(|segment| {
        if segment.contains("-i ") || segment.ends_with(" -i") {
            return false;
        }
        let first_token = first_shell_token(&segment);
        if first_token == "git" {
            return is_read_only_git_command(&segment);
        }

        matches!(
            first_token,
            "cat"
                | "head"
                | "tail"
                | "less"
                | "more"
                | "wc"
                | "ls"
                | "find"
                | "grep"
                | "rg"
                | "awk"
                | "sed"
                | "echo"
                | "printf"
                | "which"
                | "where"
                | "whereis"
                | "whoami"
                | "pwd"
                | "env"
                | "printenv"
                | "date"
                | "cal"
                | "df"
                | "du"
                | "free"
                | "uptime"
                | "uname"
                | "file"
                | "stat"
                | "diff"
                | "sort"
                | "uniq"
                | "tr"
                | "cut"
                | "paste"
                | "xargs"
                | "test"
                | "true"
                | "false"
                | "type"
                | "readlink"
                | "realpath"
                | "basename"
                | "dirname"
                | "sha256sum"
                | "md5sum"
                | "b3sum"
                | "xxd"
                | "hexdump"
                | "od"
                | "strings"
                | "tree"
                | "jq"
                | "yq"
        )
    })
}

#[must_use]
pub fn is_read_only_git_command(command: &str) -> bool {
    const READ_ONLY_GIT_SUBCOMMANDS: &[&str] = &[
        "status",
        "log",
        "diff",
        "show",
        "branch",
        "tag",
        "remote",
        "ls-files",
        "ls-tree",
        "cat-file",
        "rev-parse",
        "describe",
        "shortlog",
        "blame",
        "reflog",
    ];

    let parts = command.split_whitespace().collect::<Vec<_>>();
    let mut index = 1;
    while index < parts.len() {
        let part = parts[index];
        if part == "-C" || part == "-c" {
            index += 2;
            continue;
        }
        if part.starts_with('-') {
            index += 1;
            continue;
        }
        return READ_ONLY_GIT_SUBCOMMANDS.contains(&part);
    }
    true
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathPolicy {
    root: PathBuf,
}

impl PathPolicy {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn canonical_root(&self) -> io::Result<PathBuf> {
        self.root.canonicalize()
    }

    pub fn resolve_existing(&self, path: &str) -> io::Result<PathBuf> {
        let root = self.canonical_root()?;
        let candidate = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            root.join(path)
        };
        let resolved = candidate.canonicalize()?;
        ensure_within_workspace(&resolved, &root)?;
        Ok(resolved)
    }

    pub fn resolve_missing(&self, path: &str) -> io::Result<PathBuf> {
        let root = self.canonical_root()?;
        let candidate = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            root.join(path)
        };

        if let Ok(canonical) = candidate.canonicalize() {
            ensure_within_workspace(&canonical, &root)?;
            return Ok(canonical);
        }

        let file_name = candidate.file_name().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("path {} has no file name", candidate.display()),
            )
        })?;
        let mut missing_components = vec![file_name.to_os_string()];
        let mut ancestor = candidate.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("path {} has no parent directory", candidate.display()),
            )
        })?;

        while !ancestor.exists() {
            let name = ancestor.file_name().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("no existing ancestor found for {}", candidate.display()),
                )
            })?;
            missing_components.push(name.to_os_string());
            ancestor = ancestor.parent().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("no existing ancestor found for {}", candidate.display()),
                )
            })?;
        }

        let mut resolved = ancestor.canonicalize()?;
        ensure_within_workspace(&resolved, &root)?;
        for component in missing_components.into_iter().rev() {
            resolved.push(component);
        }
        ensure_within_workspace(&resolved, &root)?;
        Ok(resolved)
    }

    #[must_use]
    pub fn is_within_workspace(&self, path: &Path) -> bool {
        let Ok(root) = self.canonical_root().or_else(|_| lexical_absolute(&self.root)) else {
            return false;
        };
        let candidate = if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        };
        let resolved = candidate
            .canonicalize()
            .or_else(|_| lexical_absolute(&candidate))
            .unwrap_or(candidate);
        resolved.starts_with(root)
    }
}

pub fn ensure_within_workspace(path: &Path, root: &Path) -> io::Result<()> {
    let normalized_path = path.canonicalize().or_else(|_| lexical_absolute(path))?;
    let normalized_root = root.canonicalize().or_else(|_| lexical_absolute(root))?;
    if !normalized_path.starts_with(&normalized_root) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "path {} escapes workspace boundary {}",
                path.display(),
                root.display()
            ),
        ));
    }
    Ok(())
}

fn lexical_absolute(path: &Path) -> io::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(normalize_lexically(&absolute))
}

fn normalize_lexically(path: &Path) -> PathBuf {
    let mut prefix: Option<OsString> = None;
    let mut has_root = false;
    let mut parts = Vec::new();

    for component in path.components() {
        match component {
            Component::Prefix(value) => prefix = Some(value.as_os_str().to_os_string()),
            Component::RootDir => has_root = true,
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            Component::Normal(value) => parts.push(value.to_os_string()),
        }
    }

    let mut normalized = PathBuf::new();
    if let Some(value) = prefix {
        normalized.push(value);
    }
    if has_root {
        normalized.push(std::path::MAIN_SEPARATOR.to_string());
    }
    for part in parts {
        normalized.push(part);
    }
    normalized
}

#[must_use]
pub fn required_tool_names_for_budget() -> BTreeSet<&'static str> {
    BTreeSet::from(["read_file", "write_file", "edit_file", "grep_search"])
}

#[cfg(test)]
mod tests {
    use super::{
        sanitize_tool_result_pairing, OAuthTokenSet, SharedContentBlock,
        SharedConversationMessage, SharedMessageRole,
    };

    #[test]
    fn oauth_token_set_accepts_api_snake_case_and_serializes_stored_camel_case() {
        let token: OAuthTokenSet = serde_json::from_str(
            r#"{"access_token":"access","refresh_token":"refresh","expires_at":42,"scopes":["a"]}"#,
        )
        .expect("snake_case API token should parse");

        assert_eq!(token.access_token, "access");
        assert_eq!(token.refresh_token.as_deref(), Some("refresh"));
        assert_eq!(token.expires_at, Some(42));

        let stored = serde_json::to_value(&token).expect("token should serialize");
        assert_eq!(stored["accessToken"], "access");
        assert_eq!(stored["refreshToken"], "refresh");
        assert_eq!(stored["expiresAt"], 42);
    }

    #[test]
    fn sanitize_tool_result_pairing_drops_orphan_results() {
        let messages = vec![
            SharedConversationMessage {
                role: SharedMessageRole::User,
                blocks: vec![SharedContentBlock::ToolResult {
                    tool_use_id: "orphan".to_string(),
                    tool_name: "bash".to_string(),
                    output: "ignored".to_string(),
                    is_error: false,
                }],
            },
            SharedConversationMessage {
                role: SharedMessageRole::Assistant,
                blocks: vec![SharedContentBlock::ToolUse {
                    id: "tool-1".to_string(),
                    name: "bash".to_string(),
                    input: "{}".to_string(),
                }],
            },
            SharedConversationMessage {
                role: SharedMessageRole::Tool,
                blocks: vec![SharedContentBlock::ToolResult {
                    tool_use_id: "tool-1".to_string(),
                    tool_name: "bash".to_string(),
                    output: "ok".to_string(),
                    is_error: false,
                }],
            },
        ];

        let sanitized = sanitize_tool_result_pairing(&messages);
        assert_eq!(sanitized.len(), 2);
        assert_eq!(sanitized[0].role, SharedMessageRole::Assistant);
        assert_eq!(sanitized[1].role, SharedMessageRole::Tool);
    }
}
