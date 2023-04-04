use std::{collections::VecDeque, thread::LocalKey, cell::RefCell};

use candid::Principal;
use ic_xrc_types::{GetExchangeRateRequest, GetExchangeRateResult};

use crate::PRIVILEGED_REQUEST_LOG;

pub(crate) struct RequestLogEntry {
    pub timestamp: u64,
    pub caller: Principal,
    pub request: GetExchangeRateRequest,
    pub result: GetExchangeRateResult,
}

pub(crate) struct RequestLog {
    entries: VecDeque<RequestLogEntry>,
    max_entries: usize,
}

impl RequestLog {
    pub(crate) fn new(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            max_entries,
        }
    }

    pub fn log(
        &mut self,
        caller: &Principal,
        timestamp: u64,
        request: &GetExchangeRateRequest,
        result: &GetExchangeRateResult,
    ) {
        self.entries.push_front(RequestLogEntry {
            timestamp,
            caller: caller.clone(),
            request: request.clone(),
            result: result.clone(),
        });
        if self.entries.len() > self.max_entries {
            self.entries.pop_back();
        }
    }

    pub fn entries(&self) -> &VecDeque<RequestLogEntry> {
        &self.entries
    }
}

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
