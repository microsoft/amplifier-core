# Polyglot Contracts Implementation Plan

> **Execution:** Use the subagent-driven-development workflow to implement this plan.

**Goal:** Enable community module development in any language (Rust, Python, Go, TypeScript, C#, WASM) with proto as the single source of truth for all contracts.

**Architecture:** Proto files define every module type, message, and error. The Rust kernel (amplifier-core) hosts a KernelService gRPC server and dispatches to modules via four transports: python (PyO3, unchanged), native (Rust linked directly), grpc (any language, out-of-process), and wasm (in-process, sandboxed). Transport bridges wrap modules as `Arc<dyn Trait>` — the kernel never knows or cares about the module's language.

**Tech Stack:** Protocol Buffers 3, tonic/prost (Rust gRPC), grpcio (Python gRPC), wasmtime (WASM runtime), protoc with plugins for Go/TypeScript/C#, cargo + maturin + uv build toolchain.

**Design Document:** `docs/plans/2026-03-01-polyglot-contracts-design.md`

---

## Repository Layout Reference

```
amplifier-core/
├── proto/                          # Proto source of truth
│   └── amplifier_module.proto      # Existing: ToolService only (39 lines)
├── crates/amplifier-core/
│   └── src/
│       ├── traits.rs               # 6 module trait definitions
│       ├── models.rs               # Data models (HookResult, ToolResult, etc.)
│       ├── messages.rs             # Chat protocol (ChatRequest, ChatResponse, etc.)
│       ├── errors.rs               # Error taxonomy (ProviderError, ToolError, etc.)
│       ├── events.rs               # 51 canonical event name constants
│       ├── hooks.rs                # HookRegistry dispatch pipeline
│       ├── coordinator.rs          # Module mount points + capabilities
│       ├── session.rs              # Session lifecycle
│       └── lib.rs                  # Crate root with re-exports
├── bindings/python/
│   └── src/lib.rs                  # PyO3 bridge (2675 lines)
├── python/amplifier_core/
│   ├── interfaces.py               # Hand-written Protocol classes (280 lines)
│   ├── _engine.pyi                 # Hand-written type stubs (240 lines)
│   ├── _grpc_gen/                  # Generated Python gRPC stubs (Tool only)
│   ├── loader_dispatch.py          # Transport router
│   ├── loader_grpc.py              # GrpcToolBridge
│   └── validation/                 # Hand-written validation (~3287 lines)
└── tests/                          # Python pytest tests
```

**Build commands:**
- Rust tests: `cargo test -p amplifier-core`
- Python build: `cd amplifier-core && maturin develop`
- Python tests: `cd amplifier-core && uv run pytest`
- Full cycle: `cd amplifier-core && maturin develop && uv run pytest`

---

## Phase 1: Proto Expansion (No Runtime Changes)

**Goal:** Expand `proto/amplifier_module.proto` from 1 service (ToolService) to all 8 services (6 module services + KernelService + ModuleLifecycle). Define all typed messages. Generate stubs for Python, Rust, Go, TypeScript, C#. Set up CI pipeline.

**Invariant:** Existing Python path works unchanged throughout. No runtime code is modified.

---

### Task 1: Common messages — ModuleInfo, MountRequest, HealthCheck, errors

**Files:**
- Modify: `proto/amplifier_module.proto`

**Step 1: Add common enums and messages to the proto file**

Open `proto/amplifier_module.proto` and add the following after the existing `ToolExecuteResponse` message (after line 39). Keep the existing ToolService, Empty, ToolSpec, ToolExecuteRequest, and ToolExecuteResponse exactly as they are.

```protobuf
// ===========================================================================
// Common enums
// ===========================================================================

enum ModuleType {
  MODULE_TYPE_UNSPECIFIED = 0;
  MODULE_TYPE_ORCHESTRATOR = 1;
  MODULE_TYPE_PROVIDER = 2;
  MODULE_TYPE_TOOL = 3;
  MODULE_TYPE_CONTEXT = 4;
  MODULE_TYPE_HOOK = 5;
  MODULE_TYPE_APPROVAL = 6;
}

enum HealthStatus {
  HEALTH_STATUS_UNSPECIFIED = 0;
  HEALTH_STATUS_HEALTHY = 1;
  HEALTH_STATUS_DEGRADED = 2;
  HEALTH_STATUS_UNHEALTHY = 3;
}

enum ConfigFieldType {
  CONFIG_FIELD_TYPE_UNSPECIFIED = 0;
  CONFIG_FIELD_TYPE_TEXT = 1;
  CONFIG_FIELD_TYPE_SECRET = 2;
  CONFIG_FIELD_TYPE_CHOICE = 3;
  CONFIG_FIELD_TYPE_BOOLEAN = 4;
}

// ===========================================================================
// Module identity and lifecycle messages
// ===========================================================================

message ModuleInfo {
  string id = 1;
  string name = 2;
  string version = 3;
  ModuleType module_type = 4;
  string mount_point = 5;
  string description = 6;
  string config_schema_json = 7;  // JSON Schema as string
  repeated string capabilities = 8;
  string author = 9;
}

message MountRequest {
  map<string, string> config = 1;
  string module_id = 2;
}

message MountResponse {
  bool success = 1;
  string error = 2;
  HealthStatus status = 3;
}

message HealthCheckResponse {
  HealthStatus status = 1;
  string message = 2;
}

message ConfigField {
  string id = 1;
  string display_name = 2;
  ConfigFieldType field_type = 3;
  string prompt = 4;
  string env_var = 5;
  repeated string choices = 6;
  bool required = 7;
  string default_value = 8;
  map<string, string> show_when = 9;
  bool requires_model = 10;
}

// ===========================================================================
// Error types — consistent taxonomy across all languages
// ===========================================================================

enum ProviderErrorType {
  PROVIDER_ERROR_TYPE_UNSPECIFIED = 0;
  PROVIDER_ERROR_TYPE_RATE_LIMIT = 1;
  PROVIDER_ERROR_TYPE_AUTHENTICATION = 2;
  PROVIDER_ERROR_TYPE_CONTEXT_LENGTH = 3;
  PROVIDER_ERROR_TYPE_CONTENT_FILTER = 4;
  PROVIDER_ERROR_TYPE_INVALID_REQUEST = 5;
  PROVIDER_ERROR_TYPE_UNAVAILABLE = 6;
  PROVIDER_ERROR_TYPE_TIMEOUT = 7;
  PROVIDER_ERROR_TYPE_OTHER = 8;
}

enum ToolErrorType {
  TOOL_ERROR_TYPE_UNSPECIFIED = 0;
  TOOL_ERROR_TYPE_EXECUTION_FAILED = 1;
  TOOL_ERROR_TYPE_NOT_FOUND = 2;
  TOOL_ERROR_TYPE_OTHER = 3;
}

enum HookErrorType {
  HOOK_ERROR_TYPE_UNSPECIFIED = 0;
  HOOK_ERROR_TYPE_HANDLER_FAILED = 1;
  HOOK_ERROR_TYPE_TIMEOUT = 2;
  HOOK_ERROR_TYPE_OTHER = 3;
}

message ProviderError {
  ProviderErrorType error_type = 1;
  string message = 2;
  string provider = 3;
  string model = 4;
  double retry_after = 5;
  bool retryable = 6;
  int32 status_code = 7;
}

message ToolError {
  ToolErrorType error_type = 1;
  string message = 2;
  string stdout = 3;
  string stderr = 4;
  int32 exit_code = 5;
  string tool_name = 6;
}

message HookError {
  HookErrorType error_type = 1;
  string message = 2;
  string handler_name = 3;
}

message AmplifierError {
  oneof error {
    ProviderError provider_error = 1;
    ToolError tool_error = 2;
    HookError hook_error = 3;
    string session_error = 4;
    string context_error = 5;
  }
}
```

**Step 2: Verify proto compiles**

Run:
```bash
cd amplifier-core && protoc --proto_path=proto --python_out=/tmp/proto_test proto/amplifier_module.proto
```
Expected: Exit code 0, no errors.

**Step 3: Commit**
```bash
cd amplifier-core && git add proto/amplifier_module.proto && git commit -m "proto: add common messages — ModuleInfo, lifecycle, error taxonomy"
```

---

### Task 2: LLM conversation messages — ChatRequest, ChatResponse, Message, ContentBlock

**Files:**
- Modify: `proto/amplifier_module.proto`

**Step 1: Add conversation message types**

Add the following to `proto/amplifier_module.proto` after the error messages from Task 1:

```protobuf
// ===========================================================================
// LLM conversation types — maps 1:1 to Rust messages.rs
// ===========================================================================

enum Role {
  ROLE_UNSPECIFIED = 0;
  ROLE_SYSTEM = 1;
  ROLE_DEVELOPER = 2;
  ROLE_USER = 3;
  ROLE_ASSISTANT = 4;
  ROLE_FUNCTION = 5;
  ROLE_TOOL = 6;
}

enum Visibility {
  VISIBILITY_UNSPECIFIED = 0;
  VISIBILITY_INTERNAL = 1;
  VISIBILITY_DEVELOPER = 2;
  VISIBILITY_USER = 3;
}

// Content block discriminated union — uses oneof for the type-specific payload.
message ContentBlock {
  oneof block {
    TextBlock text_block = 1;
    ThinkingBlock thinking_block = 2;
    RedactedThinkingBlock redacted_thinking_block = 3;
    ToolCallBlock tool_call_block = 4;
    ToolResultBlock tool_result_block = 5;
    ImageBlock image_block = 6;
    ReasoningBlock reasoning_block = 7;
  }
  Visibility visibility = 8;
}

message TextBlock {
  string text = 1;
}

message ThinkingBlock {
  string thinking = 1;
  string signature = 2;
  repeated string content = 3;
}

message RedactedThinkingBlock {
  string data = 1;
}

message ToolCallBlock {
  string id = 1;
  string name = 2;
  string input_json = 3;  // JSON object as string (preserves arbitrary structure)
}

message ToolResultBlock {
  string tool_call_id = 1;
  string output_json = 2;  // JSON value as string
}

message ImageBlock {
  string media_type = 1;
  string data = 2;  // base64-encoded
  string source_json = 3;  // Full source object as JSON for flexibility
}

message ReasoningBlock {
  repeated string content = 1;
  repeated string summary = 2;
}

message Message {
  Role role = 1;
  // Content is either a plain string or structured content blocks
  oneof content {
    string text_content = 2;
    ContentBlockList block_content = 3;
  }
  string name = 4;
  string tool_call_id = 5;
  string metadata_json = 6;  // JSON object as string
}

message ContentBlockList {
  repeated ContentBlock blocks = 1;
}

message ToolCallMessage {
  string id = 1;
  string name = 2;
  string arguments_json = 3;  // JSON object as string
}

message ToolSpecProto {
  string name = 1;
  string description = 2;
  string parameters_json = 3;  // JSON Schema as string
}

// Response format
message ResponseFormat {
  oneof format {
    bool text = 1;      // true = text format
    bool json = 2;      // true = json format
    JsonSchemaFormat json_schema = 3;
  }
}

message JsonSchemaFormat {
  string schema_json = 1;
  bool strict = 2;
}

message Usage {
  int64 input_tokens = 1;
  int64 output_tokens = 2;
  int64 total_tokens = 3;
  int64 reasoning_tokens = 4;
  int64 cache_read_tokens = 5;
  int64 cache_write_tokens = 6;
}

message Degradation {
  string requested = 1;
  string actual = 2;
  string reason = 3;
}

message ChatRequest {
  repeated Message messages = 1;
  repeated ToolSpecProto tools = 2;
  ResponseFormat response_format = 3;
  double temperature = 4;
  double top_p = 5;
  int64 max_output_tokens = 6;
  string conversation_id = 7;
  bool stream = 8;
  string metadata_json = 9;
  string model = 10;
  string tool_choice = 11;  // "auto", "none", or JSON object as string
  repeated string stop = 12;
  string reasoning_effort = 13;
  double timeout = 14;
}

message ChatResponse {
  repeated ContentBlock content = 1;
  repeated ToolCallMessage tool_calls = 2;
  Usage usage = 3;
  Degradation degradation = 4;
  string finish_reason = 5;
  string metadata_json = 6;
}
```

**Step 2: Verify proto compiles**

Run:
```bash
cd amplifier-core && protoc --proto_path=proto --python_out=/tmp/proto_test proto/amplifier_module.proto
```
Expected: Exit code 0, no errors.

**Step 3: Commit**
```bash
cd amplifier-core && git add proto/amplifier_module.proto && git commit -m "proto: add LLM conversation types — ChatRequest, ChatResponse, Message, ContentBlock"
```

---

### Task 3: Module-specific messages — ToolResult, HookResult, ModelInfo, ProviderInfo, Approval

**Files:**
- Modify: `proto/amplifier_module.proto`

**Step 1: Add module-specific types**

Add the following to `proto/amplifier_module.proto` after the conversation types from Task 2:

```protobuf
// ===========================================================================
// Module-specific result and metadata types
// ===========================================================================

message ToolResult {
  bool success = 1;
  string output_json = 2;  // JSON value as string
  string error_json = 3;   // JSON object as string: {"message": "..."}
}

// Hook action enum — mirrors Rust HookAction and Python HookAction
enum HookAction {
  HOOK_ACTION_UNSPECIFIED = 0;
  HOOK_ACTION_CONTINUE = 1;
  HOOK_ACTION_DENY = 2;
  HOOK_ACTION_MODIFY = 3;
  HOOK_ACTION_INJECT_CONTEXT = 4;
  HOOK_ACTION_ASK_USER = 5;
}

enum ContextInjectionRole {
  CONTEXT_INJECTION_ROLE_UNSPECIFIED = 0;
  CONTEXT_INJECTION_ROLE_SYSTEM = 1;
  CONTEXT_INJECTION_ROLE_USER = 2;
  CONTEXT_INJECTION_ROLE_ASSISTANT = 3;
}

enum ApprovalDefault {
  APPROVAL_DEFAULT_UNSPECIFIED = 0;
  APPROVAL_DEFAULT_ALLOW = 1;
  APPROVAL_DEFAULT_DENY = 2;
}

enum UserMessageLevel {
  USER_MESSAGE_LEVEL_UNSPECIFIED = 0;
  USER_MESSAGE_LEVEL_INFO = 1;
  USER_MESSAGE_LEVEL_WARNING = 2;
  USER_MESSAGE_LEVEL_ERROR = 3;
}

message HookResult {
  HookAction action = 1;
  string data_json = 2;  // Modified event data as JSON object
  string reason = 3;
  // Context injection fields
  string context_injection = 4;
  ContextInjectionRole context_injection_role = 5;
  bool ephemeral = 6;
  // Approval gate fields
  string approval_prompt = 7;
  repeated string approval_options = 8;
  double approval_timeout = 9;  // default 300.0
  ApprovalDefault approval_default = 10;
  // Output control fields
  bool suppress_output = 11;
  string user_message = 12;
  UserMessageLevel user_message_level = 13;
  string user_message_source = 14;
  bool append_to_last_tool_result = 15;
}

message ModelInfo {
  string id = 1;
  string display_name = 2;
  int64 context_window = 3;
  int64 max_output_tokens = 4;
  repeated string capabilities = 5;
  string defaults_json = 6;  // JSON object as string
}

message ProviderInfo {
  string id = 1;
  string display_name = 2;
  repeated string credential_env_vars = 3;
  repeated string capabilities = 4;
  string defaults_json = 5;  // JSON object as string
  repeated ConfigField config_fields = 6;
}

message ApprovalRequest {
  string tool_name = 1;
  string action = 2;
  string details_json = 3;  // JSON object as string
  string risk_level = 4;
  double timeout = 5;  // 0 = wait indefinitely
}

message ApprovalResponse {
  bool approved = 1;
  string reason = 2;
  bool remember = 3;
}
```

**Step 2: Verify proto compiles**

Run:
```bash
cd amplifier-core && protoc --proto_path=proto --python_out=/tmp/proto_test proto/amplifier_module.proto
```
Expected: Exit code 0, no errors.

**Step 3: Commit**
```bash
cd amplifier-core && git add proto/amplifier_module.proto && git commit -m "proto: add module-specific types — ToolResult, HookResult, ModelInfo, ProviderInfo, Approval"
```

---

### Task 4: Module services — ProviderService, OrchestratorService, ContextService, HookService, ApprovalService

**Files:**
- Modify: `proto/amplifier_module.proto`

**Step 1: Add the five remaining module services**

Add the following service definitions to `proto/amplifier_module.proto` after the message types. The existing `ToolService` stays as-is. These five new services complete the six module service types:

```protobuf
// ===========================================================================
// Module services — implemented by community modules in any language
// ===========================================================================

// ProviderService — LLM provider modules
service ProviderService {
  rpc GetInfo(Empty) returns (ProviderInfo);
  rpc ListModels(Empty) returns (ListModelsResponse);
  rpc Complete(ChatRequest) returns (ChatResponse);
  rpc CompleteStreaming(ChatRequest) returns (stream ChatResponse);
  rpc ParseToolCalls(ChatResponse) returns (ParseToolCallsResponse);
}

message ListModelsResponse {
  repeated ModelInfo models = 1;
}

message ParseToolCallsResponse {
  repeated ToolCallMessage tool_calls = 1;
}

// OrchestratorService — agent-loop orchestrator modules
service OrchestratorService {
  rpc Execute(OrchestratorExecuteRequest) returns (OrchestratorExecuteResponse);
}

message OrchestratorExecuteRequest {
  string prompt = 1;
  string session_id = 2;
  // Out-of-process orchestrators use KernelService RPCs for provider/tool access
}

message OrchestratorExecuteResponse {
  string response = 1;
  AmplifierError error = 2;
}

// ContextService — context management modules
service ContextService {
  rpc AddMessage(AddMessageRequest) returns (Empty);
  rpc GetMessages(Empty) returns (GetMessagesResponse);
  rpc GetMessagesForRequest(GetMessagesForRequestParams) returns (GetMessagesResponse);
  rpc SetMessages(SetMessagesRequest) returns (Empty);
  rpc Clear(Empty) returns (Empty);
}

message AddMessageRequest {
  Message message = 1;
}

message GetMessagesResponse {
  repeated Message messages = 1;
}

message GetMessagesForRequestParams {
  int64 token_budget = 1;  // 0 = no explicit budget
  string provider_name = 2;  // Provider name for dynamic budget calculation
}

message SetMessagesRequest {
  repeated Message messages = 1;
}

// HookService — hook handler modules
service HookService {
  rpc Handle(HookHandleRequest) returns (HookResult);
}

message HookHandleRequest {
  string event = 1;
  string data_json = 2;  // Event payload as JSON
}

// ApprovalService — approval provider modules
service ApprovalService {
  rpc RequestApproval(ApprovalRequest) returns (ApprovalResponse);
}
```

**Step 2: Verify proto compiles**

Run:
```bash
cd amplifier-core && protoc --proto_path=proto --python_out=/tmp/proto_test proto/amplifier_module.proto
```
Expected: Exit code 0, no errors.

**Step 3: Commit**
```bash
cd amplifier-core && git add proto/amplifier_module.proto && git commit -m "proto: add 5 module services — Provider, Orchestrator, Context, Hook, Approval"
```

---

### Task 5: KernelService — the hub service that modules call back into

**Files:**
- Modify: `proto/amplifier_module.proto`

**Step 1: Add the KernelService**

Add the following to `proto/amplifier_module.proto` after the module services:

```protobuf
// ===========================================================================
// KernelService — hosted by the Rust kernel, called by out-of-process modules
// ===========================================================================

// Out-of-process modules (Go orchestrator, TS context manager, etc.) call
// back to the kernel via this service for all cross-module operations.
// In-process modules (Rust native, Python PyO3) access the coordinator directly.
service KernelService {
  // Provider operations
  rpc CompleteWithProvider(CompleteWithProviderRequest) returns (ChatResponse);
  rpc CompleteWithProviderStreaming(CompleteWithProviderRequest) returns (stream ChatResponse);

  // Tool operations
  rpc ExecuteTool(ExecuteToolRequest) returns (ToolResult);

  // Hook operations
  rpc EmitHook(EmitHookRequest) returns (HookResult);
  rpc EmitHookAndCollect(EmitHookAndCollectRequest) returns (EmitHookAndCollectResponse);

  // Context operations (for orchestrators that need to manage context)
  rpc GetMessages(GetMessagesRequest) returns (GetMessagesResponse);
  rpc AddMessage(KernelAddMessageRequest) returns (Empty);

  // Module discovery
  rpc GetMountedModule(GetMountedModuleRequest) returns (GetMountedModuleResponse);

  // Capability registry
  rpc RegisterCapability(RegisterCapabilityRequest) returns (Empty);
  rpc GetCapability(GetCapabilityRequest) returns (GetCapabilityResponse);
}

message CompleteWithProviderRequest {
  string provider_name = 1;
  ChatRequest request = 2;
}

message ExecuteToolRequest {
  string tool_name = 1;
  string input_json = 2;  // JSON object as string
}

message EmitHookRequest {
  string event = 1;
  string data_json = 2;
}

message EmitHookAndCollectRequest {
  string event = 1;
  string data_json = 2;
  double timeout_seconds = 3;
}

message EmitHookAndCollectResponse {
  repeated string responses_json = 1;  // Each response as JSON object
}

message GetMessagesRequest {
  string session_id = 1;
}

message KernelAddMessageRequest {
  string session_id = 1;
  Message message = 2;
}

message GetMountedModuleRequest {
  string module_name = 1;
  ModuleType module_type = 2;
}

message GetMountedModuleResponse {
  bool found = 1;
  ModuleInfo info = 2;
}

message RegisterCapabilityRequest {
  string name = 1;
  string value_json = 2;
}

message GetCapabilityRequest {
  string name = 1;
}

message GetCapabilityResponse {
  bool found = 1;
  string value_json = 2;
}
```

**Step 2: Verify proto compiles**

Run:
```bash
cd amplifier-core && protoc --proto_path=proto --python_out=/tmp/proto_test proto/amplifier_module.proto
```
Expected: Exit code 0, no errors.

**Step 3: Commit**
```bash
cd amplifier-core && git add proto/amplifier_module.proto && git commit -m "proto: add KernelService — hub service for out-of-process module callbacks"
```

---

### Task 6: ModuleLifecycle service — mount, cleanup, health, info

**Files:**
- Modify: `proto/amplifier_module.proto`

**Step 1: Add the ModuleLifecycle service**

Add the following to `proto/amplifier_module.proto` after KernelService:

```protobuf
// ===========================================================================
// ModuleLifecycle — shared by all module types
// ===========================================================================

// Every gRPC module also implements this service for mount/cleanup/health.
// The kernel calls these RPCs during module lifecycle management.
service ModuleLifecycle {
  rpc Mount(MountRequest) returns (MountResponse);
  rpc Cleanup(Empty) returns (Empty);
  rpc HealthCheck(Empty) returns (HealthCheckResponse);
  rpc GetModuleInfo(Empty) returns (ModuleInfo);
}
```

**Step 2: Verify proto compiles**

Run:
```bash
cd amplifier-core && protoc --proto_path=proto --python_out=/tmp/proto_test proto/amplifier_module.proto
```
Expected: Exit code 0, no errors.

**Step 3: Count the final proto**

Run:
```bash
wc -l amplifier-core/proto/amplifier_module.proto
```
Expected: ~400-500 lines. This is the complete proto with all 8 services and all message types.

**Step 4: Commit**
```bash
cd amplifier-core && git add proto/amplifier_module.proto && git commit -m "proto: add ModuleLifecycle service — mount, cleanup, health, info

Proto expansion complete: 8 services (6 module + KernelService + ModuleLifecycle),
all typed messages, error taxonomy. This is the source of truth for all contracts."
```

---

### Task 7: Code generation script — Makefile for protoc (all languages)

**Files:**
- Create: `proto/Makefile`
- Create: `proto/gen/.gitkeep`

**Step 1: Create the proto generation Makefile**

Create `proto/Makefile`:

```makefile
# Proto code generation for all supported languages.
# Run `make proto` from the repo root, or `make` from this directory.
#
# Prerequisites:
#   protoc (Protocol Buffer compiler)
#   protoc-gen-prost + protoc-gen-tonic (Rust: cargo install protoc-gen-prost protoc-gen-tonic)
#   grpc_tools (Python: pip install grpcio-tools)
#   protoc-gen-go + protoc-gen-go-grpc (Go: go install google.golang.org/protobuf/cmd/protoc-gen-go@latest)
#   protoc-gen-ts (TypeScript: npm install -g @protobuf-ts/plugin)

PROTO_SRC := amplifier_module.proto
GEN_DIR := gen

.PHONY: all clean python rust go typescript csharp check

all: python rust go typescript csharp

# --- Python ---
python:
	@mkdir -p $(GEN_DIR)/python
	python -m grpc_tools.protoc \
		-I. \
		--python_out=$(GEN_DIR)/python \
		--grpc_python_out=$(GEN_DIR)/python \
		$(PROTO_SRC)
	@echo ":: Python stubs generated in $(GEN_DIR)/python/"

# --- Rust (prost + tonic) ---
rust:
	@mkdir -p $(GEN_DIR)/rust
	protoc \
		-I. \
		--prost_out=$(GEN_DIR)/rust \
		--tonic_out=$(GEN_DIR)/rust \
		$(PROTO_SRC)
	@echo ":: Rust stubs generated in $(GEN_DIR)/rust/"

# --- Go ---
go:
	@mkdir -p $(GEN_DIR)/go
	protoc \
		-I. \
		--go_out=$(GEN_DIR)/go --go_opt=paths=source_relative \
		--go-grpc_out=$(GEN_DIR)/go --go-grpc_opt=paths=source_relative \
		$(PROTO_SRC)
	@echo ":: Go stubs generated in $(GEN_DIR)/go/"

# --- TypeScript ---
typescript:
	@mkdir -p $(GEN_DIR)/typescript
	protoc \
		-I. \
		--ts_out=$(GEN_DIR)/typescript \
		$(PROTO_SRC)
	@echo ":: TypeScript stubs generated in $(GEN_DIR)/typescript/"

# --- C# ---
csharp:
	@mkdir -p $(GEN_DIR)/csharp
	protoc \
		-I. \
		--csharp_out=$(GEN_DIR)/csharp \
		--grpc_out=$(GEN_DIR)/csharp --plugin=protoc-gen-grpc=`which grpc_csharp_plugin` \
		$(PROTO_SRC) 2>/dev/null || \
	protoc \
		-I. \
		--csharp_out=$(GEN_DIR)/csharp \
		$(PROTO_SRC)
	@echo ":: C# stubs generated in $(GEN_DIR)/csharp/"

# --- Validation: check proto compiles ---
check:
	protoc -I. --python_out=/dev/null $(PROTO_SRC)
	@echo ":: Proto syntax valid"

clean:
	rm -rf $(GEN_DIR)/python $(GEN_DIR)/rust $(GEN_DIR)/go $(GEN_DIR)/typescript $(GEN_DIR)/csharp
	@echo ":: Generated files cleaned"
```

**Step 2: Create the gen directory placeholder**

```bash
mkdir -p amplifier-core/proto/gen && touch amplifier-core/proto/gen/.gitkeep
```

**Step 3: Verify the check target works**

Run:
```bash
cd amplifier-core/proto && make check
```
Expected: `:: Proto syntax valid`

**Step 4: Generate Python stubs**

Run:
```bash
cd amplifier-core/proto && make python
```
Expected: `:: Python stubs generated in gen/python/`

**Step 5: Commit**
```bash
cd amplifier-core && git add proto/Makefile proto/gen/.gitkeep && git commit -m "build: add proto code generation Makefile for all languages"
```

---

### Task 8: Regenerate Python gRPC stubs from expanded proto

**Files:**
- Modify: `python/amplifier_core/_grpc_gen/amplifier_module_pb2.py` (regenerated)
- Modify: `python/amplifier_core/_grpc_gen/amplifier_module_pb2_grpc.py` (regenerated)

**Step 1: Regenerate the Python stubs**

Run:
```bash
cd amplifier-core && python -m grpc_tools.protoc \
  -Iproto \
  --python_out=python/amplifier_core/_grpc_gen \
  --grpc_python_out=python/amplifier_core/_grpc_gen \
  proto/amplifier_module.proto
```
Expected: Exit code 0. The files in `_grpc_gen/` are overwritten with all 8 services.

**Step 2: Verify the generated stubs import correctly**

Run:
```bash
cd amplifier-core && python -c "from amplifier_core._grpc_gen import amplifier_module_pb2; print('ToolSpec:', hasattr(amplifier_module_pb2, 'ToolSpec')); print('ChatRequest:', hasattr(amplifier_module_pb2, 'ChatRequest')); print('HookResult:', hasattr(amplifier_module_pb2, 'HookResult'))"
```
Expected:
```
ToolSpec: True
ChatRequest: True
HookResult: True
```

**Step 3: Run existing Python tests to confirm no breakage**

Run:
```bash
cd amplifier-core && uv run pytest tests/ -x -q
```
Expected: All existing tests pass. The expanded proto is backward-compatible.

**Step 4: Commit**
```bash
cd amplifier-core && git add python/amplifier_core/_grpc_gen/ && git commit -m "build: regenerate Python gRPC stubs from expanded proto (all 8 services)"
```

---

### Task 9: Generate Rust stubs via prost build script

**Files:**
- Modify: `crates/amplifier-core/Cargo.toml`
- Create: `crates/amplifier-core/build.rs`
- Create: `crates/amplifier-core/src/generated/mod.rs`
- Modify: `crates/amplifier-core/src/lib.rs`

**Step 1: Add prost and tonic dependencies to Cargo.toml**

Open `crates/amplifier-core/Cargo.toml` and add to `[dependencies]`:

```toml
prost = "0.13"
tonic = "0.12"
```

Also add `[build-dependencies]`:

```toml
[build-dependencies]
tonic-build = "0.12"
```

**Step 2: Create the build script**

Create `crates/amplifier-core/build.rs`:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir("src/generated")
        .compile_protos(
            &["../../proto/amplifier_module.proto"],
            &["../../proto"],
        )?;
    Ok(())
}
```

**Step 3: Create the generated module directory**

```bash
mkdir -p amplifier-core/crates/amplifier-core/src/generated
```

**Step 4: Run cargo build to generate the Rust code**

Run:
```bash
cd amplifier-core && cargo build -p amplifier-core
```
Expected: Build succeeds. A file `crates/amplifier-core/src/generated/amplifier.module.rs` is created.

**Step 5: Create the generated module file**

Create `crates/amplifier-core/src/generated/mod.rs`:

```rust
//! Generated protobuf and gRPC code from `proto/amplifier_module.proto`.
//!
//! DO NOT EDIT — regenerated by `tonic-build` from the proto source of truth.
//! Run `cargo build -p amplifier-core` to regenerate.

