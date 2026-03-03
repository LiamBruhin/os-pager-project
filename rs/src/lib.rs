use core::ffi::{c_char, c_int, c_uchar, c_uint};
use std::ffi::CString;
use std::fmt;
use std::sync::{LazyLock, Mutex};

// All adapted from mm_api.h
// Maximum processes allowed.
// Valid values of 'pid' arguments are 0, 1, 2, 3.
const MM_MAX_PROCESSES: usize = 4;

const MM_PAGE_SIZE_BITS: usize = 4; // 16b pages (fits 8 * 2 byte PTEs)
const MM_PHYSICAL_MEMORY_SIZE_SHIFT: usize = MM_PAGE_SIZE_BITS + 2; // 4 pages physical mem
const MM_PROCESS_VIRTUAL_MEMORY_SIZE_SHIFT: usize = MM_PHYSICAL_MEMORY_SIZE_SHIFT + 1; // 8 pages virtual mem
const MM_MAX_PTE_SIZE_BYTES: usize = 2; // Each page table entry is 1-2 bytes.

const MM_PAGE_SIZE_BYTES: usize = 1 << MM_PAGE_SIZE_BITS;
const MM_PAGE_OFFSET_MASK: usize = MM_PAGE_SIZE_BYTES - 1;

const MM_PHYSICAL_MEMORY_SIZE_BYTES: usize = 1 << MM_PHYSICAL_MEMORY_SIZE_SHIFT;
const MM_PROCESS_VIRTUAL_MEMORY_SIZE_BYTES: usize = 1 << MM_PROCESS_VIRTUAL_MEMORY_SIZE_SHIFT;
const MM_PHYSICAL_PAGES: usize = MM_PHYSICAL_MEMORY_SIZE_BYTES / MM_PAGE_SIZE_BYTES;
const MM_NUM_PTES: usize = MM_PROCESS_VIRTUAL_MEMORY_SIZE_BYTES / MM_PAGE_SIZE_BYTES;
const MM_PAGE_TABLE_SIZE_BYTES: usize = MM_NUM_PTES * MM_MAX_PTE_SIZE_BYTES;

static mut debug_print: bool = false;

macro_rules! debug {
    () => {
        if unsafe { debug_print } {
        #[cfg(debug_assertions)]
            println!("{}:{}", file!(), line!());
        }
    };
    // This arm handles all other cases with format strings and arguments
    ($($arg:tt)*) => {
        if unsafe { debug_print } {
        #[cfg(debug_assertions)]
        print!("{}:{}: ", file!(), line!());
        println!($($arg)*);
        }
    };
}

#[unsafe(no_mangle)]
pub extern "C" fn Debug() {
    unsafe {
        debug_print = true;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn free_string(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    unsafe {
        // Recreate the CString from the raw pointer to allow Rust to drop it and free the memory
        CString::from_raw(s);
    }
}

// A single page table entry.
struct PageTableEntry {
    value: u16,
}

impl PageTableEntry {
    const VALID_BIT: usize = 15;
    const WRITABLE_BIT: usize = 14;
    const SWAPPED_BIT: usize = 13;
    fn physical_page(&self) -> u8 {
        (self.value & 0xff) as u8
    }
    fn set_physical_page(&mut self, physical_page: u8) {
        self.value = (self.value & 0xFF00) | physical_page as u16;
    }
    fn is_bit_set(&self, bit: usize) -> bool {
        if (self.value & (1 << bit)) == (1 << bit) {
            true
        } else {
            false
        }
    }
    fn set_bit(&mut self, bit: usize, value: bool) {
        if value {
            self.value |= 1 << bit;
        } else {
            self.value &= !(1 << bit);
        }
    }
    fn valid(&self) -> bool {
        self.is_bit_set(Self::VALID_BIT)
    }
    fn set_valid(&mut self, value: bool) {
        self.set_bit(Self::VALID_BIT, value);
    }
    fn writable(&self) -> bool {
        self.is_bit_set(Self::WRITABLE_BIT)
    }
    fn set_writable(&mut self, value: bool) {
        self.set_bit(Self::WRITABLE_BIT, value);
    }
    fn swapped(&self) -> bool {
        self.is_bit_set(Self::SWAPPED_BIT)
    }
    fn set_swapped(&mut self, value: bool) {
        self.set_bit(Self::SWAPPED_BIT, value);
    }
}

impl fmt::Display for PageTableEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "v:{} w:{} s:{} pp:{}",
            self.valid(),
            self.writable(),
            self.swapped(),
            self.physical_page()
        )
    }
}

struct PageTable {
    ptes: [PageTableEntry; MM_NUM_PTES],
}

#[derive(Debug)]
struct Process {
    page_table_resident: bool,
    page_table_exists: bool,
    page_table_pagenum: u8,
    num_resident_data_pages: u8,
    swap_file: Option<std::fs::File>,
}
impl Process {
    pub fn new() -> Self {
        Self {
            page_table_resident: false,
            page_table_exists: false,
            page_table_pagenum: 0,
            num_resident_data_pages: 0,
            swap_file: None,
        }
    }
}

struct State {
    processes: [Process; MM_MAX_PROCESSES],
    phys_mem: [u8; MM_PHYSICAL_MEMORY_SIZE_BYTES],
}
impl State {
    pub fn new() -> Self {
        Self {
            processes: std::array::from_fn(|_| Process::new()),
            phys_mem: [0; MM_PHYSICAL_MEMORY_SIZE_BYTES],
        }
    }
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| Mutex::new(State::new()));

//struct MM_MapResult MM_Map(int pid, uint32_t address, int writable);
#[unsafe(no_mangle)]
pub extern "C" fn MM_Map(pid: c_int, address: c_uint, writable: c_int) -> *mut c_char {
    let state = STATE.lock().expect("mutex poisoned");
    let mut proc = &state.processes[pid as usize];
    let mut virt_page = 2; // THIS IS WRONG

    // NB: page_table_pagenum is NOT correct here, at least not yet.
    let pt_offset = proc.page_table_pagenum as usize * MM_PAGE_SIZE_BYTES;
    let pt_offset_end = pt_offset + MM_PAGE_SIZE_BYTES;
    let phys_mem_page_slice = &state.phys_mem[pt_offset..pt_offset_end];
    let pt: &mut PageTable = unsafe { &mut *(phys_mem_page_slice.as_ptr() as *mut PageTable) };
    let pte: &mut PageTableEntry = &mut pt.ptes[virt_page];
    debug!("pte contents pre: {}", pte);
    pte.set_physical_page(3 /* WRONG */);
    pte.set_writable(writable != 0);
    debug!("pte contents post: {}", pte);

    //let s = CString::new("Hello, world!").expect("CString::new failed");
    //s.into_raw()
    std::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub extern "C" fn MM_SwapOn() {
    let state = STATE.lock().expect("mutex poisoned");
}

#[unsafe(no_mangle)]
pub extern "C" fn MM_LoadByte(pid: c_int, address: c_uint, value: *mut c_uchar) -> c_int {
    let state = STATE.lock().expect("mutex poisoned");
    -1
}

#[unsafe(no_mangle)]
pub extern "C" fn MM_StoreByte(pid: c_int, address: c_uint, value: c_uchar) -> c_int {
    let state = STATE.lock().expect("mutex poisoned");
    -1
}

