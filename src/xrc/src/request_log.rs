use std::{cell::RefCell, collections::VecDeque, thread::LocalKey};

use candid::Principal;
use ic_xrc_types::{GetExchangeRateRequest, GetExchangeRateResult};

/// A single entry in the log containing the timestamp, caller, request, and
/// result of the request.
pub(crate) struct RequestLogEntry {
    pub timestamp: u64,
    pub caller: Principal,
    pub request: GetExchangeRateRequest,
    pub result: GetExchangeRateResult,
}

/// Data structure that contains the most recent requests and results.
pub(crate) struct RequestLog {
    entries: VecDeque<RequestLogEntry>,
    /// Max number of entries the log should contain.
    max_entries: usize,
}

impl RequestLog {
    /// Create a new log for recording requests.
    pub(crate) fn new(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            max_entries,
        }
    }

    /// Writes the provided parameters into a new log entry and if the log
    /// has more entries than `max_entries`, it prunes off the oldest.
    pub fn log(
        &mut self,
        caller: &Principal,
        timestamp: u64,
        request: &GetExchangeRateRequest,
        result: &GetExchangeRateResult,
    ) {
        self.entries.push_front(RequestLogEntry {
            timestamp,
            caller: *caller,
            request: request.clone(),
            result: result.clone(),
        });
        if self.entries.len() > self.max_entries {
            self.entries.pop_back();
        }
    }

    /// Returns a reference to the internal log entries for read purposes.
    pub fn entries(&self) -> &VecDeque<RequestLogEntry> {
        &self.entries
    }
}

/// A simple helper to quickly log to a global state request log.
pub(crate) fn log(
    safe_log: &'static LocalKey<RefCell<RequestLog>>,
    caller: &Principal,
    timestamp: u64,
    request: &GetExchangeRateRequest,
    result: &GetExchangeRateResult,
) {
    safe_log.with(|cell| {
        cell.borrow_mut().log(caller, timestamp, request, result);
    });
}