#[allow(clippy::all)]
pub mod amplifier_module {
    include!("amplifier.module.rs");
}
```

**Step 6: Wire into lib.rs**

Open `crates/amplifier-core/src/lib.rs` and add after the existing module declarations (after `pub mod traits;`):

```rust
pub mod generated;
```

**Step 7: Run cargo test to verify everything compiles**

Run:
```bash
cd amplifier-core && cargo test -p amplifier-core
```
Expected: All existing tests pass. The generated module compiles alongside hand-written code.

**Step 8: Commit**
```bash
cd amplifier-core && git add crates/amplifier-core/Cargo.toml crates/amplifier-core/build.rs crates/amplifier-core/src/generated/ crates/amplifier-core/src/lib.rs && git commit -m "build: add prost/tonic and generate Rust stubs from proto

Generated Rust types live alongside hand-written types. Phase 4 will
replace hand-written types with generated ones."
```

---

### Task 10: Proto compilation test (proto is valid, generates without errors)

**Files:**
- Create: `tests/test_proto_compilation.py`

**Step 1: Write the proto compilation test**

Create `tests/test_proto_compilation.py`:

```python
"""Tests that proto files compile and generated stubs are importable.

These tests verify the proto source of truth is valid and the generated
Python stubs are in sync with the proto definitions.
"""

