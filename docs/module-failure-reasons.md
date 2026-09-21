# Module-load failure categories

The `module:load_failed` event includes an optional `reason_code` in addition to its existing module type, ID, and legacy error text. Hosts can expose the bounded category without forwarding exception text or configuration.

Categories are `invalid_package_layout`, `missing_source`, `invalid_entry_point`, `invalid_module_metadata`, `validation_failed`, and `unknown`. Codes originate from typed loader failures and named validation checks, never from exception-string parsing. Unknown exceptions fall back to `unknown`. Missing metadata still uses the existing naming fallback; explicitly invalid declared types are rejected instead of swallowing the validation error and falling back.

The legacy `error` field remains unchanged for compatibility and is not a safe public diagnostic. Consumers should allowlist `reason_code` and supply their own remediation text. This addition changes neither the provider/tool/hook nonfatal session policy nor host decisions about incomplete configured sessions.

Validation used Python source over the installed native kernel: 44 loader and session-initialization tests passed, including fixtures for the four requested categories, unknown and forged codes, and event compatibility. No native code or provider protocol changed.
