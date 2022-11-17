use std::{cell::RefCell, thread::LocalKey};

use ic_stable_structures::{
    cell::Cell,
    log::Log,
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, RestrictedMemory,
};

use crate::types::Config;

const WASM_PAGE_SIZE: u64 = 65536;

const GIB: usize = 1024 * 1024 * 1024;

/// How much memory do we want to allocate for raw blocks.
const DEFAULT_MEMORY_LIMIT: usize = 3 * GIB;

/// The maximum number of blocks to return in a single get_transactions request.
const DEFAULT_MAX_TRANSACTIONS_PER_GET_TRANSACTION_RESPONSE: usize = 2000;

/// The maximum number of Wasm pages that we allow to use for the stable storage.
const NUM_WASM_PAGES: u64 = 4 * (GIB as u64) / WASM_PAGE_SIZE;

const ENTRIES_INDEX_MEMORY_ID: MemoryId = MemoryId::new(0);
const ENTRIES_DATA_MEMORY_ID: MemoryId = MemoryId::new(1);

type Memory = RestrictedMemory<DefaultMemoryImpl>;
type ConfigCell = Cell<Config, Memory>;
type EntryLog = Log<VirtualMemory<Memory>, VirtualMemory<Memory>>;

fn config_memory() -> Memory {
    RestrictedMemory::new(DefaultMemoryImpl::default(), 0..1)
}

fn entries_memory() -> Memory {
    RestrictedMemory::new(DefaultMemoryImpl::default(), 1..NUM_WASM_PAGES)
}

thread_local! {

    static MEMORY: DefaultMemoryImpl = DefaultMemoryImpl::default();

    static MEMORY_MANAGER: RefCell<MemoryManager<Memory>> = RefCell::new(MemoryManager::init(entries_memory()));

    static CONFIG: RefCell<ConfigCell> = RefCell::new(ConfigCell::init(config_memory(), Config::default()).expect("failed to initialize stable cell"));

    static ENTRIES: RefCell<EntryLog> = with_memory_manager(|manager| {
        RefCell::new(EntryLog::init(manager.get(ENTRIES_INDEX_MEMORY_ID), manager.get(ENTRIES_DATA_MEMORY_ID)).expect("failed to initialize stable log"))
    });

}

fn with_memory_manager<R>(f: impl FnOnce(&MemoryManager<Memory>) -> R) -> R {
    MEMORY_MANAGER.with(|cell| f(&*cell.borrow()))
}

pub(crate) fn with_config<R>(f: impl FnOnce(&Config) -> R) -> R {
    CONFIG.with(|cell| f(cell.borrow().get()))
}

pub(crate) fn with_entries<R>(f: impl FnOnce(&EntryLog) -> R) -> R {
    ENTRIES.with(|cell| f(&*cell.borrow()))
}