import importlib


class TestProtoCompilation:
    """Proto compilation and import tests."""

    def test_pb2_module_imports(self):
        """Generated pb2 module must be importable."""
        mod = importlib.import_module("amplifier_core._grpc_gen.amplifier_module_pb2")
        assert mod is not None

    def test_pb2_grpc_module_imports(self):
        """Generated pb2_grpc module must be importable."""
        mod = importlib.import_module(
            "amplifier_core._grpc_gen.amplifier_module_pb2_grpc"
        )
        assert mod is not None

    def test_tool_service_messages_exist(self):
        """Existing ToolService messages must still be present (backward compat)."""
        from amplifier_core._grpc_gen import amplifier_module_pb2

        assert hasattr(amplifier_module_pb2, "ToolSpec")
        assert hasattr(amplifier_module_pb2, "ToolExecuteRequest")
        assert hasattr(amplifier_module_pb2, "ToolExecuteResponse")
        assert hasattr(amplifier_module_pb2, "Empty")

    def test_new_services_messages_exist(self):
        """All new proto messages from expansion must be present."""
        from amplifier_core._grpc_gen import amplifier_module_pb2 as pb2

        # Common messages
        assert hasattr(pb2, "ModuleInfo")
        assert hasattr(pb2, "MountRequest")
        assert hasattr(pb2, "MountResponse")
        assert hasattr(pb2, "HealthCheckResponse")
        assert hasattr(pb2, "ConfigField")

        # Error types
        assert hasattr(pb2, "ProviderError")
        assert hasattr(pb2, "ToolError")
        assert hasattr(pb2, "HookError")
        assert hasattr(pb2, "AmplifierError")

        # Conversation types
        assert hasattr(pb2, "ChatRequest")
        assert hasattr(pb2, "ChatResponse")
        assert hasattr(pb2, "Message")
        assert hasattr(pb2, "ContentBlock")
        assert hasattr(pb2, "Usage")

        # Module-specific types
        assert hasattr(pb2, "ToolResult")
        assert hasattr(pb2, "HookResult")
        assert hasattr(pb2, "ModelInfo")
        assert hasattr(pb2, "ProviderInfo")
        assert hasattr(pb2, "ApprovalRequest")
        assert hasattr(pb2, "ApprovalResponse")

        # Kernel messages
        assert hasattr(pb2, "CompleteWithProviderRequest")
        assert hasattr(pb2, "ExecuteToolRequest")
        assert hasattr(pb2, "EmitHookRequest")

    def test_service_stubs_exist(self):
        """All 8 service stubs must be generated."""
        from amplifier_core._grpc_gen import amplifier_module_pb2_grpc as grpc

        # Module services
        assert hasattr(grpc, "ToolServiceStub")
        assert hasattr(grpc, "ProviderServiceStub")
        assert hasattr(grpc, "OrchestratorServiceStub")
        assert hasattr(grpc, "ContextServiceStub")
        assert hasattr(grpc, "HookServiceStub")
        assert hasattr(grpc, "ApprovalServiceStub")

        # Kernel service
        assert hasattr(grpc, "KernelServiceStub")

        # Lifecycle service
        assert hasattr(grpc, "ModuleLifecycleStub")

    def test_enum_values_present(self):
        """Key enum values must be present in generated code."""
        from amplifier_core._grpc_gen import amplifier_module_pb2 as pb2

        # ModuleType enum
        assert pb2.MODULE_TYPE_TOOL == 3
        assert pb2.MODULE_TYPE_PROVIDER == 2

        # HookAction enum
        assert pb2.HOOK_ACTION_CONTINUE == 1
        assert pb2.HOOK_ACTION_DENY == 2

        # ProviderErrorType enum
        assert pb2.PROVIDER_ERROR_TYPE_RATE_LIMIT == 1
        assert pb2.PROVIDER_ERROR_TYPE_TIMEOUT == 7
