mod client;
mod error;
mod http_client;
mod prompt_cache;
mod providers;
mod sse;
mod types;

pub use client::{
    oauth_token_is_expired, read_base_url, read_xai_base_url, resolve_saved_oauth_token,
    resolve_startup_auth_source, MessageStream, ProviderClient,
};
pub use claw_core::OAuthTokenSet;
pub use error::ApiError;
pub use http_client::{
    build_http_client, build_http_client_or_default, build_http_client_with, ProxyConfig,
};
pub use prompt_cache::{
    CacheBreakEvent, PromptCache, PromptCacheConfig, PromptCachePaths, PromptCacheRecord,
    PromptCacheStats,
};
pub use providers::anthropic::{AnthropicClient, AnthropicClient as ApiClient, AuthSource};
pub use providers::openai_compat::{OpenAiCompatClient, OpenAiCompatConfig};
pub use providers::{
    context_budget_for_request, context_budget_for_request_with_requested, detect_provider_kind,
    dynamic_max_tokens_for_request, dynamic_max_tokens_for_request_with_requested,
    estimate_message_request_input_tokens,
    max_tokens_for_model, max_tokens_for_model_with_override, metadata_for_model,
    resolve_model_alias, ProviderKind, ProviderMetadata,
};
pub use sse::{parse_frame, SseParser};
pub use types::{
    ContentBlockDelta, ContentBlockDeltaEvent, ContentBlockStartEvent, ContentBlockStopEvent,
    InputContentBlock, InputMessage, MessageDelta, MessageDeltaEvent, MessageRequest,
    MessageResponse, MessageStartEvent, MessageStopEvent, OutputContentBlock, StreamEvent,
    RuntimeContentBlock, RuntimeConversationMessage, RuntimeMessageRole, ToolChoice,
    ToolDefinition, ToolResultContentBlock, Usage, runtime_messages_to_input_messages,
};

pub use telemetry::{
    AnalyticsEvent, AnthropicRequestProfile, ClientIdentity, JsonlTelemetrySink,
    MemoryTelemetrySink, SessionTraceRecord, SessionTracer, TelemetryEvent, TelemetrySink,
    DEFAULT_ANTHROPIC_VERSION,
};
