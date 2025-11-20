# Changelog

All notable changes to amplifier-core will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed
- Standardized on ChatRequest/ChatResponse throughout provider and orchestrator interfaces
- Simplified provider implementations by removing legacy format support
- **BREAKING**: Approval and display systems now injected by app layer (not hardcoded in kernel)
  - `AmplifierSession` accepts optional `approval_system` and `display_system` parameters
  - `ModuleCoordinator` accepts injected systems instead of hardcoding `CLIApprovalSystem`/`CLIDisplaySystem`
  - Removes `rich` dependency from kernel (moved to app layer where it belongs)
  - Aligns with kernel philosophy: mechanism (Protocol) not policy (CLI implementation)
- Enhanced PyPI package metadata (keywords, classifiers, project URLs)

### Added
- `ApprovalSystem` and `DisplaySystem` Protocol definitions (mechanism)
- Support for app-layer UX system injection via session constructor

### Removed
- `CLIApprovalSystem` implementation (moved to amplifier-app-cli)
- `CLIDisplaySystem` implementation (moved to amplifier-app-cli)
- `rich>=13.0` dependency (UX is app-layer policy, not kernel mechanism)

### Fixed
- Added missing `pytest-asyncio` dev dependency for async test support

## [1.0.0] - YYYY-MM-DD (To Be Released)

Initial release of the ultra-thin kernel for Amplifier modular AI agent system.

### Core Features
- Module discovery and loading (entry points, git URLs)
- Session lifecycle management
- Hook system with canonical event emission
- Coordinator infrastructure with capability registration
- Mount plan configuration system
- Session forking for sub-agents

### Module Protocols
- Provider: LLM backends
- Tool: Agent capabilities
- Orchestrator: Execution loops
- Context: Memory management
- Hook: Observability and control

[Unreleased]: https://github.com/microsoft/amplifier-core/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/microsoft/amplifier-core/releases/tag/v1.0.0