```

**Step 2: Run the tests**

Run:
```bash
cd amplifier-core && uv run pytest tests/test_proto_compilation.py -v
```
Expected: All tests PASS.

**Step 3: Commit**
```bash
cd amplifier-core && git add tests/test_proto_compilation.py && git commit -m "test: add proto compilation and stub import tests"
```

---

### Task 11: Equivalence tests — proto messages map 1:1 to existing Rust structs

**Files:**
- Create: `crates/amplifier-core/src/generated/equivalence_tests.rs`
- Modify: `crates/amplifier-core/src/generated/mod.rs`

**Step 1: Write the equivalence tests**

Create `crates/amplifier-core/src/generated/equivalence_tests.rs`:

```rust
//! Equivalence tests: verify proto-generated types can represent the same
//! data as the hand-written Rust types in models.rs, messages.rs, errors.rs.
//!
//! These tests ensure the proto expansion is a faithful representation of
//! the existing Rust type system. When Phase 4 replaces hand-written types
//! with generated ones, these tests verify zero data loss.

#[cfg(test)]
mod tests {
    use crate::generated::amplifier_module;

    // ---- Proto message instantiation tests ----

    #[test]
    fn proto_module_info_has_all_fields() {
        let info = amplifier_module::ModuleInfo {
            id: "bash-tool".into(),
            name: "Bash Tool".into(),
            version: "1.0.0".into(),
            module_type: amplifier_module::ModuleType::Tool.into(),
            mount_point: "tools".into(),
            description: "Execute bash commands".into(),
            config_schema_json: "{}".into(),
            capabilities: vec!["execute".into()],
            author: "amplifier".into(),
        };
        assert_eq!(info.id, "bash-tool");
        assert_eq!(info.name, "Bash Tool");
    }

    #[test]
    fn proto_tool_result_has_all_fields() {
        let result = amplifier_module::ToolResult {
            success: true,
            output_json: r#"{"key": "value"}"#.into(),
            error_json: String::new(),
        };
        assert!(result.success);
        assert!(!result.output_json.is_empty());
    }

    #[test]
    fn proto_hook_result_has_all_15_fields() {
        let result = amplifier_module::HookResult {
            action: amplifier_module::HookAction::Continue.into(),
            data_json: String::new(),
            reason: String::new(),
            context_injection: String::new(),
            context_injection_role: amplifier_module::ContextInjectionRole::System.into(),
            ephemeral: false,
            approval_prompt: String::new(),
            approval_options: vec![],
            approval_timeout: 300.0,
            approval_default: amplifier_module::ApprovalDefault::Deny.into(),
            suppress_output: false,
            user_message: String::new(),
            user_message_level: amplifier_module::UserMessageLevel::Info.into(),
            user_message_source: String::new(),
            append_to_last_tool_result: false,
        };
        assert_eq!(result.approval_timeout, 300.0);
    }

