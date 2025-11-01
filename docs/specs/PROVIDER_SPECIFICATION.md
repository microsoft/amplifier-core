# Provider Module Specification

**Purpose**: Complete specification for implementing provider modules - the boundary between kernel and LLM vendors.

**Scope**: Covers protocol, message format, streaming, errors, capabilities, and implementation patterns.

---

## Overview

Provider modules connect Amplifier to LLM vendors (Anthropic, OpenAI, Azure, Ollama, etc.) through a unified interface.

**Key principles**:
- **Preserve fidelity**: Keep all content types exactly as provider returns them
- **Degrade explicitly**: Record degradations when features unsupported
- **Emit events**: Observability through canonical events
- **Non-interference**: Provider failures don't crash kernel

---

## Protocol Interface

### Core Types

```python
from typing import Protocol, AsyncIterator, Dict, Any, List, Literal, Optional
from dataclasses import dataclass

Role = Literal["system", "developer", "user", "assistant", "function"]

@dataclass
class ChatRequest:
    messages: List[Dict[str, Any]]  # See Request Envelope below
    tools: Optional[List[ToolSpec]] = None
    response_format: Optional[Dict[str, Any]] = None
    temperature: Optional[float] = None
    top_p: Optional[float] = None
    max_output_tokens: Optional[int] = None
    conversation_id: Optional[str] = None
    stream: bool = False
    metadata: Optional[Dict[str, Any]] = None

@dataclass
class ChatResponse:
    content: Union[str, List[ContentBlock]]
    tool_calls: Optional[List[ToolCall]] = None
    usage: Optional[Usage] = None
    item_id: Optional[str] = None
    degradations: Optional[List[Degradation]] = None
    raw: Optional[Dict[str, Any]] = None  # Native provider payload

@dataclass
class Degradation:
    feature: str      # What feature couldn't be used
    reason: str       # Why it wasn't supported
    fallback: str     # What was done instead
    details: Optional[Dict[str, Any]] = None
```

### Provider Protocol

```python
class ProviderModule(Protocol):
    name: str

    async def capabilities(self) -> Dict[str, Any]:
        """Advertise supported features."""

    async def complete(self, req: ChatRequest) -> ChatResponse:
        """Non-streaming completion."""

    async def stream(self, req: ChatRequest) -> AsyncIterator[Dict[str, Any]]:
        """Streaming completion with normalized events."""
```

**Implementation notes**:
- Use Pydantic models from `amplifier_core.message_models`
- Convert REQUEST_ENVELOPE → provider-specific format
- Convert provider response → REQUEST_ENVELOPE
- Emit `provider:request/response/error` events

---

## Request Envelope (Message Format)

### Message Structure

```python
{
  "role": "system|developer|user|assistant|function",
  "content": "text or array<ContentBlock>",
  "name": "optional tool/function name",
  "tool_call_id": "for function result messages"
}
```

### Content Block Types

**Text**:
```python
{"type": "text", "text": "..."}
```

**Tool Call**:
```python
{
  "type": "tool_call",
  "id": "call_1",
  "name": "search",
  "input": {...}
}
```

**Tool Result**:
```python
{
  "type": "tool_result",
  "tool_call_id": "call_1",
  "output": {...}
}
```

**Image**:
```python
{
  "type": "image",
  "source": {
    "type": "base64",
    "media_type": "image/png",
    "data": "..."
  }
}
```

**Reasoning/Thinking**:
- **OpenAI**: `reasoning` blocks with `content[]` and `summary[]`
- **Anthropic**: `thinking` / `redacted_thinking` blocks with `signature`

**Critical**: Preserve all block types exactly as received. Do NOT flatten to text.

### Tools

Function-style with JSON Schema parameters:

```python
{
  "name": "search",
  "description": "Search the web",
  "parameters": {
    "type": "object",
    "properties": {
      "query": {"type": "string"}
    },
    "required": ["query"]
  }
}
```

### Response Format

```python
# Preferred (strict schema)
{"type": "json_schema", "schema": {...}, "strict": true}

# Fallbacks
{"type": "json"}        # JSON mode without schema
{"type": "text"}        # Plain text

# When downgrading: Record Degradation!
```

### Parameters

- `temperature` (float) - Sampling temperature
- `top_p` (float) - Nucleus sampling
- `max_output_tokens` (int) - Response length limit
- `stream` (bool) - Enable streaming
- `conversation_id` (str, optional) - Thread ID for multi-turn

---

## Streaming Events

### Event Envelope

All streaming events include:
- `ts` (timestamp)
- `session_id`, `turn_id`, `span_id`, `parent_span_id`
- `seq` (monotonic sequence number)
- `visibility` (optional: "internal", "developer", "user")

### Event Types

**message.start**:
```python
{
  "type": "message.start",
  "item_id": "msg_123",
  "role": "assistant"
}
```

**text.delta**:
```python
{
  "type": "text.delta",
  "text": "incremental text chunk"
}
```

**tool_call.start**:
```python
{
  "type": "tool_call.start",
  "id": "call_1",
  "name": "search"
}
```

**tool_call.delta**:
```python
{
  "type": "tool_call.delta",
  "id": "call_1",
  "delta": "{\"query\": \"partial"
}
```

**tool_call.end**:
```python
{
  "type": "tool_call.end",
  "id": "call_1",
  "input": {"query": "complete args"}
}
```

**message.end**:
```python
{
  "type": "message.end",
  "usage": {"input_tokens": 100, "output_tokens": 50},
  "tool_calls": [...],
  "degradations": [...]
}
```

**error**:
```python
{
  "type": "error",
  "kind": "rate_limit",
  "message": "Rate limit exceeded",
  "status": 429
}
```

