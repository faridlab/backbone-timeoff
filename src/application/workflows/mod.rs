//! Workflow orchestrator (saga / multi-step patterns).
//!
//! No generated workflows for the timeoff module. The load-bearing
//! approve/cancel drawdown is a transactional service method (see TODO in
//! `timeoff_request_service_custom.rs` — port from backbone-hr's
//! `hr_write_service.rs`), not a saga. Add custom workflows here inside
//! `// <<< CUSTOM` markers.

// <<< CUSTOM
// END CUSTOM