    #[test]
    fn proto_provider_error_has_all_fields() {
        let err = amplifier_module::ProviderError {
            error_type: amplifier_module::ProviderErrorType::RateLimit.into(),
            message: "429 Too Many Requests".into(),
            provider: "openai".into(),
            model: "gpt-4".into(),
            retry_after: 2.5,
            retryable: true,
            status_code: 429,
        };
        assert_eq!(err.message, "429 Too Many Requests");
        assert!(err.retryable);
    }

    #[test]
    fn proto_chat_request_has_all_fields() {
        let req = amplifier_module::ChatRequest {
            messages: vec![],
            tools: vec![],
            response_format: None,
            temperature: 0.7,
            top_p: 0.9,
            max_output_tokens: 4096,
            conversation_id: "conv-123".into(),
            stream: true,
            metadata_json: "{}".into(),
            model: "gpt-4".into(),
            tool_choice: "auto".into(),
            stop: vec!["END".into()],
            reasoning_effort: "high".into(),
            timeout: 30.0,
        };
        assert_eq!(req.model, "gpt-4");
        assert_eq!(req.temperature, 0.7);
    }

    #[test]
    fn proto_usage_has_all_token_fields() {
        let usage = amplifier_module::Usage {
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
            reasoning_tokens: 20,
            cache_read_tokens: 10,
            cache_write_tokens: 5,
        };
        assert_eq!(usage.total_tokens, 150);
    }

    #[test]
    fn proto_model_info_has_all_fields() {
        let info = amplifier_module::ModelInfo {
            id: "claude-sonnet-4-5".into(),
            display_name: "Claude Sonnet 4.5".into(),
            context_window: 200_000,
            max_output_tokens: 8192,
            capabilities: vec!["tools".into(), "vision".into()],
            defaults_json: r#"{"temperature": 0.7}"#.into(),
        };
        assert_eq!(info.context_window, 200_000);
    }

    #[test]
    fn proto_approval_roundtrip() {
        let req = amplifier_module::ApprovalRequest {
            tool_name: "bash".into(),
            action: "rm -rf /tmp/test".into(),
            details_json: r#"{"command": "rm -rf /tmp/test"}"#.into(),
            risk_level: "high".into(),
            timeout: 300.0,
        };
        assert_eq!(req.tool_name, "bash");

        let resp = amplifier_module::ApprovalResponse {
            approved: true,
            reason: "User approved".into(),
            remember: false,
        };
        assert!(resp.approved);
    }

    // ---- Enum coverage ----

    #[test]
    fn proto_module_type_covers_all_variants() {
        let types = [
            amplifier_module::ModuleType::Orchestrator,
            amplifier_module::ModuleType::Provider,
            amplifier_module::ModuleType::Tool,
            amplifier_module::ModuleType::Context,
            amplifier_module::ModuleType::Hook,
            amplifier_module::ModuleType::Approval,
        ];
        assert_eq!(types.len(), 6);
    }

    #[test]
    fn proto_provider_error_type_covers_all_variants() {
        let types = [
            amplifier_module::ProviderErrorType::RateLimit,
            amplifier_module::ProviderErrorType::Authentication,
            amplifier_module::ProviderErrorType::ContextLength,
            amplifier_module::ProviderErrorType::ContentFilter,
            amplifier_module::ProviderErrorType::InvalidRequest,
            amplifier_module::ProviderErrorType::Unavailable,
            amplifier_module::ProviderErrorType::Timeout,
            amplifier_module::ProviderErrorType::Other,
        ];
        assert_eq!(types.len(), 8);
    }

    #[test]
    fn proto_hook_action_covers_all_variants() {
        let actions = [
            amplifier_module::HookAction::Continue,
            amplifier_module::HookAction::Deny,
            amplifier_module::HookAction::Modify,
            amplifier_module::HookAction::InjectContext,
            amplifier_module::HookAction::AskUser,
        ];
        assert_eq!(actions.len(), 5);
    }
}
```

**Step 2: Wire the test file into the generated module**

Open `crates/amplifier-core/src/generated/mod.rs` and add:

```rust
mod equivalence_tests;
```

**Step 3: Run the tests**

Run:
```bash
cd amplifier-core && cargo test -p amplifier-core -- generated::equivalence_tests
```
Expected: All tests PASS.

**Step 4: Commit**
```bash
cd amplifier-core && git add crates/amplifier-core/src/generated/ && git commit -m "test: add proto-to-Rust equivalence tests for all message types"
```

---

### Task 12: Code quality fix — `to_dict()` on Rust-backed objects

**Files:**
- Modify: `crates/amplifier-core/src/coordinator.rs`
- Modify: `bindings/python/src/lib.rs`
- Create: `tests/test_rust_to_dict.py`

**Step 1: Write the failing test**

Create `tests/test_rust_to_dict.py`:

```python
"""Tests for to_dict() on Rust-backed objects.

Addresses production audit finding: vars(coordinator) returns only the
Python __dict__, missing all Rust-managed state. The to_dict() method
exposes all fields for serialization, debugging, and logging.
"""

from amplifier_core._engine import RustCoordinator


class TestRustToDict:
    """Tests for to_dict() on Rust-backed PyO3 objects."""

    def test_coordinator_has_to_dict(self):
        """RustCoordinator must expose a to_dict() method."""
        coord = RustCoordinator({})
        assert hasattr(coord, "to_dict"), "RustCoordinator must have to_dict()"

    def test_coordinator_to_dict_returns_dict(self):
        """to_dict() must return a plain Python dict."""
        coord = RustCoordinator({})
        result = coord.to_dict()
        assert isinstance(result, dict)

    def test_coordinator_to_dict_includes_tools(self):
        """to_dict() must include the tools mount point."""
        coord = RustCoordinator({})
        result = coord.to_dict()
        assert "tools" in result

    def test_coordinator_to_dict_includes_providers(self):
        """to_dict() must include the providers mount point."""
        coord = RustCoordinator({})
        result = coord.to_dict()
        assert "providers" in result

    def test_coordinator_to_dict_includes_capabilities(self):
        """to_dict() must include registered capabilities."""
        coord = RustCoordinator({})
        result = coord.to_dict()
        assert "capabilities" in result
```

**Step 2: Run tests to verify they fail**

Run:
```bash
cd amplifier-core && maturin develop && uv run pytest tests/test_rust_to_dict.py -v
```
Expected: FAIL — `RustCoordinator` does not have `to_dict()` yet.

**Step 3: Add read-only accessor methods to Coordinator in Rust**

Open `crates/amplifier-core/src/coordinator.rs` and add these methods to the `impl Coordinator` block:

```rust
    /// Return names of all mounted tools.
    pub fn tool_names(&self) -> Vec<String> {
        self.tools.lock().unwrap().keys().cloned().collect()
    }

    /// Return names of all mounted providers.
    pub fn provider_names(&self) -> Vec<String> {
        self.providers.lock().unwrap().keys().cloned().collect()
    }

    /// Check if an orchestrator is mounted.
    pub fn has_orchestrator(&self) -> bool {
        self.orchestrator.lock().unwrap().is_some()
    }

    /// Check if a context manager is mounted.
    pub fn has_context(&self) -> bool {
        self.context.lock().unwrap().is_some()
    }

    /// Return names of all registered capabilities.
    pub fn capability_names(&self) -> Vec<String> {
        self.capabilities.lock().unwrap().keys().cloned().collect()
    }

    /// Get a tool by name.
    pub fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.lock().unwrap().get(name).cloned()
    }

    /// Mount a tool on the coordinator.
    pub fn mount_tool(&self, name: &str, tool: Arc<dyn Tool>) {
        self.tools.lock().unwrap().insert(name.to_string(), tool);
    }
```

**Step 4: Add `to_dict()` to PyCoordinator in the PyO3 bridge**

Open `bindings/python/src/lib.rs`, find the `#[pymethods] impl PyCoordinator` block, and add:

```rust
    /// Return a dict with all coordinator state for serialization/debugging.
    ///
    /// Addresses production audit finding: vars(coordinator) misses Rust state.
    /// This method exposes all mount points and capabilities as a plain dict.
    fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        let dict = PyDict::new(py);

        // Tools
        let tools_list: Vec<String> = self.inner.tool_names();
        dict.set_item("tools", tools_list)?;

        // Providers
        let providers_list: Vec<String> = self.inner.provider_names();
        dict.set_item("providers", providers_list)?;

        // Orchestrator
        dict.set_item("has_orchestrator", self.inner.has_orchestrator())?;

        // Context manager
        dict.set_item("has_context", self.inner.has_context())?;

        // Capabilities
        let caps = self.inner.capability_names();
        dict.set_item("capabilities", caps)?;

        Ok(dict.into())
    }
```

**Step 5: Rebuild and run tests**

Run:
```bash
cd amplifier-core && maturin develop && uv run pytest tests/test_rust_to_dict.py -v
```
Expected: All tests PASS.

**Step 6: Run full test suite to confirm no breakage**

Run:
```bash
cd amplifier-core && cargo test -p amplifier-core && uv run pytest tests/ -x -q
```
Expected: All tests pass.

**Step 7: Commit**
```bash
cd amplifier-core && git add crates/amplifier-core/src/coordinator.rs bindings/python/src/lib.rs tests/test_rust_to_dict.py && git commit -m "fix: add to_dict() on RustCoordinator — audit finding #1

vars(coordinator) only returns Python __dict__, missing Rust state.
to_dict() exposes all mount points and capabilities for serialization."
```

---

### Task 13: Code quality fix — resolver public API methods

**Files:**
- Modify: `python/amplifier_core/module_sources.py`
- Create: `tests/test_resolver_public_api.py`

**Step 1: Write the failing test**

Create `tests/test_resolver_public_api.py`:

