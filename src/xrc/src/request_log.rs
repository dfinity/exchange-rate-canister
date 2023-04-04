use std::collections::VecDeque;

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
        self.entries.push_back(RequestLogEntry {
            timestamp,
            caller: caller.clone(),
            request: request.clone(),
            result: result.clone(),
        });
        if self.entries.len() > self.max_entries {
            self.entries.pop_front();
        }
    }

    pub fn entries(&self) -> &VecDeque<RequestLogEntry> {
        &self.entries
    }
}

pub(crate) fn log(
    caller: &Principal,
    timestamp: u64,
    request: &GetExchangeRateRequest,
    result: &GetExchangeRateResult,
) {
    PRIVILEGED_REQUEST_LOG.with(|cell| {
        cell.borrow_mut().log(caller, timestamp, request, result);
    });
}