**Reasoning/Thinking streams**: Include deltas when policy allows; otherwise stream summaries or presence metadata only.

---

## Error Handling

### Error Types

```python
class ProviderTransportError(Exception):
    """DNS, TLS, connect, timeout errors."""
    retry_after_ms: Optional[int]
    raw: Optional[Dict]

class ProviderRateLimitError(Exception):
    """429 errors, quota exceeded."""
    retry_after_ms: Optional[int]
    raw: Optional[Dict]

class ProviderInvalidRequestError(Exception):
    """Schema errors, size limits, invalid content."""
    status: Optional[int]
    raw: Optional[Dict]

class ProviderCapabilityError(Exception):
    """Required feature unsupported (fail-closed mode)."""
```

### Error Handling Pattern

```python
async def complete(self, req: ChatRequest) -> ChatResponse:
    try:
        # Call provider API
        response = await self._call_api(req)
        return self._parse_response(response)

    except httpx.TimeoutException as e:
        raise ProviderTransportError("Request timeout", retry_after_ms=1000)

    except httpx.HTTPStatusError as e:
        if e.response.status_code == 429:
            raise ProviderRateLimitError(
                "Rate limit exceeded",
                retry_after_ms=int(e.response.headers.get("retry-after", 60)) * 1000
            )
        elif e.response.status_code >= 400 and e.response.status_code < 500:
            raise ProviderInvalidRequestError(
                f"Invalid request: {e.response.text}",
                status=e.response.status_code
            )
        raise ProviderTransportError(f"HTTP {e.response.status_code}")
```

### Event Emission

```python
# Emit provider:error event
self.emit_event("provider:error", {
    "error_type": type(e).__name__,
    "message": str(e),
    "retry_after_ms": getattr(e, "retry_after_ms", None)
})
```

---

## Capabilities & Degradation

### Capabilities Advertisement

```python
async def capabilities(self) -> Dict[str, Any]:
    return {
        "modalities": ["text", "image"],
        "roles": ["system", "developer", "user", "assistant", "function"],
        "tools": {
            "supported": True,
            "parallel": True
        },
        "streaming": {
            "text": True,
            "tools": True,
            "reasoning": True
        },
        "response_format": {
            "text": True,
            "json": True,
            "json_schema": {"strict": True}
        },
        "limits": {
            "max_input_tokens": 200000,
            "max_output_tokens": 8000
        },
        "threading": {
            "conversation_id": True
        },
        "auth": {
            "api_key": True,
            "azure_msi": False
        },
        "reasoning": {
            "openai": False,
            "anthropic_thinking": True
        }
    }
```

### Graceful Degradation

When a requested feature isn't supported:

```python
# Example: Strict json_schema not supported
degradations = []

if req.response_format and req.response_format.get("strict"):
    degradations.append(Degradation(
        feature="response_format.strict",
        reason="Provider doesn't support strict mode",
        fallback="Using json mode without schema enforcement"
    ))
    # Downgrade to json mode
    response_format = {"type": "json"}

# Include in response
return ChatResponse(
    content=...,
    degradations=degradations
)
```

**Critical**: Never silently downgrade. Always record degradations.

---

## Provider-Specific Notes

### Anthropic

- Merge `developer` role → `system` role
- Preserve `thinking` and `signature` fields across tool turns
- Re-send `thinking` blocks unmodified with signature for tool result messages

### OpenAI / Azure

- Use structured response format (items API)
- Preserve function args/IDs and `reasoning` blocks
- Support conversation threading via `conversation_id`

### Ollama

- Map to OpenAI-compatible format where supported
- Record degradations for unsupported features
- May have limited capability set

---

## Implementation Checklist

### Basic Implementation

- [ ] Implement `ProviderModule` protocol
- [ ] Convert REQUEST_ENVELOPE → provider API format
- [ ] Convert provider response → REQUEST_ENVELOPE
- [ ] Preserve all content block types (no flattening)
- [ ] Handle all error types with proper exceptions

### Streaming

- [ ] Implement `stream()` method
- [ ] Emit normalized streaming events
- [ ] Include proper event envelope (session_id, etc.)
- [ ] Handle partial tool calls correctly

### Capabilities

- [ ] Implement `capabilities()` method
- [ ] Advertise accurate feature support
- [ ] Degrade gracefully when features unsupported
- [ ] Record degradations in response

### Observability

- [ ] Emit `provider:request` event before API call
- [ ] Emit `provider:response` event after success
- [ ] Emit `provider:error` event on failure
- [ ] Include degradations in events

### Testing

- [ ] Test with all message types (text, tool_call, tool_result, image)
- [ ] Test streaming with partial chunks
- [ ] Test error handling (timeout, rate limit, invalid request)
- [ ] Test capability negotiation and degradation
- [ ] Verify event emission

---

## Reference Implementation

See `amplifier-module-provider-anthropic` for complete reference implementation showing:
- Full protocol implementation
- Message conversion (envelope ↔ Anthropic format)
- Tool pair validation
- Streaming with normalized events
- Capability advertisement
- Error handling and event emission

---

## Summary

**Provider modules**:
- Implement `ProviderModule` protocol
- Convert between REQUEST_ENVELOPE and vendor format
- Preserve all content types (no flattening)
- Degrade explicitly with recorded degradations
- Emit canonical events for observability
- Handle errors with proper exception types

**Key contracts**:
- Request/Response format (REQUEST_ENVELOPE)
- Streaming events (normalized across vendors)
- Error types (normalized exceptions)
- Capabilities (feature advertisement)

**Philosophy**:
- Kernel provides mechanism (protocol)
- Providers implement policy (how to call vendor API)
- Observability through events (not internal logging)
- Non-interference (failures contained)