```python
"""Tests for resolver public API methods.

Addresses production audit finding: _bundle, _paths, _bundle_mappings
accessed via hasattr chains in session_spawner.py. This adds public
get_module_paths() and get_mention_mappings() methods.
"""

from amplifier_core.module_sources import (
    FileSystemModuleSource,
)


class TestResolverPublicAPI:
    """Tests for public resolver methods."""

    def test_filesystem_source_has_get_module_paths(self):
        """FileSystemModuleSource must have get_module_paths()."""
        source = FileSystemModuleSource(paths=["/tmp"])
        assert hasattr(source, "get_module_paths")

    def test_filesystem_source_get_module_paths_returns_list(self):
        """get_module_paths() must return a list."""
        source = FileSystemModuleSource(paths=["/tmp"])
        result = source.get_module_paths()
        assert isinstance(result, list)

    def test_filesystem_source_has_get_mention_mappings(self):
        """FileSystemModuleSource must have get_mention_mappings()."""
        source = FileSystemModuleSource(paths=["/tmp"])
        assert hasattr(source, "get_mention_mappings")

    def test_filesystem_source_get_mention_mappings_returns_dict(self):
        """get_mention_mappings() must return a dict."""
        source = FileSystemModuleSource(paths=["/tmp"])
        result = source.get_mention_mappings()
        assert isinstance(result, dict)
```

**Step 2: Run tests to verify they fail**

Run:
```bash
cd amplifier-core && uv run pytest tests/test_resolver_public_api.py -v
```
Expected: FAIL — methods do not exist yet.

**Step 3: Add public API methods to FileSystemModuleSource**

Open `python/amplifier_core/module_sources.py` and find the `FileSystemModuleSource` class. Add the following methods:

```python
    def get_module_paths(self) -> list[str]:
        """Return configured module search paths.

        Public API replacing direct access to _paths.
        """
        return list(self._paths) if hasattr(self, "_paths") else []

    def get_mention_mappings(self) -> dict[str, str]:
        """Return mention-to-path mappings.

        Public API replacing direct access to _bundle_mappings.
        Returns empty dict if this source type doesn't support mention mappings.
        """
        if hasattr(self, "_bundle_mappings"):
            return dict(self._bundle_mappings)
        return {}
```

If there is also a `BundleModuleSource` class in the same file, add the same methods there too, adapting to its internal attribute names.

**Step 4: Run tests to verify they pass**

Run:
```bash
cd amplifier-core && uv run pytest tests/test_resolver_public_api.py -v
```
Expected: All tests PASS.

**Step 5: Run full test suite**

Run:
```bash
cd amplifier-core && uv run pytest tests/ -x -q
```
Expected: All tests pass.

**Step 6: Commit**
```bash
cd amplifier-core && git add python/amplifier_core/module_sources.py tests/test_resolver_public_api.py && git commit -m "fix: add public API to resolvers — audit finding #2

Replaces _bundle, _paths, _bundle_mappings hasattr probing with
public get_module_paths() and get_mention_mappings() methods."
```

---

## Phase 2: Rust gRPC Infrastructure

**Goal:** Implement KernelService gRPC server in Rust. Add transport bridge traits for all 6 module types (GrpcToolBridge, GrpcProviderBridge, etc.). Wire transport dispatch into the module loader. End-to-end: a gRPC tool can be loaded and executed.

**Invariant:** Existing Python path still works unchanged.

---

### Task 14: KernelService gRPC server skeleton

**Files:**
- Create: `crates/amplifier-core/src/grpc_server.rs`
- Modify: `crates/amplifier-core/src/lib.rs`
- Modify: `crates/amplifier-core/Cargo.toml`

**Step 1: Add tokio-stream dependency**

Add to `crates/amplifier-core/Cargo.toml` under `[dependencies]`:

```toml
tokio-stream = "0.1"
```

**Step 2: Implement the KernelServiceImpl struct**

Create `crates/amplifier-core/src/grpc_server.rs`:

```rust
//! KernelService gRPC server implementation.
//!
//! This module implements the KernelService proto as a tonic gRPC server.
//! Out-of-process modules (Go, TypeScript, etc.) call back to the kernel
//! via this service for provider/tool/hook/context access.

use std::sync::Arc;

use tonic::{Request, Response, Status};

use crate::coordinator::Coordinator;
use crate::generated::amplifier_module;
use crate::generated::amplifier_module::kernel_service_server::KernelService;

/// Implementation of the KernelService gRPC server.
///
/// Wraps an `Arc<Coordinator>` and translates proto requests into
/// coordinator operations.
pub struct KernelServiceImpl {
    coordinator: Arc<Coordinator>,
}

impl KernelServiceImpl {
    /// Create a new KernelServiceImpl wrapping the given coordinator.
    pub fn new(coordinator: Arc<Coordinator>) -> Self {
        Self { coordinator }
    }
}

#[tonic::async_trait]
impl KernelService for KernelServiceImpl {
    async fn complete_with_provider(
        &self,
        _request: Request<amplifier_module::CompleteWithProviderRequest>,
    ) -> Result<Response<amplifier_module::ChatResponse>, Status> {
        Err(Status::unimplemented("CompleteWithProvider not yet implemented"))
    }

    type CompleteWithProviderStreamingStream = tokio_stream::wrappers::ReceiverStream<
        Result<amplifier_module::ChatResponse, Status>,
    >;

    async fn complete_with_provider_streaming(
        &self,
        _request: Request<amplifier_module::CompleteWithProviderRequest>,
    ) -> Result<Response<Self::CompleteWithProviderStreamingStream>, Status> {
        Err(Status::unimplemented(
            "CompleteWithProviderStreaming not yet implemented",
        ))
    }

    async fn execute_tool(
        &self,
        request: Request<amplifier_module::ExecuteToolRequest>,
    ) -> Result<Response<amplifier_module::ToolResult>, Status> {
        let req = request.into_inner();
        let tool_name = &req.tool_name;

        // Look up the tool in the coordinator
        let tool = self
            .coordinator
            .get_tool(tool_name)
            .ok_or_else(|| Status::not_found(format!("Tool not found: {tool_name}")))?;

        // Parse input JSON
        let input: serde_json::Value = serde_json::from_str(&req.input_json)
            .map_err(|e| Status::invalid_argument(format!("Invalid input JSON: {e}")))?;

        // Execute the tool
        match tool.execute(input).await {
            Ok(result) => {
                let output_json = result
                    .output
                    .map(|v| serde_json::to_string(&v).unwrap_or_default())
                    .unwrap_or_default();
                let error_json = result
                    .error
                    .map(|e| serde_json::to_string(&e).unwrap_or_default())
                    .unwrap_or_default();
                Ok(Response::new(amplifier_module::ToolResult {
                    success: result.success,
                    output_json,
                    error_json,
                }))
            }
            Err(e) => Err(Status::internal(format!("Tool execution failed: {e}"))),
        }
    }

    async fn emit_hook(
        &self,
        _request: Request<amplifier_module::EmitHookRequest>,
    ) -> Result<Response<amplifier_module::HookResult>, Status> {
        Err(Status::unimplemented("EmitHook not yet implemented"))
    }

    async fn emit_hook_and_collect(
        &self,
        _request: Request<amplifier_module::EmitHookAndCollectRequest>,
    ) -> Result<Response<amplifier_module::EmitHookAndCollectResponse>, Status> {
        Err(Status::unimplemented(
            "EmitHookAndCollect not yet implemented",
        ))
    }

    async fn get_messages(
        &self,
        _request: Request<amplifier_module::GetMessagesRequest>,
    ) -> Result<Response<amplifier_module::GetMessagesResponse>, Status> {
        Err(Status::unimplemented("GetMessages not yet implemented"))
    }

    async fn add_message(
        &self,
        _request: Request<amplifier_module::KernelAddMessageRequest>,
    ) -> Result<Response<amplifier_module::Empty>, Status> {
        Err(Status::unimplemented("AddMessage not yet implemented"))
    }

    async fn get_mounted_module(
        &self,
        _request: Request<amplifier_module::GetMountedModuleRequest>,
    ) -> Result<Response<amplifier_module::GetMountedModuleResponse>, Status> {
        Err(Status::unimplemented(
            "GetMountedModule not yet implemented",
        ))
    }

    async fn register_capability(
        &self,
        _request: Request<amplifier_module::RegisterCapabilityRequest>,
    ) -> Result<Response<amplifier_module::Empty>, Status> {
        Err(Status::unimplemented(
            "RegisterCapability not yet implemented",
        ))
    }

    async fn get_capability(
        &self,
        _request: Request<amplifier_module::GetCapabilityRequest>,
    ) -> Result<Response<amplifier_module::GetCapabilityResponse>, Status> {
        Err(Status::unimplemented("GetCapability not yet implemented"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_service_impl_compiles() {
        let coord = Arc::new(Coordinator::new(Default::default()));
        let _service = KernelServiceImpl::new(coord);
    }
}
```

**Step 3: Wire into lib.rs**

Add to `crates/amplifier-core/src/lib.rs`:

```rust
pub mod grpc_server;
```

**Step 4: Run tests**

Run:
```bash
cd amplifier-core && cargo test -p amplifier-core -- grpc_server
```
Expected: Test passes.

**Step 5: Commit**
```bash
cd amplifier-core && git add crates/amplifier-core/src/grpc_server.rs crates/amplifier-core/src/lib.rs crates/amplifier-core/Cargo.toml && git commit -m "feat: add KernelService gRPC server skeleton with ExecuteTool implementation"
```

---

### Task 15: GrpcToolBridge — Rust-side gRPC client for tool modules

**Files:**
- Create: `crates/amplifier-core/src/bridges/mod.rs`
- Create: `crates/amplifier-core/src/bridges/grpc_tool.rs`
- Modify: `crates/amplifier-core/src/lib.rs`

**Step 1: Create the bridges module**

