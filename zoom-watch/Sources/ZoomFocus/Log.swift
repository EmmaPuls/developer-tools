import OSLog

/// Logs go to the unified log: `log show --predicate 'subsystem == "local.zoomfocus"'`.
let logger = Logger(subsystem: "local.zoomfocus", category: "ZoomFocus")
