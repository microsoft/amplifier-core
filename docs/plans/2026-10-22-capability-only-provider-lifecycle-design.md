# Capability-only provider lifecycle

**Status:** Draft proposal for Core maintainers; not accepted or implemented.
**Baseline:** Core `9614052f` / the separately observed Core 2.0.2 wheel.
**Decision requested:** Whether a Python provider package may explicitly declare
a capability-only mount lifecycle, while ordinary provider validation stays strict.

This PR changes documentation only. Names below are proposed API spellings, not
available features. It assigns no Core version and changes no host or worker.

## Problem and existing contract

An explicitly configured decision service can supply a non-chat capability while
reusing a provider account's credentials, source pin, and configuration. Two
existing packages do this: [Jev][jev] and [local decisions][local]. They register
`decision.backends` and return cleanup, without implementing chat completion.
Their decision protocols and selection policy belong outside Core.

Core correctly rejects these mounts under its current provider contract:
[`_session_init.py`](../../python/amplifier_core/_session_init.py) requires a new
chat-provider object and, for named instances, a new object at the default slot
before remapping. [PR #122][strict] prevents a missing or failed account from
borrowing an earlier account. Those protections must remain.

An offline probe using the actual Core 2.0.2 `RustSession`, Jev, Streaming and
Simple Context reproduced this sequence without inference or network calls:

1. Jev registers its decision backend and returns a cleanup callback.
2. Core registers the callback, then rejects the missing chat-provider object
   with `Provider mount did not install a new provider instance`.
3. The provider table is restored, but the decision capability remains usable.
4. Normal session cleanup later removes and closes that backend.

A separate application run used the surviving backend successfully. That is
evidence of a lifecycle gap, **not** a supported successful mount. Consumers must
not depend on effects surviving rejection. The independent [library rollback
fix][library-fix] handles registration errors within the decisions library; it
does not change Core's mount acceptance.

The current authority is [provider initialization failures][failures]. The
provider-guide entrypoint example is corrected in this PR to require explicit
mounting and report missing required configuration as failure. That correction
describes existing enforcement; it does not implement this proposal or restore
silent ordinary mounts.

## Proposed boundary

Keep provider accounts and their configuration in `providers`. Add an explicit,
module-authored lifecycle declaration for Python modules whose entire entrypoint
supplies capabilities rather than chat providers. Default remains ordinary chat.
No decision protocol, model routing, credential store, browser policy, or vendor
name enters Core.

Illustrative declaration in the module that defines `mount`:

```python
__amplifier_module_type__ = "provider"
__amplifier_provider_lifecycle__ = "capabilities-v1"  # proposed, not available
```

The loader validates the declaration from the **resolved implementation**, before
invoking its mount, and carries it onto the bound mount callable. Missing means
the existing ordinary lifecycle. Unknown values, wrong types, or a declaration on
an ineligible module fail validation; they do not downgrade to ordinary loading.
User configuration, offline catalog metadata, a returned cleanup callback, and
changes to the capability registry cannot grant this exception.

Bind this declaration to the exact resolved entrypoint and mount attempt, along
with its source and account identity. Do not re-resolve it from a shared module
name or let configuration change the lifecycle between preflight and invocation.
The static declaration proposed here is suitable only when the entire entrypoint
uses that lifecycle; it cannot describe a provider that conditionally skips chat.

The same declaration must survive cached, resolver-selected filesystem, direct
entrypoint, and direct filesystem loads. A cache hit still binds fresh account
configuration. A same-ID installed package cannot substitute its declaration or
implementation for the selected source. Validation failures cannot be swallowed
by a discovery fallback to a different package.

The initial scope is Python modules loaded through `ModuleLoader`, using either
the Python `AmplifierSession` entry path or PyO3 `RustSession`. Both delegate
initialization to the shared helper; their teardown paths require separate
qualification. This is not a new Rust `Provider` trait or `ModuleType`. It grants
no capability-only support to WASM, gRPC, native Rust/Node, or sidecar transports;
those need an explicit future transport contract.

## Acceptance and account identity

| Mount | Required result |
| --- | --- |
| Ordinary / undeclared | Existing new-provider validation, exact named default-slot remapping and failure handling remain. `None` or a cleanup callable alone is insufficient. |
| Declared capability-only | Entire chat-provider table is unchanged: keys, object identities and iteration order. Return a cleanup callable or `None`; no chat object, remapping, or fake fallback provider. |
| Declared capability-only, disabled | A deliberate no-op may return `None`. The module must not publish a backend or perform inference. Acceptance does not claim a usable decision backend. |
| Declared capability-only mutates chat table or returns an invalid result | Reject, roll back the attempt, and do not queue readiness. |

Strict callable/`None` return validation is new only for the explicit branch.
Existing generic handling of non-callable returns is not silently changed.
Normal mixed chat-plus-capability providers remain ordinary providers and must
still install a real chat-provider object.

In the new branch, `None` is permitted only for a true no-op with no owned
executable registration or resource to undo. A module publishing such state must
return usable cleanup. This is a module conformance obligation, not a claim that
Core can audit arbitrary opaque state or sandbox third-party code.

`provider.before_load` and `provider.load_failure` retain the exact configured
source, `instance_id` and copied configuration privately. Failure events keep
their existing module/instance identity, reason and error fields; this proposal
adds no source/configuration payload or credentials to the public event stream.
Existing error text can contain secrets and still requires host redaction. Duplicate-instance
validation still applies. Capability-only acceptance skips chat remapping, not
account preflight or failure reporting. Core does not rename opaque capability
keys; providers/consumers validate their own binding identities and collisions.
Successful mounts alone can enqueue `on_session_ready`.

## Cleanup and rejection

Mounting is not a transaction over arbitrary third-party state. The registry
stores opaque values and lacks general ownership-aware rollback. Shallow copies
of a shared dictionary cannot provide that guarantee.

For a mount that **returns** a cleanup callback, keep that callback local until
the applicable acceptance/remapping checks pass. Then register it for ordinary
session cleanup and enqueue readiness. If a later check rejects the returned
mount, invoke that attempt's callback immediately, restore the provider table,
and only then deliver the ordinary failure event and host policy. Do not also
leave that callback in the session cleanup stack. This closes the rejected-return
gap for provider mounts; it does not alter successful ordinary provider checks.

Each returned handle has one owner and two exclusive destinations: normal
teardown after acceptance, or immediate rollback after rejection. Mark the
rollback attempt consumed **before** invoking or awaiting its handle. It is
invoked at most once for that rejected attempt, even if it raises or is cancelled.
Do not register it later, replay it during retry/teardown, or enqueue readiness.
An interrupted rollback is incomplete, not successful. Retaining and awaiting
an already-running cleanup task, where one exists, does not authorize starting
the callback again.

The callback must remove only its own callable registrations and close its owned
resources, preserving siblings and replacement registrations. Sync callbacks and
returned coroutines follow the existing cleanup contract; accepting other awaitable
types would require an explicit contract decision. Cleanup remains
idempotent because general session teardown can repeat. A callback must not
recursively await the session's cleanup.

For a mount that **raises or is cancelled before returning**, the module owns
unwinding its partial registrations and allocations before propagating the error.
Core cannot call a callback it never received or undo external effects. A module
must not register the same cleanup both directly and as its return value. A
previous direct registration is not proof that failure rollback has completed.
Providers must test interruption as well as `Exception`; the new library helper
does not, by itself, prove whole-module cancellation cleanup.

If the new capability-only branch rejects a returned mount and has no usable
cleanup or established unwind (for example, an invalid return after publication),
provider-table restoration is insufficient. Abort initialization and discard that
coordinator; do not ask host recovery to continue using it. Do not infer clean
state from `None` or from a shallow capability-registry comparison. This stricter
rule is scoped to the new opt-in branch; ordinary missing-config/no-new-provider
handling is not silently converted into a fatal error.

The proposed failure outcomes are:

| Outcome | Reporting and continuation |
| --- | --- |
| Mount/acceptance failure, rollback and provider-table restoration complete | Preserve the initiating failure; only then invoke normal failure reporting and host policy. |
| Cleanup or provider-table restoration fails | Abort initialization. Preserve the initiating mount/acceptance failure and separately report each cleanup/restoration failure through the exception chain or structured diagnostics. Do not replace the initiating error with a successful cleanup result or an ordinary recoverable warning. |
| Cancellation before return | The module unwinds its partial allocations; cancellation remains cancellation through the public session API. |
| Cancellation after return or during rollback | Consume any still-local handle through the rollback path at most once. Cancellation remains the control-flow outcome; retain any earlier failure and secondary cleanup failures as diagnostic context. Incomplete cleanup cannot be reported as success. |

On unsafe or incomplete rollback, do not start the next account, enqueue readiness,
invoke recovery that continues on the contaminated coordinator, or execute its
backends. Abort and discard the session. A secondary failure must remain observable
without adding credentials or private configuration to events. Cancellation must
not be translated into a provider warning or an ordinary exception by the Python
or Rust-backed session boundary.

A cleanup failure does not waive provider-table restoration: still attempt that
restoration and retain any additional failure, without allowing continuation.
Restoring the table alone does not establish that opaque backends are unusable.

No cancellation-proof drain, hidden deadline, or automatic replay is promised.
The host owns retaining/awaiting an existing cleanup task and abandoning or
rebuilding the session. Exact exception representation and cancellation sequencing
need maintainer agreement; the outcome and no-continuation rules above are part
of the proposal, not guarantees of current Core.

Observation contributors are not generic executable-capability leases. This draft
does not invent an unregister API or claim every contributor mutation is undone.
If an implementation needs new ownership primitives, that is an explicit separate
contract decision, not permission to restore somebody else's registry wholesale.

## Validation and compatibility

The package validator must recognize the same declaration. It validates the
new branch's mount signature, returned cleanup and unchanged chat table without
requiring a fake `complete()` method. Ordinary `ProviderValidator` structural
checks remain intact. Resolver validation currently invokes a mount on a test
coordinator before the real mount: both paths must clean up, and validation must
not be mistaken for evidence that the later real session accepted the module.
The new validator branch must fail if its cleanup fails or is incomplete; it
cannot use the existing validator's catch-and-ignore cleanup behavior as proof
of success. Its test coordinator/validation environment must expose the same
ratified support signal as the real session.

Add a public, versioned Core support signal, with spelling ratified alongside the
declaration. Updated capability-only modules check support **before publishing
anything**. An unsupported enabled mount raises an actionable compatibility error;
it cannot silently succeed or publish first. Hosts can surface that prerequisite
before session start. A signal is not user consent to enable decisions.

Once a qualified Core release exists, provider packaging/documentation can name
its actual compatibility floor. No future version number is assumed here. Test
both the support check and installed Core/worker provenance, including older Core
that ignores the declaration. Ordinary CLI/browser/desktop workflows do not need
to enable decisions or migrate merely because the packages are available.
Disabled no-op acceptance also requires the new contract: older Core rejects an
explicitly listed disabled Jev/local row because it installs no chat provider.
Hosts on those runtimes must omit inactive optional runtime rows while retaining
saved account configuration; they must not claim that `enabled: false` changes
Core's acceptance rules.

| Consumer | Adoption work |
| --- | --- |
| Jev and local-decisions | Declare the accepted lifecycle, guard older Core before side effects, retain saved account/auth/source formats, and qualify real-session success, disabled mode and interrupted partial mounts. No weights or inference server installation is implied. |
| OpenAI and ordinary Azure chat+decisions | Stay on the strict ordinary path. No module-wide capability-only declaration. |
| Azure's optional `chat_enabled: false` path | Source inspection of [Azure PR #45][azure] at `2f36a9d` shows it skips the chat mount; this is not a new runtime reproduction. It is **not covered** by the narrow Jev/local declaration. Hold this combination unsupported unless separately reviewed as a per-attempt or distinct-entrypoint lifecycle; configuration alone cannot choose Core acceptance semantics. Endpoint-fix clearance does not qualify this mode. No claim that default Azure has this failure. |
| Unified | Qualify both service and actual packaged worker against accepted Core/provider commits; preserve saved accounts, child/resume credential binding and frozen worker generations. Use the existing explicit component-update path. |
| CLI / other apps | Updated modules retain provider configuration shape on supported Core. Older Core must reject enabled optional support without side effects. Existing ordinary workflows remain first-class. |
| UI-use, Work, loops | No new routing or dispatch policy in Core. Preserve explicit opt-in, ordinary default/recovery and Work's single behavior include. |

## Supported alternative if Core declines

Keep `providers` chat-only and expose decision hooks from the existing provider
packages. Core already supports hook mounts that register capabilities and return
cleanup, so this needs no new Core lifecycle exception or repository.

This is more than an entrypoint alias. A host must project a validated saved
provider account into a hook mount with the same exact source, account identity,
explicit enablement and authorized configuration. Unified currently materializes,
persists and rebinds credentials through provider rows, including child/resumed
sessions. Moving a row earlier can drop credentials; copying it later without its
binding rules can broaden access. The projection and saved logical account must
be qualified together. Arbitrary hooks must not acquire provider credentials.

Current hook loading does not invoke `provider.before_load` or
`provider.load_failure`; a hook failure alone does not preserve the saved account's
provider policy. The approved host/provider-hook adapter must explicitly preserve
logical account preflight and failure handling for the exact authorized source,
instance and configuration, including child/resume snapshots. An unavailable
selected account must remain unavailable; do not substitute another account or
credential. This adapter is additional host integration work, not an existing
Core guarantee or a free hook alias.

A CLI user can explicitly compose the hook with its documented environment
references on existing Core, but automatic migration of saved provider accounts
is not supplied by an alias. Do not make a new CLI migration an unrelated release
prerequisite. Ordinary OpenAI/Azure remain providers. Cleanup ownership and partial
failure requirements still apply to the hook implementation.

## Implementation and acceptance after agreement

The proposed implementation is bounded to `loader.py`,
`validation/provider.py`, the shared `_session_init.py`, and focused lifecycle
tests/contracts. Investigate bindings only if implementing the agreed cleanup or
support signal requires them; do not claim a native artifact is unchanged without
checking. Ship any changed Core through its own qualified artifact process.

Required tests, not results claimed by this documentation PR:

- Existing ordinary silent-mount, wrong-slot, named-account and sibling-order
  regressions pass unchanged; true provider failures still report and call policy.
- Declared capability-only success/disabled mounts produce no false load failure
  and no chat entries; accidental chat-table mutation and invalid returns reject.
- Direct/resolved/cached loader and validator paths agree, including malformed
  declarations, selected-source conflicts and fresh multi-account configuration.
- Failed or cancelled partial mounts leave no executable backend; multi-account
  rejected-return cleanup precedes the next account and failure policy, preserving
  sibling/replacement registrations and restoring provider-table order/identity.
  Cleanup failure or unestablished unwind on the new branch aborts. Rejected mounts
  never become ready; teardown/retry cannot register or replay consumed cleanup.
  Assert initiating and secondary failures remain separately observable.
- Exercise enabled success, disabled no-op, failure and cancellation through the
  actual installed `RustSession`, real coordinator and loader, as well as the
  Python session entry path. Assert the public session boundary preserves
  cancellation and failed initialization, including cancellation before return
  and during rejected-return rollback. Direct helper or mock-coordinator tests
  alone do not qualify the installed Rust-backed API. Record wheel, source and
  import provenance for the supported runtime/version, independently of the old
  Core 2.0.2 rejection reproduction.
- Real Jev/local lifecycle with fake transports makes zero inference calls during
  mount, disabled mode, validation and cleanup. Older unsupported Core publishes
  nothing; supported Core mounts successfully. No network keys are needed for this.
- Clean public host and packaged worker installs then prove ordinary-default and
  explicitly configured decision tasks, exact selected account/model and recovery.
  Earlier CLI, application and Mac evidence keeps its original source identity.

Maintainers are asked to decide the lifecycle mechanism versus hooks, declaration
and support-signal spelling, and the rejected-return/cancellation contract before
implementation. Acceptance of this document alone is not runtime support. The
configured-decision integration depends on resolving this boundary; unrelated
application releases do not.

[jev]: https://github.com/microsoft/amplifier-module-provider-jev/tree/7ccade1eed84b17f215bc2e80253a1fb4c3a2130
[local]: https://github.com/microsoft/amplifier-module-provider-local-decisions/tree/75d743fb2f7ea599dd92709625a8e0661dcf7ae8
[strict]: https://github.com/microsoft/amplifier-core/pull/122
[library-fix]: https://github.com/microsoft/amplifier-lib-decisions/pull/1
[azure]: https://github.com/microsoft/amplifier-module-provider-azure-openai/pull/45
[failures]: ../CAPABILITY_REGISTRY.md#provider-initialization-failures