Create `crates/amplifier-core/src/bridges/mod.rs`:

```rust
//! Transport bridge implementations.
//!
//! Each bridge wraps a remote module (gRPC, WASM, etc.) as an `Arc<dyn Trait>`,
//! making it indistinguishable from an in-process Rust module.

pub mod grpc_tool;
```

**Step 2: Implement GrpcToolBridge**

Create `crates/amplifier-core/src/bridges/grpc_tool.rs`. The bridge must:

- Hold a `ToolServiceClient<Channel>` (tonic gRPC client)
- Implement the `Tool` trait from `crate::traits`
- `connect(endpoint)` — async constructor that calls `GetSpec` to discover name/description
- `name()`, `description()`, `get_spec()` — return cached spec
- `execute(input)` — serialize input to `ToolExecuteRequest`, call gRPC, deserialize response to `ToolResult`
- Include a `#[cfg(test)]` compile-time trait object check: `fn _assert(_: Arc<dyn Tool>) {}`

**Step 3: Wire into lib.rs**

Add to `crates/amplifier-core/src/lib.rs`:

```rust
pub mod bridges;
```

**Step 4: Run tests**

Run:
```bash
cd amplifier-core && cargo test -p amplifier-core -- bridges
```
Expected: Compile-time trait check passes.

**Step 5: Commit**
```bash
cd amplifier-core && git add crates/amplifier-core/src/bridges/ crates/amplifier-core/src/lib.rs && git commit -m "feat: add GrpcToolBridge — Rust gRPC client implementing Tool trait"
```

---

### Task 16: GrpcProviderBridge — Rust-side gRPC client for provider modules

**Files:**
- Create: `crates/amplifier-core/src/bridges/grpc_provider.rs`
- Modify: `crates/amplifier-core/src/bridges/mod.rs`

**Step 1: Implement GrpcProviderBridge**

Create `crates/amplifier-core/src/bridges/grpc_provider.rs` following the same pattern as `grpc_tool.rs` but implementing the `Provider` trait. The bridge must:

- Hold a `ProviderServiceClient<Channel>`
- Implement `name()`, `get_info()`, `list_models()`, `complete()`, `parse_tool_calls()`
- Convert between proto messages and Rust types in each method
- Include compile-time trait check

**Step 2: Add to mod.rs**

```rust
pub mod grpc_provider;
```

**Step 3: Run tests**

Run:
```bash
cd amplifier-core && cargo test -p amplifier-core -- bridges::grpc_provider
```
Expected: Compile-time trait check passes.

**Step 4: Commit**
```bash
cd amplifier-core && git add crates/amplifier-core/src/bridges/ && git commit -m "feat: add GrpcProviderBridge — Rust gRPC client implementing Provider trait"
```

---

### Task 17: Remaining gRPC bridges — Orchestrator, Context, Hook, Approval

**Files:**
- Create: `crates/amplifier-core/src/bridges/grpc_orchestrator.rs`
- Create: `crates/amplifier-core/src/bridges/grpc_context.rs`
- Create: `crates/amplifier-core/src/bridges/grpc_hook.rs`
- Create: `crates/amplifier-core/src/bridges/grpc_approval.rs`
- Modify: `crates/amplifier-core/src/bridges/mod.rs`

**Step 1: Implement all four bridges**

Each bridge follows the same pattern: hold the gRPC client, implement the Rust trait, convert between proto and Rust types, include compile-time trait check.

- `GrpcOrchestratorBridge` implements `Orchestrator` trait, calls `OrchestratorService.Execute`
- `GrpcContextBridge` implements `ContextManager` trait, calls `ContextService.*`
- `GrpcHookBridge` implements `HookHandler` trait, calls `HookService.Handle`
- `GrpcApprovalBridge` implements `ApprovalProvider` trait, calls `ApprovalService.RequestApproval`

**Step 2: Wire all into mod.rs**

```rust
pub mod grpc_tool;
pub mod grpc_provider;
pub mod grpc_orchestrator;
pub mod grpc_context;
pub mod grpc_hook;
pub mod grpc_approval;
```

**Step 3: Run tests**

Run:
```bash
cd amplifier-core && cargo test -p amplifier-core -- bridges
```
Expected: All compile-time trait checks pass.

**Step 4: Commit**
```bash
cd amplifier-core && git add crates/amplifier-core/src/bridges/ && git commit -m "feat: add gRPC bridges for all 6 module types

Complete bridge set: GrpcToolBridge, GrpcProviderBridge,
GrpcOrchestratorBridge, GrpcContextBridge, GrpcHookBridge,
GrpcApprovalBridge. All implement the corresponding Rust traits."
```

---

### Task 18: Transport dispatch in the Rust module loader

**Files:**
- Create: `crates/amplifier-core/src/transport.rs`
- Modify: `crates/amplifier-core/src/lib.rs`

**Step 1: Create the transport dispatch module**

Create `crates/amplifier-core/src/transport.rs`:

```rust
//! Transport dispatch — routes module loading to the correct bridge.

use std::sync::Arc;
use crate::traits::Tool;

/// Supported transport types.
#[derive(Debug, Clone, PartialEq)]
pub enum Transport {
    Python,
    Grpc,
    Native,
    Wasm,
}

impl Transport {
    /// Parse a transport string from amplifier.toml.
    pub fn from_str(s: &str) -> Self {
        match s {
            "grpc" => Transport::Grpc,
            "native" => Transport::Native,
            "wasm" => Transport::Wasm,
            _ => Transport::Python,
        }
    }
}

/// Load a tool module via gRPC transport.
pub async fn load_grpc_tool(
    endpoint: &str,
) -> Result<Arc<dyn Tool>, Box<dyn std::error::Error>> {
    let bridge = crate::bridges::grpc_tool::GrpcToolBridge::connect(endpoint).await?;
    Ok(Arc::new(bridge))
}

/// Load a native Rust tool module (zero-overhead, no bridge).
pub fn load_native_tool(tool: impl Tool + 'static) -> Arc<dyn Tool> {
    Arc::new(tool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_parsing() {
        assert_eq!(Transport::from_str("python"), Transport::Python);
        assert_eq!(Transport::from_str("grpc"), Transport::Grpc);
        assert_eq!(Transport::from_str("native"), Transport::Native);
        assert_eq!(Transport::from_str("wasm"), Transport::Wasm);
        assert_eq!(Transport::from_str("unknown"), Transport::Python);
    }
}
```

**Step 2: Wire into lib.rs**

```rust
pub mod transport;
```

**Step 3: Run tests**

Run:
```bash
cd amplifier-core && cargo test -p amplifier-core -- transport
```
Expected: All tests pass.

**Step 4: Commit**
```bash
cd amplifier-core && git add crates/amplifier-core/src/transport.rs crates/amplifier-core/src/lib.rs && git commit -m "feat: add transport dispatch module — routes to grpc/native/wasm bridges"
```

---

### Task 19: End-to-end gRPC tool test

**Files:**
- Create: `crates/amplifier-core/tests/grpc_tool_e2e.rs`

**Step 1: Write an end-to-end integration test**

Create `crates/amplifier-core/tests/grpc_tool_e2e.rs` that:

1. Starts a tonic gRPC server implementing `ToolService` (an "echo" tool that echoes input back)
2. Connects via `GrpcToolBridge::connect(endpoint)`
3. Verifies `bridge.name() == "echo"` and `bridge.description()`
4. Calls `bridge.execute(json!({"message": "hello"}))`
5. Asserts `result.success == true` and `result.output == Some(input)`
6. Cleans up the server

**Step 2: Run the test**

Run:
```bash
cd amplifier-core && cargo test -p amplifier-core --test grpc_tool_e2e -- --nocapture
```
Expected: Test PASSES — full round-trip through gRPC.

**Step 3: Commit**
```bash
cd amplifier-core && git add crates/amplifier-core/tests/ && git commit -m "test: add end-to-end gRPC tool test — full round-trip through bridge

Phase 2 complete: KernelService server, all 6 gRPC bridges, transport
dispatch, and end-to-end test proving a gRPC tool works."
```

---

## Phase 3: Rust-Native Module Support

**Goal:** Enable `transport = "native"` — Rust modules that implement traits directly without any bridge or serialization overhead.

---

### Task 20: Example Rust-native tool module + integration test

**Files:**
- Modify: `crates/amplifier-core/src/testing.rs` (add EchoTool)
- Create: `crates/amplifier-core/tests/native_tool_e2e.rs`

**Step 1: Add an example EchoTool to the testing module**

Open `crates/amplifier-core/src/testing.rs` and add a public `EchoTool` struct that implements the `Tool` trait. It should echo its input back as `ToolResult { success: true, output: Some(input), error: None }`.

**Step 2: Write the native tool end-to-end test**

Create `crates/amplifier-core/tests/native_tool_e2e.rs` that:

1. Creates an `EchoTool` and loads it via `load_native_tool(EchoTool)`
2. Verifies `tool.name() == "echo"`
3. Executes it and asserts `result.success == true`
4. Also mounts it on a `Coordinator` via `coord.mount_tool("echo", tool)` and executes via coordinator

**Step 3: Run tests**

Run:
```bash
cd amplifier-core && cargo test -p amplifier-core --test native_tool_e2e -- --nocapture
```
Expected: Both tests PASS.

**Step 4: Commit**
```bash
cd amplifier-core && git add crates/amplifier-core/src/testing.rs crates/amplifier-core/tests/native_tool_e2e.rs && git commit -m "feat: Rust-native tool module — zero-overhead, no Python or gRPC needed

Phase 3 complete: native transport works end-to-end."
```

---

## Phase 4: Generated Code Replaces Hand-Written

**Goal:** Generated Python stubs replace `interfaces.py` and `_engine.pyi`. Validation switches to proto schema where possible. Verify zero behavioral change via equivalence tests.

---

### Task 21: Proto-to-Rust conversion functions

**Files:**
- Create: `crates/amplifier-core/src/generated/conversions.rs`
- Modify: `crates/amplifier-core/src/generated/mod.rs`

**Step 1: Implement From conversions between proto and hand-written types**

Create `crates/amplifier-core/src/generated/conversions.rs` with bidirectional `From` impls for:

- `models::ToolResult` <-> `amplifier_module::ToolResult`
- `models::ModelInfo` <-> `amplifier_module::ModelInfo`
- `messages::Usage` <-> `amplifier_module::Usage`

Each conversion must include a roundtrip test: `original -> proto -> restored`, asserting all fields match.

**Step 2: Wire into mod.rs**

```rust
pub mod conversions;
```

**Step 3: Run tests**

Run:
```bash
cd amplifier-core && cargo test -p amplifier-core -- generated::conversions
```
Expected: All roundtrip tests PASS.

**Step 4: Commit**
```bash
cd amplifier-core && git add crates/amplifier-core/src/generated/ && git commit -m "feat: add proto<->Rust conversion functions with roundtrip tests"
```

---

### Task 22: Proto type verification script

**Files:**
- Create: `scripts/generate_pyi.py`

**Step 1: Create the verification script**

Create `scripts/generate_pyi.py` that imports `amplifier_core._grpc_gen.amplifier_module_pb2` and verifies all 14 key proto types exist (`ChatRequest`, `ChatResponse`, `Message`, `ContentBlock`, `ToolResult`, `HookResult`, `ModelInfo`, `ProviderInfo`, `ApprovalRequest`, `ApprovalResponse`, `Usage`, `ModuleInfo`, `MountRequest`, `MountResponse`).

**Step 2: Run the verification**

Run:
```bash
cd amplifier-core && python scripts/generate_pyi.py
```
Expected: All types verified.

**Step 3: Commit**
```bash
cd amplifier-core && git add scripts/generate_pyi.py && git commit -m "build: add proto type stub verification script"
```

---

### Task 23: Proto-based JSON Schema validation

**Files:**
- Create: `python/amplifier_core/validation/proto_schema.py`
- Create: `tests/test_proto_schema_validation.py`

**Step 1: Write the test**

Create `tests/test_proto_schema_validation.py` with tests for `validate_tool_result()` and `validate_hook_result()`.

**Step 2: Implement the validation**

Create `python/amplifier_core/validation/proto_schema.py` with validation functions that check dicts against proto-derived schemas (valid hook actions, required fields, type checks).

**Step 3: Run tests**

Run:
```bash
cd amplifier-core && uv run pytest tests/test_proto_schema_validation.py -v
```
Expected: All tests PASS.

**Step 4: Commit**
```bash
cd amplifier-core && git add python/amplifier_core/validation/proto_schema.py tests/test_proto_schema_validation.py && git commit -m "feat: add proto-based validation — structural checks from proto schema"
```

---

### Task 24: Equivalence tests — proto-generated Python types match hand-written

**Files:**
- Create: `tests/test_generated_equivalence.py`

**Step 1: Write equivalence tests**

Create `tests/test_generated_equivalence.py` that verifies:

- Proto `ToolResult` has same fields as Python `ToolResult`
- Proto `HookResult` has all 15 fields matching Python
- Proto `HookAction` enum values map 1:1 to Python string values
- All 8 service stubs exist in generated grpc module

**Step 2: Run tests**

Run:
```bash
cd amplifier-core && uv run pytest tests/test_generated_equivalence.py -v
```
Expected: All tests PASS.

**Step 3: Commit**
```bash
cd amplifier-core && git add tests/test_generated_equivalence.py && git commit -m "test: add proto-to-Python equivalence tests — verify zero behavioral change

Phase 4 complete: proto-generated code verified equivalent to hand-written types."
```

---

## Phase 5: WASM Transport

**Goal:** Add wasmtime to the kernel. Implement WasmToolBridge. Prove a WASM module can execute with the same proto message format as gRPC.

---

### Task 25: Add wasmtime dependency

**Files:**
- Modify: `crates/amplifier-core/Cargo.toml`

**Step 1: Add wasmtime as optional dependency**

```toml
wasmtime = { version = "29", optional = true }

[features]
default = []
wasm = ["wasmtime"]
```

**Step 2: Verify it compiles**

Run:
```bash
cd amplifier-core && cargo build -p amplifier-core --features wasm
```
Expected: Build succeeds.

**Step 3: Commit**
```bash
cd amplifier-core && git add crates/amplifier-core/Cargo.toml && git commit -m "build: add wasmtime dependency behind 'wasm' feature flag"
```

---

### Task 26: WasmToolBridge implementation

**Files:**
- Create: `crates/amplifier-core/src/bridges/wasm_tool.rs`
- Modify: `crates/amplifier-core/src/bridges/mod.rs`

**Step 1: Implement WasmToolBridge**

Create `crates/amplifier-core/src/bridges/wasm_tool.rs` (gated behind `#![cfg(feature = "wasm")]`) that:

- Holds a wasmtime `Engine` and `Module`
- Has `from_bytes(wasm_bytes: &[u8])` constructor
- Implements the `Tool` trait
- Uses proto message format for serialization (same as gRPC)
- Includes compile-time trait check

**Step 2: Wire into mod.rs**

```rust
#[cfg(feature = "wasm")]
pub mod wasm_tool;
```

**Step 3: Add WASM loading to transport.rs**

```rust
#[cfg(feature = "wasm")]
pub fn load_wasm_tool(
    wasm_bytes: &[u8],
) -> Result<Arc<dyn Tool>, Box<dyn std::error::Error>> {
    let bridge = crate::bridges::wasm_tool::WasmToolBridge::from_bytes(wasm_bytes)?;
    Ok(Arc::new(bridge))
}
```

**Step 4: Run tests**

Run:
```bash
cd amplifier-core && cargo test -p amplifier-core --features wasm -- bridges::wasm_tool
```
Expected: Compile-time trait check passes.

**Step 5: Commit**
```bash
cd amplifier-core && git add crates/amplifier-core/src/bridges/ crates/amplifier-core/src/transport.rs && git commit -m "feat: add WasmToolBridge — WASM modules using same proto format as gRPC"
```

---

### Task 27: WASM integration test

**Files:**
- Create: `crates/amplifier-core/tests/wasm_tool_e2e.rs`

**Step 1: Write a WASM integration test**

Create `crates/amplifier-core/tests/wasm_tool_e2e.rs` (gated with `#![cfg(feature = "wasm")]`) that:

- Verifies `Transport::from_str("wasm") == Transport::Wasm`
- Compile-time checks that `WasmToolBridge` satisfies `Arc<dyn Tool>`

**Step 2: Run tests**

Run:
```bash
cd amplifier-core && cargo test -p amplifier-core --features wasm --test wasm_tool_e2e
```
Expected: All tests PASS.

**Step 3: Commit**
```bash
cd amplifier-core && git add crates/amplifier-core/tests/wasm_tool_e2e.rs && git commit -m "test: add WASM transport integration test

Phase 5 complete: WASM transport behind feature flag, same proto format as gRPC."
```

---

### Task 28: CI pipeline — verify generated code is in sync

**Files:**
- Create: `.github/workflows/proto-check.yml`

**Step 1: Create the CI workflow**

Create `.github/workflows/proto-check.yml` with two jobs:

1. **proto-sync**: Install protoc, generate Python stubs, diff against committed stubs, fail if out of sync
2. **rust-build**: Build Rust with proto generation, run all tests including `--features wasm`

**Step 2: Commit**
```bash
cd amplifier-core && git add .github/workflows/proto-check.yml && git commit -m "ci: add proto sync verification — ensures generated code matches proto"
```

---

### Task 29: Final validation — full test suite

**Files:**
- No new files

**Step 1: Run the complete Rust test suite**

Run:
```bash
cd amplifier-core && cargo test -p amplifier-core
```
Expected: All tests pass.

**Step 2: Run Rust tests with all features**

Run:
```bash
cd amplifier-core && cargo test -p amplifier-core --features wasm
```
Expected: All tests pass.

**Step 3: Rebuild Python extension and run full Python test suite**

Run:
```bash
cd amplifier-core && maturin develop && uv run pytest tests/ -v
```
Expected: All tests pass.

**Step 4: Verify proto generation still works**

Run:
```bash
cd amplifier-core/proto && make check
```
Expected: `:: Proto syntax valid`

**Step 5: Final commit**
```bash
cd amplifier-core && git add -A && git commit -m "chore: polyglot contracts implementation complete — all 5 phases

Phase 1: Proto expanded to 8 services, all message types, error taxonomy
Phase 2: KernelService gRPC server, 6 transport bridges, end-to-end test
Phase 3: Native Rust module support, zero-overhead trait loading
Phase 4: Proto-to-Rust conversions, proto-based validation, equivalence tests
Phase 5: WASM transport behind feature flag

All existing Python tests continue to pass unchanged."
```

---

## Summary

| Phase | Tasks | Key Deliverables |
|-------|-------|------------------|
| **Phase 1** | 1-13 | Complete proto (8 services), Makefile, Python/Rust stubs, CI, code quality fixes |
| **Phase 2** | 14-19 | KernelService server, 6 gRPC bridges, transport dispatch, e2e test |
| **Phase 3** | 20 | Native transport, example Rust tool, e2e test |
| **Phase 4** | 21-24 | Proto-to-Rust conversions, stub verification, proto-based validation, equivalence tests |
| **Phase 5** | 25-28 | wasmtime dependency, WasmToolBridge, WASM dispatch, CI pipeline |
| **Final** | 29 | Full test suite validation |

**Total:** 29 tasks across 5 phases.

**Invariant maintained throughout:** Existing Python path (PyO3 bridge, all Python tests) works unchanged at every step.
